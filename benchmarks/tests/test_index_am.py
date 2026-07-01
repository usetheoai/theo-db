"""M26 integration gate — TheoDB's own vector Index Access Method (theodb_ivfflat / theodb_hnsw).

Phase 0 (this file, initial): the de-risk spike — prove the AM registers via pgrx 0.16.1 and that
`CREATE ACCESS METHOD` + `CREATE INDEX … USING theodb_ivfflat` load end-to-end against the container.
Later phases add: page-persisted build, EXPLAIN pushdown, recall@k parity, incremental INSERT/DELETE/VACUUM.

Connection via PG* env (same convention as test_sbq_index.py / test_ann_index.py).
"""

import os

import psycopg2
import pytest

pytestmark = pytest.mark.integration


def _conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"),
        port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"),
        password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
    )


def test_create_access_method_registered():
    """The theodb_ivfflat access method is registered in pg_am after CREATE EXTENSION."""
    with _conn() as conn, conn.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        cur.execute("SELECT count(*) FROM pg_am WHERE amname = 'theodb_ivfflat' AND amtype = 'i'")
        assert cur.fetchone()[0] == 1, "theodb_ivfflat AM not registered in pg_am"


def test_create_index_using_theodb_ivfflat_loads():
    """CREATE INDEX ... USING theodb_ivfflat reaches the AM and persists an index relation."""
    with _conn() as conn, conn.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        cur.execute("DROP TABLE IF EXISTS m26_spike CASCADE")
        cur.execute("CREATE TABLE m26_spike (id bigint, embedding vector(4))")
        cur.execute(
            "INSERT INTO m26_spike VALUES (1, '[1,0,0,0]'), (2, '[0,1,0,0]'), (3, '[0,0,1,0]')"
        )
        # Phase 0: the AM's no-op ambuild must accept CREATE INDEX without error.
        cur.execute(
            "CREATE INDEX m26_spike_idx ON m26_spike USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)"
        )
        cur.execute("SELECT count(*) FROM pg_class WHERE relname = 'm26_spike_idx' AND relkind = 'i'")
        assert cur.fetchone()[0] == 1, "theodb_ivfflat index relation was not created"
        # The index is registered against our AM.
        cur.execute(
            "SELECT am.amname FROM pg_class c JOIN pg_am am ON c.relam = am.oid WHERE c.relname = 'm26_spike_idx'"
        )
        assert cur.fetchone()[0] == "theodb_ivfflat"
        cur.execute("DROP TABLE m26_spike CASCADE")


def _seed_table(cur, name, n=200, dim=8):
    """Deterministic corpus: row i gets a one-hot-ish vector so nearest neighbors are predictable."""
    cur.execute(f"DROP TABLE IF EXISTS {name} CASCADE")
    cur.execute(f"CREATE TABLE {name} (id bigint, embedding vector({dim}))")
    rows = []
    for i in range(n):
        vec = [0.0] * dim
        vec[i % dim] = 1.0 + (i / 1000.0)  # cluster by i%dim, slight tie-break
        rows.append(f"({i}, '[{','.join(str(x) for x in vec)}]')")
    cur.execute(f"INSERT INTO {name} VALUES {','.join(rows)}")


def test_index_persists_to_pages_not_rebuild():
    """Phase 2: CREATE INDEX persists the built index to pages (pg_relation_size > 0)."""
    with _conn() as conn, conn.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        _seed_table(cur, "m26_persist")
        cur.execute("CREATE INDEX m26_persist_idx ON m26_persist USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)")
        cur.execute("SELECT pg_relation_size('m26_persist_idx')")
        assert cur.fetchone()[0] > 0, "index has no persisted pages"
        cur.execute("DROP TABLE m26_persist CASCADE")


def test_index_scan_returns_correct_neighbors():
    """Phase 3+4: the persisted index answers ORDER BY <-> LIMIT k via an Index Scan, matching brute force."""
    with _conn() as conn, conn.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        _seed_table(cur, "m26_scan", n=200, dim=8)
        cur.execute("CREATE INDEX m26_scan_idx ON m26_scan USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)")

        query = "[1,0,0,0,0,0,0,0]"  # closest to rows where i%8==0, smallest i first (id 0, 8, 16, ...)
        # Brute-force ground truth (seq scan).
        cur.execute("SET enable_indexscan = off; SET enable_seqscan = on")
        cur.execute(f"SELECT id FROM m26_scan ORDER BY embedding <-> '{query}' LIMIT 5")
        truth = [r[0] for r in cur.fetchall()]

        # Force the index and compare (recall@5 parity).
        cur.execute("SET enable_seqscan = off; SET enable_indexscan = on")
        cur.execute(f"EXPLAIN (FORMAT text) SELECT id FROM m26_scan ORDER BY embedding <-> '{query}' LIMIT 5")
        plan = "\n".join(r[0] for r in cur.fetchall())
        assert "theodb_ivfflat" in plan or "Index Scan" in plan, f"planner did not use the index:\n{plan}"

        cur.execute(f"SELECT id FROM m26_scan ORDER BY embedding <-> '{query}' LIMIT 5")
        got = [r[0] for r in cur.fetchall()]
        overlap = len(set(got) & set(truth))
        assert overlap >= 4, f"recall@5 too low via index: got {got} vs truth {truth}"
        cur.execute("RESET enable_seqscan; RESET enable_indexscan")
        cur.execute("DROP TABLE m26_scan CASCADE")


def test_incremental_insert_delete_vacuum():
    """Phase 5: INSERT (pending, no rebuild) surfaces via the index; DELETE is filtered; VACUUM folds+cleans."""
    conn = _conn()
    conn.autocommit = True  # VACUUM cannot run inside a transaction block
    try:
        cur = conn.cursor()
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        _seed_table(cur, "m26_maint", n=200, dim=8)
        cur.execute("CREATE INDEX m26_maint_idx ON m26_maint USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)")
        cur.execute("SET enable_seqscan = off")

        # INSERT a distinctive new row: an exact match for a query no existing row is closest to.
        q = "[5,5,0,0,0,0,0,0]"
        cur.execute(f"INSERT INTO m26_maint VALUES (9991, '{q}')")
        cur.execute(f"SELECT id FROM m26_maint ORDER BY embedding <-> '{q}' LIMIT 1")
        assert cur.fetchone()[0] == 9991, "inserted row not found via index (pending region)"

        # DELETE it → the executor's MVCC recheck must filter the (still-indexed) TID out.
        cur.execute("DELETE FROM m26_maint WHERE id = 9991")
        cur.execute(f"SELECT id FROM m26_maint ORDER BY embedding <-> '{q}' LIMIT 1")
        assert cur.fetchone()[0] != 9991, "deleted row still returned (MVCC recheck failed)"

        # VACUUM folds pending into the main index + drops dead TIDs; index still answers correctly afterwards.
        cur.execute("VACUUM m26_maint")
        cur.execute("SELECT id FROM m26_maint ORDER BY embedding <-> '[1,0,0,0,0,0,0,0]' LIMIT 1")
        assert cur.fetchone()[0] == 0, "index broken after VACUUM fold"
        cur.execute("RESET enable_seqscan")
        cur.execute("DROP TABLE m26_maint CASCADE")
    finally:
        conn.close()


def test_hnsw_am_persists_pushes_down_and_recalls():
    """Phase 6: the theodb_hnsw AM shares the layer — persists, EXPLAIN Index Scan, recall parity."""
    with _conn() as conn, conn.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        cur.execute("SELECT count(*) FROM pg_am WHERE amname = 'theodb_hnsw' AND amtype = 'i'")
        assert cur.fetchone()[0] == 1, "theodb_hnsw AM not registered"
        _seed_table(cur, "m26_hnsw", n=200, dim=8)
        cur.execute("CREATE INDEX m26_hnsw_idx ON m26_hnsw USING theodb_hnsw (embedding theodb_hnsw_l2_ops)")
        cur.execute("SELECT pg_relation_size('m26_hnsw_idx')")
        assert cur.fetchone()[0] > 0, "hnsw index not persisted"

        query = "[1,0,0,0,0,0,0,0]"
        cur.execute("SET enable_indexscan = off; SET enable_seqscan = on")
        cur.execute(f"SELECT id FROM m26_hnsw ORDER BY embedding <-> '{query}' LIMIT 5")
        truth = [r[0] for r in cur.fetchall()]
        cur.execute("SET enable_seqscan = off; SET enable_indexscan = on")
        cur.execute(f"EXPLAIN SELECT id FROM m26_hnsw ORDER BY embedding <-> '{query}' LIMIT 5")
        plan = "\n".join(r[0] for r in cur.fetchall())
        assert "theodb_hnsw" in plan or "Index Scan" in plan, f"planner did not use the hnsw index:\n{plan}"
        cur.execute(f"SELECT id FROM m26_hnsw ORDER BY embedding <-> '{query}' LIMIT 5")
        got = [r[0] for r in cur.fetchall()]
        assert len(set(got) & set(truth)) >= 4, f"hnsw recall@5 too low: {got} vs {truth}"
        cur.execute("RESET enable_seqscan; RESET enable_indexscan")
        cur.execute("DROP TABLE m26_hnsw CASCADE")
