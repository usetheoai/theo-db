-- M157 focused A/B harness: date_trunc group-key routing + guards (timestamp routes, timestamptz/granularity decline).
-- Columnar table `t_col` vs heap twin `t_heap`; compare grouped results + check routing via EXPLAIN.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;

DROP TABLE IF EXISTS t_col CASCADE;
DROP TABLE IF EXISTS t_heap CASCADE;
CREATE TABLE t_heap (
    id   int,
    ts   timestamp,      -- tz-independent → date_trunc routes
    tstz timestamptz,    -- tz-dependent → date_trunc DECLINES (ADR-2)
    v    int
);
-- 3000 rows spanning ~40 days across 2 months, minute/hour granularity variety.
INSERT INTO t_heap
SELECT g,
       (TIMESTAMP '2026-01-15 00:00:00' + (g % 40) * INTERVAL '1 day' + (g % 24) * INTERVAL '1 hour' + (g % 60) * INTERVAL '1 minute'),
       (TIMESTAMPTZ '2026-01-15 00:00:00-03' + (g % 40) * INTERVAL '1 day'),
       (g % 7)
FROM generate_series(1, 3000) g;
CREATE TABLE t_col (LIKE t_heap) USING theodb_columnar;
INSERT INTO t_col SELECT * FROM t_heap;

\echo '========== ROUTED (expect Custom Scan; A/B KEY-exact via symmetric EXCEPT = 0 mismatches) =========='
-- The (col EXCEPT heap) UNION ALL (heap EXCEPT col) on the full (key, count) tuple = symmetric difference. n=0 proves
-- the grouped results are byte-identical on BOTH the date_trunc KEY and the count (council-index-storage: check `k`,
-- not just count/sum). Fine-grained units (day/hour/minute/second) are epoch-invariant → correct.

\echo '### R1: date_trunc day route (KEY-exact) + Custom Scan'
EXPLAIN (COSTS OFF) SELECT date_trunc('day', ts) k, count(*) FROM t_col GROUP BY 1;
SELECT 'day_mism' q, count(*) n FROM (
  (SELECT date_trunc('day', ts) k, count(*) c FROM t_col  GROUP BY 1 EXCEPT SELECT date_trunc('day', ts), count(*) FROM t_heap GROUP BY 1)
  UNION ALL
  (SELECT date_trunc('day', ts) k, count(*) c FROM t_heap GROUP BY 1 EXCEPT SELECT date_trunc('day', ts), count(*) FROM t_col  GROUP BY 1)) d;

\echo '### R2: date_trunc hour route (KEY-exact)'
SELECT 'hour_mism' q, count(*) n FROM (
  (SELECT date_trunc('hour', ts) k, count(*) c FROM t_col  GROUP BY 1 EXCEPT SELECT date_trunc('hour', ts), count(*) FROM t_heap GROUP BY 1)
  UNION ALL
  (SELECT date_trunc('hour', ts) k, count(*) c FROM t_heap GROUP BY 1 EXCEPT SELECT date_trunc('hour', ts), count(*) FROM t_col  GROUP BY 1)) d;

\echo '### R3: date_trunc minute (the q42 shape) route (KEY-exact) + Custom Scan'
EXPLAIN (COSTS OFF) SELECT date_trunc('minute', ts) k, count(*) FROM t_col GROUP BY 1;
SELECT 'minute_mism' q, count(*) n FROM (
  (SELECT date_trunc('minute', ts) k, count(*) c FROM t_col  GROUP BY 1 EXCEPT SELECT date_trunc('minute', ts), count(*) FROM t_heap GROUP BY 1)
  UNION ALL
  (SELECT date_trunc('minute', ts) k, count(*) c FROM t_heap GROUP BY 1 EXCEPT SELECT date_trunc('minute', ts), count(*) FROM t_col  GROUP BY 1)) d;

\echo '### R4 (backward-compat): bare-Var GROUP BY still routes'
EXPLAIN (COSTS OFF) SELECT v, count(*) FROM t_col GROUP BY v ORDER BY v;

\echo '========== DECLINED (expect Seq Scan / native, NO Custom Scan; A/B still equal) =========='

\echo '### EC-1: date_trunc(''week'', ts) declines (ISO week ≠ Arrow)'
EXPLAIN (COSTS OFF) SELECT date_trunc('week', ts) k, count(*) FROM t_col GROUP BY 1 ORDER BY 1;

\echo '### EC-1b: date_trunc month/quarter/year DECLINE (PG-2000 vs Arrow-1970 calendar epoch mismatch)'
EXPLAIN (COSTS OFF) SELECT date_trunc('month',   ts) k, count(*) FROM t_col GROUP BY 1;
EXPLAIN (COSTS OFF) SELECT date_trunc('quarter', ts) k, count(*) FROM t_col GROUP BY 1;
EXPLAIN (COSTS OFF) SELECT date_trunc('year',    ts) k, count(*) FROM t_col GROUP BY 1;

\echo '### EC-2: date_trunc on timestamptz declines under TimeZone != UTC (ADR-2)'
SET TimeZone = 'America/Sao_Paulo';
EXPLAIN (COSTS OFF) SELECT date_trunc('day', tstz) k, count(*) FROM t_col GROUP BY 1 ORDER BY 1;
SELECT 'tstz_col'  q, count(*) n FROM (SELECT date_trunc('day', tstz) FROM t_col  GROUP BY 1) s;
SELECT 'tstz_heap' q, count(*) n FROM (SELECT date_trunc('day', tstz) FROM t_heap GROUP BY 1) s;
RESET TimeZone;

\echo '### EC-3: EXTRACT declines (numeric group-key)'
EXPLAIN (COSTS OFF) SELECT EXTRACT(hour FROM ts) k, count(*) FROM t_col GROUP BY 1 ORDER BY 1;

\echo '### EC-4: CASE declines'
EXPLAIN (COSTS OFF) SELECT CASE WHEN v > 3 THEN 1 ELSE 0 END k, count(*) FROM t_col GROUP BY 1 ORDER BY 1;

\echo '### EC-5: arithmetic group-expr (v+1) declines'
EXPLAIN (COSTS OFF) SELECT v+1 k, count(*) FROM t_col GROUP BY 1 ORDER BY 1;

\echo '========== DONE — every col == heap; declined rows show Seq Scan =========='
