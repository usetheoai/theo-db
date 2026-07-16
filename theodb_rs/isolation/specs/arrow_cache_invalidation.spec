# M101 D1 — a committed write by another session invalidates a reader's heap-authoritative Arrow cache: the
# reader's next read (a new snapshot) rebuilds under it and sees the new row. Proves cross-backend invalidation via
# the shared columnar.cache_state generation (bumped by the statement trigger, made visible on commit).
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
step a_refresh { SELECT theodb_cache_refresh('hc'::regclass); }
step a1 { SELECT count(*) FROM hc; }
step a2 { SELECT count(*) FROM hc; }

session b
step b_ins { INSERT INTO hc VALUES (4, 4.0); }

# a builds its cache (3 rows), reads 3; b commits a 4th row (bumps the generation); a's next read rebuilds -> 4.
permutation a_refresh a1 b_ins a2
