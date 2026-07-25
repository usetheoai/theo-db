-- M153 focused A/B harness: GROUP BY text (AGG_SORTED) routing + guards.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET enable_hashagg = off;  -- force GroupAgg (AGG_SORTED) so the text branch is exercised

DROP TABLE IF EXISTS g_col CASCADE;
DROP TABLE IF EXISTS g_heap CASCADE;
CREATE TABLE g_col (phrase text, region int) USING theodb_columnar;
CREATE TABLE g_heap (phrase text, region int);
INSERT INTO g_heap SELECT ('p' || (g % 12))::text, (g % 7) FROM generate_series(1, 2000) g;
UPDATE g_heap SET phrase = NULL WHERE region = 3;   -- NULL group key (EC-2)
INSERT INTO g_col SELECT * FROM g_heap;

\echo '### EC: GROUP BY text ORDER BY count DESC LIMIT (re-sorted above) — expect ROUTED + A/B equal'
EXPLAIN (COSTS OFF) SELECT phrase, COUNT(*) AS c FROM g_col GROUP BY phrase ORDER BY COUNT(*) DESC LIMIT 5;
\echo '-- columnar:'
SELECT phrase, COUNT(*) AS c FROM g_col GROUP BY phrase ORDER BY c DESC, phrase LIMIT 5;
\echo '-- heap:'
SELECT phrase, COUNT(*) AS c FROM g_heap GROUP BY phrase ORDER BY c DESC, phrase LIMIT 5;

\echo '### EC full A/B: full grouped set order-insensitive (columnar vs heap) — expect 0 divergent rows'
SELECT count(*) AS divergent_rows FROM (
  SELECT phrase, COUNT(*) c FROM g_col  GROUP BY phrase
  EXCEPT
  SELECT phrase, COUNT(*) c FROM g_heap GROUP BY phrase
) d;

\echo '### EC-2: NULL text group key present — A/B on the NULL group'
SELECT 'null_group' AS q,
  (SELECT COUNT(*) FROM g_col  WHERE phrase IS NULL) AS col_nullcount,
  (SELECT COUNT(*) FROM g_heap WHERE phrase IS NULL) AS heap_nullcount;

\echo '### EC-3: GROUP BY text ORDER BY text (direct group order, no re-sort by non-key) — routed-if-resorted OR declined, A/B correct'
EXPLAIN (COSTS OFF) SELECT phrase, COUNT(*) FROM g_col GROUP BY phrase ORDER BY phrase LIMIT 5;
SELECT count(*) AS divergent FROM (
  (SELECT phrase, COUNT(*) c FROM g_col GROUP BY phrase ORDER BY phrase)
  EXCEPT
  (SELECT phrase, COUNT(*) c FROM g_heap GROUP BY phrase ORDER BY phrase)
) d;

\echo '### EC-4 (grouping correctness): GROUP BY text under NON-deterministic collation must DECLINE'
CREATE COLLATION IF NOT EXISTS ci2 (provider = icu, locale = 'und-u-ks-level2', deterministic = false);
DROP TABLE IF EXISTS gci_col CASCADE;
DROP TABLE IF EXISTS gci_heap CASCADE;
CREATE TABLE gci_col (s text COLLATE ci2) USING theodb_columnar;
CREATE TABLE gci_heap (s text COLLATE ci2);
INSERT INTO gci_heap VALUES ('abc'),('ABC'),('abc'),('xyz'),('XYZ');
INSERT INTO gci_col SELECT * FROM gci_heap;
\echo '-- EXPLAIN must NOT show theodb_columnar_agg (non-det collation declines at admit):'
EXPLAIN (COSTS OFF) SELECT s, COUNT(*) FROM gci_col GROUP BY s ORDER BY COUNT(*) DESC;
\echo '-- A/B: PG groups abc==ABC (ci2) -> 2 groups; a byte-wise router would give 4 groups. Both must match (declined):'
SELECT 'ci_groups' AS q,
  (SELECT count(*) FROM (SELECT s FROM gci_col  GROUP BY s) z) AS col_grps,
  (SELECT count(*) FROM (SELECT s FROM gci_heap GROUP BY s) z) AS heap_grps;

\echo '### EC-5 (regression): numeric GROUP BY (AGG_SORTED) unchanged — still routes'
EXPLAIN (COSTS OFF) SELECT region, COUNT(*) FROM g_col GROUP BY region ORDER BY region;
SELECT count(*) AS divergent FROM (
  (SELECT region, COUNT(*) c FROM g_col GROUP BY region)
  EXCEPT
  (SELECT region, COUNT(*) c FROM g_heap GROUP BY region)
) d;
