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
import re
import socket
import subprocess
import sys
import time
import urllib.request

import psycopg2
import pytest

_REPO = pathlib.Path(__file__).resolve().parents[2]


def _free_port():
    s = socket.socket()
    s.bind(("", 0))
    p = s.getsockname()[1]
    s.close()
    return p


@pytest.fixture(scope="module")
def chat_server():
    """Deterministic OpenAI-compatible stub (tools/chat_server.py) reached from the container via
    host.docker.internal — the container must be run with --add-host=host.docker.internal:host-gateway."""
    port = _free_port()
    proc = subprocess.Popen(
        [sys.executable, str(_REPO / "tools" / "chat_server.py"), "--host", "0.0.0.0", "--port", str(port)],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(60):
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2) as r:
                    if r.status == 200:
                        break
            except OSError:
                time.sleep(0.5)
        else:
            raise RuntimeError("chat stub did not become healthy")
        yield f"http://host.docker.internal:{port}/v1/chat/completions"
    finally:
        proc.terminate()
        proc.wait(timeout=10)

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
        # Install theodb_rs (the Rust extension) CASCADE: it `requires = theodb`, so this pulls the umbrella
        # theodb (and vector/vectorscale) first, then layers the Rust surface — ai._chat / ai.summarize /
        # theodb.embed live in theodb_rs since M18/M19. Mirrors the container's docker-entrypoint-initdb.d
        # (`CREATE EXTENSION theodb_rs CASCADE`). `CREATE EXTENSION theodb` alone would lack the ai.* generative
        # surface (it no longer carries the plpython3u ai._chat), so the +AI leg of the unified moat would 42883.
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
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


def test_unified_query_with_ai_leg(admin_conn, chat_server):
    """The FULL unification: vector ORDER BY + relational JOIN + WHERE + ai.* in one transactional SQL.
    Proves the '+AI' third of the moat — ai.summarize routes through the stub (prefix 'A concise summary')."""
    conn = _fresh_db_with_ext(admin_conn, "m16_unified")
    try:
        with conn.cursor() as cur:
            cur.execute("SET theodb.llm_endpoint = %s", (chat_server,))
            cur.execute("SET theodb.llm_model = 'stub-chat'")
            cur.execute("""
                CREATE TABLE products (id text PRIMARY KEY, description text, category_id int, embedding vector(8));
                CREATE TABLE inventory (product_id text PRIMARY KEY, in_stock bool);
            """)
            cur.execute("INSERT INTO products VALUES ('p1','red running shoes',3,%s),('p2','blue boots',3,%s)",
                        (_vec([0.99] + [0] * 7), _vec([0] + [1] + [0] * 6)))
            cur.execute("INSERT INTO inventory VALUES ('p1',true),('p2',true)")
            # vector + JOIN + WHERE + AI, one statement
            cur.execute("""
                SELECT p.id, ai.summarize(p.description) AS gist
                FROM products p JOIN inventory i ON i.product_id = p.id
                WHERE i.in_stock AND p.category_id = 3
                ORDER BY p.embedding <=> %s::vector
                LIMIT 1
            """, (_vec([1] + [0] * 7),))
            row = cur.fetchone()
            assert row[0] == "p1"                                  # vector+relational legs
            assert row[1].startswith("A concise summary")          # AI leg actually ran (stub routing)
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

def test_migrate_doc_runnable_sql_executes(admin_conn):
    """The migration guide's SQL actually runs against the container (T2.2) — catches a broken example
    (e.g. a dim-mismatch) that a string grep would miss. Runs the runnable ```sql blocks in order."""
    assert MIGRATE_DOC.exists(), "docs/migrate-from-pinecone.md missing"
    text = MIGRATE_DOC.read_text()
    assert "theodb.import_pinecone" in text  # the guide must teach the function
    blocks = re.findall(r"```sql\n(.*?)```", text, re.DOTALL)
    assert blocks, "migration guide has no runnable SQL block"
    conn = _fresh_db_with_ext(admin_conn, "m16_import")
    try:
        with conn.cursor() as cur:
            for b in blocks:
                if "..." in b or "<" in b:   # skip illustrative blocks with placeholders
                    continue
                cur.execute(b)               # must execute without error (real dims, real signature)
            conn.commit()
    finally:
        conn.close()


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
