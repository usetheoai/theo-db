"""Integration tests — run against a real theo-db:dev container (pgvector).

Marked `integration`; skipped by `pytest -m "not integration"`. Connection from PG* env vars
(PGHOST/PGPORT/PGUSER/PGPASSWORD/PGDATABASE).
"""
import os

import pytest

from theodb_bench.db import IndexNotUsedError, VectorDB
from theodb_bench.dataset import make_dataset
from theodb_bench.harness import run_benchmark

pytestmark = pytest.mark.integration


def _dsn() -> str:
    return (
        f"host={os.environ.get('PGHOST', 'localhost')} "
        f"port={os.environ.get('PGPORT', '5432')} "
        f"dbname={os.environ.get('PGDATABASE', 'postgres')} "
        f"user={os.environ.get('PGUSER', 'postgres')} "
        f"password={os.environ.get('PGPASSWORD', 'postgres')}"
    )


@pytest.fixture
def db():
    d = VectorDB(_dsn()).connect()
    d.ping()
    yield d
    d.close()


def test_extension_load_and_topk_query(db):
    db.ensure_extension()
    db.create_table("it_vectors", 8)
    corpus, queries = make_dataset(300, 8, 5, seed=3)
    db.load_vectors("it_vectors", corpus)
    db.build_index("CREATE INDEX it_hnsw ON it_vectors USING hnsw (embedding vector_l2_ops)")
    # Force the index on (pgvector recall-test methodology) — on a small table the planner would
    # otherwise pick a seqscan, and we want to exercise the index path.
    db.set_session("SET enable_seqscan = off")
    db.set_session("SET hnsw.ef_search = 100")
    db.assert_index_used("it_vectors", queries[0], 5, "l2")
    ids, dists, latency = db.query_topk("it_vectors", queries[0], 5, "l2")
    assert len(ids) == 5
    assert latency > 0
    assert db.index_size_bytes("it_hnsw") > 0


def test_index_not_used_raises(db):
    db.ensure_extension()
    db.create_table("it_seq", 8)
    corpus, queries = make_dataset(100, 8, 2, seed=5)
    db.load_vectors("it_seq", corpus)
    db.build_index("CREATE INDEX it_seq_hnsw ON it_seq USING hnsw (embedding vector_l2_ops)")
    db.set_session("SET enable_indexscan = off")  # force the planner away from the index
    db.set_session("SET enable_bitmapscan = off")
    with pytest.raises(IndexNotUsedError):
        db.assert_index_used("it_seq", queries[0], 5, "l2")


def test_vectorscale_extension_and_diskann_index(db):
    db.ensure_extension()
    db.set_session("CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE")
    db.create_table("it_scale", 32)
    corpus, queries = make_dataset(500, 32, 3, seed=9)
    db.load_vectors("it_scale", corpus)
    db.build_index("CREATE INDEX it_scale_dann ON it_scale USING diskann (embedding vector_cosine_ops)")
    db.set_session("SET enable_seqscan = off")
    db.set_session("SET diskann.query_search_list_size = 100")
    db.assert_index_used("it_scale", queries[0], 5, "cosine")  # diskann index actually used
    ids, dists, latency = db.query_topk("it_scale", queries[0], 5, "cosine")
    assert len(ids) == 5
    assert latency > 0
    assert db.index_size_bytes("it_scale_dann") > 0


def test_harness_measures_diskann(db, tmp_path):
    from theodb_bench.__main__ import build_config, build_parser
    from theodb_bench.harness import run_benchmark

    db.ensure_extension()
    db.set_session("CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE")  # self-sufficient (no test-ordering dep)
    args = build_parser().parse_args(
        ["--index", "diskann", "--metric", "cosine", "--seed", "7",
         "--n", "2000", "--dim", "32", "--n-queries", "30", "--k", "10", "--runs", "2"]
    )
    report = run_benchmark(build_config(args), db, tmp_path)
    diskann = [r for r in report["results"] if r["index"] == "diskann"]
    assert diskann, "harness produced no diskann results"
    assert all(0.0 <= r["recall_at_k"] <= 1.0 and r["qps"] > 0 for r in diskann)
    # at high sls on low dim, diskann/SBQ reaches high recall (rescore scales with sls up to the
    # engine ceiling, so the curve climbs to the plan's >= 0.90 acceptance bound)
    assert max(r["recall_at_k"] for r in diskann) >= 0.90


def test_diskann_reaches_scann_quality_recall(db, tmp_path):
    # M14 fork-decision evidence: DiskANN (StreamingDiskANN, the shipped permissive ANN) must reach the
    # ScaNN-quality bar (recall@10 >= 0.90 at usable QPS). This is the measurable gate the no-fork decision
    # (wiki/decisions/0004) rests on — if it ever fails, the fork-trigger re-opens (PRD fork-gate policy).
    from theodb_bench.__main__ import build_config, build_parser
    from theodb_bench.harness import run_benchmark

    SCANN_QUALITY_BAR = 0.90
    db.ensure_extension()
    db.set_session("CREATE EXTENSION IF NOT EXISTS vectorscale CASCADE")  # diskann AM precondition (self-sufficient)
    args = build_parser().parse_args(
        ["--index", "diskann", "--metric", "cosine", "--seed", "14",
         "--n", "3000", "--dim", "32", "--n-queries", "50", "--k", "10", "--runs", "2"]
    )
    report = run_benchmark(build_config(args), db, tmp_path)
    diskann = [r for r in report["results"] if r["index"] == "diskann"]
    assert diskann, "no diskann results"
    assert all(0.0 <= r["recall_at_k"] <= 1.0 and r["qps"] > 0 for r in diskann)
    assert max(r["recall_at_k"] for r in diskann) >= SCANN_QUALITY_BAR, (
        f"DiskANN below the ScaNN-quality bar {SCANN_QUALITY_BAR} — fork-trigger would re-open: {diskann}"
    )


def test_harness_measures_ivfflat(db, tmp_path):
    # M9: IVFFlat (pgvector) validated in the recall harness — features 03/04.
    from theodb_bench.__main__ import build_config, build_parser
    from theodb_bench.harness import run_benchmark

    args = build_parser().parse_args(
        ["--index", "ivfflat", "--metric", "l2", "--seed", "11",
         "--n", "2000", "--dim", "32", "--n-queries", "30", "--k", "10", "--runs", "2"]
    )
    report = run_benchmark(build_config(args), db, tmp_path)
    ivf = [r for r in report["results"] if r["index"] == "ivfflat"]
    assert ivf, "harness produced no ivfflat results"
    assert all(0.0 <= r["recall_at_k"] <= 1.0 and r["qps"] > 0 for r in ivf)
    assert all(r["build_ms"] > 0 and r["index_bytes"] > 0 for r in ivf)
    # The probes sweep is ascending; recall must be non-decreasing across it, and at probes=lists
    # (all lists scanned) IVFFlat is exact-among-indexed (flat storage) -> high recall.
    recalls = [r["recall_at_k"] for r in ivf]
    assert recalls == sorted(recalls), f"recall not monotonic across probes sweep: {recalls}"
    assert max(recalls) >= 0.90  # probes=lists scans all clusters -> ~exact recall


def test_hnsw_recall_is_high_vs_exact(db, tmp_path):
    config = {
        "seed": 7, "n": 2000, "dim": 32, "n_queries": 50, "k": 10, "metric": "l2", "runs": 3,
        "table": "it_bench",
        "index_specs": [
            {
                "name": "hnsw", "index_name": "it_bench_hnsw",
                "ddl": "CREATE INDEX it_bench_hnsw ON it_bench USING hnsw (embedding vector_l2_ops)",
                "sweep": [{"label": "ef_search=100", "session": ["SET enable_seqscan = off", "SET hnsw.ef_search = 100"]}],
            }
        ],
    }
    report = run_benchmark(config, db, tmp_path)
    r = report["results"][0]
    assert 0.0 <= r["recall_at_k"] <= 1.0
    assert r["recall_at_k"] >= 0.90  # HNSW vs exact ground-truth should recall high
    assert r["qps"] > 0
    assert r["build_ms"] > 0
    assert r["index_bytes"] > 0


# --- M7-S1: ai.hybrid_search_rrf contract (FTS + vector + RRF) -----------------------------------
# Deterministic: explicit query_vector (no embedding endpoint call). vector(3) toy space.
import psycopg2  # noqa: E402


def _raw_conn():
    c = psycopg2.connect(_dsn())
    c.autocommit = True
    return c


def _seed_documents(cur, table: str, rows: list[tuple]) -> None:
    """rows = [(doc_id, content, embedding_or_None)]. tsv is GENERATED from content (english)."""
    cur.execute(f"DROP TABLE IF EXISTS {table}")
    cur.execute(
        f"CREATE TABLE {table} ("
        f"  doc_id text PRIMARY KEY,"
        f"  content text,"
        f"  text_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', coalesce(content,''))) STORED,"
        f"  embedding vector(3))"
    )
    cur.execute(f"CREATE INDEX {table}_gin ON {table} USING gin (text_tsv)")
    for doc_id, content, emb in rows:
        cur.execute(
            f"INSERT INTO {table}(doc_id, content, embedding) VALUES (%s, %s, %s)",
            (doc_id, content, emb),
        )


def _hybrid(cur, table, *, query_text=None, query_vector=None, k=60, per_leg_limit=20, result_limit=5):
    cur.execute(
        "SELECT id, score FROM ai.hybrid_search_rrf("
        "  tbl => %s::regclass, id_col => 'doc_id', content_tsv_col => 'text_tsv', vector_col => 'embedding',"
        "  query_text => %s, query_vector => %s, k => %s, per_leg_limit => %s, result_limit => %s)",
        (table, query_text, query_vector, k, per_leg_limit, result_limit),
    )
    return cur.fetchall()


def test_hybrid_fuses_both_legs():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_both", [
                ("d1", "database systems", "[1,0,0]"),   # matches FTS 'database' AND near query vector
                ("d2", "database tuning",  "[0,1,0]"),   # matches FTS 'database', far vector
                ("d3", "cooking recipes",  "[1,0,0]"),   # near query vector, no FTS 'database' match
            ])
            rows = _hybrid(cur, "hyb_both", query_text="database", query_vector="[1,0,0]", result_limit=5)
            ids = [r[0] for r in rows]
            assert ids[0] == "d1", f"both-legs doc must rank first, got {rows}"
            assert set(ids) == {"d1", "d2", "d3"}, f"all docs surface via fusion, got {ids}"
    finally:
        conn.close()


def test_hybrid_search_json_matches_rrf():
    # M13: the literal ai.hybrid_search(jsonb) wrapper returns the SAME rows as ai.hybrid_search_rrf.
    import json as _json
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_json", [
                ("d1", "database systems", "[1,0,0]"),
                ("d2", "database tuning",  "[0,1,0]"),
                ("d3", "cooking recipes",  "[1,0,0]"),
            ])
            explicit = _hybrid(cur, "hyb_json", query_text="database", query_vector="[1,0,0]", result_limit=5)
            cfg = {"table": "hyb_json", "id_col": "doc_id", "content_tsv_col": "text_tsv",
                   "vector_col": "embedding", "query_text": "database", "query_vector": "[1,0,0]",
                   "result_limit": 5}
            cur.execute("SELECT id, score FROM ai.hybrid_search(%s::jsonb)", (_json.dumps(cfg),))
            via_json = cur.fetchall()
            assert via_json == explicit, f"JSON wrapper must match rrf: {via_json} != {explicit}"
    finally:
        conn.close()


def _hybrid_json(cur, cfg):
    import json as _json
    cur.execute("SELECT id, score FROM ai.hybrid_search(%s::jsonb)", (_json.dumps(cfg),))
    return cur.fetchall()


def test_hybrid_search_json_weight_changes_ranking():
    # M106: per-leg weights (vector_weight/text_weight) skew the RRF fusion. Seed two docs where the vector
    # leg and the FTS leg each rank a DIFFERENT doc first; upweighting a leg must lift ITS top doc to #1.
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            # Single-leg docs so the weight alone decides: dv only in the vector leg (embedding, no 'database');
            # df only in the FTS leg (NULL embedding → excluded from the vector leg, has 'database').
            _seed_documents(cur, "hyb_w", [
                ("dv", "unrelated lexical words", "[1,0,0]"),   # vector leg #1; NO 'database' FTS
                ("df", "database database database", None),      # FTS leg #1; NULL embedding → not in vector leg
            ])
            base = {"table": "hyb_w", "id_col": "doc_id", "content_tsv_col": "text_tsv",
                    "vector_col": "embedding", "query_text": "database", "query_vector": "[1,0,0]",
                    "result_limit": 5}
            # default (1,1) must equal explicit (1,1) — backward-compat
            assert _hybrid_json(cur, base) == _hybrid_json(cur, {**base, "vector_weight": 1, "text_weight": 1})
            # upweight the VECTOR leg -> its top doc (dv) must rank first
            vec_first = [r[0] for r in _hybrid_json(cur, {**base, "vector_weight": 3, "text_weight": 1})]
            assert vec_first[0] == "dv", f"vector_weight should lift dv to #1, got {vec_first}"
            # upweight the TEXT leg -> its top doc (df) must rank first (ranking FLIPPED by weight)
            txt_first = [r[0] for r in _hybrid_json(cur, {**base, "vector_weight": 1, "text_weight": 3})]
            assert txt_first[0] == "df", f"text_weight should lift df to #1, got {txt_first}"
    finally:
        conn.close()


def test_hybrid_search_json_negative_weight_raises():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_wn", [("d1", "database", "[1,0,0]")])
            cfg = {"table": "hyb_wn", "id_col": "doc_id", "content_tsv_col": "text_tsv",
                   "vector_col": "embedding", "query_text": "database", "vector_weight": -1}
            import json as _json
            with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — weight must be >= 0
                cur.execute("SELECT * FROM ai.hybrid_search(%s::jsonb)", (_json.dumps(cfg),))
    finally:
        conn.close()


def test_hybrid_search_json_missing_keys_raises():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — missing required keys
                cur.execute("SELECT * FROM ai.hybrid_search('{}'::jsonb)")
    finally:
        conn.close()


def test_hybrid_empty_fts_leg():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_nofts", [
                ("d1", "cooking recipes", "[1,0,0]"),
                ("d2", "garden tools",    "[0,1,0]"),
            ])
            # query_text matches NO row via @@ → FTS leg empty; vector-only docs still returned.
            rows = _hybrid(cur, "hyb_nofts", query_text="zzznomatch", query_vector="[1,0,0]")
            ids = [r[0] for r in rows]
            assert "d1" in ids, f"vector-only doc must surface when FTS leg empty, got {rows}"
            assert all(r[1] > 0 for r in rows), "scores positive (vector leg contributes)"
    finally:
        conn.close()


def test_hybrid_empty_vector_leg():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            # embeddings are NULL → vector leg empty (WHERE embedding IS NOT NULL); FTS-only docs surface.
            _seed_documents(cur, "hyb_novec", [
                ("d1", "database systems", None),
                ("d2", "database tuning",  None),
            ])
            rows = _hybrid(cur, "hyb_novec", query_text="database", query_vector="[1,0,0]")
            ids = [r[0] for r in rows]
            assert set(ids) == {"d1", "d2"}, f"FTS-only docs must surface when vector leg empty, got {rows}"
    finally:
        conn.close()


def test_hybrid_invalid_k_raises():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_k0", [("d1", "database", "[1,0,0]")])
            with pytest.raises(psycopg2.errors.InvalidParameterValue):  # SQLSTATE 22023
                _hybrid(cur, "hyb_k0", query_text="database", query_vector="[1,0,0]", k=0)
    finally:
        conn.close()


def test_hybrid_k_param_changes_score():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_kp", [
                ("d1", "database systems", "[1,0,0]"),
                ("d2", "database tuning",  "[0,1,0]"),
            ])
            top_k1 = _hybrid(cur, "hyb_kp", query_text="database", query_vector="[1,0,0]", k=1)[0]
            top_k60 = _hybrid(cur, "hyb_kp", query_text="database", query_vector="[1,0,0]", k=60)[0]
            assert top_k1[0] == top_k60[0] == "d1"
            assert abs(top_k1[1] - top_k60[1]) > 1e-4, "k must change the fused score (param wired, not hardcoded)"
    finally:
        conn.close()


def test_three_retrievers_report_metrics(db):
    """M7-S1 T2.2: the 3-retriever BEIR-style eval reports finite nDCG@10 + Recall@100 (measured)."""
    from theodb_bench.beir import EMBED_DIM, lexical_embed, synthetic_dataset
    from theodb_bench.hybrid import run_three_retrievers

    db.ensure_extension()
    results = run_three_retrievers(db, synthetic_dataset(), lexical_embed, EMBED_DIM, table="it_hybrid_eval")
    assert set(results) == {"vector", "fts", "hybrid"}
    for name, m in results.items():
        assert 0.0 <= m["ndcg10"] <= 1.0, f"{name} nDCG@10 out of range: {m}"
        assert 0.0 <= m["recall100"] <= 1.0, f"{name} Recall@100 out of range: {m}"
    # the HYBRID leg specifically (the thing M7-S1 ships) must produce real signal — not vacuously
    # green because vector or fts happened to score.
    assert results["hybrid"]["ndcg10"] > 0, f"hybrid leg scored zero nDCG@10: {results}"
    assert results["hybrid"]["recall100"] > 0, f"hybrid leg scored zero Recall@100: {results}"


def test_hybrid_invalid_per_leg_limit_raises():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_pll", [("d1", "database", "[1,0,0]")])
            with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
                _hybrid(cur, "hyb_pll", query_text="database", query_vector="[1,0,0]", per_leg_limit=0)
    finally:
        conn.close()


def test_hybrid_invalid_result_limit_raises():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_rl", [("d1", "database", "[1,0,0]")])
            with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023
                _hybrid(cur, "hyb_rl", query_text="database", query_vector="[1,0,0]", result_limit=0)
    finally:
        conn.close()


def test_hybrid_both_query_args_null_raises():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            _seed_documents(cur, "hyb_null", [("d1", "database", "[1,0,0]")])
            with pytest.raises(psycopg2.errors.InvalidParameterValue):  # 22023 — need text and/or vector
                _hybrid(cur, "hyb_null", query_text=None, query_vector=None)
    finally:
        conn.close()


def test_hybrid_both_legs_empty_returns_no_rows():
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            # FTS term matches nothing AND all embeddings NULL -> both legs empty -> zero rows, no error.
            _seed_documents(cur, "hyb_empty", [
                ("d1", "cooking recipes", None),
                ("d2", "garden tools",    None),
            ])
            rows = _hybrid(cur, "hyb_empty", query_text="zzznomatch", query_vector="[1,0,0]")
            assert rows == [], f"both-legs-empty must return zero rows (not error), got {rows}"
    finally:
        conn.close()


def test_hybrid_unconfigured_endpoint_raises_typed_error():
    """Failure scenario: query_text only (no query_vector) with theodb.embedding_endpoint unset →
    the vector leg embeds via theodb.embed, which fails fast with a typed error (no silent green)."""
    conn = _raw_conn()
    try:
        with conn.cursor() as cur:
            cur.execute("RESET theodb.embedding_endpoint")  # ensure unset for this session
            _seed_documents(cur, "hyb_noep", [("d1", "database", "[1,0,0]")])
            # query_vector omitted -> function calls theodb.embed(query_text) -> 22023 (endpoint not set).
            with pytest.raises(psycopg2.errors.InvalidParameterValue):
                cur.execute(
                    "SELECT id, score FROM ai.hybrid_search_rrf("
                    "  tbl => 'hyb_noep'::regclass, id_col => 'doc_id', content_tsv_col => 'text_tsv',"
                    "  vector_col => 'embedding', query_text => 'database')"
                )
                cur.fetchall()
    finally:
        conn.close()
