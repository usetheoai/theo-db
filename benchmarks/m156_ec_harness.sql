-- M156 focused A/B harness: text WHERE predicate routing (=, <>, LIKE, NOT LIKE) + guards + round-trip.
-- Columnar table `t_col` vs heap twin `t_heap`; compare values + check routing via EXPLAIN.
-- Routing lands on the aggregate CustomScan, so every probe is an aggregate over the columnar table.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;

-- A non-deterministic ICU collation for the collation-guard decline probe (EC-3).
DROP COLLATION IF EXISTS ci_nd CASCADE;
CREATE COLLATION ci_nd (provider = icu, locale = 'und-u-ks-level2', deterministic = false);

DROP TABLE IF EXISTS t_col CASCADE;
DROP TABLE IF EXISTS t_heap CASCADE;
CREATE TABLE t_heap (
    uid     int,
    phrase  text,
    url     text,
    bp      char(4),
    citext  text COLLATE ci_nd
);
-- 2000 rows. phrase in {p0..p9} + some NULL; url mixes literal-% strings for the LIKE-escape probe.
INSERT INTO t_heap
SELECT (g % 50),
       ('p' || (g % 10))::text,
       CASE WHEN g % 7 = 0 THEN 'a%b' ELSE ('http://x/' || (g % 4))::text END,
       ('ab' || (g % 2))::char(4),
       ('c' || (g % 3))::text
FROM generate_series(1, 2000) g;
UPDATE t_heap SET phrase = NULL WHERE uid = 7;      -- NULL exclusion in <>
UPDATE t_heap SET url = '' WHERE uid = 13;           -- empty string coverage
CREATE TABLE t_col (LIKE t_heap) USING theodb_columnar;
INSERT INTO t_col SELECT * FROM t_heap;

\echo '========== ROUTED (expect Custom Scan + A/B equal) =========='

\echo '### R1: count(*) WHERE phrase = ''p1'''
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE phrase = 'p1';
SELECT 'eq' AS q, (SELECT count(*) FROM t_col WHERE phrase = 'p1') AS col_val,
                  (SELECT count(*) FROM t_heap WHERE phrase = 'p1') AS heap_val;

\echo '### R2: count(*) WHERE phrase <> '''' (NULL excluded)'
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE phrase <> '';
SELECT 'ne_empty' AS q, (SELECT count(*) FROM t_col WHERE phrase <> '') AS col_val,
                        (SELECT count(*) FROM t_heap WHERE phrase <> '') AS heap_val;

\echo '### R3: count(*) WHERE url LIKE ''%x%'''
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE url LIKE '%x%';
SELECT 'like' AS q, (SELECT count(*) FROM t_col WHERE url LIKE '%x%') AS col_val,
                    (SELECT count(*) FROM t_heap WHERE url LIKE '%x%') AS heap_val;

\echo '### R4: count(*) WHERE url NOT LIKE ''http%'''
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE url NOT LIKE 'http%';
SELECT 'not_like' AS q, (SELECT count(*) FROM t_col WHERE url NOT LIKE 'http%') AS col_val,
                        (SELECT count(*) FROM t_heap WHERE url NOT LIKE 'http%') AS heap_val;

\echo '### EC-1: LIKE escape — WHERE url LIKE ''a\%b'' matches literal a%b (default \\ escape)'
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE url LIKE 'a\%b';
SELECT 'like_escape' AS q, (SELECT count(*) FROM t_col WHERE url LIKE 'a\%b') AS col_val,
                           (SELECT count(*) FROM t_heap WHERE url LIKE 'a\%b') AS heap_val;

\echo '### EC-5: mixed text + numeric WHERE — phrase = ''p1'' AND uid > 5'
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE phrase = 'p1' AND uid > 5;
SELECT 'mixed' AS q, (SELECT count(*) FROM t_col WHERE phrase = 'p1' AND uid > 5) AS col_val,
                     (SELECT count(*) FROM t_heap WHERE phrase = 'p1' AND uid > 5) AS heap_val;

\echo '### EC-2: round-trip of special needles (empty / % / _ / backslash / multibyte)'
SELECT 'rt_empty'  AS q, (SELECT count(*) FROM t_col  WHERE url = '')       AS col_val, (SELECT count(*) FROM t_heap WHERE url = '')       AS heap_val
UNION ALL
SELECT 'rt_pct',        (SELECT count(*) FROM t_col  WHERE url = 'a%b'),                 (SELECT count(*) FROM t_heap WHERE url = 'a%b')
UNION ALL
SELECT 'rt_uscore',     (SELECT count(*) FROM t_col  WHERE phrase LIKE '%\_%'),          (SELECT count(*) FROM t_heap WHERE phrase LIKE '%\_%')
UNION ALL
SELECT 'rt_multibyte',  (SELECT count(*) FROM t_col  WHERE url = 'café'),                (SELECT count(*) FROM t_heap WHERE url = 'café');

\echo '========== DECLINED (expect Seq Scan / native, NO Custom Scan; A/B still equal) =========='

\echo '### EC-3a: ILIKE declines'
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE url ILIKE '%X%';
SELECT 'ilike' AS q, (SELECT count(*) FROM t_col WHERE url ILIKE '%X%') AS col_val,
                     (SELECT count(*) FROM t_heap WHERE url ILIKE '%X%') AS heap_val;

\echo '### EC-3b: regex (~) declines'
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE url ~ 'x';
SELECT 'regex' AS q, (SELECT count(*) FROM t_col WHERE url ~ 'x') AS col_val,
                     (SELECT count(*) FROM t_heap WHERE url ~ 'x') AS heap_val;

\echo '### EC-3c: bpchar (char(4)) predicate declines'
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE bp = 'ab0';
SELECT 'bpchar' AS q, (SELECT count(*) FROM t_col WHERE bp = 'ab0') AS col_val,
                      (SELECT count(*) FROM t_heap WHERE bp = 'ab0') AS heap_val;

\echo '### EC-3d: non-deterministic collation declines'
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE citext = 'c1';
SELECT 'nd_coll' AS q, (SELECT count(*) FROM t_col WHERE citext = 'c1') AS col_val,
                       (SELECT count(*) FROM t_heap WHERE citext = 'c1') AS heap_val;

\echo '### EC-4: NULL const (phrase = NULL) declines; both sides 0 (3-valued)'
EXPLAIN (COSTS OFF) SELECT count(*) FROM t_col WHERE phrase = NULL;
SELECT 'null_const' AS q, (SELECT count(*) FROM t_col WHERE phrase = NULL) AS col_val,
                          (SELECT count(*) FROM t_heap WHERE phrase = NULL) AS heap_val;

\echo '========== DONE — every col_val MUST equal its heap_val =========='
