-- M158 late-materialization PERF artifact (reproducible). Measures wall-clock with `\timing` on the BARE query
-- (NOT EXPLAIN ANALYZE — per council-benchmark H2, ANALYZE's per-row TIMING taxes the OFF path's 2M tuples far more
-- than ON's 10, inflating the OFF baseline). 1 warm-up + 5 measured runs per path; take the median of the 5.
-- Deterministic generator (generate_series, no random()) → no ADR-0012 degeneracy, reproducible without a seed.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET client_min_messages = warning;
DROP TABLE IF EXISTS big CASCADE;
-- 2,000,000 rows × 30 columns (wide SELECT * — the M148 form_row-heavy regime).
-- NOTE (council-benchmark M2): the g%N columns are LOW cardinality (highly compressible). This biases the late-mat win
-- UP (cheap Arrow decode) and the memory cost DOWN (small batch) vs real high-cardinality data (URLs/user-agents),
-- where the 1.6× would shrink and the Arrow batch would balloon. Treat the number as an upper bound on synthetic data.
CREATE TABLE big (
  wid bigint, c1 int, c2 int, c3 int, c4 int, c5 int, c6 int, c7 int, c8 int,
  s1 text, s2 text, s3 text, s4 text, s5 text,
  f1 float8, f2 float8, f3 float8, f4 float8,
  c9 int, c10 int, c11 int, c12 int, c13 int, c14 int,
  s6 text, s7 text, s8 text, f5 float8, f6 float8, v int
) USING theodb_columnar;
INSERT INTO big
SELECT g, g%1000, g%7, g%13, g%97, g%3, g%50, g%17, g%9,
       'aaa'||(g%11), 'bbb'||(g%23), 'ccc'||(g%5), 'ddd'||(g%31), 'eee'||(g%3),
       g*1.1, g*2.2, g*3.3, g*0.5,
       g%19, g%29, g%37, g%41, g%43, g%47,
       'fff'||(g%7), 'ggg'||(g%13), 'hhh'||(g%2), g*0.25, g*0.75, g%5
FROM generate_series(1, 2000000) g;
ANALYZE big;

-- Measure the BARE `SELECT *` (no count(*) wrapper — that would let PG prune the wide projection the outer agg does
-- not reference, changing what we measure; no EXPLAIN ANALYZE — its per-row TIMING taxes the 2M-tuple OFF path, H2).
-- The 10-row output render is tiny and SYMMETRIC across off/on, so it cancels in the ratio.
\timing on
\echo '===== BASELINE (late_mat OFF): 1 warm-up + 5 measured ====='
SET theodb.enable_columnar_late_mat = off;
SELECT * FROM big ORDER BY wid LIMIT 10;  -- warm-up (discard)
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 1
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 2
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 3
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 4
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 5

\echo '===== LATE-MAT (ON): 1 warm-up + 5 measured ====='
SET theodb.enable_columnar_late_mat = on;
SELECT * FROM big ORDER BY wid LIMIT 10;  -- warm-up (discard)
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 1
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 2
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 3
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 4
SELECT * FROM big ORDER BY wid LIMIT 10;  -- run 5
\timing off
