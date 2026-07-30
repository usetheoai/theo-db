-- M169 T2.1 — RED: o caminho AGREGADO usa o `ColumnarChunkStream`, e concorda com o eager.
--
-- POR QUE ESTE ARQUIVO EXISTE, e por que ele não pode ser só uma comparação de resultados.
--
-- Hoje os dois braços (`enable_columnar_agg_stream` on e off) rodam o MESMO código eager, porque a GUC ainda não
-- existe e o call-site ainda chama `decode_to_batch`. Um teste que apenas comparasse `count/sum/avg/min/max`
-- entre os braços passaria — comparando o eager consigo mesmo. Verde vacuoso perfeito: ele ficaria verde antes do
-- fix, depois do fix, e continuaria verde se o fix fosse revertido.
--
-- O que torna este RED honesto é o CONTADOR: `theodb_columnar_stream_chunk_groups()` é zerado em
-- `ColumnarChunkStream::new` e incrementado a cada `next()`. Se o agregado não passou pelo stream, ele fica em 0.
-- Portanto o teste falha HOJE por não-vacuidade, não por divergência — e é essa falha que prova que ele mede a
-- mudança em vez de acompanhá-la.
--
-- A tabela é construída com >= 2 chunk-groups DE PROPÓSITO (CHUNK_GROUP_ROWS = 10.000): com um único
-- chunk-group, streaming e eager são indistinguíveis por construção, e o teste voltaria a ser vacuoso.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;
SET work_mem = '64MB';

DROP TABLE IF EXISTS t_aggstream CASCADE;
CREATE TABLE t_aggstream (k bigint, v int, f float8, s text) USING theodb_columnar;

-- 25.000 linhas => 3 chunk-groups (10.000 + 10.000 + 5.000). O terceiro é PARCIAL de propósito: um cursor que
-- assume chunk-groups cheios erra exatamente na última fronteira, e é a fronteira que o eager nunca exercita.
INSERT INTO t_aggstream
SELECT g, g % 97, (g % 1000)::float8 / 7.0, 'row_' || g
FROM generate_series(1, 25000) g;

DROP TABLE IF EXISTS aggstream_res;
CREATE TEMP TABLE aggstream_res (arm text, metric text, val numeric);

\echo '### M169-A1: braço EAGER (GUC off) — a referência'
SET theodb.enable_columnar_agg_stream = off;
INSERT INTO aggstream_res
SELECT 'eager', m, v FROM (
  SELECT 'count' AS m, count(*)::numeric AS v FROM t_aggstream
  UNION ALL SELECT 'sum_v',  sum(v)::numeric        FROM t_aggstream
  UNION ALL SELECT 'avg_f',  round(avg(f)::numeric, 9) FROM t_aggstream
  UNION ALL SELECT 'min_k',  min(k)::numeric        FROM t_aggstream
  UNION ALL SELECT 'max_k',  max(k)::numeric        FROM t_aggstream
) q;

\echo '### M169-A2: braço STREAMING (GUC on)'
SET theodb.enable_columnar_agg_stream = on;
INSERT INTO aggstream_res
SELECT 'stream', m, v FROM (
  SELECT 'count' AS m, count(*)::numeric AS v FROM t_aggstream
  UNION ALL SELECT 'sum_v',  sum(v)::numeric        FROM t_aggstream
  UNION ALL SELECT 'avg_f',  round(avg(f)::numeric, 9) FROM t_aggstream
  UNION ALL SELECT 'min_k',  min(k)::numeric        FROM t_aggstream
  UNION ALL SELECT 'max_k',  max(k)::numeric        FROM t_aggstream
) q;

\echo '### M169-A3: GATE DE NÃO-VACUIDADE — o braço streaming REALMENTE usou o stream?'
-- Roda uma agregação isolada com a GUC on e lê o contador imediatamente. Sem isto, A4 compararia o eager
-- consigo mesmo e passaria. Este é o assert que falha HOJE.
SELECT count(*) FROM t_aggstream;
SELECT
  theodb_columnar_stream_chunk_groups() AS stream_calls,
  CASE WHEN theodb_columnar_stream_chunk_groups() > 0
       THEN 'OK — o agregado passou pelo ColumnarChunkStream'
       ELSE 'FALHA — stream_calls=0: o braço "on" ainda roda o caminho EAGER; a comparação de A4 seria vacuosa'
  END AS nao_vacuidade \gset gate_
\echo :gate_nao_vacuidade
SELECT CASE WHEN theodb_columnar_stream_chunk_groups() > 0 THEN 1
            ELSE 1/0 END AS forca_falha_quando_vacuoso;

\echo '### M169-A4: symmetric-EXCEPT entre os braços — diverged tem de ser 0'
SELECT count(*) AS diverged FROM (
  (SELECT metric, val FROM aggstream_res WHERE arm = 'eager'
   EXCEPT ALL
   SELECT metric, val FROM aggstream_res WHERE arm = 'stream')
  UNION ALL
  (SELECT metric, val FROM aggstream_res WHERE arm = 'stream'
   EXCEPT ALL
   SELECT metric, val FROM aggstream_res WHERE arm = 'eager')
) d \gset ab_
\echo 'diverged =' :ab_diverged
SELECT CASE WHEN :ab_diverged = 0 THEN 1 ELSE 1/0 END AS forca_falha_quando_diverge;

\echo '### M169-A5: CONTROLE POSITIVO — o oráculo de A4 consegue ficar vermelho?'
-- Um symmetric-EXCEPT que nunca reprova não é oráculo. Injeta uma divergência deliberada e exige que ela seja
-- detectada; se este bloco reportar 0, o oráculo de A4 está quebrado e o verde dele não vale nada.
INSERT INTO aggstream_res VALUES ('stream', 'sentinela_divergente', -1);
SELECT count(*) AS ctrl FROM (
  (SELECT metric, val FROM aggstream_res WHERE arm = 'eager'
   EXCEPT ALL
   SELECT metric, val FROM aggstream_res WHERE arm = 'stream')
  UNION ALL
  (SELECT metric, val FROM aggstream_res WHERE arm = 'stream'
   EXCEPT ALL
   SELECT metric, val FROM aggstream_res WHERE arm = 'eager')
) d \gset pc_
\echo 'controle positivo (tem de ser > 0) =' :pc_ctrl
SELECT CASE WHEN :pc_ctrl > 0 THEN 1 ELSE 1/0 END AS forca_falha_se_oraculo_cego;

\echo '### M169: todos os gates passaram'
