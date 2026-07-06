"""M49 — cosine + inner-product opclasses for theodb_hnsw / theodb_ivfflat.

Phase 1: the non-default cosine (<=>) / ip (<#>) opclasses register for both AMs and the operator pushes down
(EXPLAIN Index Scan). Later phases add metric-resolution correctness (Phase 2) + recall parity (Phase 4).
"""
import os
import psycopg2
import pytest

PGHOST = os.environ.get("PGHOST", "localhost")
PGPORT = os.environ.get("PGPORT", "55448")
PGUSER = os.environ.get("PGUSER", "postgres")
PGPASSWORD = os.environ.get("PGPASSWORD", "theodb")


@pytest.fixture()
def conn():
    c = psycopg2.connect(host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD, dbname="postgres",
                         connect_timeout=5)
    c.autocommit = True
    yield c
    c.close()


def _mk(cur, table, n=200, dim=8):
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    for i in range(n):
        cur.execute(f"INSERT INTO {table} VALUES (%s,%s)", (i, str([float(i + j * 1000) for j in range(dim)])))


@pytest.mark.parametrize("am,opclass,op", [
    ("theodb_hnsw", "theodb_hnsw_cosine_ops", "<=>"),
    ("theodb_hnsw", "theodb_hnsw_ip_ops", "<#>"),
    ("theodb_ivfflat", "theodb_ivfflat_cosine_ops", "<=>"),
    ("theodb_ivfflat", "theodb_ivfflat_ip_ops", "<#>"),
])
def test_cosine_ip_opclass_registers_and_pushes_down(conn, am, opclass, op):
    cur = conn.cursor()
    table = f"m49_{opclass}"
    _mk(cur, table)
    # opclass exists → CREATE INDEX succeeds (RED pre-fix: "operator class ... does not exist")
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING {am} (v {opclass})")
    # pushdown: EXPLAIN of ORDER BY <op> LIMIT uses the index
    cur.execute("SET enable_seqscan = off")
    q = str([50.0 + j * 1000 for j in range(8)])
    cur.execute(f"EXPLAIN SELECT id FROM {table} ORDER BY v {op} %s LIMIT 5", (q,))
    plan = "\n".join(r[0] for r in cur.fetchall())
    assert "Index Scan" in plan, f"{opclass}: {op} did not push down to the index:\n{plan}"
    cur.execute("RESET enable_seqscan")
    cur.execute(f"DROP TABLE {table}")
