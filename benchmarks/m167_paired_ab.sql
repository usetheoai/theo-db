-- M167 — paired same-binary A/B for the projection top-k.
--
-- WHY: the first verdict compared runs taken in different build/cluster states; the repo's own
-- m166-clickbench-agg.json contradicted three of four baselines by ~2x. Toggling the GUC inside ONE session on ONE
-- binary removes build, cluster, session and thermal drift by construction.
--
-- WHY `CREATE TEMP TABLE … AS` AND NOT `SELECT count(*) FROM (…)`: a count(*) wrapper lets PostgreSQL skip
-- materializing the projected columns — which is EXACTLY the cost late materialization saves. Measured: under a
-- count(*) wrapper q23 showed 1.03x (no effect); the whole mechanism was being optimized away by the harness.
-- CTAS forces the k surviving rows to be fully formed, so the arms differ by the thing under test. (Same pattern
-- m158_ec_harness.sql already uses.)
--
-- METHOD: 5 alternating off/on pairs per query, interleaved so monotonic box drift hits both arms equally.
-- Report the median per arm and the per-pair win count; a single min is not a measurement (Georges et al. 2007).
SET theodb.enable_columnar_agg = on;
\echo '=== M167 paired A/B (CTAS, 5 alternating off/on pairs per query) ==='

\echo '--- q23 ---'
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS w_q23;
CREATE TEMP TABLE w_q23 AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q23_1o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q23_1o AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q23_1n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q23_1n AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q23_2o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q23_2o AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q23_2n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q23_2n AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q23_3o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q23_3o AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q23_3n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q23_3n AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q23_4o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q23_4o AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q23_4n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q23_4n AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q23_5o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q23_5o AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q23_5n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q23_5n AS SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10;
\timing off

\echo '--- q24 ---'
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS w_q24;
CREATE TEMP TABLE w_q24 AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q24_1o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q24_1o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q24_1n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q24_1n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q24_2o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q24_2o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q24_2n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q24_2n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q24_3o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q24_3o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q24_3n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q24_3n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q24_4o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q24_4o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q24_4n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q24_4n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q24_5o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q24_5o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q24_5n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q24_5n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10;
\timing off

\echo '--- q25 ---'
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS w_q25;
CREATE TEMP TABLE w_q25 AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q25_1o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q25_1o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q25_1n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q25_1n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q25_2o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q25_2o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q25_2n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q25_2n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q25_3o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q25_3o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q25_3n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q25_3n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q25_4o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q25_4o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q25_4n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q25_4n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q25_5o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q25_5o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q25_5n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q25_5n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10;
\timing off

\echo '--- q26 ---'
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS w_q26;
CREATE TEMP TABLE w_q26 AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q26_1o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q26_1o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q26_1n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q26_1n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q26_2o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q26_2o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q26_2n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q26_2n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q26_3o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q26_3o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q26_3n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q26_3n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q26_4o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q26_4o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q26_4n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q26_4n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS p_q26_5o;
\echo 'OFF'
\timing on
CREATE TEMP TABLE p_q26_5o AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
SET theodb.enable_columnar_late_mat = on;
DROP TABLE IF EXISTS p_q26_5n;
\echo 'ON'
\timing on
CREATE TEMP TABLE p_q26_5n AS SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10;
\timing off
