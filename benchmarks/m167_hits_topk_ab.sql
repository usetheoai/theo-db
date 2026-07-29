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
\echo '### M167-H0: routing precondition — EVERY shape run below MUST show the top-k node'
-- Machine-checked, not printed-and-hoped: an EXPLAIN a human has to read is not a gate. This RAISES, and with
-- ON_ERROR_STOP the whole oracle aborts, so a vacuous pass is impossible rather than merely visible.
DO $h0$
DECLARE
  shapes text[] := ARRAY[
    'SELECT * FROM hits WHERE URL LIKE ''%google%'' ORDER BY EventTime LIMIT 10',
    'SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '''' ORDER BY EventTime LIMIT 10',
    'SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '''' ORDER BY SearchPhrase LIMIT 10',
    'SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '''' ORDER BY EventTime, SearchPhrase LIMIT 10',
    -- H5/H5b sort by a THREE-key shape none of the four above exercises. They are the full-row blocks — the ones
    -- that catch key<->payload misalignment, and the only comparison over all 105 columns. Omitting their shape
    -- here left exactly those two free to pass vacuously if the 3-key swap declined.
    'SELECT CounterID, WatchID, UserID, SearchPhrase FROM hits WHERE SearchPhrase <> '''' ORDER BY CounterID, WatchID, UserID LIMIT 20',
    'SELECT * FROM hits WHERE URL LIKE ''%google%'' ORDER BY CounterID, WatchID, UserID LIMIT 10',
    -- H1/H2/H4 project different columns than the four suite shapes above (EventTime vs SearchPhrase vs both).
    -- Routing is only *implied* for them — the decode guard bills the whole relation regardless of projection
    -- (`columnar_agg.rs` `relation_physical_bytes`) — and "implied" is exactly what this gate exists to replace.
    'SELECT EventTime FROM hits WHERE URL LIKE ''%google%'' ORDER BY EventTime LIMIT 10',
    'SELECT EventTime FROM hits WHERE SearchPhrase <> '''' ORDER BY EventTime LIMIT 10',
    'SELECT EventTime, SearchPhrase FROM hits WHERE SearchPhrase <> '''' ORDER BY EventTime, SearchPhrase LIMIT 10'
  ];
  shape text;
  plan  text;
  i     int := 0;
BEGIN
  FOREACH shape IN ARRAY shapes LOOP
    i := i + 1;
    EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) ' || shape INTO plan;
    IF position('theodb_columnar_agg' IN plan) = 0 THEN
      RAISE EXCEPTION
        'M167-H0 FAILED: shape % did not route to the top-k node; every block below would pass vacuously. Plan: %',
        i, plan;
    END IF;
    -- A surviving Sort means the swap did not replace the native path, only sat beside it.
    IF position('"Node Type": "Sort"' IN plan) > 0 THEN
      RAISE EXCEPTION 'M167-H0 FAILED: shape % still carries a Sort node — the swap did not take. Plan: %', i, plan;
    END IF;
  END LOOP;
  RAISE NOTICE 'M167-H0 ok: all % shapes route to theodb_columnar_agg with no surviving Sort', i;
END
$h0$;

-- Results land in a table so the FINAL GATE can assert on them. Until this revision every block below was a bare
-- SELECT that printed a number for a human to read — `rc=0` certified that the queries ran, never that the zeros
-- were zero. That is precisely the "printed and hoped" defect this file's own H0 comment condemns.
DROP TABLE IF EXISTS h_res;
CREATE TEMP TABLE h_res (q text PRIMARY KEY, n bigint);

\echo '### M167-H1 (q23): SELECT * WHERE URL LIKE ... ORDER BY EventTime LIMIT 10 — sort-key multiset'
DROP TABLE IF EXISTS h1_col; DROP TABLE IF EXISTS h1_heap;
CREATE TEMP TABLE h1_col  AS SELECT EventTime FROM hits      WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
CREATE TEMP TABLE h1_heap AS SELECT EventTime FROM hits_heap WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
INSERT INTO h_res SELECT 'h1_key_mism', count(*) FROM (
  (SELECT * FROM h1_col EXCEPT ALL SELECT * FROM h1_heap)
  UNION ALL (SELECT * FROM h1_heap EXCEPT ALL SELECT * FROM h1_col)) d;

\echo '### M167-H2 (q24): narrow projection, ORDER BY EventTime LIMIT 10 — sort-key multiset'
DROP TABLE IF EXISTS h2_col; DROP TABLE IF EXISTS h2_heap;
CREATE TEMP TABLE h2_col  AS SELECT EventTime FROM hits      WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
CREATE TEMP TABLE h2_heap AS SELECT EventTime FROM hits_heap WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
INSERT INTO h_res SELECT 'h2_key_mism', count(*) FROM (
  (SELECT * FROM h2_col EXCEPT ALL SELECT * FROM h2_heap)
  UNION ALL (SELECT * FROM h2_heap EXCEPT ALL SELECT * FROM h2_col)) d;

\echo '### M167-H3 (q25): text sort key, ORDER BY SearchPhrase LIMIT 10 — sort-key multiset'
DROP TABLE IF EXISTS h3_col; DROP TABLE IF EXISTS h3_heap;
CREATE TEMP TABLE h3_col  AS SELECT SearchPhrase FROM hits      WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
CREATE TEMP TABLE h3_heap AS SELECT SearchPhrase FROM hits_heap WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
INSERT INTO h_res SELECT 'h3_key_mism', count(*) FROM (
  (SELECT * FROM h3_col EXCEPT ALL SELECT * FROM h3_heap)
  UNION ALL (SELECT * FROM h3_heap EXCEPT ALL SELECT * FROM h3_col)) d;

\echo '### M167-H4 (q26): multi-key, ORDER BY EventTime, SearchPhrase LIMIT 10 — sort-key multiset'
DROP TABLE IF EXISTS h4_col; DROP TABLE IF EXISTS h4_heap;
CREATE TEMP TABLE h4_col  AS SELECT EventTime, SearchPhrase FROM hits      WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
CREATE TEMP TABLE h4_heap AS SELECT EventTime, SearchPhrase FROM hits_heap WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
INSERT INTO h_res SELECT 'h4_key_mism', count(*) FROM (
  (SELECT * FROM h4_col EXCEPT ALL SELECT * FROM h4_heap)
  UNION ALL (SELECT * FROM h4_heap EXCEPT ALL SELECT * FROM h4_col)) d;

\echo '### M167-H5: TIE-BROKEN total order -> FULL ROWS compared (catches key<->payload misalignment)'
DROP TABLE IF EXISTS h5_col; DROP TABLE IF EXISTS h5_heap;
CREATE TEMP TABLE h5_col  AS SELECT CounterID, WatchID, UserID, SearchPhrase FROM hits
  WHERE SearchPhrase <> '' ORDER BY CounterID, WatchID, UserID LIMIT 20;
CREATE TEMP TABLE h5_heap AS SELECT CounterID, WatchID, UserID, SearchPhrase FROM hits_heap
  WHERE SearchPhrase <> '' ORDER BY CounterID, WatchID, UserID LIMIT 20;
INSERT INTO h_res SELECT 'h5_fullrow_mism', count(*) FROM (
  (SELECT * FROM h5_col EXCEPT ALL SELECT * FROM h5_heap)
  UNION ALL (SELECT * FROM h5_heap EXCEPT ALL SELECT * FROM h5_col)) d;
-- non-vacuity: the FIRST key must tie, or the tie-break was never exercised and this block proves less than it claims
INSERT INTO h_res SELECT 'h5_distinct_firstkey', count(DISTINCT CounterID) FROM h5_col;
INSERT INTO h_res SELECT 'h5_rows', count(*) FROM h5_col;

\echo '### M167-H5b: WIDE projection (all 105 columns), tie-broken -> FULL ROWS compared'
-- H1 compares only the sort key for q23; the 105-column payload it actually projects was compared by nothing.
DROP TABLE IF EXISTS h5b_col; DROP TABLE IF EXISTS h5b_heap;
CREATE TEMP TABLE h5b_col  AS SELECT * FROM hits      WHERE URL LIKE '%google%' ORDER BY CounterID, WatchID, UserID LIMIT 10;
CREATE TEMP TABLE h5b_heap AS SELECT * FROM hits_heap WHERE URL LIKE '%google%' ORDER BY CounterID, WatchID, UserID LIMIT 10;
INSERT INTO h_res SELECT 'h5b_wide_fullrow_mism', count(*) FROM (
  (SELECT * FROM h5b_col EXCEPT ALL SELECT * FROM h5b_heap)
  UNION ALL (SELECT * FROM h5b_heap EXCEPT ALL SELECT * FROM h5b_col)) d;
INSERT INTO h_res SELECT 'h5b_cols', count(*) FROM information_schema.columns WHERE table_name = 'h5b_col';

\echo '### M167-H6: NEGATIVE CONTROL — the same comparison must FAIL on seeded data'
DROP TABLE IF EXISTS h6_bad;
CREATE TEMP TABLE h6_bad AS SELECT CounterID, WatchID + 1 AS WatchID, UserID, SearchPhrase FROM h5_heap;
INSERT INTO h_res SELECT 'h6_control_diff', count(*) FROM (
  (SELECT * FROM h5_col EXCEPT ALL SELECT * FROM h6_bad)
  UNION ALL (SELECT * FROM h6_bad EXCEPT ALL SELECT * FROM h5_col)) d;

-- POSITIVE CONTROL for the gate itself. Run with `-v gate_selftest=1` and the gate below MUST abort: that is the
-- only proof it can fail. Seeding here (rather than duplicating the gate in a second file) keeps one copy of the
-- logic, so the control can never drift away from the thing it controls.
\if :{?gate_selftest}
  \echo '### GATE SELF-TEST ARMED: forcing h1_key_mism = 1 — the FINAL GATE MUST abort below'
  UPDATE h_res SET n = 1 WHERE q = 'h1_key_mism';
\endif

\echo '### M167 FINAL GATE — machine-checked, not printed'
SELECT * FROM h_res ORDER BY q;
DO $gate$
DECLARE
  bad text := '';
  v   bigint;
BEGIN
  -- every mismatch counter must be exactly 0
  bad := bad || coalesce(
    (SELECT 'non-zero mismatch counters: ' || string_agg(format('%s=%s', q, n), ', ') || '; '
       FROM h_res WHERE q LIKE '%\_mism' AND n <> 0), '');
  -- the negative control must be able to fail, or every zero above is meaningless
  SELECT n INTO v FROM h_res WHERE q = 'h6_control_diff';
  IF v IS NULL OR v = 0 THEN bad := bad || 'h6_control_diff must be > 0 (the oracle cannot fail); '; END IF;
  -- the tie-break must actually have been exercised
  IF (SELECT n FROM h_res WHERE q = 'h5_distinct_firstkey') >= (SELECT n FROM h_res WHERE q = 'h5_rows') THEN
    bad := bad || 'h5_distinct_firstkey >= h5_rows: the first key did not tie, so the tie-break proved nothing; ';
  END IF;
  -- the wide block must really have compared the wide row
  IF (SELECT n FROM h_res WHERE q = 'h5b_cols') < 100 THEN
    bad := bad || 'h5b_cols < 100: the wide comparison was not wide; ';
  END IF;
  -- every expected assertion must be present (a block silently skipped is not a pass)
  IF (SELECT count(*) FROM h_res) <> 10 THEN
    bad := bad || format('expected 10 assertions, found %s; ', (SELECT count(*) FROM h_res));
  END IF;

  IF bad <> '' THEN RAISE EXCEPTION 'M167 FINAL GATE FAILED: %', bad; END IF;
  RAISE NOTICE 'M167 FINAL GATE ok: % assertions, all mismatch counters 0, negative control fires, tie-break exercised, wide row wide',
    (SELECT count(*) FROM h_res);
END
$gate$;
