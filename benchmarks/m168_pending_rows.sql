-- M168 — a consulta enxerga as escritas da PRÓPRIA transação?
--
-- WHY THIS EXISTS. O top-k streaming planeja o scan a partir de `read_visible_stripes` — só stripes já gravados.
-- Linhas escritas pela transação corrente e ainda não descarregadas em stripe vivem em `WRITE_STATES` e são
-- invisíveis para esse plano. `decode_columns_v2` guarda contra isso no topo e cai no caminho legado; o planejador
-- streaming foi extraído de BAIXO dessa guarda e não herdou nada dela, então um estado MISTO perdia as linhas
-- novas — em silêncio.
--
-- Nenhum oráculo existente pega isso, e não é acidente: `m167_hits_topk_ab.sql` e `m158_ec_harness.sql` carregam
-- em massa e depois só leem. A forma `BEGIN; INSERT; SELECT` nunca é construída ali. Este arquivo constrói.
--
-- O caso puramente pendente (tabela sem stripe algum) já funcionava por acidente: a sonda de schema devolve
-- `None` e o caminho eager assume. O caso que quebrava é o MISTO — stripes JÁ existem E há pendentes —, que é
-- justamente o que um smoke test tende a não montar.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;

DROP TABLE IF EXISTS t_pending CASCADE;
CREATE TABLE t_pending (k bigint, v int, s text) USING theodb_columnar;

-- Transação 1: cria stripes de verdade (linhas 1..5000).
INSERT INTO t_pending SELECT g, g % 7, 'row_'||g FROM generate_series(1, 5000) g;

DROP TABLE IF EXISTS pend_res;
CREATE TEMP TABLE pend_res (q text, n bigint);

\echo '### M168-P1: estado MISTO — stripes gravados + pendentes na mesma transação'
BEGIN;
  -- Estas 3 linhas ficam em WRITE_STATES; nenhum stripe as contém ainda. As chaves são menores que qualquer
  -- linha já gravada, então um top-k ASC correto TEM de devolvê-las.
  INSERT INTO t_pending VALUES (-3, 1, 'pending_a'), (-2, 2, 'pending_b'), (-1, 3, 'pending_c');

  -- Com o streaming ligado (default).
  SET theodb.enable_columnar_topk_stream = on;
  CREATE TEMP TABLE p_on AS SELECT k, s FROM t_pending ORDER BY k LIMIT 5;

  -- Com o streaming desligado — o caminho eager, que sempre tratou pendentes corretamente. É o oráculo.
  SET theodb.enable_columnar_topk_stream = off;
  CREATE TEMP TABLE p_off AS SELECT k, s FROM t_pending ORDER BY k LIMIT 5;

  INSERT INTO pend_res SELECT 'p1_stream_vs_eager_mism', count(*) FROM (
    (SELECT * FROM p_on EXCEPT ALL SELECT * FROM p_off)
    UNION ALL (SELECT * FROM p_off EXCEPT ALL SELECT * FROM p_on)) d;

  -- Não-vacuidade: o top-k TEM de conter as pendentes, senão o bloco compara dois resultados igualmente errados.
  INSERT INTO pend_res SELECT 'p1_pending_rows_seen', count(*) FROM p_on WHERE k < 0;
COMMIT;

\echo '### M168-P2: estado PURAMENTE pendente (tabela nova, sem stripe) — já funcionava, fica travado'
DROP TABLE IF EXISTS t_pure CASCADE;
CREATE TABLE t_pure (k bigint, s text) USING theodb_columnar;
BEGIN;
  INSERT INTO t_pure VALUES (1, 'a'), (2, 'b'), (3, 'c');
  SET theodb.enable_columnar_topk_stream = on;
  CREATE TEMP TABLE u_on AS SELECT k, s FROM t_pure ORDER BY k LIMIT 3;
  SET theodb.enable_columnar_topk_stream = off;
  CREATE TEMP TABLE u_off AS SELECT k, s FROM t_pure ORDER BY k LIMIT 3;
  INSERT INTO pend_res SELECT 'p2_stream_vs_eager_mism', count(*) FROM (
    (SELECT * FROM u_on EXCEPT ALL SELECT * FROM u_off)
    UNION ALL (SELECT * FROM u_off EXCEPT ALL SELECT * FROM u_on)) d;
  INSERT INTO pend_res SELECT 'p2_rows_seen', count(*) FROM u_on;
COMMIT;

\echo '### M168-P3: CONTROLE POSITIVO — a mesma maquinaria tem de acusar uma divergência semeada'
DROP TABLE IF EXISTS p_bad;
CREATE TEMP TABLE p_bad AS SELECT k + 1000 AS k, s FROM p_off;
INSERT INTO pend_res SELECT 'p3_control_diff', count(*) FROM (
  (SELECT * FROM p_on EXCEPT ALL SELECT * FROM p_bad)
  UNION ALL (SELECT * FROM p_bad EXCEPT ALL SELECT * FROM p_on)) d;

\echo '### GATE FINAL — verificado por máquina'
SELECT * FROM pend_res ORDER BY q;
-- CONTROLE POSITIVO do próprio gate: com `-v gate_selftest=1` ele DEVE abortar.
\if :{?gate_selftest}
  \echo '### GATE SELF-TEST ARMADO: forçando p1_stream_vs_eager_mism = 1 — o gate abaixo DEVE abortar'
  UPDATE pend_res SET n = 1 WHERE q = 'p1_stream_vs_eager_mism';
\endif
DO $gate$
DECLARE bad text := '';
BEGIN
  bad := bad || coalesce(
    (SELECT 'divergências stream-vs-eager: ' || string_agg(format('%s=%s', q, n), ', ') || '; '
       FROM pend_res WHERE q LIKE '%\_mism' AND n <> 0), '');
  IF coalesce((SELECT n FROM pend_res WHERE q = 'p1_pending_rows_seen'), 0) <> 3 THEN
    bad := bad || format('p1_pending_rows_seen=%s, esperado 3 — o top-k NÃO devolveu as linhas pendentes, então '
                      || 'os zeros acima comparam dois resultados igualmente errados; ',
                      (SELECT n FROM pend_res WHERE q = 'p1_pending_rows_seen'));
  END IF;
  IF coalesce((SELECT n FROM pend_res WHERE q = 'p2_rows_seen'), 0) <> 3 THEN
    bad := bad || 'p2_rows_seen <> 3: a tabela puramente pendente não devolveu suas linhas; ';
  END IF;
  IF coalesce((SELECT n FROM pend_res WHERE q = 'p3_control_diff'), 0) = 0 THEN
    bad := bad || 'p3_control_diff = 0: a comparação não consegue falhar, então nada acima significa algo; ';
  END IF;
  IF (SELECT count(*) FROM pend_res) <> 5 THEN
    bad := bad || format('esperadas 5 asserções, encontradas %s; ', (SELECT count(*) FROM pend_res));
  END IF;
  IF bad <> '' THEN RAISE EXCEPTION 'M168 PENDING GATE FAILED: %', bad; END IF;
  RAISE NOTICE 'M168 PENDING GATE ok: streaming e eager concordam com pendentes na mesma transação (misto e puro), '
               'as 3 linhas pendentes aparecem no top-k, e o controle negativo dispara';
END
$gate$;
