-- M167 — top-k A/B at the MEASURED scale: columnar `hits` (1M, 105 cols) vs the heap twin `hits_heap`.
--
-- WHY THIS EXISTS (review finding F6): `m158_ec_harness.sql` proves top-k correctness on `t_col` — 20k rows, 6
-- columns, a fixture. The suite A/B in `run_m128_clickbench.py` strips the trailing LIMIT (`:283`) so it never
-- exercises the top-k path at all. Between them, the 1M-row / 105-column output that produced every published
-- number was verified by NEITHER. This closes that gap on the actual relation.
--
-- TIE HANDLING: q23-q26 order by columns with duplicates, so "which rows" among equal keys is unspecified and a
-- full-row compare would false-positive. Two complementary assertions per query:
--   (a) the SORT-KEY MULTISET is deterministic under ORDER BY + LIMIT -> symmetric EXCEPT ALL must be empty;
--   (b) a TIE-BROKEN variant is a total order -> FULL ROWS compared, which is what catches a key<->payload
--       misalignment (the signature defect of late materialization). The first key must ACTUALLY tie or the
--       tie-break is never exercised: measured, `CounterID` has exactly 1 distinct value in the top-20, so the
--       second and third keys decide every row. (An earlier draft used EventTime, which turned out unique in its
--       top-20 — the block passed while proving nothing about tie-breaking.)
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;   -- the path under test, on the columnar arm

-- ROUTING ASSERTION (review finding: without it every block below passes vacuously when the swap declines —
-- e.g. at stock `work_mem` the ADR-4 decode guard refuses and both arms would run the native plan, giving 0
-- differences that prove nothing). `theodb_columnar_agg` in the plan of a query with NO aggregate can only come
-- from the top-k swap, so its presence is a positive proof that the path under test actually ran.
\echo '### M167-H0: routing precondition — the four shapes MUST show the top-k node'
EXPLAIN (COSTS OFF) SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
EXPLAIN (COSTS OFF) SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
EXPLAIN (COSTS OFF) SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
EXPLAIN (COSTS OFF) SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;

\echo '### M167-H1 (q23): SELECT * WHERE URL LIKE ... ORDER BY EventTime LIMIT 10 — sort-key multiset'
DROP TABLE IF EXISTS h1_col; DROP TABLE IF EXISTS h1_heap;
CREATE TEMP TABLE h1_col  AS SELECT EventTime FROM hits      WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
CREATE TEMP TABLE h1_heap AS SELECT EventTime FROM hits_heap WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
SELECT 'h1_key_mism' q, count(*) n FROM (
  (SELECT * FROM h1_col EXCEPT ALL SELECT * FROM h1_heap)
  UNION ALL (SELECT * FROM h1_heap EXCEPT ALL SELECT * FROM h1_col)) d;

\echo '### M167-H2 (q24): narrow projection, ORDER BY EventTime LIMIT 10 — sort-key multiset'
DROP TABLE IF EXISTS h2_col; DROP TABLE IF EXISTS h2_heap;
CREATE TEMP TABLE h2_col  AS SELECT EventTime FROM hits      WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
CREATE TEMP TABLE h2_heap AS SELECT EventTime FROM hits_heap WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
SELECT 'h2_key_mism' q, count(*) n FROM (
  (SELECT * FROM h2_col EXCEPT ALL SELECT * FROM h2_heap)
  UNION ALL (SELECT * FROM h2_heap EXCEPT ALL SELECT * FROM h2_col)) d;

\echo '### M167-H3 (q25): text sort key, ORDER BY SearchPhrase LIMIT 10 — sort-key multiset'
DROP TABLE IF EXISTS h3_col; DROP TABLE IF EXISTS h3_heap;
CREATE TEMP TABLE h3_col  AS SELECT SearchPhrase FROM hits      WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
CREATE TEMP TABLE h3_heap AS SELECT SearchPhrase FROM hits_heap WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
SELECT 'h3_key_mism' q, count(*) n FROM (
  (SELECT * FROM h3_col EXCEPT ALL SELECT * FROM h3_heap)
  UNION ALL (SELECT * FROM h3_heap EXCEPT ALL SELECT * FROM h3_col)) d;

\echo '### M167-H4 (q26): multi-key, ORDER BY EventTime, SearchPhrase LIMIT 10 — sort-key multiset'
DROP TABLE IF EXISTS h4_col; DROP TABLE IF EXISTS h4_heap;
CREATE TEMP TABLE h4_col  AS SELECT EventTime, SearchPhrase FROM hits      WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
CREATE TEMP TABLE h4_heap AS SELECT EventTime, SearchPhrase FROM hits_heap WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
SELECT 'h4_key_mism' q, count(*) n FROM (
  (SELECT * FROM h4_col EXCEPT ALL SELECT * FROM h4_heap)
  UNION ALL (SELECT * FROM h4_heap EXCEPT ALL SELECT * FROM h4_col)) d;

\echo '### M167-H5: TIE-BROKEN total order -> FULL ROWS compared (catches key<->payload misalignment)'
DROP TABLE IF EXISTS h5_col; DROP TABLE IF EXISTS h5_heap;
CREATE TEMP TABLE h5_col  AS SELECT CounterID, WatchID, UserID, SearchPhrase FROM hits
  WHERE SearchPhrase <> '' ORDER BY CounterID, WatchID, UserID LIMIT 20;
CREATE TEMP TABLE h5_heap AS SELECT CounterID, WatchID, UserID, SearchPhrase FROM hits_heap
  WHERE SearchPhrase <> '' ORDER BY CounterID, WatchID, UserID LIMIT 20;
SELECT 'h5_fullrow_mism' q, count(*) n FROM (
  (SELECT * FROM h5_col EXCEPT ALL SELECT * FROM h5_heap)
  UNION ALL (SELECT * FROM h5_heap EXCEPT ALL SELECT * FROM h5_col)) d;
-- non-vacuity: the FIRST key must tie, or the tie-break was never exercised and this block proves less than it claims
SELECT 'h5_distinct_firstkey' q, count(DISTINCT CounterID) n FROM h5_col;
SELECT 'h5_rows' q, count(*) n FROM h5_col;

\echo '### M167-H5b: WIDE projection (all 105 columns), tie-broken -> FULL ROWS compared'
-- H1 compares only the sort key for q23; the 105-column payload it actually projects was compared by nothing.
DROP TABLE IF EXISTS h5b_col; DROP TABLE IF EXISTS h5b_heap;
CREATE TEMP TABLE h5b_col  AS SELECT * FROM hits      WHERE URL LIKE '%google%' ORDER BY CounterID, WatchID, UserID LIMIT 10;
CREATE TEMP TABLE h5b_heap AS SELECT * FROM hits_heap WHERE URL LIKE '%google%' ORDER BY CounterID, WatchID, UserID LIMIT 10;
SELECT 'h5b_wide_fullrow_mism' q, count(*) n FROM (
  (SELECT * FROM h5b_col EXCEPT ALL SELECT * FROM h5b_heap)
  UNION ALL (SELECT * FROM h5b_heap EXCEPT ALL SELECT * FROM h5b_col)) d;
SELECT 'h5b_cols' q, count(*) n FROM information_schema.columns WHERE table_name = 'h5b_col';

\echo '### M167-H6: NEGATIVE CONTROL — the same comparison must FAIL on seeded data'
DROP TABLE IF EXISTS h6_bad;
CREATE TEMP TABLE h6_bad AS SELECT CounterID, WatchID + 1 AS WatchID, UserID, SearchPhrase FROM h5_heap;
SELECT 'h6_control_diff' q, count(*) n FROM (
  (SELECT * FROM h5_col EXCEPT ALL SELECT * FROM h6_bad)
  UNION ALL (SELECT * FROM h6_bad EXCEPT ALL SELECT * FROM h5_col)) d;

\echo '========== all *_mism MUST be 0; h6_control_diff MUST be > 0; h5_distinct_firstkey MUST be < h5_rows (else the tie-break was not exercised) =========='
