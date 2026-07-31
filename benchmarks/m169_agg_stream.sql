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
-- `t` é uma chave de texto BARE de baixa cardinalidade (97 valores). Ela existe porque o alvo medido do M169
-- NÃO é só escalar: das três instâncias de `byte array offset overflow` a 100M, a q20 é escalar
-- (`COUNT(*) … WHERE URL LIKE`) e a q33/q34 são AGRUPADAS por uma coluna de texto. Um RED que só exercitasse
-- agregados escalares ficaria verde com dois terços do alvo ainda quebrados — e é exatamente essa a forma de
-- teste que passa e passaria também sem o fix, em versão parcial.
CREATE TABLE t_aggstream (k bigint, v int, f float8, s text, t text) USING theodb_columnar;

INSERT INTO t_aggstream
SELECT g, g % 97, (g % 1000)::float8 / 7.0, 'row_' || g, 'grp_' || (g % 97)
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

-- ---------------------------------------------------------------------------------------------------------------
-- A4/A5/A6 — o caminho AGRUPADO, onde vivem 2 das 3 instâncias medidas do overflow (q33/q34, `GROUP BY <texto>`).
-- O escalar e o agrupado são funções diferentes em `df_executor.rs`; consertar uma não conserta a outra, e só um
-- braço agrupado prova isso.
DROP TABLE IF EXISTS aggstream_grp;
CREATE TEMP TABLE aggstream_grp (arm text, gkey text, c bigint, sv numeric);

\echo '### M169-A4: braço EAGER AGRUPADO (GUC off) — a referência'
SET theodb.enable_columnar_agg_stream = off;
INSERT INTO aggstream_grp SELECT 'eager', t, count(*), sum(v)::numeric FROM t_aggstream GROUP BY t;

\echo '### M169-A5: braço STREAMING AGRUPADO (GUC on)'
SET theodb.enable_columnar_agg_stream = on;
INSERT INTO aggstream_grp SELECT 'stream', t, count(*), sum(v)::numeric FROM t_aggstream GROUP BY t;

\echo '### M169-A6: o braço AGRUPADO passou mesmo pelo stream?'
-- Agregação agrupada isolada + leitura do contador na sequência. `ColumnarChunkStream::new` zera o contador, então
-- este número é o desta consulta, não um acumulado.
-- Sem OFFSET/LIMIT: qualquer um dos dois entra no plano acima do agregado e poderia mudar a decisão de
-- roteamento — e aí o contador mediria outro plano. São 97 linhas; devolvê-las não custa nada.
SELECT t, count(*) FROM t_aggstream GROUP BY t;
INSERT INTO aggstream_meta VALUES ('grp_stream_calls', theodb_columnar_stream_chunk_groups());

\if :{?gate_selftest}
  \echo '### AUTO-TESTE ARMADO: corrompendo AMBOS os braços stream — o gate abaixo DEVE abortar'
  UPDATE aggstream_res SET val = val + 1 WHERE arm = 'stream' AND metric = 'sum_v';
  -- O controle positivo tem de cobrir o agrupado também: um oráculo que só reprova o escalar não prova nada
  -- sobre a metade agrupada do alvo.
  UPDATE aggstream_grp SET c = c + 1 WHERE arm = 'stream' AND gkey = 'grp_5';
\endif

DO $gate$
DECLARE
  calls     bigint;
  grp_calls bigint;
  diverge   bigint;
  grp_div   bigint;
  bad       text := '';
BEGIN
  SELECT v INTO calls     FROM aggstream_meta WHERE k = 'stream_calls';
  SELECT v INTO grp_calls FROM aggstream_meta WHERE k = 'grp_stream_calls';

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
    bad := bad || format('symmetric-EXCEPT ESCALAR diverged=%s entre eager e streaming. ', diverge);
  END IF;

  -- (3) NÃO-VACUIDADE DO AGRUPADO. Separada da (1) de propósito: as duas chamam funções diferentes
  -- (`run_columnar_aggs` vs `run_columnar_grouped_aggs`), e um fix que ligasse só uma deixaria esta em zero.
  IF grp_calls IS NULL OR grp_calls = 0 THEN
    bad := bad || format(
      'grp_stream_calls=%s: o AGRUPADO ainda roda o caminho EAGER — é onde estão q33/q34, 2 das 3 instâncias '
      'medidas do overflow. ', coalesce(grp_calls::text, 'NULL'));
  END IF;

  -- (4) symmetric-EXCEPT sobre a SAÍDA AGRUPADA INTEIRA (chave + agregados), não só sobre os agregados. Comparar
  -- apenas totais é cego a uma chave de GROUP BY errada: trocar duas chaves entre si preserva todo agregado
  -- global e muda cada linha.
  SELECT count(*) INTO grp_div FROM (
    (SELECT gkey, c, sv FROM aggstream_grp WHERE arm = 'eager'
     EXCEPT ALL
     SELECT gkey, c, sv FROM aggstream_grp WHERE arm = 'stream')
    UNION ALL
    (SELECT gkey, c, sv FROM aggstream_grp WHERE arm = 'stream'
     EXCEPT ALL
     SELECT gkey, c, sv FROM aggstream_grp WHERE arm = 'eager')
  ) d;
  IF grp_div <> 0 THEN
    bad := bad || format('symmetric-EXCEPT AGRUPADO diverged=%s entre eager e streaming. ', grp_div);
  END IF;

  IF bad <> '' THEN
    RAISE EXCEPTION 'M169 AGG-STREAM GATE FAILED: %', bad;
  END IF;
  -- A unidade é AVANÇO DE CURSOR, não "chunk-groups entregues" — a doc de `theodb_columnar_stream_chunk_groups`
  -- diz isso explicitamente: a sonda de schema é o grupo nº 0 (não um extra), a chamada TERMINAL soma 1, e um
  -- avanço pode consumir vários grupos podados pelo zone-map contando 1. Aqui: 25.000 linhas / 10.000 = 3 grupos
  -- (o último parcial) + 1 terminal = 4. Dizer "4 chunk-groups" seria reportar a unidade errada.
  RAISE NOTICE 'M169 AGG-STREAM GATE ok: escalar avançou o cursor % vezes e o AGRUPADO %, sobre 3 chunk-groups '
               '(o último parcial) + a chamada terminal; ambos concordam com o eager (diverged=0/0)',
               calls, grp_calls;
END
$gate$;
