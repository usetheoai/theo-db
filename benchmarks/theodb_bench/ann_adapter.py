"""M127 — TheoDB adapter matching the ann-benchmarks `BaseANN` contract.

The exact interface a public ann-benchmarks entry uses (`fit`/`query`/`set_query_arguments`), wrapping the
existing `VectorDB` boundary so a later public-box PR reuses this class verbatim (ADR M127-1). It is a thin
signature-compatible wrapper — we do NOT vendor the ann-benchmarks framework (parsimony rung 4: reuse VectorDB +
our own driver honours the single-thread protocol).

The `db` dependency is injected (DIP) so the contract is unit-testable without a live PostgreSQL (a fake db in the
tests), and drives a real `VectorDB` in the measured run.
"""
from __future__ import annotations

# ann-benchmarks 'metric' -> (theodb distance op label used by VectorDB, the theodb_hnsw index opclass)
_METRIC_MAP = {
    "euclidean": ("l2", "theodb_hnsw_l2_ops"),
    "angular": ("cosine", "theodb_hnsw_cosine_ops"),   # GloVe is angular/cosine
    "cosine": ("cosine", "theodb_hnsw_cosine_ops"),
}


class TheoDBAnn:
    """ann-benchmarks BaseANN-shaped adapter for TheoDB's `theodb_hnsw` index."""

    def __init__(self, db, metric: str = "euclidean", *, table: str = "ann_bench",
                 embed_col: str = "embedding", m: int | None = None, ef_construction: int | None = None):
        if metric not in _METRIC_MAP:
            raise ValueError(f"unsupported metric {metric!r} (want one of {sorted(_METRIC_MAP)})")
        self._db = db
        self._metric = metric
        self._vdb_metric, self._opclass = _METRIC_MAP[metric]
        self._table = table
        self._embed_col = embed_col
        self._m = m
        self._ef_construction = ef_construction
        self._ef_search: int | None = None
        self._dim: int | None = None
        self.build_seconds: float | None = None

    # --- pure helpers (unit-testable, no DB) ------------------------------
    def _index_ddl(self) -> str:
        """The `CREATE INDEX … USING theodb_hnsw` statement for this metric."""
        with_opts = []
        if self._m is not None:
            with_opts.append(f"m = {int(self._m)}")
        if self._ef_construction is not None:
            with_opts.append(f"ef_construction = {int(self._ef_construction)}")
        tail = f" WITH ({', '.join(with_opts)})" if with_opts else ""
        return (f"CREATE INDEX {self._table}_hnsw ON {self._table} "
                f"USING theodb_hnsw ({self._embed_col} {self._opclass}){tail}")

    def _ef_statement(self, ef: int) -> str:
        return f"SET theodb_hnsw.ef_search = {int(ef)}"

    # --- BaseANN contract -------------------------------------------------
    def fit(self, X) -> None:
        """Build phase: create the table, COPY the corpus, build the theodb_hnsw index (timed)."""
        self._dim = int(len(X[0]))
        self._db.create_table(self._table, self._dim, embed_col=self._embed_col)
        # ann-benchmarks ids are the row positions 0..N-1
        self._db.load_vectors(self._table, list(X), embed_col=self._embed_col)
        self.build_seconds = self._db.build_index(self._index_ddl())

    def set_query_arguments(self, ef_search: int) -> None:
        """One Pareto point = one ef_search value (the recall/QPS knob)."""
        self._ef_search = int(ef_search)
        self._db.set_session(self._ef_statement(self._ef_search))

    def query(self, v, n: int):
        """Return the top-n neighbour ids for query vector `v` (single query, single-thread)."""
        ids, _dists, _latency = self._db.query_topk(
            self._table, v, int(n), metric=self._vdb_metric, embed_col=self._embed_col)
        return ids

    def __str__(self) -> str:
        return (f"TheoDBAnn(metric={self._metric}, m={self._m}, "
                f"ef_construction={self._ef_construction}, ef_search={self._ef_search})")
