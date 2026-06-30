# Implementation Summary: m21-own-ann-index

**Date:** 2026-06-30 · **Plan:** `.claude/knowledge-base/plans/m21-own-ann-index-plan.md` (SHIPPABLE_WITH_CAVEATS 89.2)
**Verdict:** IMPLEMENTATION_COMPLETE · **Branch:** develop · **Scope:** SQL-callable measurement-first (locked 2026-06-30)

## What shipped

TheoDB's own **HNSW + IVFFlat** ANN search, in Rust, exposed as `theodb.hnsw_knn` / `theodb.ivfflat_knn`
SQL set-returning functions, at **recall@k parity with pgvector** (proven by a reproducible benchmark + gate test).

| Task | Deliverable | Commit | Wiring triad |
|---|---|---|---|
| T1.1 | HNSW build+search (`theodb_rs/src/ann.rs::HnswIndex`) | `9ea9150` | caller: `ann_query::knn` → externs → SQL wrapper · integration: `test_hnsw_knn_recall_*` · proof: benchmark recall sweep |
| T1.2 | IVFFlat build+search (`ann.rs::IvfflatIndex`, k-means++) | `9ea9150` | caller: `ann_query::knn` · integration: `test_ivfflat_knn_recall_*` · proof: benchmark |
| T2.1 | SQL surface `theodb.{hnsw,ivfflat}_knn` + Spi read + boundary validation + REVOKE (`ann_query.rs` + `lib.rs`) | `9ea9150` | caller: extension_sql wrappers · integration: 12 container tests · proof: `has_function_privilege=false` |
| T2.2 | Container integration + parity gate (`benchmarks/tests/test_ann_index.py`) | `b871410` | — |
| T3.1 | Recall@k parity benchmark + doc (`benchmarks/bench_ann_index.py`, `docs/benchmarks/m21-ann-index-parity.md`) | `db092bb` | — |

## Gates (all green)

- **Rust compiles** — `cargo pgrx install --release --features pg17` (Docker `theodb-rs-builder` stage) ✓
- **Lint** — `cargo clippy --release --features pg17 -- -D warnings` CLEAN ✓
- **Algorithm unit proof** — standalone prototype 10/10 (`rustc --test`); `#[pg_test]` mod locks the contract ✓
- **Container integration** — `pytest benchmarks/tests/test_ann_index.py` → **12 passed** (recall, parity gate, 22023 negatives, NULL-skip, empty-queries, REVOKE) ✓
- **Benchmark** — `bench_ann_index.py` → **PARITY_REACHED** at every swept point (HNSW ef_search∈{10,40,100,200}, IVF probes∈{1,8,16,32}), mean±std over 3 runs ✓
- **CHANGELOG** `[Unreleased] § Added` updated ✓ · **No new dependency** (deps-audit PASS_WITH_CAVEATS) ✓

## Recall@k parity (benchmark, n=1500 dim=32 nq=60 runs=3 tol=0.05)

Own ≥ pgvector − tol at ALL points; own ≥ pgvector at most IVF points (e.g. probes=16: own 0.8983 vs pg 0.8672).
HNSW reaches 1.0000 by ef_search=100 (matching pgvector). Full table: `docs/benchmarks/m21-ann-index-parity.md`.

## Honest notes (Rule 3)

- **Scope:** SQL-callable build+search (one build per call). The planner-integrated on-disk access method
  (`CREATE INDEX … USING theodb_hnsw`) is **deferred to M21b** (ADR D1/D3) — the user-chosen measurement-first
  scope. The DoD is met: own HNSW/IVFFlat build + answer `<=>` at recall@k parity, measured + reproducible.
- **`#[pg_test]` not run in CI** (same as M18-M20) — the always-on proof is the Python container suite + the
  standalone prototype. Disclosed.
- **Latency** not compared to pgvector (own rebuilds per call vs pgvector's persisted index) — recall is the gate;
  latency parity awaits M21b's persisted index. Documented in the benchmark doc.
- **Coexistence:** pgvector type/operators/HNSW/IVFFlat indexes + `theodb.embed/hybrid/import` untouched (own
  functions read `embed_col::real[]`).
