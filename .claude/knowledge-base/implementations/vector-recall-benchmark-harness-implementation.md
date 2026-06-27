# Implementation Summary — vector-recall-benchmark-harness

**Slug:** vector-recall-benchmark-harness · **Date:** 2026-06-27 · **Commits:** `84aead5` (code), `c829040` (evidence)
**Plan:** `.claude/knowledge-base/plans/vector-recall-benchmark-harness-plan.md` (plan-confidence SHIPPABLE 91.2)
**Status:** IMPLEMENTATION_COMPLETE — DoD + acceptance criteria validated with real evidence.

## What was built

`benchmarks/theodb_bench/` — a Python harness that measures recall@k + latency/QPS + build-time + index
size of a pgvector index against the `theo-db:dev` container, with exact brute-force ground-truth and
ANN-Benchmarks distance-thresholded recall semantics. The measurement-first gate of M2 (ADR 0002).

| Module | LoC | Responsibility |
|---|---|---|
| `recall.py` | 79 | recall@k (distance-threshold) + brute-force ground-truth (l2/cosine) |
| `dataset.py` | 28 | seeded reproducible synthetic dataset |
| `metrics.py` | 33 | latency percentiles + best-of-N QPS |
| `db.py` | 123 | psycopg2 adapter (only I/O boundary, DIP) — gate/load/index/query/size, typed errors |
| `harness.py` | 114 | orchestration: dataset→gt→build→measure→JSON/markdown report |
| `__main__.py` | 86 | CLI |

## DoD / acceptance evidence (all validated)

- **Tests:** 36/36 pass (`pytest -q`), unit + integration against real container.
- **Coverage:** 98% total; `recall.py` + `metrics.py` (critical paths) **100%**.
- **Lint:** `ruff check benchmarks/` → All checks passed. **Dead code:** vulture clean.
- **File size:** every module ≤ 500 (max 123).
- **Runtime-metric proof (the gate):** real run produced `docs/benchmarks/2026-06-27-pgvector-l2.json`
  with measured numbers — **HNSW recall@10 = 0.832 (ef_search=40, ~2232 QPS) and 0.958 (ef_search=100,
  ~1454 QPS)**, seed=42 n=5000 dim=128. Reproducible (seed-stamped + commit sha `84aead5`).
- **Failure scenarios exercised:** DB unavailable → `DBUnavailableError` (unit); planner seqscan →
  `IndexNotUsedError` (integration `test_index_not_used_raises`); empty/k>N dataset → `ValueError` (unit).

## Wiring triad

1. **Caller:** `__main__.main()` CLI exercises the full path end-to-end.
2. **Integration test:** `tests/test_integration.py` (real container).
3. **Runtime metric:** the published `docs/benchmarks/*.json` recall/QPS report.

## Methodology note (honest)

To measure the *index* (not the planner's seqscan choice on small/medium N), the harness forces
`SET enable_seqscan = off` per the pgvector recall-test methodology (blueprint §Integration). `assert_index_used`
(EXPLAIN guard) confirms the index is actually used before measuring. Today the harness measures pgvector
HNSW/IVFFlat (M0 image); it is index-agnostic (receives the index DDL) → extensible to pgvectorscale
StreamingDiskANN / ScaNN-as-PG-AM when the image carries them, without rewrites.
