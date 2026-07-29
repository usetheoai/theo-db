-- M168 — o oráculo de CANCELAMENTO do top-k streaming.
--
-- WHY. Uma revisão de pgrx encontrou o BLOCKER da série: o safe-point de interrupção do M168 chamava
-- `pgrx::check_for_interrupts!()` de dentro do `block_on`, sob a alegação — escrita no comentário — de que "um
-- longjmp daqui vira panic". É FALSO no pgrx 0.19: o macro expande para a extern crua
-- (`pgrx-pg-sys-0.19.0/src/submodules/elog.rs:587`), e nada instala um `sigsetjmp` ali. O `ereport(ERROR)` de
-- dentro do `ProcessInterrupts` faz `siglongjmp` direto para o `PG_exception_stack`, PULANDO todos os frames Rust
-- vivos: runtime tokio, `SessionContext`, plano físico, as k linhas do TopK, o `RecordBatch` em voo, o
-- `relation_close`.
--
-- O projeto já tinha registrado e fechado esse risco no M98 (`am/datafusion_probe.rs:10-14`). O M168 o reabriu.
--
-- E NENHUM oráculo desta série podia pegá-lo, por uma razão estrutural: `m168_pending_rows.sql`,
-- `m168_stream_ab.sql`, `m168_peak.sql` e `m168_large_k.sql` **rodam até o fim**. Nenhum cancela nada. O caminho
-- mais perigoso do milestone era o único sem oráculo.
--
-- O SINTOMA QUE ESTE ARQUIVO PROCURA não é o erro de cancelamento — esse é esperado e correto. É o que sobra
-- DEPOIS dele. Com o defeito, o `EnterRuntimeGuard` do tokio não roda seu `Drop`, o thread-local fica `Entered`,
-- e o PRÓXIMO `block_on` da mesma conexão bate em
-- `panic!("Cannot start a runtime from within a runtime")`. Como o backend é por conexão, toda consulta
-- DataFusion seguinte daquela sessão morre. Então o teste é: cancele, e depois PROVE que a sessão continua viva.
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
--   EXPLAIN CREATE TABLE x AS SELECT … LIMIT 100      ->  Limit -> Custom Scan (theodb_columnar_agg)   [ROTEIA]
--   EXPLAIN SELECT count(*) FROM (SELECT … LIMIT 100) ->  Aggregate -> Limit -> Sort                   [NATIVO]
--
-- Ou seja: os passos de sobrevivência rodavam o plano NATIVO do PostgreSQL, que não tem runtime tokio algum, e
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
      'M168-C0 FAILED: a consulta alvo NÃO roteia para o caminho colunar. O cancelamento abaixo interromperia o '
      'plano NATIVO do PostgreSQL, que não tem runtime tokio algum — o teste passaria sem nunca exercitar o '
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
-- A evidência diferencial existe e está no log, mas era um humano contando: na coleta de 2026-07-29 o C1 emitiu
-- 17 linhas `theodb_decode_batch_stream` contra as 100 do C2 (1M linhas ÷ CHUNK_GROUP_ROWS=10.000). ISSO prova
-- que o scan foi cortado em ~17%. A asserção C4 abaixo transforma essa contagem em gate.
--
-- Limite honesto que permanece: o corte depende do `statement_timeout` cair DENTRO de um `poll_next`. Um k menor,
-- cache mais quente ou uma box mais rápida põem o cancelamento depois do último batch — por isso o C4 reprova como
-- INCONCLUSIVO (não como sucesso) quando a contagem não indica corte.
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
  WHEN OTHERS THEN
    INSERT INTO cancel_res VALUES ('c1_outcome', 'other:' || SQLSTATE || ':' || SQLERRM);
END
$c1$;
RESET statement_timeout;
\set ON_ERROR_STOP on

\echo '### M168-C2: A SESSÃO SOBREVIVEU? — este é o teste, não o C1'
-- Com o defeito, o thread-local do tokio ficou `Entered` e este `block_on` entra no
-- `panic!("Cannot start a runtime from within a runtime")`. Mesma forma do C1 (para exercitar o mesmo caminho),
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

\echo '### M168-C4: quanto tempo o scan COMPLETO leva? — a referência do sinal diferencial'
-- Sem esta medida, "c1 levou 200ms" não significa nada: pode ser que o scan inteiro leve 200ms. A referência é o
-- MESMO trabalho sem timeout. O gate compara os dois.
SET theodb.enable_columnar_topk_stream = on;
DO $c4$
DECLARE t0 timestamptz := clock_timestamp();
BEGIN
  EXECUTE 'CREATE TEMP TABLE c4_out AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000';
  INSERT INTO cancel_res VALUES ('c4_full_scan_ms',
    round(extract(epoch FROM clock_timestamp() - t0) * 1000)::text);
END
$c4$;

\echo '### GATE FINAL'
SELECT * FROM cancel_res ORDER BY q;
\if :{?gate_selftest}
  \echo '### GATE SELF-TEST ARMADO: dois braços — o gate abaixo DEVE abortar em AMBOS'
  -- Braço 1 — sessão morta: a assinatura do longjmp atravessando os frames Rust.
  UPDATE cancel_res SET v = 'ERRO:XX000:Cannot start a runtime from within a runtime'
   WHERE q = 'c2_rows_after_cancel';
  -- Braço 2 — safe-point AUSENTE: o cancelamento só é notado depois do scan completo, então o tempo decorrido
  -- de C1 iguala o do scan inteiro. Sem esta asserção o arquivo passa com o safe-point removido, que é o
  -- contra-exemplo que uma revisão construiu. Simular por dado é o único jeito de exercitar o ramo sem
  -- desinstalar o safe-point e reconstruir por 14 minutos.
  UPDATE cancel_res SET v = (SELECT v FROM cancel_res WHERE q = 'c4_full_scan_ms')
   WHERE q = 'c1_elapsed_ms';
\endif
DO $gate$
DECLARE bad text := ''; c1 text; c2 text; c3 text; el numeric; full_ms numeric;
BEGIN
  SELECT v INTO c1 FROM cancel_res WHERE q = 'c1_outcome';
  SELECT v INTO c2 FROM cancel_res WHERE q = 'c2_rows_after_cancel';
  SELECT v INTO c3 FROM cancel_res WHERE q = 'c3_eager_rows_after_cancel';
  SELECT v::numeric INTO el FROM cancel_res WHERE q = 'c1_elapsed_ms';
  SELECT v::numeric INTO full_ms FROM cancel_res WHERE q = 'c4_full_scan_ms';

  -- DIFERENCIALIDADE. Este é o gate que faltava: sem o safe-point, o cancelamento só é notado DEPOIS do scan
  -- completo, então `c1_elapsed` ~ `c4_full_scan`. Com ele, o corte acontece na fronteira de chunk-group e
  -- `c1_elapsed` fica perto do timeout. Sem esta comparação o arquivo inteiro passa com o safe-point REMOVIDO
  -- (contra-exemplo construído em review).
  IF el IS NULL OR full_ms IS NULL THEN
    bad := bad || 'faltam as medidas de tempo (c1_elapsed_ms / c4_full_scan_ms): o gate diferencial não pôde '
               || 'rodar, e sem ele este arquivo passa mesmo com o safe-point removido; ';
  ELSIF full_ms < 400 THEN
    -- INCONCLUSIVO, não sucesso: se o scan completo já cabe perto do timeout, os dois regimes são
    -- indistinguíveis por relógio. Aumente o k (ou baixe o timeout) e repita.
    bad := bad || format('c4_full_scan_ms=%s (<400): o scan completo é rápido demais para separar "cancelou no '
                      || 'meio" de "cancelou no fim" — teste INCONCLUSIVO, não aprovado; ', full_ms);
  ELSIF el > full_ms * 0.5 THEN
    bad := bad || format('c1_elapsed_ms=%s contra c4_full_scan_ms=%s: o cancelamento levou mais da metade do '
                      || 'scan completo, ou seja NÃO foi notado na fronteira de chunk-group. É a assinatura de '
                      || 'um safe-point ausente ou inerte; ', el, full_ms);
  END IF;

  -- NÃO-VACUIDADE. Se a consulta terminou antes do timeout, o safe-point nunca viu um cancelamento pendente e
  -- os sucessos de C2/C3 não provam nada. Isso é INCONCLUSIVO, e dizer isso é obrigatório — um gate que passa
  -- sem ter exercitado o caminho é o falso-verde que esta série inteira combate.
  IF c1 IS DISTINCT FROM 'canceled' THEN
    bad := bad || format('c1_outcome=%s: o cancelamento NÃO ocorreu, então C2/C3 não exercitaram o caminho de '
                      || 'longjmp. Aumente o k ou reduza o statement_timeout e repita; ', coalesce(c1, '(nulo)'));
  END IF;

  -- NÃO-VACUIDADE, PARTE 2. Um passo que não roteou rodou o plano nativo, que não toca o runtime tokio — ele
  -- passaria idêntico COM o defeito. Foi o falso-verde da primeira versão deste arquivo.
  IF c2 = 'NAO_ROTEOU' OR c3 = 'NAO_ROTEOU' THEN
    bad := bad || format('roteamento ausente (c2=%s, c3=%s): o passo de sobrevivência rodou o plano NATIVO, que '
                      || 'não tem runtime tokio — ele passaria com o defeito presente; ', c2, c3);
  END IF;

  -- O TESTE. Com o defeito, o thread-local do tokio fica sujo e este ramo pega o panic do runtime.
  IF c2 IS NULL OR c2 LIKE 'ERRO:%' THEN
    bad := bad || format('c2_rows_after_cancel=%s: a sessão NÃO sobreviveu ao cancelamento. É a assinatura do '
                      || 'longjmp atravessando os frames Rust (runtime tokio nunca silenciado). Ver '
                      || 'interrupt_is_pending em df_executor.rs; ', coalesce(c2, '(nulo)'));
  ELSIF c2::bigint <> 100 THEN
    bad := bad || format('c2_rows_after_cancel=%s, esperado 100; ', c2);
  END IF;

  IF c3 IS NULL OR c3 LIKE 'ERRO:%' THEN
    bad := bad || format('c3_eager_rows_after_cancel=%s: o caminho EAGER da mesma sessão também morreu — o dano '
                      || 'não ficou contido no caminho streaming; ', coalesce(c3, '(nulo)'));
  END IF;

  IF bad <> '' THEN RAISE EXCEPTION 'M168 CANCEL GATE FAILED: %', bad; END IF;
  RAISE NOTICE 'M168 CANCEL GATE ok: o top-k streaming foi cancelado de verdade (57014) e a sessão continuou '
               'servindo consultas DataFusion nos dois caminhos — o estado Rust desenrolou antes do longjmp.';
END
$gate$;
DROP TABLE cancel_res;
