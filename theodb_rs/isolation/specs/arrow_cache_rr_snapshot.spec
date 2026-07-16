# M101 D1 — a REPEATABLE READ reader over the heap-authoritative Arrow cache holds its snapshot: a concurrent
# committed write does NOT change what the RR reader sees (the cache's generation read under the RR snapshot is
# unchanged, so the cache is reused), and a fresh transaction after commit sees the new row (rebuild). Proves the
# cache respects snapshot isolation — the MVCC correctness gate.
setup
{
    CREATE EXTENSION IF NOT EXISTS theodb_rs;
    CREATE TABLE hc (id int, measure float8);
    INSERT INTO hc SELECT g, g::float8 FROM generate_series(1, 3) g;
    SELECT theodb_columnarize('hc'::regclass, ARRAY['measure']);
}
teardown { DROP TABLE IF EXISTS hc; }

session a
setup { SET theodb.enable_columnar_agg = on; }
step a_begin   { BEGIN ISOLATION LEVEL REPEATABLE READ; }
step a_refresh { SELECT theodb_cache_refresh('hc'::regclass); }
step a1        { SELECT count(*) FROM hc; }
step a2        { SELECT count(*) FROM hc; }
step a_commit  { COMMIT; }
step a3        { SELECT count(*) FROM hc; }

session b
step b_ins { INSERT INTO hc VALUES (4, 4.0); }

# a (RR) sees 3 (a1), b commits a 4th row, a STILL sees 3 (a2 — snapshot held), after commit a sees 4 (a3).
permutation a_begin a_refresh a1 b_ins a2 a_commit a3
