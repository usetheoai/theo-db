"""M21 integration + recall-parity gate — TheoDB's own HNSW/IVFFlat ANN search vs pgvector, against the container.

Asserts the own `theodb.hnsw_knn` / `theodb.ivfflat_knn` SQL functions (built in Rust, theodb_rs M21):
  - return correct top-k (high recall@k vs an exact brute-force ground truth) — the functional proof,
  - reach recall@k PARITY with pgvector's own hnsw/ivfflat indexes within a tolerance band (ADR D2) — the gate,
  - fail-fast with typed SQLSTATE 22023 on the negative cases (bad metric, dim mismatch, non-integer id_col,
    over-cap params),
  - skip NULL-vector rows (EC-1), return 0 rows for empty queries (EC-5), and are REVOKEd from PUBLIC.

Reuses the existing harness (`theodb_bench.recall`): NO harness rebuild. Requires the `theo-db` image
(theodb_rs + pgvector). Connection via PG* env (PGHOST/PGPORT/PGUSER/PGPASSWORD/PGDATABASE).
"""
import os

import numpy as np
import psycopg2
import pytest

from theodb_bench.recall import brute_force_ground_truth, recall_at_k

pytestmark = pytest.mark.integration

DIM = 16
N = 400
NQ = 30
K = 10
SEED = 1234
# Parity tolerance (ADR D2): own recall@k must be within TOL of pgvector at a matched strong operating point.
# HNSW build is order-dependent (blueprint Q5), so parity is a band, not bit-exact identity.
TOL = 0.05


def _conn():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "localhost"), port=os.environ.get("PGPORT", "5432"),
        user=os.environ.get("PGUSER", "postgres"), password=os.environ.get("PGPASSWORD", "postgres"),
        dbname=os.environ.get("PGDATABASE", "postgres"),
    )


def _vec_lit(v) -> str:
    return "[" + ",".join(repr(float(x)) for x in v) + "]"


@pytest.fixture(scope="module")
def data():
    rng = np.random.default_rng(SEED)
    corpus = rng.standard_normal((N, DIM)).astype(np.float32)
    queries = rng.standard_normal((NQ, DIM)).astype(np.float32)
    return corpus, queries


@pytest.fixture(scope="module")
def conn(data):
    corpus, _ = data
    c = _conn()
    c.autocommit = True
    with c.cursor() as cur:
        cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
        cur.execute("CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE")
        cur.execute("DROP TABLE IF EXISTS m21_corpus")
        cur.execute(f"CREATE TABLE m21_corpus (id integer PRIMARY KEY, embedding vector({DIM}))")
        cur.executemany(
            "INSERT INTO m21_corpus (id, embedding) VALUES (%s, %s::vector)",
            [(i, _vec_lit(v)) for i, v in enumerate(corpus)],
        )
    yield c
    with c.cursor() as cur:
        cur.execute("DROP TABLE IF EXISTS m21_corpus")
    c.close()


def _own_knn(cur, fn: str, queries, k: int, extra: str):
    """Call theodb.<fn> with a batch of queries; return run_distances (list length Q of distance lists)."""
    qlits = [_vec_lit(q) for q in queries]
    cur.execute(
        f"SELECT query_idx, distance FROM theodb.{fn}"
        f"('m21_corpus'::regclass, 'embedding', %s::vector[], {extra}) ORDER BY query_idx, distance",
        (qlits,),
    )
    per = [[] for _ in range(len(queries))]
    for qi, d in cur.fetchall():
        per[qi].append(float(d))
    return per


def test_hnsw_knn_recall_high_vs_bruteforce(conn, data):
    corpus, queries = data
    _, true_d = brute_force_ground_truth(corpus, queries, K, metric="l2")
    with conn.cursor() as cur:
        run = _own_knn(cur, "hnsw_knn", queries, K, "k => 10, m => 16, ef_construction => 64, ef_search => 120, metric => 'l2'")
    r = recall_at_k(true_d, run, K)
    assert r >= 0.90, f"own HNSW recall@{K} = {r} < 0.90"


def test_ivfflat_knn_recall_high_vs_bruteforce(conn, data):
    corpus, queries = data
    _, true_d = brute_force_ground_truth(corpus, queries, K, metric="l2")
    with conn.cursor() as cur:
        run = _own_knn(cur, "ivfflat_knn", queries, K, "k => 10, lists => 16, probes => 16, metric => 'l2'")
    r = recall_at_k(true_d, run, K)
    assert r >= 0.90, f"own IVFFlat recall@{K} = {r} < 0.90"


def test_recall_parity_gate(conn, data):
    """The M21 gate (ADR D2/D3): own HNSW recall@k >= pgvector hnsw recall@k - TOL at a matched ef_search.
    On FAIL this is the honest anti-sunk-cost signal (keep pgvector); the assertion makes the gap loud."""
    corpus, queries = data
    _, true_d = brute_force_ground_truth(corpus, queries, K, metric="l2")
    ef = 120
    # pgvector arm
    with conn.cursor() as cur:
        cur.execute("DROP INDEX IF EXISTS m21_pgv_hnsw")
        cur.execute("CREATE INDEX m21_pgv_hnsw ON m21_corpus USING hnsw (embedding vector_l2_ops) WITH (m=16, ef_construction=64)")
        cur.execute(f"SET hnsw.ef_search = {ef}")
        cur.execute("SET enable_seqscan = off")
        pg_run = []
        for q in queries:
            cur.execute("SELECT embedding <-> %s::vector FROM m21_corpus ORDER BY embedding <-> %s::vector LIMIT %s",
                        (_vec_lit(q), _vec_lit(q), K))
            pg_run.append([float(r[0]) for r in cur.fetchall()])
        cur.execute("SET enable_seqscan = on")
        cur.execute("DROP INDEX IF EXISTS m21_pgv_hnsw")
    pg_recall = recall_at_k(true_d, pg_run, K)
    # own arm
    with conn.cursor() as cur:
        own_run = _own_knn(cur, "hnsw_knn", queries, K, f"k => 10, m => 16, ef_construction => 64, ef_search => {ef}, metric => 'l2'")
    own_recall = recall_at_k(true_d, own_run, K)
    assert own_recall >= pg_recall - TOL, (
        f"PARITY GATE FAIL (anti-sunk-cost signal): own HNSW recall@{K}={own_recall:.4f} < "
        f"pgvector {pg_recall:.4f} - {TOL}. Keep pgvector + record honest ADR."
    )


@pytest.mark.parametrize("fn,extra", [
    ("hnsw_knn", "metric => 'nope'"),
    ("ivfflat_knn", "metric => 'nope'"),
])
def test_knn_bad_metric_raises_22023(conn, data, fn, extra):
    _, queries = data
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.Error) as exc:
            _own_knn(cur, fn, queries[:1], K, extra)
        assert exc.value.pgcode == "22023", f"{fn} bad metric must be 22023, got {exc.value.pgcode}"


def test_knn_dim_mismatch_raises_22023(conn):
    bad = np.zeros((1, DIM + 3), dtype=np.float32)  # query dim != corpus dim
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.Error) as exc:
            _own_knn(cur, "hnsw_knn", bad, K, "k => 5, ef_search => 40, metric => 'l2'")
        assert exc.value.pgcode == "22023", f"dim mismatch must be 22023, got {exc.value.pgcode}"


def test_knn_non_integer_id_col_raises_22023(conn, data):
    _, queries = data
    with conn.cursor() as cur:
        cur.execute("DROP TABLE IF EXISTS m21_textid")
        cur.execute(f"CREATE TABLE m21_textid (id text PRIMARY KEY, embedding vector({DIM}))")
        cur.execute("INSERT INTO m21_textid VALUES ('a', %s::vector)", (_vec_lit(np.zeros(DIM)),))
        with pytest.raises(psycopg2.Error) as exc:
            qlits = [_vec_lit(queries[0])]
            cur.execute(
                "SELECT query_idx FROM theodb.hnsw_knn('m21_textid'::regclass, 'embedding', %s::vector[], "
                "k => 5, ef_search => 40, metric => 'l2')", (qlits,))
            cur.fetchall()
        assert exc.value.pgcode == "22023", f"non-integer id_col must be 22023, got {exc.value.pgcode}"
        cur.execute("DROP TABLE IF EXISTS m21_textid")


def test_knn_param_over_cap_raises_22023(conn, data):
    _, queries = data
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.Error) as exc:
            _own_knn(cur, "hnsw_knn", queries[:1], K, "k => 5, ef_construction => 2000000, ef_search => 40, metric => 'l2'")
        assert exc.value.pgcode == "22023", f"over-cap param must be 22023, got {exc.value.pgcode}"


@pytest.mark.parametrize("fn,extra", [
    ("hnsw_knn", "k => 0, ef_search => 40, metric => 'l2'"),            # k < 1
    ("hnsw_knn", "k => 5, m => 1, ef_search => 40, metric => 'l2'"),    # m < 2
    ("hnsw_knn", "k => 5, m => 101, ef_search => 40, metric => 'l2'"),  # m > 100
    ("hnsw_knn", "k => 50, ef_construction => 10, ef_search => 60, metric => 'l2'"),  # ef_construction < k
    ("hnsw_knn", "k => 50, ef_search => 10, metric => 'l2'"),           # ef_search < k
    ("ivfflat_knn", "k => 5, lists => 0, probes => 1, metric => 'l2'"),   # lists < 1
    ("ivfflat_knn", "k => 5, lists => 8, probes => 0, metric => 'l2'"),   # probes < 1
])
def test_knn_param_lower_bounds_raise_22023(conn, data, fn, extra):
    """EC-2 lower-bound caps: every out-of-range knob fails fast with the specific 22023."""
    _, queries = data
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.Error) as exc:
            _own_knn(cur, fn, queries[:1], K, extra)
        assert exc.value.pgcode == "22023", f"{fn} {extra} must be 22023, got {exc.value.pgcode}"


def test_knn_inconsistent_vector_dims_raises_22023(conn, data):
    """A corpus with mixed vector dimensions fails fast with 22023 (not a panic / wrong answer)."""
    _, queries = data
    with conn.cursor() as cur:
        cur.execute("DROP TABLE IF EXISTS m21_mixdim")
        # No typmod → column accepts any-dim vectors, so we can store inconsistent dims.
        cur.execute("CREATE TABLE m21_mixdim (id integer PRIMARY KEY, embedding vector)")
        cur.execute("INSERT INTO m21_mixdim VALUES (1, %s::vector), (2, %s::vector)",
                    (_vec_lit(np.ones(DIM)), _vec_lit(np.ones(DIM + 4))))
        with pytest.raises(psycopg2.Error) as exc:
            qlits = [_vec_lit(np.ones(DIM))]
            cur.execute(
                "SELECT query_idx FROM theodb.hnsw_knn('m21_mixdim'::regclass, 'embedding', %s::vector[], "
                "k => 5, ef_search => 40, metric => 'l2')", (qlits,))
            cur.fetchall()
        assert exc.value.pgcode == "22023", f"inconsistent dims must be 22023, got {exc.value.pgcode}"
        cur.execute("DROP TABLE IF EXISTS m21_mixdim")


def test_knn_empty_table_returns_zero_rows(conn, data):
    """An empty source table returns 0 rows (no panic) — the build-over-nothing edge."""
    _, queries = data
    with conn.cursor() as cur:
        cur.execute("DROP TABLE IF EXISTS m21_empty")
        cur.execute(f"CREATE TABLE m21_empty (id integer PRIMARY KEY, embedding vector({DIM}))")
        qlits = [_vec_lit(queries[0])]
        cur.execute(
            "SELECT count(*) FROM theodb.hnsw_knn('m21_empty'::regclass, 'embedding', %s::vector[], "
            "k => 5, ef_search => 40, metric => 'l2')", (qlits,))
        assert cur.fetchone()[0] == 0
        cur.execute("DROP TABLE IF EXISTS m21_empty")


@pytest.mark.parametrize("bad_col", ["embedding; DROP TABLE m21_corpus; --", "e'); DROP TABLE m21_corpus; --", "embedding OR 1=1"])
def test_knn_injection_in_column_name_rejected(conn, data, bad_col):
    """A hostile embed_col identifier is rejected by the allowlist (22023) — and the corpus survives."""
    _, queries = data
    with conn.cursor() as cur:
        with pytest.raises(psycopg2.Error) as exc:
            qlits = [_vec_lit(queries[0])]
            cur.execute(
                "SELECT query_idx FROM theodb.hnsw_knn('m21_corpus'::regclass, %s, %s::vector[], "
                "k => 5, ef_search => 40, metric => 'l2')", (bad_col, qlits))
            cur.fetchall()
        assert exc.value.pgcode == "22023", f"injection ident must be 22023, got {exc.value.pgcode}"
    # the corpus table must still exist (the malicious DDL never executed)
    with conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM m21_corpus")
        assert cur.fetchone()[0] == N


def test_knn_empty_queries_returns_zero_rows(conn):
    with conn.cursor() as cur:
        cur.execute(
            "SELECT count(*) FROM theodb.hnsw_knn('m21_corpus'::regclass, 'embedding', ARRAY[]::vector[], "
            "k => 5, ef_search => 40, metric => 'l2')")
        assert cur.fetchone()[0] == 0


def test_knn_skips_null_vector_rows(conn, data):
    _, queries = data
    with conn.cursor() as cur:
        cur.execute("DROP TABLE IF EXISTS m21_withnull")
        cur.execute(f"CREATE TABLE m21_withnull (id integer PRIMARY KEY, embedding vector({DIM}))")
        cur.execute("INSERT INTO m21_withnull VALUES (1, %s::vector), (2, NULL), (3, %s::vector)",
                    (_vec_lit(np.ones(DIM)), _vec_lit(np.zeros(DIM))))
        qlits = [_vec_lit(np.ones(DIM))]
        cur.execute(
            "SELECT id, distance FROM theodb.hnsw_knn('m21_withnull'::regclass, 'embedding', %s::vector[], "
            "k => 5, ef_search => 40, metric => 'l2') ORDER BY distance", (qlits,))
        ids = [r[0] for r in cur.fetchall()]
        assert 2 not in ids, "NULL-vector row (id 2) must be skipped"
        assert set(ids) == {1, 3}, f"expected the 2 non-null rows, got {ids}"
        cur.execute("DROP TABLE IF EXISTS m21_withnull")


@pytest.mark.parametrize("sig", [
    "theodb.hnsw_knn(regclass, text, vector[], int, int, int, int, text, text, bigint)",
    "theodb.ivfflat_knn(regclass, text, vector[], int, int, int, text, text, bigint)",
    "theodb_rs._hnsw_knn(text, text, text, text, real[], int, int, int, int, int, bigint)",
    "theodb_rs._ivfflat_knn(text, text, text, text, real[], int, int, int, int, bigint)",
])
def test_knn_revoked_from_public(conn, sig):
    with conn.cursor() as cur:
        cur.execute("SELECT has_function_privilege('public', %s, 'execute')", (sig,))
        assert cur.fetchone()[0] is False, f"{sig} must be REVOKEd from PUBLIC"
