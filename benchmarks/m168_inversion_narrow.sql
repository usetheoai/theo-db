-- M168 § 3.5 — driver da INVERSÃO (o streaming serve o que o caminho eager não serve).
--
-- Estava em /tmp e não commitado, o que tornava a conclusão mais forte da § 3.5 irreproduzível a partir do
-- repositório enquanto todo o resto tinha harness (achado de review). `ON_ERROR_STOP off` é deliberado: o ponto
-- é registrar QUAL braço estoura — um erro aqui é o dado.
--
-- REQUER `THEODB_ADMIT_TRACE=1` no ambiente do postmaster para as linhas `theodb_stream_pool` (renomeada no M169; nos artefatos do M168 ela aparece como `theodb_topk_pool`).
SET work_mem='32MB'; SET theodb.enable_columnar_agg=on; SET theodb.enable_columnar_late_mat=on;
\echo === stream=ON ===
SET theodb.enable_columnar_topk_stream=on;
DROP TABLE IF EXISTS t1;
CREATE TEMP TABLE t1 AS SELECT EventTime, SearchPhrase FROM hits ORDER BY EventTime LIMIT 400000;
SELECT count(*) AS stream_on_rows FROM t1;
\echo === stream=OFF ===
SET theodb.enable_columnar_topk_stream=off;
DROP TABLE IF EXISTS t2;
CREATE TEMP TABLE t2 AS SELECT EventTime, SearchPhrase FROM hits ORDER BY EventTime LIMIT 400000;
SELECT count(*) AS stream_off_rows FROM t2;
