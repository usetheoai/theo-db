"""M35 — theodb_hnsw page-native structured scan (integration, container).

Behavioral gate through the SQL surface — a correct on-demand traversal of the structured graph is proven by the
index returning the true k-NN (recall preserved vs a seqscan). If the pack address math or the per-layer neighbor
slice were wrong, recall would collapse — so these tests ARE the codec-correctness gate at the SQL boundary.

- structured scan returns the true k-NN (recall preserved);
- INSERT folds into the scan (pending region), DELETE+VACUUM drops the row (structured rebuild);
- `theodb_hnsw.ef_search` GUC is honored (higher ef ⇒ recall ≥ lower ef); default (64) preserved;
- empty index + NULL query are safe (no crash).
"""
from __future__ import annotations

import io
import os
import random

import psycopg2
import pytest

pytestmark = pytest.mark.integration

DIM = 32
N = 3000
SEED = 42


def _conn():
    conn = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=os.environ.get("PGDATABASE", "postgres"), connect_timeout=30,
    )
    conn.autocommit = True
    return conn


def _seed(cur, n=N):
    rng = random.Random(SEED)
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
    cur.execute("DROP TABLE IF EXISTS hrel CASCADE")
    cur.execute(f"CREATE TABLE hrel (id bigint, embedding vector({DIM}))")
    buf = io.StringIO()
    for g in range(1, n + 1):
        buf.write(f"{g}\t[" + ",".join(f"{rng.random():.5f}" for _ in range(DIM)) + "]\n")
    buf.seek(0)
    cur.copy_expert("COPY hrel (id, embedding) FROM STDIN", buf)


def _qvec():
    rng = random.Random(SEED + 1)
    return "[" + ",".join(f"{rng.random():.5f}" for _ in range(DIM)) + "]"


def _truth(cur, qv, k):
    cur.execute("SET enable_indexscan=off; SET enable_seqscan=on")
    cur.execute(f"SELECT id FROM hrel ORDER BY embedding <-> '{qv}' LIMIT {k}")
    return [r[0] for r in cur.fetchall()]


def _indexed(cur, qv, k):
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
    cur.execute(f"SELECT id FROM hrel ORDER BY embedding <-> '{qv}' LIMIT {k}")
    return [r[0] for r in cur.fetchall()]


def _recall(cur, qv, k=10):
    truth = set(_truth(cur, qv, k))
    got = set(_indexed(cur, qv, k))
    return len(truth & got)


def test_structured_scan_returns_true_knn():
    """The structured on-demand scan reaches high recall vs the exact seqscan — proves pack+traversal correctness."""
    conn = _conn()
    cur = conn.cursor()
    _seed(cur)
    cur.execute("CREATE INDEX h_idx ON hrel USING theodb_hnsw (embedding)")
    qv = _qvec()
    # default ef_search=64 on N=3000, DIM=32 must recover ≥8/10 of the exact neighbors.
    assert _recall(cur, qv, 10) >= 8, "structured theodb_hnsw scan must preserve recall vs seqscan"
    # the index path is actually used (not a silent seqscan fallback)
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
    cur.execute(f"EXPLAIN SELECT id FROM hrel ORDER BY embedding <-> '{qv}' LIMIT 10")
    plan = "\n".join(r[0] for r in cur.fetchall())
    assert "h_idx" in plan or "Index Scan" in plan, f"expected an index scan, got:\n{plan}"
    conn.close()


def test_insert_folds_into_scan():
    """A row INSERTed after build lands in the pending region and is folded into the ranking."""
    conn = _conn()
    cur = conn.cursor()
    _seed(cur)
    cur.execute("CREATE INDEX h_idx ON hrel USING theodb_hnsw (embedding)")
    # insert an exact duplicate of the query vector — it must rank #1 (distance 0).
    qv = _qvec()
    cur.execute(f"INSERT INTO hrel (id, embedding) VALUES (999999, '{qv}')")
    got = _indexed(cur, qv, 5)
    assert 999999 in got, "INSERTed near-duplicate must be folded into the structured scan results"
    conn.close()


def test_delete_vacuum_drops_row():
    """DELETE + VACUUM rebuilds the structured graph over live TIDs only — the deleted id never returns."""
    conn = _conn()
    cur = conn.cursor()
    _seed(cur)
    cur.execute("CREATE INDEX h_idx ON hrel USING theodb_hnsw (embedding)")
    qv = _qvec()
    victim = _truth(cur, qv, 1)[0]  # the exact nearest neighbor
    cur.execute(f"DELETE FROM hrel WHERE id = {victim}")
    cur.execute("VACUUM hrel")
    got = set(_indexed(cur, qv, 10))
    assert victim not in got, "a DELETEd + VACUUMed row must not be returned by the structured scan"
    conn.close()


def test_ef_search_guc_is_honored():
    """Higher theodb_hnsw.ef_search ⇒ recall ≥ lower ef; the default (64) is preserved when unset."""
    conn = _conn()
    cur = conn.cursor()
    _seed(cur)
    cur.execute("CREATE INDEX h_idx ON hrel USING theodb_hnsw (embedding)")
    qv = _qvec()
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
    cur.execute("SHOW theodb_hnsw.ef_search")
    assert cur.fetchone()[0] == "64", "default ef_search must be 64"
    cur.execute("SET theodb_hnsw.ef_search = 1")
    low = _recall(cur, qv, 10)
    cur.execute("SET theodb_hnsw.ef_search = 200")
    high = _recall(cur, qv, 10)
    assert high >= low, f"higher ef_search must not reduce recall (ef1={low}, ef200={high})"
    conn.close()


def test_single_node_graph():
    """A one-row graph: the entry node is the only node (no neighbor tuple to deref) — must return it, no crash."""
    conn = _conn()
    cur = conn.cursor()
    _seed(cur, n=1)
    cur.execute("CREATE INDEX h_idx ON hrel USING theodb_hnsw (embedding)")
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
    cur.execute(f"SELECT id FROM hrel ORDER BY embedding <-> '{_qvec()}' LIMIT 5")
    assert [r[0] for r in cur.fetchall()] == [1], "single-node graph must return its only row"
    conn.close()


def test_empty_index_and_null_query_are_safe():
    """An index on an empty table + a NULL ORDER BY key return no rows without crashing."""
    conn = _conn()
    cur = conn.cursor()
    _seed(cur, n=0)
    cur.execute("CREATE INDEX h_idx ON hrel USING theodb_hnsw (embedding)")
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
    cur.execute("SELECT id FROM hrel ORDER BY embedding <-> '[" + ",".join(["0"] * DIM) + "]' LIMIT 5")
    assert cur.fetchall() == []
    cur.execute("SELECT id FROM hrel ORDER BY embedding <-> NULL LIMIT 5")
    assert cur.fetchall() == []
    conn.close()
