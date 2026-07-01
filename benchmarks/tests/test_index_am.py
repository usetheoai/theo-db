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
