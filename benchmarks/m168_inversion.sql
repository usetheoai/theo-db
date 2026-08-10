-- M168 § 3.5 — driver da INVERSÃO (o streaming serve o que o caminho eager não serve).
--
-- Estava em /tmp e não commitado, o que tornava a conclusão mais forte da § 3.5 irreproduzível a partir do
-- repositório enquanto todo o resto tinha harness (achado de review). `ON_ERROR_STOP off` é deliberado: o ponto
-- é registrar QUAL braço estoura — um erro aqui é o dado.
--
-- REQUER `THEODB_ADMIT_TRACE=1` no ambiente do postmaster para as linhas `theodb_stream_pool` (renomeada no M169; nos artefatos do M168 ela aparece como `theodb_topk_pool`).
\set ON_ERROR_STOP off
SET max_parallel_workers_per_gather=0;
SET theodb.enable_columnar_agg=on; SET theodb.enable_columnar_late_mat=on;
SET work_mem='32MB';
\echo === BANDA PREVISTA: SELECT * (105 col) com k PEQUENO ===
\echo --- stream ---
SET theodb.enable_columnar_topk_stream=on;
DROP TABLE IF EXISTS b1; CREATE TEMP TABLE b1 AS SELECT * FROM hits ORDER BY EventTime, CounterID LIMIT 1000;
SELECT count(*) AS stream_rows FROM b1;
\echo --- eager ---
SET theodb.enable_columnar_topk_stream=off;
DROP TABLE IF EXISTS b2; CREATE TEMP TABLE b2 AS SELECT * FROM hits ORDER BY EventTime, CounterID LIMIT 1000;
SELECT count(*) AS eager_rows FROM b2;
