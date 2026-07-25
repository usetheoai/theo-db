-- M154 focused A/B harness: COUNT(DISTINCT) routing + guards (EC-1..EC-4).
-- Columnar table `t_col` vs heap twin `t_heap`; compare values + check routing via EXPLAIN.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;

DROP TABLE IF EXISTS t_col CASCADE;
DROP TABLE IF EXISTS t_heap CASCADE;
CREATE TABLE t_col (uid int, phrase text, allnull int) USING theodb_columnar;
CREATE TABLE t_heap (uid int, phrase text, allnull int);

-- data: 1000 rows, uid in [0,49] (50 distinct), phrase in 10 distinct + some NULL, allnull always NULL
INSERT INTO t_heap
SELECT (g % 50), ('p' || (g % 10))::text, NULL
FROM generate_series(1, 1000) g;
-- inject NULLs into uid/phrase for NULL-exclusion coverage
UPDATE t_heap SET phrase = NULL WHERE uid = 7;
UPDATE t_heap SET uid = NULL WHERE phrase = 'p3';
INSERT INTO t_col SELECT * FROM t_heap;

\echo '### EC: COUNT(DISTINCT int) — expect routed + A/B equal'
EXPLAIN (COSTS OFF) SELECT COUNT(DISTINCT uid) FROM t_col;
SELECT 'int_col' AS q, (SELECT COUNT(DISTINCT uid) FROM t_col) AS col_val,
                        (SELECT COUNT(DISTINCT uid) FROM t_heap) AS heap_val;

\echo '### EC: COUNT(DISTINCT text, default collation) — expect routed + A/B equal'
EXPLAIN (COSTS OFF) SELECT COUNT(DISTINCT phrase) FROM t_col;
SELECT 'text_col' AS q, (SELECT COUNT(DISTINCT phrase) FROM t_col) AS col_val,
                        (SELECT COUNT(DISTINCT phrase) FROM t_heap) AS heap_val;

\echo '### EC-2: COUNT(DISTINCT all-NULL) -> 0'
SELECT 'allnull' AS q, (SELECT COUNT(DISTINCT allnull) FROM t_col) AS col_val,
                       (SELECT COUNT(DISTINCT allnull) FROM t_heap) AS heap_val;

\echo '### EC-2b: COUNT(DISTINCT) on empty table -> 0'
CREATE TEMP TABLE e_col (x int) ON COMMIT PRESERVE ROWS;
SELECT 'empty' AS q, (SELECT COUNT(DISTINCT uid) FROM t_col WHERE uid < -1) AS col_val,
                     (SELECT COUNT(DISTINCT uid) FROM t_heap WHERE uid < -1) AS heap_val;

\echo '### EC-3: count(DISTINCT col+1) must DECLINE (no theodb_columnar_agg)'
EXPLAIN (COSTS OFF) SELECT COUNT(DISTINCT uid+1) FROM t_col;
SELECT 'distinct_expr' AS q, (SELECT COUNT(DISTINCT uid+1) FROM t_col) AS col_val,
                             (SELECT COUNT(DISTINCT uid+1) FROM t_heap) AS heap_val;

\echo '### EC-3b: sum(DISTINCT uid) must DECLINE + A/B equal'
EXPLAIN (COSTS OFF) SELECT SUM(DISTINCT uid) FROM t_col;
SELECT 'sum_distinct' AS q, (SELECT SUM(DISTINCT uid) FROM t_col) AS col_val,
                            (SELECT SUM(DISTINCT uid) FROM t_heap) AS heap_val;

\echo '### EC-4: COUNT(DISTINCT) with GROUP BY — routed OR declined, A/B must match'
EXPLAIN (COSTS OFF) SELECT (uid % 5) AS k, COUNT(DISTINCT phrase) FROM t_col GROUP BY (uid % 5) ORDER BY k;
\echo '-- columnar:'
SELECT (uid % 5) AS k, COUNT(DISTINCT phrase) AS v FROM t_col GROUP BY (uid % 5) ORDER BY k;
\echo '-- heap:'
SELECT (uid % 5) AS k, COUNT(DISTINCT phrase) AS v FROM t_heap GROUP BY (uid % 5) ORDER BY k;

\echo '### EC-5 (review HIGH): COUNT(DISTINCT float) must DECLINE (-0.0 vs +0.0 IEEE divergence)'
DROP TABLE IF EXISTS f_col CASCADE;
DROP TABLE IF EXISTS f_heap CASCADE;
CREATE TABLE f_col (x float8) USING theodb_columnar;
CREATE TABLE f_heap (x float8);
INSERT INTO f_heap VALUES (0.0), (-0.0), (1.5), (1.5), ('NaN'), ('NaN');
INSERT INTO f_col SELECT * FROM f_heap;
\echo '-- EXPLAIN must NOT show theodb_columnar_agg (float declines):'
EXPLAIN (COSTS OFF) SELECT COUNT(DISTINCT x) FROM f_col;
\echo '-- A/B: both give PG semantics (0.0==-0.0 -> 1, NaN==NaN -> 1, 1.5 -> 1) = 3; a byte-wise router would give 5:'
SELECT 'float_distinct' AS q, (SELECT COUNT(DISTINCT x) FROM f_col) AS col_val,
                              (SELECT COUNT(DISTINCT x) FROM f_heap) AS heap_val;

\echo '### EC-1: COUNT(DISTINCT text) under NON-deterministic collation must DECLINE'
CREATE COLLATION IF NOT EXISTS ci (provider = icu, locale = 'und-u-ks-level2', deterministic = false);
DROP TABLE IF EXISTS t_col_ci CASCADE;
DROP TABLE IF EXISTS t_heap_ci CASCADE;
CREATE TABLE t_col_ci (s text COLLATE ci) USING theodb_columnar;
CREATE TABLE t_heap_ci (s text COLLATE ci);
INSERT INTO t_heap_ci VALUES ('abc'),('ABC'),('abc'),('xyz'),('XYZ');
INSERT INTO t_col_ci SELECT * FROM t_heap_ci;
\echo '-- EXPLAIN must NOT show theodb_columnar_agg (declines under non-det collation):'
EXPLAIN (COSTS OFF) SELECT COUNT(DISTINCT s) FROM t_col_ci;
SELECT 'ci_collation' AS q, (SELECT COUNT(DISTINCT s) FROM t_col_ci) AS col_val,
                            (SELECT COUNT(DISTINCT s) FROM t_heap_ci) AS heap_val;
\echo '### EC-1 COUNTERFACTUAL: byte-wise (COLLATE "C", = DataFusion semantics) DIVERGES from collation-aware (PG)'
\echo '-- PG collation-aware DISTINCT (what the native plan returns) vs byte-wise DISTINCT (what count_distinct would return if it routed):'
SELECT 'guard_off_counterfactual' AS q,
       (SELECT COUNT(DISTINCT s COLLATE "C") FROM t_heap_ci) AS bytewise_datafusion_would_give,
       (SELECT COUNT(DISTINCT s)             FROM t_heap_ci) AS pg_collation_aware;
-- bytewise=4 (abc,ABC,xyz,XYZ) != collation-aware=2 (abc==ABC, xyz==XYZ) → the ADR-M154-3 guard prevents this divergence.
