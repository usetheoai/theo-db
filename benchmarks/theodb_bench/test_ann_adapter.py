"""M127 — unit tests for the ann-benchmarks BaseANN adapter + the byte-identical regression wrap module.

DB-free: the adapter's `db` dependency is injected, so a FakeDB records calls + returns canned ids. The measured
recall×QPS run against a real TheoDB is the driver (`run_m127_ann_vector.py`), not a unit test.
"""
import pytest

from theodb_bench.ann_adapter import TheoDBAnn
from theodb_bench.regression import assert_byte_identical, QidMismatchError


class FakeDB:
    """Records DDL/session calls; returns the first `k` ids of a fixed 5-row corpus for any query."""
    def __init__(self):
        self.calls = []
        self._corpus_ids = [0, 1, 2, 3, 4]

    def create_table(self, table, dim, embed_col="embedding"):
        self.calls.append(("create_table", table, dim, embed_col))

    def load_vectors(self, table, vectors, embed_col="embedding"):
        self.calls.append(("load_vectors", table, len(vectors), embed_col))

    def build_index(self, ddl):
        self.calls.append(("build_index", ddl))
        return 0.123

    def set_session(self, stmt):
        self.calls.append(("set_session", stmt))

    def query_topk(self, table, qvec, k, metric="l2", embed_col="embedding"):
        return self._corpus_ids[:k], [0.0] * k, 0.001


# --- adapter contract (T1.1) ---------------------------------------------

def test_adapter_exposes_baseann_contract():
    a = TheoDBAnn(FakeDB(), metric="euclidean")
    assert hasattr(a, "fit") and hasattr(a, "query") and hasattr(a, "set_query_arguments")


def test_query_returns_exactly_n_ids():
    a = TheoDBAnn(FakeDB(), metric="euclidean")
    a.fit([[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0]])
    out = a.query([0.1, 0.1], 3)
    assert out == [0, 1, 2] and len(out) == 3


def test_fit_builds_table_load_and_index_in_order():
    db = FakeDB()
    a = TheoDBAnn(db, metric="euclidean")
    a.fit([[0.0, 0.0], [1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0]])
    kinds = [c[0] for c in db.calls]
    assert kinds == ["create_table", "load_vectors", "build_index"]
    assert a.build_seconds == 0.123


def test_index_ddl_uses_theodb_hnsw_and_metric_opclass():
    assert "USING theodb_hnsw" in TheoDBAnn(FakeDB(), metric="euclidean")._index_ddl()
    assert "theodb_hnsw_l2_ops" in TheoDBAnn(FakeDB(), metric="euclidean")._index_ddl()
    assert "theodb_hnsw_cosine_ops" in TheoDBAnn(FakeDB(), metric="angular")._index_ddl()


def test_set_query_arguments_sets_ef_search():
    db = FakeDB()
    a = TheoDBAnn(db, metric="angular")
    a.set_query_arguments(200)
    assert ("set_session", "SET theodb_hnsw.ef_search = 200") in db.calls


def test_unsupported_metric_raises():
    with pytest.raises(ValueError):
        TheoDBAnn(FakeDB(), metric="hamming")


# --- byte-identical regression (T2.1) ------------------------------------

def test_regression_identical_is_byte_identical():
    base = {"q1": [1, 2, 3], "q2": [4, 5, 6]}
    cand = {"q1": [1, 2, 3], "q2": [4, 5, 6]}
    r = assert_byte_identical(base, cand)
    assert r["identical"] is True and r["diverged"] == 0 and r["first_divergence"] is None


def test_regression_detects_reorder_with_offending_qid():
    base = {"q1": [1, 2, 3], "q2": [4, 5, 6], "q3": [7, 8, 9]}
    cand = {"q1": [1, 2, 3], "q2": [4, 6, 5], "q3": [7, 8, 9]}  # q2 reordered
    r = assert_byte_identical(base, cand)
    assert r["identical"] is False and r["first_divergence"] == "q2" and r["diverged"] == 1


def test_regression_mismatched_qids_is_typed_error():
    with pytest.raises(QidMismatchError):
        assert_byte_identical({"q1": [1]}, {"q1": [1], "q2": [2]})
