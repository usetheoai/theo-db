-- M168 — evidência dos DOIS planos que o § 3.6 do verdict cita como "Medido:".
--
-- WHY. O verdict afirma que a forma CTAS roteia para o caminho colunar e que a forma `count(*) FROM (…)` NÃO
-- roteia, e usa isso para explicar por que a primeira versão do oráculo de cancelamento era um falso-verde. Uma
-- revisão apontou que os dois EXPLAIN eram introduzidos com "Medido:" e não tinham artefato algum — a mesma
-- classe de alegação sem lastro que o resto do documento evita. Este arquivo produz o artefato.
--
-- A diferença importa porque é sutil: as duas consultas pedem exatamente as mesmas linhas. O que muda é o PAI do
-- nó Sort. Na forma CTAS o pai é um Limit, e o admit aceita; embrulhada num `count(*)` o pai vira um Aggregate, o
-- admit emite `topk_parent_not_limit`, e a consulta cai no executor de linha do PostgreSQL — que não tem runtime
-- tokio algum, e portanto não exercita nada do que o M168 mudou.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;
SET theodb.enable_columnar_topk_stream = on;
SET work_mem = '64MB';

\echo '################ FORMA A — CTAS (a que o oráculo usa): DEVE rotear ################'
EXPLAIN (COSTS OFF)
CREATE TABLE shape_a AS
SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits
 ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 100;

\echo '################ FORMA B — count(*) wrapper (a da 1ª versão): NÃO roteia ################'
EXPLAIN (COSTS OFF)
SELECT count(*) FROM (
  SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits
   ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 100) s;

\echo '################ GATE — a diferença TEM de estar presente, senão a explicação do §3.6 é falsa ##########'
DO $gate$
DECLARE pa text; pb text; bad text := '';
BEGIN
  EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) CREATE TABLE shape_a2 AS '
          'SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 100' INTO pa;
  EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) SELECT count(*) FROM ('
          'SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits '
          'ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 100) s' INTO pb;

  IF position('theodb_columnar_agg' IN pa) = 0 THEN
    bad := bad || 'FORMA A não roteia — a premissa do oráculo de cancelamento caiu; ';
  END IF;
  IF position('theodb_columnar_agg' IN pb) > 0 THEN
    bad := bad || 'FORMA B ROTEIA — então a explicação do §3.6 para o falso-verde está errada, e a primeira '
               || 'versão do oráculo não era vazia pelo motivo alegado; ';
  END IF;

  IF bad <> '' THEN RAISE EXCEPTION 'M168 ROUTING-SHAPES GATE FAILED: %', bad; END IF;
  RAISE NOTICE 'M168 ROUTING-SHAPES GATE ok: CTAS roteia, count(*) wrapper NÃO roteia — é a diferença que o '
               '§3.6 do verdict alega, agora com artefato.';
END
$gate$;
