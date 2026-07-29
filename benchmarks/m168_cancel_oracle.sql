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

\echo '### M168-C1: cancela um top-k streaming no meio via statement_timeout'
SET theodb.enable_columnar_topk_stream = on;
SET statement_timeout = '150ms';
\set ON_ERROR_STOP off
-- ESPERADO: erro 57014 (query_canceled). Um resultado aqui significa que a consulta terminou antes do timeout —
-- o gate abaixo trata isso como teste INCONCLUSIVO, não como sucesso.
DO $c1$
BEGIN
  EXECUTE 'CREATE TEMP TABLE c1_out AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000';
  INSERT INTO cancel_res VALUES ('c1_outcome', 'completed_before_timeout');
EXCEPTION
  WHEN query_canceled THEN
    INSERT INTO cancel_res VALUES ('c1_outcome', 'canceled');
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

\echo '### GATE FINAL'
SELECT * FROM cancel_res ORDER BY q;
\if :{?gate_selftest}
  \echo '### GATE SELF-TEST ARMADO: forçando c2 a reportar erro — o gate abaixo DEVE abortar'
  UPDATE cancel_res SET v = 'ERRO:XX000:Cannot start a runtime from within a runtime'
   WHERE q = 'c2_rows_after_cancel';
\endif
DO $gate$
DECLARE bad text := ''; c1 text; c2 text; c3 text;
BEGIN
  SELECT v INTO c1 FROM cancel_res WHERE q = 'c1_outcome';
  SELECT v INTO c2 FROM cancel_res WHERE q = 'c2_rows_after_cancel';
  SELECT v INTO c3 FROM cancel_res WHERE q = 'c3_eager_rows_after_cancel';

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
