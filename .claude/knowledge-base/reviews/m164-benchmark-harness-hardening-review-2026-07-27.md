# Review — M164 benchmark harness hardening

**Date:** 2026-07-27 · **Slug:** m164-benchmark-harness-hardening · **Commits:** ad7e5f3 (impl), 54631ff (review fixes).
**Verdict: READY_TO_MERGE** (round 1 NEEDS_FIXES → all findings fixed + re-validated by both reviewers in round 2).

## Round 1 (commit ad7e5f3) — NEEDS_FIXES

Two independent reviewers found real defects — including a false-green inside the anti-false-green milestone itself:

- **council-benchmark HIGH-2:** `classify_ab` was fed the broad `columnar_customscan` (`"theodb_columnar_agg" OR "Custom
  Scan"`), but `theodb.enable_projection` defaults ON so `theodb_columnar_project` renders a `Custom Scan` under EVERY
  columnar query — the signal was ~always True and a declined agg still classified `routed_identical`, making
  `no_pushdown_exercised` dead. B2 was theater.
- **council-benchmark + cross-validation HIGH-1:** `sample_is_fresh` was permanently stale for `--n > HITS_TOTAL_ROWS`
  (the canonical `--n 100000000` materializes 99,997,497 rows) → re-streamed ~74 GB every run (the M162 pain #2 it exists
  to prevent).
- **cross-validation MEDIUM:** a `TRIVIAL` verdict overwrote `DIVERGENCE` (masked a real pushdown bug).
- **council-benchmark MEDIUM:** disk-BLOCK sized only the sample file, not the heap+columnar tables the load writes.
- LOW: unit label 745→800; re-stream branch untested; DoD "ruff clean" false (4 pre-existing E702).

## Round 2 (commit 54631ff) — all closed

- **HIGH-1 CLOSED:** `sample_is_fresh` and `preflight_sizing` clamp the target to `min(n_rows, HITS_TOTAL_ROWS)`. Both
  reviewers hand-checked the materializer math (n=HITS, n=HITS+1) — no edge left open; a good over-dataset cache is never
  re-streamed, a short cache is still rejected.
- **HIGH-2 CLOSED:** new `plan_shows_agg_pushdown` keys on `theodb_columnar_agg` specifically; `_bench_query` sets
  `columnar_agg_routed`; `classify_ab` consumes THAT (the broad `columnar_customscan` survives only as its own count). A
  projection-only plan now correctly yields `declined_trivial`. Proven by `test_plan_shows_agg_pushdown_distinguishes_projection_from_agg`.
- **MEDIUM (verdict) CLOSED:** extracted `decide_ab_verdict` checks `ab_diverged != 0` first — DIVERGENCE outranks
  TRIVIAL. Tested.
- **MEDIUM (disk) CLOSED:** `EST_DISK_BYTES_PER_ROW = 800 + 1000 + 150` (sample + heap + columnar). cross-validation
  recomputed all 4 preflight tests by hand — every asserted boolean matches for the right reason.
- **LOW/INFO CLOSED:** unit 800; re-stream integration test (monkeypatched, no network); 4 E702 split → file fully
  ruff-clean; CHANGELOG/impl-summary wording corrected; test count 13→20.
- No new defect introduced; JSON backward-compatible (`est_ram_bytes` rename has no dangling reader; `sizing` block is
  new in M164).

## Gates

- `/plan-confidence`: SHIPPABLE 98.
- `/code-quality`: FAIL_SOFT HARD=0 (env-only; M164 touches no Rust).
- Tests: `benchmarks/test_m164_harness_guards.py` — **20 green**, ruff-clean.

Handoff: proceed to `/release` (v0.155.0), self-merge, flip M164 checkbox → ROADMAP_COMPLETED for M163+M164.
