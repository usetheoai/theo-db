# Code-Quality Audit — m32-scale-benchmark

**Date:** 2026-07-02 · **Verdict:** PASS · **Milestone:** M32

## Method

M32 is a Python-only change (the `benchmarks/theodb_bench` harness + tests + operator driver — dev tooling, not
the shipped Rust extension). Detectors:

- **Dead code (D1):** `vulture theodb_bench/{recall,dataset,harness,__main__}.py run_m32_sift1m.py` — clean (no output).
- **Lint / style:** `ruff check theodb_bench/ tests/test_scale_benchmark.py run_m32_sift1m.py` — All checks passed.
- **Symbol fabrication (D2):** the test suite imports + exercises every new symbol (`neighbors_ground_truth`,
  `load_hdf5_full`, `_theodb_spec`, `_train_size`, `query_cap` path) — 42 unit tests + 1 integration green ⇒ no
  fabricated references.
- **Wiring:** the new harness paths are exercised end-to-end by `test_4way_scale_harness_runs_on_real_sift`
  (real SIFT, all 4 AMs via their own index) and produced the committed ≥1M artifact.

## Findings

| Severity | Finding |
|---|---|
| INFO | No dead code (vulture clean), no lint issues (ruff clean), no fabricated symbols (tests green). |
| INFO | Error handling: fail-fast typed `ValueError`s on bad inputs (missing `neighbors`, k>neighbors, out-of-range ids, query_cap<1, empty specs) — added under /review. No swallowed exceptions. |

## Verdict

**PASS** — no dead code, no fabrication, wiring proven end-to-end. Proceeds to `/review` (done — READY_TO_MERGE).
