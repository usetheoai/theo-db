# M99 D1 — two concurrent OPEN transactions each insert into a theodb_columnar table; each flushes a stripe
# with a non-overlapping row_number range (reserved under the metapage buffer lock) at its own commit, and after
# both commit every row is present exactly once. Proves the pre-commit flush + reservation race is correct.
setup
{
    CREATE EXTENSION IF NOT EXISTS theodb_rs;
    CREATE TABLE cw (a int, sess int) USING theodb_columnar;
}
teardown { DROP TABLE IF EXISTS cw; }

session s1
step s1b { BEGIN; }
step s1i { INSERT INTO cw SELECT g, 1 FROM generate_series(1, 5) g; }
step s1c { COMMIT; }

session s2
step s2b { BEGIN; }
step s2i { INSERT INTO cw SELECT g, 2 FROM generate_series(1, 5) g; }
step s2c { COMMIT; }

session chk
step total { SELECT count(*) AS n, count(DISTINCT (a, sess)) AS distinct_pairs FROM cw; }

# Both xacts open + insert while uncommitted, then commit both, then read: 10 rows, all distinct (no overlap/loss).
permutation s1b s2b s1i s2i s1c s2c total
