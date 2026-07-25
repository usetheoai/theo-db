-- M158 late-materialization top-k A/B harness (LIMIT-preserving symmetric-EXCEPT — the CORRECT oracle).
-- Columnar table `t_col`. For each query we run it with `theodb.enable_columnar_late_mat = off` (native
-- Limit→Sort→theodb_columnar_project) and `= on` (the top-k CustomScan), and take the symmetric difference of the two
-- FULL result sets. n = 0 proves the top-k rows are byte-identical to the eager plan. The sort key `wid` is UNIQUE, so
-- the top-k boundary has no ties → the comparison is deterministic (M155 tie caveat neutralized by a unique key).
-- NOTE: the top-k node reuses the aggregate CustomScan methods, so EXPLAIN labels it `theodb_columnar_agg` — a
-- cosmetic quirk (the node is the top-k path when it sits directly under a Limit); correctness is proven by the A/B.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;

DROP TABLE IF EXISTS t_col CASCADE;
CREATE TABLE t_col (
    wid  bigint,        -- UNIQUE sort key (no boundary ties)
    cid  int,
    v    int,
    s    text,
    f    float8,
    ts   timestamp
) USING theodb_columnar;

-- 20000 rows; wid unique (= g), other columns varied. Wide enough that k=10 ≪ N.
INSERT INTO t_col
SELECT g,
       (g % 97),
       (g % 7),
       'row_' || (g % 13)::text || CASE WHEN g % 5 = 0 THEN '_foo' ELSE '_bar' END,
       (g * 1.5)::float8,
       (TIMESTAMP '2026-01-01 00:00:00' + (g % 500) * INTERVAL '1 hour')
FROM generate_series(1, 20000) g;

-- Helper: symmetric-EXCEPT of the same query under both GUC states. We inline per query below (psql has no fn).

\echo '========== M158 A/B — each query: (off EXCEPT on) UNION ALL (on EXCEPT off) must be 0 =========='

-- ---- Q1: prime — SELECT * (wide projection), no filter, unique ASC key ----
\echo '### Q1: SELECT * ORDER BY wid LIMIT 10 (prime late-mat target)'
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col ORDER BY wid LIMIT 10;
-- A/B: compare on vs off, materialized into temps (SET cannot vary inside a CTE).
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q1_off AS SELECT * FROM t_col ORDER BY wid LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE q1_on  AS SELECT * FROM t_col ORDER BY wid LIMIT 10;
SELECT 'q1_ab_mism' q, count(*) n FROM (
  (SELECT * FROM q1_off EXCEPT SELECT * FROM q1_on)
  UNION ALL
  (SELECT * FROM q1_on  EXCEPT SELECT * FROM q1_off)) d;
SELECT 'q1_count' q, (SELECT count(*) FROM q1_off) off_n, (SELECT count(*) FROM q1_on) on_n;
-- Order-preserving oracle (council-benchmark M1): capture EMISSION order via row_number() over the raw query output
-- and compare wid position-by-position. Proves the top-k emits the SAME SEQUENCE, not just the same SET.
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q1o_off AS SELECT row_number() OVER () ord, wid FROM (SELECT wid FROM t_col ORDER BY wid LIMIT 10) x;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE q1o_on  AS SELECT row_number() OVER () ord, wid FROM (SELECT wid FROM t_col ORDER BY wid LIMIT 10) x;
SELECT 'q1_order_mism' q, count(*) n FROM q1o_off o JOIN q1o_on n USING (ord) WHERE o.wid <> n.wid;

-- ---- Q2: numeric zone predicate + late-mat ----
\echo '### Q2: SELECT * WHERE v >= 3 ORDER BY wid LIMIT 10'
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q2_off AS SELECT * FROM t_col WHERE v >= 3 ORDER BY wid LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col WHERE v >= 3 ORDER BY wid LIMIT 10;
CREATE TEMP TABLE q2_on  AS SELECT * FROM t_col WHERE v >= 3 ORDER BY wid LIMIT 10;
SELECT 'q2_ab_mism' q, count(*) n FROM (
  (SELECT * FROM q2_off EXCEPT SELECT * FROM q2_on)
  UNION ALL
  (SELECT * FROM q2_on  EXCEPT SELECT * FROM q2_off)) d;

-- ---- Q3: text LIKE predicate + late-mat ----
\echo '### Q3: SELECT * WHERE s LIKE ''%foo%'' ORDER BY wid LIMIT 10'
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q3_off AS SELECT * FROM t_col WHERE s LIKE '%foo%' ORDER BY wid LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col WHERE s LIKE '%foo%' ORDER BY wid LIMIT 10;
CREATE TEMP TABLE q3_on  AS SELECT * FROM t_col WHERE s LIKE '%foo%' ORDER BY wid LIMIT 10;
SELECT 'q3_ab_mism' q, count(*) n FROM (
  (SELECT * FROM q3_off EXCEPT SELECT * FROM q3_on)
  UNION ALL
  (SELECT * FROM q3_on  EXCEPT SELECT * FROM q3_off)) d;

-- ---- Q4: DESC direction ----
\echo '### Q4: SELECT * ORDER BY wid DESC LIMIT 10'
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q4_off AS SELECT * FROM t_col ORDER BY wid DESC LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col ORDER BY wid DESC LIMIT 10;
CREATE TEMP TABLE q4_on  AS SELECT * FROM t_col ORDER BY wid DESC LIMIT 10;
SELECT 'q4_ab_mism' q, count(*) n FROM (
  (SELECT * FROM q4_off EXCEPT SELECT * FROM q4_on)
  UNION ALL
  (SELECT * FROM q4_on  EXCEPT SELECT * FROM q4_off)) d;

-- ---- Q5: projected subset (not SELECT *), key IS a projected column ----
\echo '### Q5: SELECT wid, cid, f WHERE cid > 0 ORDER BY wid LIMIT 15'
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q5_off AS SELECT wid, cid, f FROM t_col WHERE cid > 0 ORDER BY wid LIMIT 15;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT wid, cid, f FROM t_col WHERE cid > 0 ORDER BY wid LIMIT 15;
CREATE TEMP TABLE q5_on  AS SELECT wid, cid, f FROM t_col WHERE cid > 0 ORDER BY wid LIMIT 15;
SELECT 'q5_ab_mism' q, count(*) n FROM (
  (SELECT * FROM q5_off EXCEPT SELECT * FROM q5_on)
  UNION ALL
  (SELECT * FROM q5_on  EXCEPT SELECT * FROM q5_off)) d;

-- ---- Q6: bpchar as a PROJECTED OUTPUT column (not a sort key) — council-rust-pgrx LOW-2 ----
-- bpchar declines as a SORT KEY (PG trims trailing blanks), but is a valid OUTPUT column materialized via
-- arrow_value_to_datum. This is the first path projecting bpchar through the top-k — prove it byte-identical.
\echo '### Q6: SELECT wid, bc(char(8)) ORDER BY wid LIMIT 12 (bpchar projected output)'
DROP TABLE IF EXISTS t_bc CASCADE;
CREATE TABLE t_bc (wid bigint, bc char(8), v int) USING theodb_columnar;
INSERT INTO t_bc SELECT g, ('c'||(g%13))::char(8), g%5 FROM generate_series(1, 5000) g;
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q6_off AS SELECT wid, bc FROM t_bc ORDER BY wid LIMIT 12;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT wid, bc FROM t_bc ORDER BY wid LIMIT 12;
CREATE TEMP TABLE q6_on  AS SELECT wid, bc FROM t_bc ORDER BY wid LIMIT 12;
SELECT 'q6_ab_mism' q, count(*) n FROM (
  (SELECT * FROM q6_off EXCEPT SELECT * FROM q6_on)
  UNION ALL
  (SELECT * FROM q6_on  EXCEPT SELECT * FROM q6_off)) d;

-- ---- Q7/Q8: text sort-key collation guard (council-index-storage HIGH) ----
-- Under a linguistic collation (this DB is en_US.UTF-8), a TEXT sort key MUST decline to the native plan (byte-order
-- ≠ collation order). Under COLLATE "C" (byte order) it MUST swap. EXPLAIN-only (text keys have ties → A/B set-oracle
-- is not tie-safe; the guard LOGIC is what we assert here; byte-identity of text OUTPUT columns is proven by Q3/Q6).
\echo '### Q7: text sort key under DB collation (en_US, linguistic) MUST show Sort (declined to native)'
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col ORDER BY s LIMIT 10;
\echo '### Q8: BARE text sort key on a COLLATE "C" column MUST show Custom Scan (admitted — byte order == PG C order)'
DROP TABLE IF EXISTS t_cc CASCADE;
CREATE TABLE t_cc (wid bigint, sc text COLLATE "C", v int) USING theodb_columnar;
INSERT INTO t_cc SELECT g, 'k'||lpad(g::text, 7, '0'), g%5 FROM generate_series(1, 5000) g;  -- sc UNIQUE (no ties)
EXPLAIN (COSTS OFF) SELECT * FROM t_cc ORDER BY sc LIMIT 10;
-- And prove byte-identity on the admitted C-collation text key (sc unique → tie-safe).
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q8_off AS SELECT * FROM t_cc ORDER BY sc LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE q8_on  AS SELECT * FROM t_cc ORDER BY sc LIMIT 10;
SELECT 'q8_ab_mism' q, count(*) n FROM (
  (SELECT * FROM q8_off EXCEPT SELECT * FROM q8_on)
  UNION ALL
  (SELECT * FROM q8_on  EXCEPT SELECT * FROM q8_off)) d;

\echo '========== ALL *_ab_mism AND q1_order_mism above MUST be 0 (byte-identical + order-identical top-k vs eager) =========='
