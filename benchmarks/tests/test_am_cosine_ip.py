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


def _seqscan_order(cur, table, op, q, k):
    cur.execute("SET enable_indexscan = off; SET enable_bitmapscan = off; SET enable_seqscan = on")
    cur.execute(f"SELECT id FROM {table} ORDER BY v {op} %s LIMIT {k}", (q,))
    out = [r[0] for r in cur.fetchall()]
    cur.execute("RESET enable_indexscan; RESET enable_bitmapscan")
    return out


def _index_order(cur, table, op, q, k):
    cur.execute("SET enable_seqscan = off")
    cur.execute(f"SELECT id FROM {table} ORDER BY v {op} %s LIMIT {k}", (q,))
    out = [r[0] for r in cur.fetchall()]
    cur.execute("RESET enable_seqscan")
    return out


@pytest.mark.parametrize("am", ["theodb_hnsw", "theodb_ivfflat"])
def test_build_resolves_cosine_metric(conn, am):
    """Phase 2 (ADR-1): a cosine index must build+scan with COSINE, not the old hardcoded L2. Uses vectors of
    DIFFERENT NORMS so the cosine order (direction-only) differs from L2 (magnitude). Pre-fix, the index
    persisted an L2 tag and a `<=>` query returned L2 order → mismatch with the cosine oracle."""
    cur = conn.cursor()
    table = f"m49_metric_{am}"
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector(4))")
    # id 1..30: same DIRECTION as the query but growing magnitude (cosine-near, L2-far as magnitude grows);
    # id 100..130: rotated direction, small magnitude (L2-near, cosine-far).
    for i in range(1, 31):
        cur.execute(f"INSERT INTO {table} VALUES (%s,%s)", (i, str([float(i), float(i) * 0.01, 0.0, 0.0])))
    for i in range(100, 131):
        cur.execute(f"INSERT INTO {table} VALUES (%s,%s)", (i, str([0.3, 0.3, 0.0, 0.0])))
    # IVF needs lists << n or every point becomes its own list (probes then examines ~probes points → recall
    # collapses regardless of metric). lists=4 over 61 points is the correct regime for this fixture; HNSW (graph)
    # has no such parameter. This isolates metric RESOLUTION (Phase 2) from IVF list-count degeneracy.
    with_clause = " WITH (lists=4)" if am == "theodb_ivfflat" else ""
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING {am} (v {am}_cosine_ops){with_clause}")

    q = str([1.0, 0.0, 0.0, 0.0])  # direction of the id 1..30 cluster
    truth_cos = _seqscan_order(cur, table, "<=>", q, 10)
    truth_l2 = _seqscan_order(cur, table, "<->", q, 10)
    # sanity: the fixture actually distinguishes the metrics
    assert set(truth_cos) != set(truth_l2), "fixture failed to separate cosine from L2 order"
    idx_cos = _index_order(cur, table, "<=>", q, 10)
    overlap = len(set(idx_cos) & set(truth_cos))
    assert overlap >= 8, f"{am}: cosine index did not scan with cosine (overlap {overlap}/10; idx={idx_cos} cos-truth={truth_cos})"
    cur.execute(f"DROP TABLE {table}")
