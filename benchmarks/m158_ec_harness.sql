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
DROP TABLE IF EXISTS ec_res;
CREATE TEMP TABLE ec_res (q text PRIMARY KEY, n bigint);

\echo '### Q1: SELECT * ORDER BY wid LIMIT 10 (prime late-mat target)'
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col ORDER BY wid LIMIT 10;
-- A/B: compare on vs off, materialized into temps (SET cannot vary inside a CTE).
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q1_off AS SELECT * FROM t_col ORDER BY wid LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE q1_on  AS SELECT * FROM t_col ORDER BY wid LIMIT 10;
INSERT INTO ec_res SELECT 'q1_ab_mism', count(*) FROM (
  (SELECT * FROM q1_off EXCEPT SELECT * FROM q1_on)
  UNION ALL
  (SELECT * FROM q1_on  EXCEPT SELECT * FROM q1_off)) d;
-- non-vacuity: a comparison over zero rows reports zero mismatches and proves nothing
INSERT INTO ec_res SELECT 'q1_rows', count(*) FROM q1_on;
SELECT 'q1_count' q, (SELECT count(*) FROM q1_off) off_n, (SELECT count(*) FROM q1_on) on_n;
-- Order-preserving oracle (council-benchmark M1): capture EMISSION order via row_number() over the raw query output
-- and compare wid position-by-position. Proves the top-k emits the SAME SEQUENCE, not just the same SET.
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q1o_off AS SELECT row_number() OVER () ord, wid FROM (SELECT wid FROM t_col ORDER BY wid LIMIT 10) x;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE q1o_on  AS SELECT row_number() OVER () ord, wid FROM (SELECT wid FROM t_col ORDER BY wid LIMIT 10) x;
INSERT INTO ec_res SELECT 'q1_order_mism', count(*) FROM q1o_off o JOIN q1o_on n USING (ord) WHERE o.wid <> n.wid;

-- ---- Q2: numeric zone predicate + late-mat ----
\echo '### Q2: SELECT * WHERE v >= 3 ORDER BY wid LIMIT 10'
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q2_off AS SELECT * FROM t_col WHERE v >= 3 ORDER BY wid LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col WHERE v >= 3 ORDER BY wid LIMIT 10;
CREATE TEMP TABLE q2_on  AS SELECT * FROM t_col WHERE v >= 3 ORDER BY wid LIMIT 10;
INSERT INTO ec_res SELECT 'q2_ab_mism', count(*) FROM (
  (SELECT * FROM q2_off EXCEPT SELECT * FROM q2_on)
  UNION ALL
  (SELECT * FROM q2_on  EXCEPT SELECT * FROM q2_off)) d;
-- non-vacuity: a comparison over zero rows reports zero mismatches and proves nothing
INSERT INTO ec_res SELECT 'q2_rows', count(*) FROM q2_on;

-- ---- Q3: text LIKE predicate + late-mat ----
\echo '### Q3: SELECT * WHERE s LIKE ''%foo%'' ORDER BY wid LIMIT 10'
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q3_off AS SELECT * FROM t_col WHERE s LIKE '%foo%' ORDER BY wid LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col WHERE s LIKE '%foo%' ORDER BY wid LIMIT 10;
CREATE TEMP TABLE q3_on  AS SELECT * FROM t_col WHERE s LIKE '%foo%' ORDER BY wid LIMIT 10;
INSERT INTO ec_res SELECT 'q3_ab_mism', count(*) FROM (
  (SELECT * FROM q3_off EXCEPT SELECT * FROM q3_on)
  UNION ALL
  (SELECT * FROM q3_on  EXCEPT SELECT * FROM q3_off)) d;
-- non-vacuity: a comparison over zero rows reports zero mismatches and proves nothing
INSERT INTO ec_res SELECT 'q3_rows', count(*) FROM q3_on;

-- ---- Q4: DESC direction ----
\echo '### Q4: SELECT * ORDER BY wid DESC LIMIT 10'
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q4_off AS SELECT * FROM t_col ORDER BY wid DESC LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col ORDER BY wid DESC LIMIT 10;
CREATE TEMP TABLE q4_on  AS SELECT * FROM t_col ORDER BY wid DESC LIMIT 10;
INSERT INTO ec_res SELECT 'q4_ab_mism', count(*) FROM (
  (SELECT * FROM q4_off EXCEPT SELECT * FROM q4_on)
  UNION ALL
  (SELECT * FROM q4_on  EXCEPT SELECT * FROM q4_off)) d;
-- non-vacuity: a comparison over zero rows reports zero mismatches and proves nothing
INSERT INTO ec_res SELECT 'q4_rows', count(*) FROM q4_on;

-- ---- Q5: projected subset (not SELECT *), key IS a projected column ----
\echo '### Q5: SELECT wid, cid, f WHERE cid > 0 ORDER BY wid LIMIT 15'
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE q5_off AS SELECT wid, cid, f FROM t_col WHERE cid > 0 ORDER BY wid LIMIT 15;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT wid, cid, f FROM t_col WHERE cid > 0 ORDER BY wid LIMIT 15;
CREATE TEMP TABLE q5_on  AS SELECT wid, cid, f FROM t_col WHERE cid > 0 ORDER BY wid LIMIT 15;
INSERT INTO ec_res SELECT 'q5_ab_mism', count(*) FROM (
  (SELECT * FROM q5_off EXCEPT SELECT * FROM q5_on)
  UNION ALL
  (SELECT * FROM q5_on  EXCEPT SELECT * FROM q5_off)) d;
-- non-vacuity: a comparison over zero rows reports zero mismatches and proves nothing
INSERT INTO ec_res SELECT 'q5_rows', count(*) FROM q5_on;

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
INSERT INTO ec_res SELECT 'q6_ab_mism', count(*) FROM (
  (SELECT * FROM q6_off EXCEPT SELECT * FROM q6_on)
  UNION ALL
  (SELECT * FROM q6_on  EXCEPT SELECT * FROM q6_off)) d;
-- non-vacuity: a comparison over zero rows reports zero mismatches and proves nothing
INSERT INTO ec_res SELECT 'q6_rows', count(*) FROM q6_on;

-- ---- Q7/Q8: text sort-key collation guard (council-index-storage HIGH) ----
-- Under a linguistic collation (this DB is en_US.UTF-8), a TEXT sort key MUST decline to the native plan (byte-order
-- ≠ collation order). Under COLLATE "C" (byte order) it MUST swap. EXPLAIN-only (text keys have ties → A/B set-oracle
-- is not tie-safe; the guard LOGIC is what we assert here; byte-identity of text OUTPUT columns is proven by Q3/Q6).
\echo '### Q7: text sort key under the DATABASE DEFAULT collation — outcome depends on the cluster, so this'
\echo '###     block asserts NOTHING by itself. On a byte-order cluster (datcollate=C + libc) it ROUTES; on a'
\echo '###     linguistic one it declines. The environment-independent assertions are M167-C2 / D3 / D3b.'
\echo '###     (The old text said "MUST show Sort", a premise false on this C cluster — the committed log'
\echo '###      showed that MUST next to a Custom Scan, i.e. a violated assertion nobody was asserting.)'
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
INSERT INTO ec_res SELECT 'q8_ab_mism', count(*) FROM (
  (SELECT * FROM q8_off EXCEPT SELECT * FROM q8_on)
  UNION ALL
  (SELECT * FROM q8_on  EXCEPT SELECT * FROM q8_off)) d;


-- ============================================================================
-- M167 — projection top-k: the four ClickBench shapes (q23-q26) + a negative control.
-- Mirrors ClickBench q23/q24/q25/q26 onto t_col/t_cc. Each block: EXPLAIN (is it routed?) + full-row
-- symmetric-EXCEPT (tie-free via a unique key) + emission-order oracle where the projection has ties.
-- ============================================================================

-- ---- M167-A: q23 analog — WIDE SELECT * + text filter + unique non-text key ----
\echo '### M167-A (q23): SELECT * WHERE s LIKE ''%foo%'' ORDER BY wid LIMIT 10'
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col WHERE s LIKE '%foo%' ORDER BY wid LIMIT 10;
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE m167a_off AS SELECT * FROM t_col WHERE s LIKE '%foo%' ORDER BY wid LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE m167a_on  AS SELECT * FROM t_col WHERE s LIKE '%foo%' ORDER BY wid LIMIT 10;
INSERT INTO ec_res SELECT 'm167a_ab_mism', count(*) FROM (
  (SELECT * FROM m167a_off EXCEPT SELECT * FROM m167a_on)
  UNION ALL
  (SELECT * FROM m167a_on  EXCEPT SELECT * FROM m167a_off)) d;

-- ---- M167-B: q24 analog — NARROW projection + filter + non-text key; output has ties -> order oracle ----
\echo '### M167-B (q24): SELECT s WHERE s <> '''' ORDER BY wid LIMIT 10 (emission-order oracle)'
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT s FROM t_col WHERE s <> '' ORDER BY wid LIMIT 10;
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE m167b_off AS SELECT row_number() OVER () ord, s FROM (SELECT s FROM t_col WHERE s <> '' ORDER BY wid LIMIT 10) x;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE m167b_on  AS SELECT row_number() OVER () ord, s FROM (SELECT s FROM t_col WHERE s <> '' ORDER BY wid LIMIT 10) x;
INSERT INTO ec_res SELECT 'm167b_order_mism', count(*) FROM m167b_off o JOIN m167b_on n USING (ord) WHERE o.s IS DISTINCT FROM n.s;

-- ---- M167-C: q25 analog — TEXT sort key carrying the DATABASE DEFAULT collation (OID 100) ----
-- The expectation is CLUSTER-DEPENDENT by design: byte-order is a property of the collation, so this block
-- prints pg_database.datcollate alongside the plan. Under datcollate=C/POSIX the key IS byte-order and MUST route (M167 T3.1);
-- under a linguistic locale it MUST decline. `sd` is unique so the comparison is tie-free either way.
\echo '### M167-C (q25): text sort key with DEFAULT collation — routes iff datcollate is byte-order'
SELECT 'm167c_datcollate' q, datcollate v FROM pg_database WHERE datname = current_database();
-- NOTE: a fresh table, not ALTER TABLE t_cc ADD COLUMN — theodb_columnar rejects adding a column to a relation
-- that already has stripes ("stripe ncols N != relation natts N+1"). Out of M167 scope; recorded, not worked around.
DROP TABLE IF EXISTS t_dc CASCADE;
CREATE TABLE t_dc (wid bigint, sd text) USING theodb_columnar;   -- sd carries the DATABASE DEFAULT collation (OID 100)
INSERT INTO t_dc SELECT g, 'd'||lpad(g::text, 7, '0') FROM generate_series(1, 5000) g;  -- sd UNIQUE (tie-free)
-- Multi-key fixture: `v` ties on purpose (g % 11) so the SECOND key decides; `sd` unique keeps it tie-free;
-- `bc` is bpchar, a type the sort-key guard must reject in ANY key position.
DROP TABLE IF EXISTS t_dc2 CASCADE;
CREATE TABLE t_dc2 (wid bigint, v int, sd text, bc char(8)) USING theodb_columnar;
INSERT INTO t_dc2 SELECT g, g % 11, 'd'||lpad(g::text, 7, '0'), lpad(g::text, 8, '0') FROM generate_series(1, 5000) g;
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT wid, sd FROM t_dc ORDER BY sd LIMIT 10;
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE m167c_off AS SELECT wid, sd FROM t_dc ORDER BY sd LIMIT 10;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE m167c_on  AS SELECT wid, sd FROM t_dc ORDER BY sd LIMIT 10;
INSERT INTO ec_res SELECT 'm167c_ab_mism', count(*) FROM (
  (SELECT * FROM m167c_off EXCEPT SELECT * FROM m167c_on)
  UNION ALL
  (SELECT * FROM m167c_on  EXCEPT SELECT * FROM m167c_off)) d;

-- ---- M167-C2: a NAMED linguistic collation MUST decline regardless of cluster locale ----
-- Replaces the environment-dependent premise of Q7 ("this DB is en_US.UTF-8"), which is FALSE on a C cluster.
\echo '### M167-C2: ORDER BY sd COLLATE "en_US.utf8" MUST decline (linguistic != byte order)'
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT wid, sd FROM t_dc ORDER BY sd COLLATE "en_US.utf8" LIMIT 10;

-- ---- M167-D: q26 analog — MULTI-KEY, first key ties so the second key decides the order ----
\echo '### M167-D (q26): ORDER BY v, wid LIMIT 10 (v ties on purpose; wid breaks them)'
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT * FROM t_col ORDER BY v, wid LIMIT 10;
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE m167d_off AS SELECT row_number() OVER () ord, wid, v FROM (SELECT wid, v FROM t_col ORDER BY v, wid LIMIT 10) x;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE m167d_on  AS SELECT row_number() OVER () ord, wid, v FROM (SELECT wid, v FROM t_col ORDER BY v, wid LIMIT 10) x;
INSERT INTO ec_res SELECT 'm167d_order_mism', count(*) FROM m167d_off o JOIN m167d_on n USING (ord) WHERE o.wid <> n.wid OR o.v <> n.v;

-- ---- M167-D2: multi-key where the SECOND key is TEXT (q26's real shape: timestamp + text) ----
-- The two mechanisms M167 added (multi-key wire format; byte-order collation predicate) intersect ONLY here.
-- M167-D is int+int and M167-C is single-key text, so neither covers a per-key loop that checks key 0's collation
-- and forgets key 1's. `sd` is unique, so the comparison stays tie-free.
\echo '### M167-D2: ORDER BY v, sd LIMIT 10 (multi-key with a TEXT second key)'
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT wid, sd FROM t_dc2 ORDER BY v, sd LIMIT 10;
SET theodb.enable_columnar_late_mat = off;
CREATE TEMP TABLE m167d2_off AS SELECT row_number() OVER () ord, wid, sd FROM (SELECT wid, sd FROM t_dc2 ORDER BY v, sd LIMIT 10) x;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE m167d2_on  AS SELECT row_number() OVER () ord, wid, sd FROM (SELECT wid, sd FROM t_dc2 ORDER BY v, sd LIMIT 10) x;
INSERT INTO ec_res SELECT 'm167d2_order_mism', count(*) FROM m167d2_off o JOIN m167d2_on n USING (ord) WHERE o.wid <> n.wid OR o.sd IS DISTINCT FROM n.sd;

-- ---- M167-D3: a guard-failing key ANYWHERE in the list must decline the WHOLE swap (fail-closed per key) ----
\echo '### M167-D3: ORDER BY v, sd COLLATE "en_US.utf8" MUST decline (second key not byte-order)'
SET theodb.enable_columnar_late_mat = on;
EXPLAIN (COSTS OFF) SELECT wid, sd FROM t_dc2 ORDER BY v, sd COLLATE "en_US.utf8" LIMIT 10;
\echo '### M167-D3b: bpchar as a NON-FIRST sort key MUST decline (PG trims trailing blanks, DataFusion does not)'
EXPLAIN (COSTS OFF) SELECT wid, sd FROM t_dc2 ORDER BY v, bc LIMIT 10;

-- ---- M167-D4: more sort keys than TOPK_MAX_SORT_KEYS (8) must decline ----
-- The first version of this block wrote `ORDER BY v, wid, sd, bc, v, wid, sd, bc, v` over the 4-column `t_dc2`.
-- Nine items, but PostgreSQL deduplicates redundant pathkeys, so the planner produced `Sort Key: v, wid, sd, bc` —
-- FOUR keys. The ceiling was never reached and the decline came from `bc` (bpchar), i.e. it re-proved D3b under a
-- different name. A fixture with 4 columns cannot express 9 distinct keys, so the fixture is the fix.
DROP TABLE IF EXISTS t_dc9 CASCADE;
CREATE TABLE t_dc9 (k1 int, k2 int, k3 int, k4 int, k5 int, k6 int, k7 int, k8 int, k9 int, payload bigint)
  USING theodb_columnar;
INSERT INTO t_dc9 SELECT g%2, g%3, g%5, g%7, g%11, g%13, g%17, g%19, g%23, g FROM generate_series(1, 5000) g;

-- Boundary control: exactly 8 distinct keys, every one an admitted type, MUST route. Without this, a decline at 9
-- proves nothing — it could be the fixture, the width, or any other guard rather than the ceiling.
\echo '### M167-D4a: exactly 8 sort keys (== TOPK_MAX_SORT_KEYS) MUST route'
EXPLAIN (COSTS OFF) SELECT payload FROM t_dc9 ORDER BY k1, k2, k3, k4, k5, k6, k7, k8 LIMIT 10;

\echo '### M167-D4: 9 DISTINCT sort keys MUST decline (over TOPK_MAX_SORT_KEYS)'
EXPLAIN (COSTS OFF) SELECT payload FROM t_dc9 ORDER BY k1, k2, k3, k4, k5, k6, k7, k8, k9 LIMIT 10;

-- ---- M167-E: NEGATIVE CONTROL — the oracle MUST be able to fail ----
-- Seed a real divergence (a twin table with one row moved into the top-k) and prove the SAME comparison
-- machinery reports it. n = 0 here would mean every 0 above is meaningless.
\echo '### M167-E: negative control — seeded divergence MUST report n > 0'
DROP TABLE IF EXISTS t_seed CASCADE;
-- NOTE: the divergence is seeded at INSERT time, not by UPDATE — theodb_columnar is append-only (M99:
-- "tuple fetch by TID is not supported"). Recorded, not worked around.
CREATE TABLE t_seed (LIKE t_col) USING theodb_columnar;
INSERT INTO t_seed SELECT CASE WHEN wid = 19999 THEN -1 ELSE wid END, cid, v, s, f, ts FROM t_col;
SET theodb.enable_columnar_late_mat = on;
CREATE TEMP TABLE m167e_a AS SELECT * FROM t_col  ORDER BY wid LIMIT 10;
CREATE TEMP TABLE m167e_b AS SELECT * FROM t_seed ORDER BY wid LIMIT 10;
INSERT INTO ec_res SELECT 'm167e_control_diff', count(*) FROM (
  (SELECT * FROM m167e_a EXCEPT SELECT * FROM m167e_b)
  UNION ALL
  (SELECT * FROM m167e_b EXCEPT SELECT * FROM m167e_a)) d;

\echo '### FINAL GATE — machine-checked, not printed'
SELECT * FROM ec_res ORDER BY q;
-- POSITIVE CONTROL: run with `-v gate_selftest=1`; the gate MUST abort. One copy of the logic, so the control
-- cannot drift from what it controls.
\if :{?gate_selftest}
  \echo '### GATE SELF-TEST ARMED: forcing q1_ab_mism = 1 — the FINAL GATE MUST abort below'
  UPDATE ec_res SET n = 1 WHERE q = 'q1_ab_mism';
\endif
DO $gate$
DECLARE bad text := '';
BEGIN
  bad := bad || coalesce(
    (SELECT 'non-zero mismatch counters: ' || string_agg(format('%s=%s', q, n), ', ') || '; '
       FROM ec_res WHERE (q LIKE '%\_ab\_mism' OR q LIKE '%\_order\_mism') AND n <> 0), '');
  bad := bad || coalesce(
    (SELECT 'blocks that compared ZERO rows: ' || string_agg(q, ', ') || '; '
       FROM ec_res WHERE q LIKE '%\_rows' AND n = 0), '');
  IF coalesce((SELECT n FROM ec_res WHERE q = 'm167e_control_diff'), 0) = 0 THEN
    bad := bad || 'm167e_control_diff must be > 0 (an oracle that cannot fail is not an oracle); ';
  END IF;
  IF (SELECT count(*) FROM ec_res) <> 20 THEN
    bad := bad || format('expected 20 assertions, found %s (a block was silently skipped); ',
                         (SELECT count(*) FROM ec_res));
  END IF;
  IF bad <> '' THEN RAISE EXCEPTION 'M167 EC FINAL GATE FAILED: %', bad; END IF;
  RAISE NOTICE 'M167 EC FINAL GATE ok: % assertions, all mismatch counters 0, negative control fires',
    (SELECT count(*) FROM ec_res);
END
$gate$;
