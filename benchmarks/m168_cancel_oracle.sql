-- M168 — o oráculo de CANCELAMENTO do top-k streaming.
--
-- WHY. Ao decodificar por partes, o M168 fez o holdoff de interrupções cobrir a leitura de TODAS as páginas —
-- antes o decode acontecia fora dele. Sem um safe-point, um scan longo ignora `Ctrl-C`, `statement_timeout`,
-- `transaction_timeout` e `pg_terminate_backend` do começo ao fim. O safe-point existe para fechar isso, e este
-- arquivo é o oráculo dele.
--
-- ATENÇÃO AO QUE ESTE COMENTÁRIO NÃO DIZ MAIS. Uma versão anterior afirmava que chamar
-- `pgrx::check_for_interrupts!()` dentro do `block_on` causaria um `siglongjmp` pulando os frames Rust vivos, e
-- citava `elog.rs:587` como prova. **Isso é falso, e a própria linha citada o refuta:** ela expande para
-- `$crate::ProcessInterrupts()`, que é o wrapper reescrito por `#[pg_guard]` — o único bloco `extern "C-unwind"`
-- do `pgrx-pg-sys-0.19.0` carrega esse atributo (`src/include/pg18.rs:35462`, com `ProcessInterrupts` dentro em
-- `:39525`), o macro reescreve cada função para `pg_guard_ffi_boundary` (`pgrx-macros/rewriter.rs:184-193`), e
-- `ffi.rs:85` declara que a função protege **toda** extern gerada. O PG `ERROR` vira `panic_any` e os frames Rust
-- DESENROLAM. Este repositório já dizia isso em `am/build.rs:466` e `Cargo.toml:85-86`.
--
-- Corrigir só a prosa do verdict e deixar a alegação falsa aqui seria pior que inútil: este é o arquivo que um
-- revisor futuro abre. E o dano seria concreto — há QUATRO `check_for_interrupts!()` vivos em laços de
-- `CREATE INDEX` (`am/build.rs:420,474,487,812`) que o racional falso condenaria em bloco, deixando
-- `CREATE INDEX` incancelável. (Há um quinto em `bench_symqg.rs:76`, que é benchmark, não produção.)
--
-- POR QUE O DESENHO É LER-E-DEVOLVER-`Err`, ENTÃO. Por duas razões verdadeiras, e bastam: (1) não desenrolar por
-- dentro de frames async de terceiros — um panic no `poll_next` atravessa o executor do tokio e o plano do
-- DataFusion, código cuja exception-safety não auditamos; devolver `Err` faz o DataFusion desmontar o plano pelo
-- caminho que ele mesmo testa; (2) ponto de cancelamento determinístico. Isso torna o desenho mais fácil de
-- auditar; **não** torna o anterior inseguro.
--
-- NENHUM outro oráculo desta série exercita este caminho: `m168_pending_rows.sql`, `m168_stream_ab.sql`,
-- `m168_peak.sql` e `m168_large_k.sql` **rodam até o fim**. Nenhum cancela nada.
--
-- O QUE ESTE ARQUIVO TESTA, em duas partes: (a) que o cancelamento é notado NO MEIO do scan, não no fim — é o
-- gate de contagem do C4, e sem ele o arquivo passa com o safe-point removido; e (b) que a sessão continua servindo
-- consultas depois, nos dois caminhos.
--
-- REQUER: tabela colunar `hits` com ≥ 2 chunk-groups (o safe-point roda na fronteira entre eles).
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;
SET work_mem = '64MB';

DROP TABLE IF EXISTS cancel_res;
CREATE TEMP TABLE cancel_res (q text, v text);

-- A FORMA DA CONSULTA É LOAD-BEARING, e a primeira versão deste arquivo errou nisso — falso-verde clássico.
-- Ela envolvia o top-k em `count(*) FROM (...) s` / `PERFORM * FROM (...) s`, e essa forma **DECLINA**: o pai do
-- Sort vira um Aggregate, não um Limit, e o admit emite `topk_parent_not_limit`. Medido:
--
--   CTAS:      Limit -> Custom Scan (theodb_columnar_agg)                                  [caminho do top-k]
--   count(*):  Aggregate -> Limit -> Sort -> Result -> Custom Scan (theodb_columnar_project)  [NÃO é o top-k]
--
-- (planos completos em `docs/benchmarks/m168-artifacts/routing-shapes.log`)
--
-- A forma count(*) não roda "o plano nativo" puro — ela ainda usa o `theodb_columnar_project` do M149. O que ela
-- não engaja é o caminho do TOP-K, que é o único que instancia runtime tokio e DataFusion
-- (`columnar_project.rs` não tem uma referência sequer a nenhum dos dois). Por isso os passos de sobrevivência
-- teriam passado idênticos COM o defeito presente. É a lição do M161 pela enésima vez — um A/B que não confirma
-- o plano é falso-verde. Daqui em diante todo passo usa CTAS (que preserva o Limit como pai) e **assere o
-- roteamento no próprio passo**, não só uma vez no C0.
\echo '### M168-C0: precondição — a forma CTAS TEM de rotear para o caminho colunar'
DO $c0$
DECLARE plan text;
BEGIN
  EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) CREATE TABLE c0_probe AS '
          'SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000' INTO plan;
  IF position('theodb_columnar_agg' IN plan) = 0 THEN
    RAISE EXCEPTION
      'M168-C0 FAILED: a consulta alvo NÃO roteia para o caminho do top-k colunar. O cancelamento abaixo cairia '
      'num plano que não instancia runtime tokio nem DataFusion — o teste passaria sem nunca exercitar o '
      'safe-point. Plano: %', plan;
  END IF;
  RAISE NOTICE 'M168-C0 ok: a forma CTAS roteia — o cancelamento vai cair no safe-point de verdade';
END
$c0$;

-- O GATE DESTE ARQUIVO NÃO É DIFERENCIAL SOZINHO, e isso precisa estar dito onde alguém leia.
--
-- Um review construiu o contra-exemplo: APAGUE o bloco `if interrupt_is_pending()` de df_executor.rs e o gate
-- abaixo continua verde. O `statement_timeout` arma os flags, o stream percorre os 100 chunk-groups até o fim, e o
-- `check_for_interrupts!()` que vem depois do `drop(held)` levanta o 57014 assim mesmo. `c1_outcome='canceled'`,
-- `c2=100`, `c3=100` — passa. O gate não distingue "cancelou NO MEIO" de "cancelou NO FIM".
--
-- A evidência diferencial é quantas vezes o stream AVANÇOU O CURSOR: um scan completo de 1M linhas avança 101
-- (100 chunk-groups + a chamada terminal e a sonda de schema); um cortado avança menos.
-- `theodb_columnar_stream_chunk_groups()` expõe o número — ele conta CHAMADAS, não chunk-groups, e a doc da
-- função explica as duas direções em que os dois diferem. O gate compara C1 contra C4 como RAZÃO, então o +1
-- constante se cancela. Uma versão anterior gateava TEMPO DE RELÓGIO enquanto o comentário afirmava gatear
-- a contagem (achado de review) — e uma amostra única de relógio, sujeita a carga da box e a estado de cache,
-- viola o R3 de `discover-phd-rigor.md` para alegação de tempo. A contagem não depende de nada disso.
--
-- Limite honesto que permanece: o corte depende do `statement_timeout` cair DENTRO de um `poll_next`. Se o scan
-- tiver poucos chunk-groups não há onde cortar no meio — por isso o gate reprova como INCONCLUSIVO (não como
-- sucesso) quando `c4_chunk_groups < 4`.
\echo '### M168-C1: cancela um top-k streaming no meio via statement_timeout'
SET theodb.enable_columnar_topk_stream = on;
SET statement_timeout = '150ms';
\set ON_ERROR_STOP off
-- ESPERADO: erro 57014 (query_canceled). Um resultado aqui significa que a consulta terminou antes do timeout —
-- o gate abaixo trata isso como teste INCONCLUSIVO, não como sucesso.
DO $c1$
DECLARE t0 timestamptz := clock_timestamp();
BEGIN
  EXECUTE 'CREATE TEMP TABLE c1_out AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000';
  INSERT INTO cancel_res VALUES ('c1_outcome', 'completed_before_timeout');
EXCEPTION
  WHEN query_canceled THEN
    INSERT INTO cancel_res VALUES ('c1_outcome', 'canceled');
    -- O SINAL DIFERENCIAL. Com o safe-point, o cancelamento é notado na fronteira de chunk-group e a consulta
    -- morre logo após os 150ms do timeout. SEM ele, o scan percorre os 100 chunk-groups até o fim e só então o
    -- `check_for_interrupts!()` pós-`drop(held)` levanta o mesmo 57014 — mesmo veredito, ordem de grandeza
    -- diferente no relógio. É isto que separa "cancelou no meio" de "cancelou no fim".
    INSERT INTO cancel_res VALUES ('c1_elapsed_ms',
      round(extract(epoch FROM clock_timestamp() - t0) * 1000)::text);
    -- O SINAL DETERMINÍSTICO, que é o que o gate usa. O tempo fica registrado como informação secundária:
    -- ele depende de velocidade de máquina, cache e carga, e uma amostra única de relógio viola o R3 de
    -- `discover-phd-rigor.md`. A contagem de chunk-groups não depende de nada disso.
    INSERT INTO cancel_res VALUES ('c1_chunk_groups', theodb_columnar_stream_chunk_groups()::text);
  WHEN OTHERS THEN
    INSERT INTO cancel_res VALUES ('c1_outcome', 'other:' || SQLSTATE || ':' || SQLERRM);
END
$c1$;
RESET statement_timeout;
\set ON_ERROR_STOP on

\echo '### M168-C2: A SESSÃO SOBREVIVEU? — este é o teste, não o C1'
-- Prova que a sessão continua utilizável depois de um cancelamento: nenhum recurso ficou meio-desmontado, nem o
-- runtime tokio, nem o `SessionContext`, nem a `Relation`. Mesma forma do C1 (para exercitar o mesmo caminho),
-- k pequeno para terminar rápido, e roteamento asserido AQUI — não herdado do C0.
\set ON_ERROR_STOP off
DO $c2$
DECLARE n bigint; plan text;
BEGIN
  EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) CREATE TABLE c2_probe AS '
          'SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 100' INTO plan;
  IF position('theodb_columnar_agg' IN plan) = 0 THEN
    INSERT INTO cancel_res VALUES ('c2_rows_after_cancel', 'NAO_ROTEOU');
    RETURN;
  END IF;
  EXECUTE 'CREATE TEMP TABLE c2_out AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 100';
  SELECT count(*) INTO n FROM c2_out;
  INSERT INTO cancel_res VALUES ('c2_rows_after_cancel', n::text);
EXCEPTION
  WHEN OTHERS THEN
    INSERT INTO cancel_res VALUES ('c2_rows_after_cancel', 'ERRO:' || SQLSTATE || ':' || SQLERRM);
END
$c2$;
\set ON_ERROR_STOP on

\echo '### M168-C3: e o caminho eager da mesma sessão também sobreviveu?'
SET theodb.enable_columnar_topk_stream = off;
\set ON_ERROR_STOP off
DO $c3$
DECLARE n bigint; plan text;
BEGIN
  EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) CREATE TABLE c3_probe AS '
          'SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 100' INTO plan;
  IF position('theodb_columnar_agg' IN plan) = 0 THEN
    INSERT INTO cancel_res VALUES ('c3_eager_rows_after_cancel', 'NAO_ROTEOU');
    RETURN;
  END IF;
  EXECUTE 'CREATE TEMP TABLE c3_out AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 100';
  SELECT count(*) INTO n FROM c3_out;
  INSERT INTO cancel_res VALUES ('c3_eager_rows_after_cancel', n::text);
EXCEPTION
  WHEN OTHERS THEN
    INSERT INTO cancel_res VALUES ('c3_eager_rows_after_cancel', 'ERRO:' || SQLSTATE || ':' || SQLERRM);
END
$c3$;
\set ON_ERROR_STOP on
SET theodb.enable_columnar_topk_stream = on;

\echo '### M168-C4: quantos avanços de cursor o scan COMPLETO faz? — a referência do sinal diferencial'
-- Sem esta referência, "c1 avançou 16 vezes" não significa nada: pode ser que o scan inteiro avance 16. A
-- referência é o MESMO trabalho sem timeout. O gate compara os dois como razão — o que também faz o +1 constante
-- (chamada terminal + sonda de schema) se cancelar.
SET theodb.enable_columnar_topk_stream = on;
DO $c4$
DECLARE t0 timestamptz := clock_timestamp(); plan text;
BEGIN
  -- C4 é o DENOMINADOR do gate, então ele assere o próprio roteamento como todos os outros passos. Uma versão
  -- anterior era o único passo sem essa asserção, contradizendo a regra que este arquivo declara acima.
  EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) CREATE TABLE c4_probe AS '
          'SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000' INTO plan;
  IF position('theodb_columnar_agg' IN plan) = 0 THEN
    INSERT INTO cancel_res VALUES ('c4_chunk_groups', 'NAO_ROTEOU');
    RETURN;
  END IF;
  EXECUTE 'CREATE TEMP TABLE c4_out AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000';
  INSERT INTO cancel_res VALUES ('c4_full_scan_ms',
    round(extract(epoch FROM clock_timestamp() - t0) * 1000)::text);
  INSERT INTO cancel_res VALUES ('c4_chunk_groups', theodb_columnar_stream_chunk_groups()::text);
END
$c4$;

\echo '### GATE FINAL'
SELECT * FROM cancel_res ORDER BY q;
\if :{?gate_selftest}
  \echo '### GATE SELF-TEST ARMADO: dois braços — o gate abaixo DEVE abortar em AMBOS'
  -- Braço 1 — sessão inutilizável depois do cancelamento (qualquer erro em C2 serve; a string abaixo é só um
  -- exemplo plausível de estado meio-desmontado).
  UPDATE cancel_res SET v = 'ERRO:XX000:sessao inutilizavel apos cancelamento'
   WHERE q = 'c2_rows_after_cancel';
  -- Braço 2 — safe-point AUSENTE: o cancelamento só é notado depois do scan completo, então C1 avança o cursor
  -- tantas vezes quanto o scan inteiro. Sem esta asserção o arquivo passa com o safe-point removido, que é o
  -- contra-exemplo que uma revisão construiu. Simular por dado é o único jeito de exercitar o ramo sem
  -- desinstalar o safe-point e reconstruir por 14 minutos.
  UPDATE cancel_res SET v = (SELECT v FROM cancel_res WHERE q = 'c4_chunk_groups')
   WHERE q = 'c1_chunk_groups';
\endif
DO $gate$
DECLARE bad text := ''; c1 text; c2 text; c3 text; el numeric; full_ms numeric; c1cg text; c4cg text;
BEGIN
  SELECT v INTO c1 FROM cancel_res WHERE q = 'c1_outcome';
  SELECT v INTO c2 FROM cancel_res WHERE q = 'c2_rows_after_cancel';
  SELECT v INTO c3 FROM cancel_res WHERE q = 'c3_eager_rows_after_cancel';
  SELECT v::numeric INTO el FROM cancel_res WHERE q = 'c1_elapsed_ms';
  SELECT v::numeric INTO full_ms FROM cancel_res WHERE q = 'c4_full_scan_ms';
  SELECT v INTO c1cg FROM cancel_res WHERE q = 'c1_chunk_groups';
  SELECT v INTO c4cg FROM cancel_res WHERE q = 'c4_chunk_groups';

  -- DIFERENCIALIDADE — o gate que faltava, e agora sobre um sinal DETERMINÍSTICO. Sem o safe-point, o
  -- cancelamento só é notado DEPOIS do scan completo, então o scan cancelado entrega TANTOS chunk-groups quanto o
  -- completo. Com ele, entrega menos. Uma versão anterior comparava tempo de relógio, e o comentário deste
  -- arquivo afirmava gatear "a contagem" quando gateava o relógio (achado de review) — além de ser amostra única,
  -- o que o R3 de `discover-phd-rigor.md` proíbe para alegação de tempo. A contagem é imune a velocidade de
  -- máquina, cache e carga.
  IF c1cg = 'NAO_ROTEOU' OR c4cg = 'NAO_ROTEOU' THEN
    bad := bad || 'C1 ou C4 não roteou para o caminho streaming — o gate diferencial não mede nada; ';
  ELSIF c1cg IS NULL OR c4cg IS NULL THEN
    bad := bad || 'falta a contagem de chunk-groups (c1/c4): sem ela este arquivo passa mesmo com o safe-point '
               || 'removido; ';
  ELSIF c1cg::numeric < 2 THEN
    -- PISO, e ele fecha um regime em que este arquivo passava sem exercitar nada (achado de review): se o
    -- cancelamento cai ANTES do primeiro poll — planning + `plan_columnar_scan` (que lê header e diretório de
    -- todos os stripes) + a sonda de schema estourando os 150ms numa box fria —, então `c1cg = 0`, a razão
    -- `0 > c4/2` é falsa, e o gate imprimia "cortou no MEIO" sem o laço do stream ter rodado uma vez sequer.
    -- Passava COM ou SEM o safe-point instalado.
    --
    -- Isto é mais grave do que parece porque `c1cg >= 1` é a ÚNICA evidência neste arquivo de que o braço
    -- STREAMING rodou em C1: as sondas EXPLAIN não distinguem streaming de eager (a GUC é lida em tempo de
    -- EXECUÇÃO dentro de `run_columnar_topk`, não no plano — o próprio C3 demonstra, pondo a GUC em `off` e
    -- ainda assim asserindo `theodb_columnar_agg`).
    bad := bad || format('c1_chunk_groups=%s (<2): o cancelamento chegou antes do laço do stream, então nada do '
                      || 'caminho streaming foi exercitado — INCONCLUSIVO, não aprovado. Aumente o '
                      || 'statement_timeout; ', c1cg);
  ELSIF c4cg::numeric < 4 THEN
    -- INCONCLUSIVO, não sucesso: com poucos chunk-groups não há onde cortar no meio.
    bad := bad || format('c4_chunk_groups=%s (<4): o scan tem chunk-groups demais poucos para distinguir "cortou '
                      || 'no meio" de "cortou no fim" — INCONCLUSIVO, não aprovado; ', c4cg);
  ELSIF c1cg::numeric > c4cg::numeric * 0.5 THEN
    bad := bad || format('c1_chunk_groups=%s contra c4_chunk_groups=%s: o cancelamento entregou mais da metade '
                      || 'dos chunk-groups do scan completo, ou seja NÃO foi notado na fronteira. É a assinatura '
                      || 'de um safe-point ausente ou inerte; ', c1cg, c4cg);
  END IF;
  -- O tempo fica como informação secundária no relatório, não como gate.

  -- NÃO-VACUIDADE. Se a consulta terminou antes do timeout, o safe-point nunca viu um cancelamento pendente e
  -- os sucessos de C2/C3 não provam nada. Isso é INCONCLUSIVO, e dizer isso é obrigatório — um gate que passa
  -- sem ter exercitado o caminho é o falso-verde que esta série inteira combate.
  IF c1 IS DISTINCT FROM 'canceled' THEN
    bad := bad || format('c1_outcome=%s: o cancelamento NÃO ocorreu, então C2/C3 não exercitaram o caminho de '
                      || 'cancelamento. Aumente o k ou reduza o statement_timeout e repita; ', coalesce(c1, '(nulo)'));
  END IF;

  -- NÃO-VACUIDADE, PARTE 2. Um passo que não roteou rodou o plano nativo, que não toca o runtime tokio — ele
  -- passaria idêntico COM o defeito. Foi o falso-verde da primeira versão deste arquivo.
  IF c2 = 'NAO_ROTEOU' OR c3 = 'NAO_ROTEOU' THEN
    bad := bad || format('roteamento ausente (c2=%s, c3=%s): o passo de sobrevivência rodou o plano NATIVO, que '
                      || 'não tem runtime tokio — ele passaria com o defeito presente; ', c2, c3);
  END IF;

  -- O TESTE. Se o cancelamento tivesse deixado algum recurso meio-desmontado, este ramo pegaria o erro.
  IF c2 IS NULL OR c2 LIKE 'ERRO:%' THEN
    bad := bad || format('c2_rows_after_cancel=%s: a sessão NÃO sobreviveu ao cancelamento. É a assinatura do '
                      || 'um recurso deixado meio-desmontado pelo cancelamento (runtime tokio, SessionContext '
                      || 'ou Relation). Ver interrupt_is_pending em df_executor.rs; ', coalesce(c2, '(nulo)'));
  ELSIF c2::bigint <> 100 THEN
    bad := bad || format('c2_rows_after_cancel=%s, esperado 100; ', c2);
  END IF;

  IF c3 IS NULL OR c3 LIKE 'ERRO:%' THEN
    bad := bad || format('c3_eager_rows_after_cancel=%s: o caminho EAGER da mesma sessão também morreu — o dano '
                      || 'não ficou contido no caminho streaming; ', coalesce(c3, '(nulo)'));
  END IF;

  IF bad <> '' THEN RAISE EXCEPTION 'M168 CANCEL GATE FAILED: %', bad; END IF;
  RAISE NOTICE 'M168 CANCEL GATE ok: o top-k streaming foi cancelado de verdade (57014) e a sessão continuou '
               'servindo consultas DataFusion nos dois caminhos, e o corte aconteceu no MEIO do scan (gate de contagem de chunk-groups).';
END
$gate$;
DROP TABLE cancel_res;
