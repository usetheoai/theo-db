-- M167 — paired same-binary A/B for the projection top-k.
--
-- WHY THIS EXISTS: the first M167 verdict compared a "before" run taken on a different build/cluster state against
-- an "after" run. The repo's own m166-clickbench-agg.json (same box, same params, one day earlier) contradicted
-- three of the four baselines by ~2x, so the published 42x/62x were not defensible. Toggling the GUC inside ONE
-- session on ONE binary removes build, cluster, session and thermal drift by construction — the only asymmetry
-- left is the toggle itself.
--
-- METHOD: 5 alternating off/on pairs per query, interleaved (not 5 off then 5 on) so any monotonic drift in the
-- box hits both arms equally. `count(*)` wraps each query so the client never pays for row transfer. Report the
-- median of each arm and the per-pair ratio; a single min is not a measurement (Georges et al., OOPSLA'07).
\timing off
SET theodb.enable_columnar_agg = on;
\echo '=== M167 paired A/B: 5 alternating off/on pairs per query ==='

\set q23 'SELECT count(*) FROM (SELECT * FROM hits WHERE URL LIKE ''%google%'' ORDER BY EventTime LIMIT 10) t'
\set q24 'SELECT count(*) FROM (SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '''' ORDER BY EventTime LIMIT 10) t'
\set q25 'SELECT count(*) FROM (SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '''' ORDER BY SearchPhrase LIMIT 10) t'
\set q26 'SELECT count(*) FROM (SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '''' ORDER BY EventTime, SearchPhrase LIMIT 10) t'

-- warm the cache once per query so the first pair is not a cold outlier
SET theodb.enable_columnar_late_mat = on;
:q23 ;
:q24 ;
:q25 ;
:q26 ;

\timing on
\echo '--- q23 ---'
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q23 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q23 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q23 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q23 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q23 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q23 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q23 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q23 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q23 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q23 ;

\echo '--- q24 ---'
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q24 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q24 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q24 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q24 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q24 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q24 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q24 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q24 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q24 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q24 ;

\echo '--- q25 ---'
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q25 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q25 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q25 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q25 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q25 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q25 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q25 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q25 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q25 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q25 ;

\echo '--- q26 ---'
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q26 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q26 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q26 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q26 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q26 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q26 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q26 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q26 ;
SET theodb.enable_columnar_late_mat = off; \echo 'OFF'
:q26 ;
SET theodb.enable_columnar_late_mat = on;  \echo 'ON'
:q26 ;
