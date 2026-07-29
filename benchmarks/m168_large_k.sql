-- M168 — top-k com k GRANDE e `work_mem` pequeno: o cenário que o fail-open existe para cobrir.
--
-- WHY. A pool do caminho streaming é constante em `k` (`2*work_mem + 64MB`) enquanto a retenção do `TopK` do
-- DataFusion cresce com ele (`topk/mod.rs` faz `reservation.try_resize(self.size())`), e a admissão não limita `k`.
-- Um reviewer apontou que, com `?`, uma consulta que o caminho eager servia passaria a ERRAR por default, e a
-- única saída seria uma GUC que o usuário não sabe existir. O `run_columnar_topk` passou a cair no eager em vez de
-- propagar — este arquivo mede se a janela existe e se o fallback funciona.
--
-- O oráculo é o próprio resultado: streaming e eager TÊM de devolver as mesmas linhas, seja o streaming servindo
-- ou caindo no fallback. Com `THEODB_ADMIT_TRACE=1` no postmaster, uma linha `theodb_topk_stream_fallback` no log
-- diz qual dos dois aconteceu.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;
SET work_mem = '4MB';   -- o default do PostgreSQL, o regime apertado

DROP TABLE IF EXISTS lk_res;
CREATE TEMP TABLE lk_res (q text, n bigint);

\echo '### M168-K1: k = 200000 sobre projeção estreita, work_mem = 4MB'
SET theodb.enable_columnar_topk_stream = on;
DROP TABLE IF EXISTS k1_on;
CREATE TEMP TABLE k1_on AS
  SELECT EventTime, SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 200000;
SET theodb.enable_columnar_topk_stream = off;
DROP TABLE IF EXISTS k1_off;
CREATE TEMP TABLE k1_off AS
  SELECT EventTime, SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 200000;
INSERT INTO lk_res SELECT 'k1_mism', count(*) FROM (
  (SELECT * FROM k1_on EXCEPT ALL SELECT * FROM k1_off)
  UNION ALL (SELECT * FROM k1_off EXCEPT ALL SELECT * FROM k1_on)) d;
INSERT INTO lk_res SELECT 'k1_rows', count(*) FROM k1_on;

\echo '### M168-K2: SELECT * (105 colunas) com k = 50000 — o caso mais pesado por linha'
SET theodb.enable_columnar_topk_stream = on;
DROP TABLE IF EXISTS k2_on;
CREATE TEMP TABLE k2_on AS
  SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY CounterID, WatchID, UserID LIMIT 50000;
SET theodb.enable_columnar_topk_stream = off;
DROP TABLE IF EXISTS k2_off;
CREATE TEMP TABLE k2_off AS
  SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY CounterID, WatchID, UserID LIMIT 50000;
INSERT INTO lk_res SELECT 'k2_mism', count(*) FROM (
  (SELECT * FROM k2_on EXCEPT ALL SELECT * FROM k2_off)
  UNION ALL (SELECT * FROM k2_off EXCEPT ALL SELECT * FROM k2_on)) d;
INSERT INTO lk_res SELECT 'k2_rows', count(*) FROM k2_on;

\echo '### M168-K3: CONTROLE POSITIVO — a maquinaria tem de acusar divergência semeada'
DROP TABLE IF EXISTS k3_bad;
CREATE TEMP TABLE k3_bad AS SELECT EventTime, SearchPhrase || 'x' AS SearchPhrase FROM k1_off LIMIT 100;
DROP TABLE IF EXISTS k3_ref;
CREATE TEMP TABLE k3_ref AS SELECT * FROM k1_on LIMIT 100;
INSERT INTO lk_res SELECT 'k3_control_diff', count(*) FROM (
  (SELECT * FROM k3_ref EXCEPT ALL SELECT * FROM k3_bad)
  UNION ALL (SELECT * FROM k3_bad EXCEPT ALL SELECT * FROM k3_ref)) d;

\echo '### GATE FINAL'
SELECT * FROM lk_res ORDER BY q;
\if :{?gate_selftest}
  \echo '### GATE SELF-TEST ARMADO: forçando k1_mism = 1 — o gate abaixo DEVE abortar'
  UPDATE lk_res SET n = 1 WHERE q = 'k1_mism';
\endif
DO $gate$
DECLARE bad text := '';
BEGIN
  bad := bad || coalesce(
    (SELECT 'divergências stream-vs-eager: ' || string_agg(format('%s=%s', q, n), ', ') || '; '
       FROM lk_res WHERE q LIKE '%\_mism' AND n <> 0), '');
  -- Não-vacuidade: um k grande que devolve poucas linhas não exercita a retenção do TopK, que é o ponto.
  IF coalesce((SELECT n FROM lk_res WHERE q = 'k1_rows'), 0) < 100000 THEN
    bad := bad || format('k1_rows=%s (<100000): o k grande não foi exercitado; ',
                         (SELECT n FROM lk_res WHERE q = 'k1_rows'));
  END IF;
  IF coalesce((SELECT n FROM lk_res WHERE q = 'k2_rows'), 0) = 0 THEN
    bad := bad || 'k2_rows=0: o caso de 105 colunas não devolveu linha alguma; ';
  END IF;
  IF coalesce((SELECT n FROM lk_res WHERE q = 'k3_control_diff'), 0) = 0 THEN
    bad := bad || 'k3_control_diff=0: a comparação não consegue falhar; ';
  END IF;
  IF bad <> '' THEN RAISE EXCEPTION 'M168 LARGE-K GATE FAILED: %', bad; END IF;
  RAISE NOTICE 'M168 LARGE-K GATE ok: streaming e eager concordam com k grande sob work_mem=4MB (seja servindo, '
               'seja caindo no fallback), e o controle negativo dispara';
END
$gate$;
