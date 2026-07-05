"""M48 maintenance tests for the theodb index AMs (plan m48-am-crash-safety, phases 2-5).

Covers the non-crash side of the crash-safe fold (issue #47): a VACUUM fold must preserve scan
results and, because it shadow-writes the new generation to fresh pages before pivoting block 0
(never in place), the relation must GROW across a single fold (the in-place rewrite it replaces
kept the size ~constant — that size delta is the structural oracle that distinguishes the two
mechanisms without needing a crash). Later phases add the pending-fold threshold (T3.1) and the
honest cost estimate (T5.1) here.

Uses a plain psycopg2 connection (env PGHOST/PGPORT/...) — these do NOT kill the container, so they
share the module-level connection. Crash tests live in test_am_crash.py.
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
    c = psycopg2.connect(
        host=PGHOST, port=PGPORT, user=PGUSER, password=PGPASSWORD, dbname="postgres",
        connect_timeout=5,
    )
    c.autocommit = True
    yield c
    c.close()


def _make_index(cur, table, am, opclass, n=400, dim=8):
    """Build a structured index over `n` deterministic vectors and return the ground-truth rows."""
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(f"CREATE TABLE {table} (id int, v vector({dim}))")
    rows = []
    for i in range(n):
        # Well-separated, distinct vectors (avoid the tie-heavy modular data that makes ANN top-k
        # ambiguous): each id gets a unique point whose distance to a query grows smoothly with id.
        vec = [float(i + j * 1000) for j in range(dim)]
        rows.append((i, vec))
        cur.execute(f"INSERT INTO {table} VALUES (%s, %s)", (i, str(vec)))
    cur.execute(f"CREATE INDEX {table}_idx ON {table} USING {am} (v {opclass})")
    return rows


def _index_knn(cur, table, query, k):
    cur.execute("SET enable_seqscan = off")
    cur.execute(f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT {k}", (str(query),))
    return [r[0] for r in cur.fetchall()]


def _seqscan_knn(cur, table, query, k):
    """Ground-truth top-k via seqscan — NEVER use the index as its own oracle (SEPA)."""
    cur.execute("SET enable_indexscan = off")
    cur.execute("SET enable_bitmapscan = off")
    cur.execute("SET enable_seqscan = on")
    cur.execute(
        f"SELECT id FROM {table} ORDER BY v <-> %s LIMIT {k}", (str(query),)
    )
    out = [r[0] for r in cur.fetchall()]
    cur.execute("RESET enable_indexscan")
    cur.execute("RESET enable_bitmapscan")
    return out


def _rel_size(cur, table):
    cur.execute(f"SELECT pg_relation_size('{table}_idx')")
    return cur.fetchone()[0]


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_fold_preserves_scan_results(conn, am, opclass):
    """A VACUUM fold preserves scan correctness AND grows the relation (shadow-write, not in-place)."""
    cur = conn.cursor()
    table = f"m48_fold_{am}"
    _make_index(cur, table, am, opclass, n=400, dim=8)
    query = [40.0 + j * 1000 for j in range(8)]  # near id 40

    size_pre = _rel_size(cur, table)
    cur.execute(f"DELETE FROM {table} WHERE id % 10 < 3")  # ~30% dead
    cur.execute(f"VACUUM {table}")
    size_post = _rel_size(cur, table)

    # Correctness (ANN is approximate — assert recall overlap, not exact order): the fold must keep the
    # index returning the true nearest neighbours of the LIVE corpus. A corrupt fold (stale bytes scored
    # as vectors) would collapse this overlap. No dead ids may appear (heap-visibility filter works).
    idx_top = _index_knn(cur, table, query, 10)
    truth = set(_seqscan_knn(cur, table, query, 10))
    assert all(i % 10 >= 3 for i in idx_top), f"{am}: fold returned a deleted id: {idx_top}"
    overlap = len(set(idx_top) & truth)
    assert overlap >= 8, f"{am}: fold degraded recall (overlap {overlap}/10; idx={idx_top} truth={truth})"

    # Structural oracle (the RED/GREEN discriminator): the crash-safe fold shadow-writes the new
    # generation to FRESH pages before pivoting block 0, so the relation GROWS across one fold. The
    # in-place rewrite it replaces kept the size ~constant. (Reclaim / bounded growth is T2.2.)
    assert size_post > size_pre, (
        f"{am}: relation did not grow after fold ({size_pre} -> {size_post}) — "
        "fold appears to still rewrite in place (pre-#47-fix behaviour)"
    )
    cur.execute(f"DROP TABLE {table}")


@pytest.mark.parametrize("am,opclass", [
    ("theodb_hnsw", "theodb_hnsw_l2_ops"),
    ("theodb_ivfflat", "theodb_ivfflat_l2_ops"),
])
def test_fold_empty_corpus(conn, am, opclass):
    """EC-5: folding an emptied index (DELETE ALL + VACUUM) yields a valid empty index, still writable."""
    cur = conn.cursor()
    table = f"m48_empty_{am}"
    _make_index(cur, table, am, opclass, n=200, dim=8)
    cur.execute(f"DELETE FROM {table}")
    cur.execute(f"VACUUM {table}")

    # empty scan returns nothing, no error
    assert _index_knn(cur, table, [1.0] * 8, 5) == []
    # still writable: a fresh INSERT is scannable through the index
    cur.execute(f"INSERT INTO {table} VALUES (999, %s)", (str([2.0] * 8),))
    assert _index_knn(cur, table, [2.0] * 8, 1) == [999]
    cur.execute(f"DROP TABLE {table}")
