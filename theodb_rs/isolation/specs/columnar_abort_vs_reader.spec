# M99 D1 — an uncommitted writer's rows are invisible to a concurrent reader, and after the writer ABORTs no
# stripe becomes visible (the pending rows are discarded, no catalog row committed). Proves abort-correctness:
# an aborted mid-flush stripe never becomes visible.
setup
{
    CREATE EXTENSION IF NOT EXISTS theodb_rs;
    CREATE TABLE cw (a int) USING theodb_columnar;
    INSERT INTO cw VALUES (1);
}
teardown { DROP TABLE IF EXISTS cw; }

session w
step wbegin { BEGIN; }
step wins   { INSERT INTO cw SELECT g FROM generate_series(2, 6) g; }
step wabort { ROLLBACK; }

session r
step rmid   { SELECT count(*) FROM cw; }
step rafter { SELECT count(*) FROM cw; }

# While w's insert is uncommitted, r sees only the 1 committed row; after w aborts, still 1.
permutation wbegin wins rmid wabort rafter
