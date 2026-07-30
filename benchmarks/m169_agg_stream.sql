-- M169 T2.1 — RED: o caminho AGREGADO usa o `ColumnarChunkStream`, e concorda com o eager.
--
-- POR QUE ESTE ARQUIVO EXISTE, e por que ele não pode ser só uma comparação de resultados.
--
-- Hoje os dois braços (`enable_columnar_agg_stream` on e off) rodam o MESMO código eager, porque a GUC ainda não
-- existe e o call-site ainda chama `decode_to_batch`. Um teste que apenas comparasse `count/sum/avg/min/max`
-- entre os braços passaria — comparando o eager consigo mesmo. Verde vacuoso perfeito: ficaria verde antes do
-- fix, depois do fix, e continuaria verde se o fix fosse revertido.
--
-- O que torna este RED honesto é o CONTADOR: `theodb_columnar_stream_chunk_groups()` é zerado em
-- `ColumnarChunkStream::new` e incrementado a cada `next()`. Se o agregado não passou pelo stream, ele fica em 0.
-- Portanto o teste falha HOJE por não-vacuidade, não por divergência — e é essa falha que prova que ele mede a
-- mudança em vez de acompanhá-la.
--
-- A tabela tem 3 chunk-groups DE PROPÓSITO (CHUNK_GROUP_ROWS = 10.000), o último PARCIAL: com um único
-- chunk-group streaming e eager são indistinguíveis por construção, e um cursor que assuma chunk-groups cheios
-- erra exatamente na última fronteira — que é a que o eager nunca exercita.
--
-- O gate usa `DO $$ ... RAISE EXCEPTION $$`, seguindo `m168_pending_rows.sql:80-104`. Um `CASE ... ELSE 1/0`
-- seria mais curto e ERRADO: `1/0` é expressão constante e o planejador a dobra no planejamento, disparando o
-- erro mesmo quando o ramo não é tomado — o gate falharia sempre, inclusive depois do fix.
--
-- Braço de auto-teste (o controle positivo):
--     psql -v gate_selftest=1 -f benchmarks/m169_agg_stream.sql
-- Ele corrompe o resultado de propósito e o gate TEM de abortar. Um oráculo que nunca reprova não é oráculo.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;
SET work_mem = '64MB';

DROP TABLE IF EXISTS t_aggstream CASCADE;
CREATE TABLE t_aggstream (k bigint, v int, f float8, s text) USING theodb_columnar;

INSERT INTO t_aggstream
SELECT g, g % 97, (g % 1000)::float8 / 7.0, 'row_' || g
FROM generate_series(1, 25000) g;

DROP TABLE IF EXISTS aggstream_res;
CREATE TEMP TABLE aggstream_res (arm text, metric text, val numeric);
DROP TABLE IF EXISTS aggstream_meta;
CREATE TEMP TABLE aggstream_meta (k text, v bigint);

\echo '### M169-A1: braço EAGER (GUC off) — a referência'
SET theodb.enable_columnar_agg_stream = off;
INSERT INTO aggstream_res
SELECT 'eager', m, val FROM (
  SELECT 'count' AS m, count(*)::numeric AS val FROM t_aggstream
  UNION ALL SELECT 'sum_v',  sum(v)::numeric           FROM t_aggstream
  UNION ALL SELECT 'avg_f',  round(avg(f)::numeric, 9) FROM t_aggstream
  UNION ALL SELECT 'min_k',  min(k)::numeric           FROM t_aggstream
  UNION ALL SELECT 'max_k',  max(k)::numeric           FROM t_aggstream
) q;

\echo '### M169-A2: braço STREAMING (GUC on)'
SET theodb.enable_columnar_agg_stream = on;
INSERT INTO aggstream_res
SELECT 'stream', m, val FROM (
  SELECT 'count' AS m, count(*)::numeric AS val FROM t_aggstream
  UNION ALL SELECT 'sum_v',  sum(v)::numeric           FROM t_aggstream
  UNION ALL SELECT 'avg_f',  round(avg(f)::numeric, 9) FROM t_aggstream
  UNION ALL SELECT 'min_k',  min(k)::numeric           FROM t_aggstream
  UNION ALL SELECT 'max_k',  max(k)::numeric           FROM t_aggstream
) q;

\echo '### M169-A3: o braço streaming REALMENTE passou pelo ColumnarChunkStream?'
-- Uma agregação isolada com a GUC on, e a leitura do contador IMEDIATAMENTE depois, na mesma sessão.
SELECT count(*) FROM t_aggstream;
INSERT INTO aggstream_meta VALUES ('stream_calls', theodb_columnar_stream_chunk_groups());

\if :{?gate_selftest}
  \echo '### AUTO-TESTE ARMADO: corrompendo o braço stream — o gate abaixo DEVE abortar'
  UPDATE aggstream_res SET val = val + 1 WHERE arm = 'stream' AND metric = 'sum_v';
\endif

DO $gate$
DECLARE
  calls   bigint;
  diverge bigint;
  bad     text := '';
BEGIN
  SELECT v INTO calls FROM aggstream_meta WHERE k = 'stream_calls';

  -- (1) NÃO-VACUIDADE. Este é o assert que falha HOJE, e é ele que torna (2) uma medição em vez de uma
  -- comparação do eager consigo mesmo.
  IF calls IS NULL OR calls = 0 THEN
    bad := bad || format(
      'stream_calls=%s: o braço "on" ainda roda o caminho EAGER, logo a comparação de (2) seria VACUOSA '
      '(eager vs eager). Este é o estado esperado ANTES do fix de T2.1. ', coalesce(calls::text, 'NULL'));
  END IF;

  -- (2) symmetric-EXCEPT entre os braços.
  SELECT count(*) INTO diverge FROM (
    (SELECT metric, val FROM aggstream_res WHERE arm = 'eager'
     EXCEPT ALL
     SELECT metric, val FROM aggstream_res WHERE arm = 'stream')
    UNION ALL
    (SELECT metric, val FROM aggstream_res WHERE arm = 'stream'
     EXCEPT ALL
     SELECT metric, val FROM aggstream_res WHERE arm = 'eager')
  ) d;
  IF diverge <> 0 THEN
    bad := bad || format('symmetric-EXCEPT diverged=%s entre eager e streaming. ', diverge);
  END IF;

  IF bad <> '' THEN
    RAISE EXCEPTION 'M169 AGG-STREAM GATE FAILED: %', bad;
  END IF;
  RAISE NOTICE 'M169 AGG-STREAM GATE ok: o agregado passou pelo stream (% chamadas) e concorda com o eager '
               '(diverged=0) sobre 3 chunk-groups, o último parcial', calls;
END
$gate$;
