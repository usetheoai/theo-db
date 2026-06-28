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


def test_build_config_hnsw_only_by_default():
    cfg = build_config(build_parser().parse_args([]))
    assert [s["name"] for s in cfg["index_specs"]] == ["hnsw"]


def test_build_config_diskann_only():
    cfg = build_config(build_parser().parse_args(["--index", "diskann"]))
    specs = cfg["index_specs"]
    assert [s["name"] for s in specs] == ["diskann"]
    assert "USING diskann" in specs[0]["ddl"]
    assert any("diskann.query_search_list_size" in s for sw in specs[0]["sweep"] for s in sw["session"])


def test_build_config_both_indexes():
    cfg = build_config(build_parser().parse_args(["--index", "both"]))
    assert [s["name"] for s in cfg["index_specs"]] == ["hnsw", "diskann"]


def test_build_config_ivfflat_only():
    # M9: --index ivfflat -> single ivfflat spec; lists = n/1000; probes clamped-then-deduped.
    cfg = build_config(build_parser().parse_args(["--index", "ivfflat", "--n", "5000"]))
    specs = cfg["index_specs"]
    assert [s["name"] for s in specs] == ["ivfflat"]
    assert "USING ivfflat" in specs[0]["ddl"] and "WITH (lists = 5)" in specs[0]["ddl"]
    # n=5000 -> lists=5; raw probes {1,10,5} clamped to {1,5,5} -> deduped {1,5}. Labels == executed.
    labels = [sw["label"] for sw in specs[0]["sweep"]]
    assert labels == ["probes=1", "probes=5"], labels
    for sw in specs[0]["sweep"]:
        p = sw["label"].split("=")[1]
        assert f"SET ivfflat.probes = {p}" in sw["session"]  # label matches executed value (honesty)


def test_build_config_ivfflat_lists_floored_to_one_for_small_n():
    # n < 1000 -> n//1000 == 0; max(1, ...) guards against the invalid `WITH (lists = 0)` DDL.
    cfg = build_config(build_parser().parse_args(["--index", "ivfflat", "--n", "200"]))
    spec = cfg["index_specs"][0]
    assert "WITH (lists = 1)" in spec["ddl"]
    assert [sw["label"] for sw in spec["sweep"]] == ["probes=1"]  # all probes clamp to lists=1, dedup -> one


def test_build_config_all_includes_hnsw_and_ivfflat():
    # --index all = hnsw + ivfflat (dependency-light; diskann stays on `both`/`diskann`).
    cfg = build_config(build_parser().parse_args(["--index", "all"]))
    assert [s["name"] for s in cfg["index_specs"]] == ["hnsw", "ivfflat"]


def _tiny_hdf5(tmp_path):
    import h5py
    import numpy as np

    path = tmp_path / "tiny-angular.hdf5"
    rng = np.random.default_rng(0)
    with h5py.File(path, "w") as f:
        f.create_dataset("train", data=rng.standard_normal((60, 6)).astype(np.float32))
        f.create_dataset("test", data=rng.standard_normal((10, 6)).astype(np.float32))
    return str(path)


def test_run_benchmark_uses_hdf5_and_infers_dim(tmp_path):
    cfg = dict(_CONFIG)
    cfg["hdf5_path"] = _tiny_hdf5(tmp_path)
    cfg["dataset_label"] = "tiny-angular"
    cfg["n"] = 40
    cfg["n_queries"] = 5
    report = run_benchmark(cfg, FakeVectorDB(), tmp_path)
    assert report["dim"] == 6  # inferred from the HDF5 file, not from config["dim"]
    assert report["dataset"] == "tiny-angular"
    # artifact filename carries the dataset label so real-data runs do not clobber synthetic ones
    assert list(tmp_path.glob("*tiny-angular*.json"))


def test_build_config_hdf5_flag_sets_path_and_label():
    cfg = build_config(build_parser().parse_args(["--hdf5", "/x/glove-25-angular.hdf5"]))
    assert cfg["hdf5_path"] == "/x/glove-25-angular.hdf5"
    assert cfg["dataset_label"] == "glove-25-angular"
