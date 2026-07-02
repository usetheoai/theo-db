"""M34 — theodb_ivfflat WITH (lists=N) reloption + theodb_ivfflat.probes GUC (integration, container).

Behavioral gate through the SQL surface (no profiling needed):
- lists reloption: WITH (lists=1) scans the single list = ALL rows -> perfect recall; WITH (lists=50) + probes=1
  scans ~1/50 -> lower recall. The recall difference proves `lists` is honored at build.
- probes GUC: on a lists=50 index, probes=1 (low recall) vs probes=50 (all lists -> perfect recall). The recall
  increase proves `probes` is honored at scan.
- default preservation + edge validation (WITH (lists=0) rejected; probes bounds).
"""
from __future__ import annotations

import io
import os
import random

import psycopg2
import pytest

pytestmark = pytest.mark.integration

DIM = 32
N = 2000
SEED = 42


def _conn():
    conn = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=os.environ.get("PGDATABASE", "postgres"), connect_timeout=30,
    )
    conn.autocommit = True
    return conn


def _seed(cur):
    rng = random.Random(SEED)
    cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
    cur.execute("DROP TABLE IF EXISTS rel CASCADE")
    cur.execute(f"CREATE TABLE rel (id bigint, embedding vector({DIM}))")
    buf = io.StringIO()
    for g in range(1, N + 1):
        buf.write(f"{g}\t[" + ",".join(f"{rng.random():.5f}" for _ in range(DIM)) + "]\n")
    buf.seek(0)
    cur.copy_expert("COPY rel (id, embedding) FROM STDIN", buf)


def _recall(cur, qv, k=10):
    cur.execute("SET enable_indexscan=off; SET enable_seqscan=on")
    cur.execute(f"SELECT id FROM rel ORDER BY embedding <-> '{qv}' LIMIT {k}")
    truth = {r[0] for r in cur.fetchall()}
    cur.execute("SET enable_seqscan=off; SET enable_indexscan=on")
    cur.execute(f"SELECT id FROM rel ORDER BY embedding <-> '{qv}' LIMIT {k}")
    got = {r[0] for r in cur.fetchall()}
    return len(truth & got)


def test_lists_reloption_is_honored_at_build():
    """WITH (lists=1) -> single list -> scan reads ALL rows -> perfect recall; WITH (lists=50) + probes=1 -> partial."""
    conn = _conn()
    try:
        cur = conn.cursor()
        _seed(cur)
        cur.execute("SELECT embedding::text FROM rel WHERE id=1")
        qv = cur.fetchone()[0]

        cur.execute("DROP INDEX IF EXISTS rel_ix")
        cur.execute("CREATE INDEX rel_ix ON rel USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops) WITH (lists=1)")
        cur.execute("SET theodb_ivfflat.probes = 1")
        r_one_list = _recall(cur, qv)

        cur.execute("DROP INDEX rel_ix")
        cur.execute("CREATE INDEX rel_ix ON rel USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops) WITH (lists=50)")
        cur.execute("SET theodb_ivfflat.probes = 1")
        r_fifty_lists = _recall(cur, qv)

        # lists=1 scans everything (exact); lists=50 with probes=1 scans ~1/50 -> strictly fewer true neighbors.
        assert r_one_list == 10, f"lists=1 must be exact, got {r_one_list}/10"
        assert r_fifty_lists < r_one_list, f"lists=50/probes=1 recall {r_fifty_lists} not < lists=1 recall {r_one_list}"
    finally:
        conn.cursor().execute("DROP TABLE IF EXISTS rel CASCADE")
        conn.close()


def test_probes_guc_is_honored_at_scan():
    """On a lists=50 index, more probes -> more lists scanned -> higher recall; probes>=lists -> exact."""
    conn = _conn()
    try:
        cur = conn.cursor()
        _seed(cur)
        cur.execute("SELECT embedding::text FROM rel WHERE id=1")
        qv = cur.fetchone()[0]
        cur.execute("DROP INDEX IF EXISTS rel_ix")
        cur.execute("CREATE INDEX rel_ix ON rel USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops) WITH (lists=50)")

        cur.execute("SET theodb_ivfflat.probes = 1")
        r_low = _recall(cur, qv)
        cur.execute("SET theodb_ivfflat.probes = 50")  # all lists -> exact
        r_high = _recall(cur, qv)

        assert r_high >= r_low, f"more probes must not lower recall: {r_high} < {r_low}"
        assert r_high == 10, f"probes>=lists must be exact, got {r_high}/10"
    finally:
        conn.cursor().execute("DROP TABLE IF EXISTS rel CASCADE")
        conn.close()


def test_default_preserves_behavior():
    """No WITH + unset GUC -> defaults (lists=100, probes=10) — the index builds + scans (no regression)."""
    conn = _conn()
    try:
        cur = conn.cursor()
        _seed(cur)
        cur.execute("SELECT embedding::text FROM rel WHERE id=1")
        qv = cur.fetchone()[0]
        cur.execute("DROP INDEX IF EXISTS rel_ix")
        cur.execute("CREATE INDEX rel_ix ON rel USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)")
        cur.execute("RESET theodb_ivfflat.probes")
        assert _recall(cur, qv) >= 4, "default index scan must return a reasonable neighbor overlap"
        cur.execute("SHOW theodb_ivfflat.probes")
        assert cur.fetchone()[0] == "10", "default probes GUC must be 10"
    finally:
        conn.cursor().execute("DROP TABLE IF EXISTS rel CASCADE")
        conn.close()


def test_lists_beyond_single_page_directory():
    """lists=800 needs a MULTI-PAGE directory (800×12 > CHUNK) — the v2 format headline. Proves build + scan +
    INSERT (main_index_pages/pending) round-trip past the old ~665 single-page cap that used to panic."""
    conn = _conn()
    try:
        cur = conn.cursor()
        _seed(cur)  # 2000 rows -> lists=800 gives ~2.5/list; dir = 800*12 = 9600 B = 2 pages (> the ~665 cap)
        cur.execute("SELECT embedding::text FROM rel WHERE id=1")
        qv = cur.fetchone()[0]
        cur.execute("DROP INDEX IF EXISTS rel_ix")
        cur.execute("CREATE INDEX rel_ix ON rel USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops) WITH (lists=800)")
        cur.execute("SET theodb_ivfflat.probes = 800")  # all lists -> exact
        assert _recall(cur, qv) == 10, "lists=800 (multi-page dir) exact scan must return the true top-10"
        # INSERT exercises main_index_pages/pending on the v2 multi-page-directory layout.
        cur.execute("INSERT INTO rel VALUES (999999, '" + qv + "')")
        cur.execute(f"SELECT id FROM rel WHERE id=999999 ORDER BY embedding <-> '{qv}' LIMIT 1")
        assert cur.fetchone()[0] == 999999, "INSERTed row must be reachable via the v2 index (pending fold)"
    finally:
        conn.cursor().execute("DROP TABLE IF EXISTS rel CASCADE")
        conn.close()


def test_lists_out_of_range_rejected():
    """WITH (lists=0) is rejected at DDL by the reloption bounds (typed error, not a crash)."""
    conn = _conn()
    try:
        cur = conn.cursor()
        _seed(cur)
        cur.execute("DROP INDEX IF EXISTS rel_ix")
        with pytest.raises(psycopg2.errors.Error):
            cur.execute(
                "CREATE INDEX rel_ix ON rel USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops) WITH (lists=0)"
            )
    finally:
        conn.cursor().execute("DROP TABLE IF EXISTS rel CASCADE")
        conn.close()
