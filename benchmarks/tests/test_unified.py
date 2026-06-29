"""M16 — Unification e2e: the unified query, recall-preserving filtered search, Pinecone import.

Proves the ADR-0005 moat is demonstrable: vector + relational + AI in one transactional SQL, filtered
search that preserves recall (no over-filtering), and a dependency-free Pinecone import. Honesty (ADR 0005):
no speed claim — correctness (recall under filter) + unification. Runs against a container with the theodb
extension (theo-db:m16, which bundles sql/80 import_pinecone). Connection via PGHOST/PGPORT.
"""

import math
import os
import pathlib
import random

import psycopg2
import pytest

ROOT = pathlib.Path(__file__).resolve().parents[2]
QUICKSTART = ROOT / "docs" / "quickstart.md"
MIGRATE_DOC = ROOT / "docs" / "migrate-from-pinecone.md"
DEMO_DOC = ROOT / "docs" / "unification-1-vs-2-systems.md"
DIM = 8


def _vec(v):
    return "[" + ",".join(str(x) for x in v) + "]"


def _l2(a, b):
    return math.sqrt(sum((x - y) ** 2 for x, y in zip(a, b)))


@pytest.fixture(scope="module")
def admin_conn():
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname="postgres",
    )
    c.autocommit = True
    yield c
    with c.cursor() as cur:
        for name in ("m16_unified", "m16_filter", "m16_import"):
            cur.execute(f"DROP DATABASE IF EXISTS {name}")
    c.close()


def _fresh_db_with_ext(admin_conn, name):
    with admin_conn.cursor() as cur:
        cur.execute(f"DROP DATABASE IF EXISTS {name}")
        cur.execute(f"CREATE DATABASE {name}")
    c = psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=name,
    )
    with c.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb CASCADE")
    c.commit()
    return c


# ---- T1.1 — canonical unified query (vector JOIN relational + WHERE) -------------------------------

def test_unified_query_returns_correct_joined_rows(admin_conn):
    """One transactional SQL: vector ORDER BY <=> + relational JOIN + WHERE returns the known nearest
    in-stock row of the requested category — the unification proof (vector + relational together)."""
    conn = _fresh_db_with_ext(admin_conn, "m16_unified")
    try:
        with conn.cursor() as cur:
            cur.execute("""
                CREATE TABLE products (id text PRIMARY KEY, category_id int, embedding vector(8));
                CREATE TABLE inventory (product_id text PRIMARY KEY, in_stock bool);
            """)
            # P_target: nearest to query among in-stock + category=3. Decoys: nearer but out-of-stock or wrong cat.
            q = [1, 0, 0, 0, 0, 0, 0, 0]
            rows = [
                ("p_target", 3, [0.99, 0.01, 0, 0, 0, 0, 0, 0], True),   # nearest valid
                ("p_nearer_oos", 3, [1, 0, 0, 0, 0, 0, 0, 0], False),    # nearest but OUT of stock
                ("p_nearer_cat", 1, [1, 0, 0, 0, 0, 0, 0, 0], True),     # nearest but WRONG category
                ("p_far", 3, [0, 1, 0, 0, 0, 0, 0, 0], True),            # valid but far
            ]
            for pid, cat, emb, stock in rows:
                cur.execute("INSERT INTO products VALUES (%s,%s,%s)", (pid, cat, _vec(emb)))
                cur.execute("INSERT INTO inventory VALUES (%s,%s)", (pid, stock))
            # the unified query: vector + JOIN + WHERE, one statement
            cur.execute("""
                SELECT p.id
                FROM products p
                JOIN inventory i ON i.product_id = p.id
                WHERE i.in_stock AND p.category_id = 3
                ORDER BY p.embedding <=> %s::vector
                LIMIT 1
            """, (_vec(q),))
            top = cur.fetchone()[0]
            assert top == "p_target", f"unified query returned {top}, expected p_target"
    finally:
        conn.close()


# ---- T1.2 — recall-preserving filtered search (over-filtering edge) --------------------------------

def test_filtered_search_preserves_recall(admin_conn):
    """A selective WHERE + vector ORDER BY must return the full k. First PROVE the over-filtering edge is
    real (without iterative_scan, fewer than k); then assert iterative_scan restores k (EC-1: no trivial pass)."""
    k = 10
    conn = _fresh_db_with_ext(admin_conn, "m16_filter")
    try:
        rng = random.Random(16)
        with conn.cursor() as cur:
            cur.execute("CREATE TABLE items (id int PRIMARY KEY, category_id int, embedding vector(8))")
            # Deterministic over-filtering: the query sits near the origin cluster (non-target rows), while
            # the target category (99) is a FAR cluster (offset +10 on every dim). A bounded HNSW scan
            # (default ef_search) returns the nearest rows — all non-99 — so a post-filter on category=99
            # yields ZERO; iterative_scan must scan further to reach the k far target rows.
            for i in range(2000):
                emb = [rng.gauss(0, 1) for _ in range(DIM)]               # near-origin cluster, cat != 99
                cur.execute("INSERT INTO items VALUES (%s,%s,%s)", (i, i % 7, _vec(emb)))
            for j in range(30):                                            # FAR cluster, cat = 99 (> k rows)
                emb = [10 + rng.gauss(0, 1) for _ in range(DIM)]
                cur.execute("INSERT INTO items VALUES (%s,99,%s)", (10000 + j, _vec(emb)))
            cur.execute("CREATE INDEX ON items USING hnsw (embedding vector_l2_ops)")
            conn.commit()
            q = _vec([0] * DIM)   # at the origin → nearest are the non-99 cluster

            cur.execute("SET enable_seqscan = off")   # force the approximate index (else a small table seq-scans → exact, no over-filtering)
            cur.execute("SET hnsw.ef_search = 40")     # default bound — the far cat=99 cluster sits outside it

            def count(iterative):
                cur.execute(f"SET hnsw.iterative_scan = {iterative}")
                cur.execute(
                    "SELECT count(*) FROM (SELECT id FROM items WHERE category_id=99 "
                    "ORDER BY embedding <-> %s::vector LIMIT %s) t", (q, k))
                return cur.fetchone()[0]

            n_without = count("off")
            n_with = count("strict_order")
            # EC-1: prove the edge is real before asserting the fix
            if n_without >= k:
                pytest.xfail(f"over-filtering not reproduced (n_without={n_without} >= k={k}) on this build")
            assert n_with == k, f"iterative_scan did not restore k: n_with={n_with}, k={k}"
            assert n_with >= n_without
    finally:
        conn.close()


def test_filtered_search_uses_index(admin_conn):
    """EXPLAIN proves the vector index is used under the filter (not a pure seq scan)."""
    conn = _fresh_db_with_ext(admin_conn, "m16_filter")  # reuse db (recreated)
    try:
        rng = random.Random(17)
        with conn.cursor() as cur:
            cur.execute("DROP TABLE IF EXISTS items2; CREATE TABLE items2 (id int PRIMARY KEY, category_id int, embedding vector(8))")
            for i in range(500):
                cur.execute("INSERT INTO items2 VALUES (%s,%s,%s)",
                            (i, i % 5, _vec([rng.gauss(0, 1) for _ in range(DIM)])))
            cur.execute("CREATE INDEX ON items2 USING hnsw (embedding vector_l2_ops)")
            cur.execute("SET LOCAL enable_seqscan = off")
            q = _vec([rng.gauss(0, 1) for _ in range(DIM)])
            cur.execute("EXPLAIN (ANALYZE, BUFFERS) SELECT id FROM items2 WHERE category_id=1 "
                        "ORDER BY embedding <-> %s::vector LIMIT 5", (q,))
            plan = "\n".join(r[0] for r in cur.fetchall())
            assert "Index Scan" in plan, f"vector index not used; plan:\n{plan}"
    finally:
        conn.close()


# ---- T2.1 — import_pinecone --------------------------------------------------------------------------

def test_import_pinecone_maps_records(admin_conn):
    """import_pinecone maps a Pinecone export {id,values,metadata} → (id, embedding vector, metadata jsonb)."""
    conn = _fresh_db_with_ext(admin_conn, "m16_import")
    try:
        with conn.cursor() as cur:
            cur.execute("CREATE TABLE items (id text PRIMARY KEY, embedding vector(3), metadata jsonb)")
            export = '[{"id":"a","values":[1,0,0],"metadata":{"cat":3}},' \
                     ' {"id":"b","values":[0,1,0],"metadata":{"cat":7}}]'
            cur.execute("SELECT theodb.import_pinecone('items'::regclass, %s::jsonb)", (export,))
            assert cur.fetchone()[0] == 2
            cur.execute("SELECT metadata->>'cat' FROM items WHERE id='a'")
            assert cur.fetchone()[0] == "3"
            cur.execute("SELECT count(*) FROM items WHERE embedding IS NOT NULL")
            assert cur.fetchone()[0] == 2
    finally:
        conn.close()


def test_import_pinecone_rejects_malformed(admin_conn):
    """Non-array export AND a record missing 'values' each raise SQLSTATE 22023 (typed, fail-fast)."""
    conn = _fresh_db_with_ext(admin_conn, "m16_import")
    try:
        with conn.cursor() as cur:
            cur.execute("CREATE TABLE t (id text PRIMARY KEY, embedding vector(3), metadata jsonb)")
            conn.commit()
            for bad in ('{"id":"a","values":[1,0,0]}',           # object, not array
                        '[{"id":"a"}]'):                          # record missing 'values'
                with pytest.raises(psycopg2.errors.InvalidParameterValue):  # SQLSTATE 22023
                    cur.execute("SELECT theodb.import_pinecone('t'::regclass, %s::jsonb)", (bad,))
                conn.rollback()
            cur.execute("SELECT count(*) FROM t")
            assert cur.fetchone()[0] == 0  # no partial/corrupt insert
    finally:
        conn.close()


def test_import_pinecone_safe_identifiers(admin_conn):
    """Hostile table/column identifiers are handled via %I/regclass — no SQL injection (EC-2)."""
    conn = _fresh_db_with_ext(admin_conn, "m16_import")
    try:
        with conn.cursor() as cur:
            cur.execute('CREATE TABLE "weird;name" (id text PRIMARY KEY, "emb;col" vector(3), metadata jsonb)')
            cur.execute(
                "SELECT theodb.import_pinecone('\"weird;name\"'::regclass, %s::jsonb, 'id', 'emb;col', 'metadata')",
                ('[{"id":"x","values":[1,2,3],"metadata":{}}]',))
            assert cur.fetchone()[0] == 1
            cur.execute('SELECT count(*) FROM "weird;name"')
            assert cur.fetchone()[0] == 1
    finally:
        conn.close()


def test_import_pinecone_dim_mismatch(admin_conn):
    """A values array of the wrong length raises a typed error (vector typmod), no corrupt insert (EC-2)."""
    conn = _fresh_db_with_ext(admin_conn, "m16_import")
    try:
        with conn.cursor() as cur:
            cur.execute("CREATE TABLE t3 (id text PRIMARY KEY, embedding vector(3), metadata jsonb)")
            conn.commit()
            with pytest.raises(psycopg2.errors.DataException):  # vector dim mismatch (22xxx)
                cur.execute("SELECT theodb.import_pinecone('t3'::regclass, %s::jsonb)",
                            ('[{"id":"a","values":[1,2,3,4,5]}]',))  # 5 != 3
            conn.rollback()
            cur.execute("SELECT count(*) FROM t3")
            assert cur.fetchone()[0] == 0
    finally:
        conn.close()


# ---- T2.2 / T3.1 — docs (pure-file + runnable SQL) -------------------------------------------------

def test_migrate_doc_exists_and_mentions_function():
    assert MIGRATE_DOC.exists(), "docs/migrate-from-pinecone.md missing"
    text = MIGRATE_DOC.read_text()
    assert "theodb.import_pinecone" in text
    assert "embedding" in text and "metadata" in text


def test_demo_doc_has_no_perf_claim():
    """The 1-vs-2 demo measures simplicity/consistency, NEVER speed (ADR 0005 / public-copy.md)."""
    assert DEMO_DOC.exists(), "docs/unification-1-vs-2-systems.md missing"
    text = DEMO_DOC.read_text().lower()
    banned = ["faster", "x speedup", "lower latency", "higher qps", "outperform", "ms p95", "throughput win"]
    hits = [b for b in banned if b in text]
    assert not hits, f"demo doc contains a performance claim (ADR 0005 forbids): {hits}"
    assert "<=>" in DEMO_DOC.read_text(), "demo doc should show the unified SQL"


def test_quickstart_has_unified_section():
    text = QUICKSTART.read_text()
    assert "Unified query" in text
