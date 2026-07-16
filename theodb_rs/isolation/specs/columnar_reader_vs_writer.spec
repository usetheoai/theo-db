# M99 D1 — a REPEATABLE READ reader must NOT see a stripe committed by another session after the reader's
# snapshot was taken (snapshot stability), and a fresh transaction after commit sees it. Proves the
# columnar.stripe catalog row's visibility is bound to the scan snapshot (MVCC delegated to Postgres).
setup
{
    CREATE EXTENSION IF NOT EXISTS theodb_rs;
    CREATE TABLE cw (a int) USING theodb_columnar;
    INSERT INTO cw VALUES (1);
}
teardown { DROP TABLE IF EXISTS cw; }

session r
step rbegin  { BEGIN ISOLATION LEVEL REPEATABLE READ; }
step rc1     { SELECT count(*) FROM cw; }
step rc2     { SELECT count(*) FROM cw; }
step rcommit { COMMIT; }
step rc3     { SELECT count(*) FROM cw; }

session w
step wbegin  { BEGIN; }
step wins    { INSERT INTO cw VALUES (2); }
step wcommit { COMMIT; }

# r takes its RR snapshot (rc1=1), w commits a new stripe, r STILL sees 1 (rc2), after commit r sees 2 (rc3).
permutation rbegin rc1 wbegin wins wcommit rc2 rcommit rc3
