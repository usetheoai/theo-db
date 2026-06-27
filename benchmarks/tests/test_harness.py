"""Unit tests for the harness orchestration (FakeVectorDB injected — no container)."""
from theodb_bench.__main__ import build_config, build_parser
from theodb_bench.harness import run_benchmark


class FakeVectorDB:
    """In-memory stand-in: query_topk returns zero distances -> recall@k == 1.0."""

    def ensure_extension(self):
        pass

    def create_table(self, *a, **k):
        pass

    def load_vectors(self, *a, **k):
        pass

    def build_index(self, ddl):
        return 0.005

    def set_session(self, stmt):
        pass

    def assert_index_used(self, *a, **k):
        pass

    def index_size_bytes(self, name):
        return 4096

    def query_topk(self, table, q, k, metric, embed_col="embedding"):
        return list(range(k)), [0.0] * k, 0.001


_CONFIG = {
    "seed": 1,
    "n": 50,
    "dim": 8,
    "n_queries": 10,
    "k": 5,
    "metric": "l2",
    "runs": 2,
    "table": "t",
    "index_specs": [
        {"name": "hnsw", "index_name": "i", "ddl": "CREATE INDEX i ON t USING hnsw (embedding vector_l2_ops)"}
    ],
}


class MissVectorDB(FakeVectorDB):
    """query_topk returns far distances -> every result misses -> recall@k == 0.0."""

    def query_topk(self, table, q, k, metric, embed_col="embedding"):
        return list(range(k)), [1e9] * k, 0.001


def test_runner_recall_reflects_misses(tmp_path):
    # proves the harness wiring is NOT hardcoded to 1.0 — it propagates real misses.
    report = run_benchmark(_CONFIG, MissVectorDB(), tmp_path)
    assert report["results"][0]["recall_at_k"] == 0.0


def test_report_persists_config_values(tmp_path):
    import json

    report = run_benchmark(_CONFIG, FakeVectorDB(), tmp_path)
    written = json.loads(list(tmp_path.glob("*.json"))[0].read_text())
    assert written["seed"] == _CONFIG["seed"]
    assert written["n"] == _CONFIG["n"]
    assert written["n_queries"] == _CONFIG["n_queries"]
    assert written["results"][0]["recall_at_k"] == report["results"][0]["recall_at_k"]


def test_runner_with_fake_db_emits_report(tmp_path):
    report = run_benchmark(_CONFIG, FakeVectorDB(), tmp_path)
    r = report["results"][0]
    assert 0.0 <= r["recall_at_k"] <= 1.0
    assert r["recall_at_k"] == 1.0  # zero-distance fake -> perfect recall
    assert r["qps"] > 0
    assert list(tmp_path.glob("*.json"))
    assert list(tmp_path.glob("*.md"))


def test_report_json_schema(tmp_path):
    report = run_benchmark(_CONFIG, FakeVectorDB(), tmp_path)
    for key in ("sha", "seed", "n", "dim", "k", "metric", "runs", "results"):
        assert key in report
    for key in ("index", "recall_at_k", "qps", "build_ms", "index_bytes", "p50", "p95", "p99"):
        assert key in report["results"][0]


def test_main_end_to_end_with_fake(tmp_path, monkeypatch):
    from theodb_bench import __main__ as cli

    class FakeConnDB(FakeVectorDB):
        def __init__(self, dsn):
            pass

        def connect(self):
            return self

        def ping(self):
            pass

        def close(self):
            pass

    monkeypatch.setattr(cli, "VectorDB", FakeConnDB)
    rc = cli.main(
        ["--seed", "1", "--n", "40", "--dim", "8", "--n-queries", "5",
         "--k", "5", "--runs", "2", "--metric", "l2", "--out", str(tmp_path)]
    )
    assert rc == 0
    assert list(tmp_path.glob("*.json"))


def test_cli_parses_args():
    args = build_parser().parse_args(["--seed", "7", "--k", "3", "--metric", "cosine"])
    assert args.seed == 7
    assert args.k == 3
    cfg = build_config(args)
    assert cfg["seed"] == 7 and cfg["k"] == 3 and cfg["metric"] == "cosine"
