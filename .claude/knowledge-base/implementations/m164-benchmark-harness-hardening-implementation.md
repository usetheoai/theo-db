# Implementation summary — M164 benchmark harness hardening

**Slug:** m164-benchmark-harness-hardening · **Milestone:** M164 · **Date:** 2026-07-27
**Plan:** `.claude/knowledge-base/plans/m164-benchmark-harness-hardening-plan.md` (plan-confidence SHIPPABLE 98)

## What shipped

Three false-green/infra guards on `benchmarks/run_m128_clickbench.py`, each a pure env-injected helper (ADR-1) so they
unit-test with no DB and no real box:

| Guard | Helper | Wired call site | Behavior |
|---|---|---|---|
| B1 — sample count integrity | `sample_is_fresh(rows_in_file, n_rows, tol=1)` | `_ensure_sample` (`wc -l` the cache; re-materialize if not fresh) | a 1M cache no longer serves a `--n 100M` request; systematic off-by-one tolerated |
| B2 — A/B routing integrity | `classify_ab(customscan, identical)` | `run` verdict (`ab_routed_identical`, `no_pushdown_exercised`) | a `--agg` run where all queries decline is flagged, not a trivial `diverged=0` |
| C — pre-flight sizing | `preflight_sizing(n_rows, disk_free, ram)` | `run` before load (`shutil.disk_usage` + `/proc/meminfo` via `os.sysconf`) | disk BLOCKS (UNBENCHMARKED); larger-than-RAM WARNS, never blocks (ADR-2) |

## Wiring triad

- **Caller (production path):** `sample_is_fresh` is called inside `_ensure_sample`; `classify_ab` and
  `preflight_sizing` are called inside `run` (`preflight_sizing` before the load; `classify_ab` in the verdict
  aggregation). Verified callable end-to-end (`python3 -c "import run_m128_clickbench; ..."`).
- **Integration test:** `test_ensure_sample_accepts_fresh_cache_without_streaming` exercises the real `_ensure_sample`
  glue (the `wc -l` + `sample_is_fresh` early-return) with a temp cache — no DB, no network — proving the count-check
  is wired, not just the pure helper. (The stale-cache re-stream branch is network-bound — CC-BY-NC-SA dataset — so it
  is covered by the pure `sample_is_fresh` unit tests, not a live download, honestly noted.)
- **Runtime metric:** the harness's verdict JSON now emits `result_ab.routed_identical`,
  `result_ab.declined_trivial`, `result_ab.no_pushdown_exercised`, and (on a refused load) `sizing` with reasons — the
  observability that makes a trivial/undersized run visible instead of silently green.

## Tests

`benchmarks/test_m164_harness_guards.py` — **13 tests green** (12 pure-logic + 1 integration). TDD RED→GREEN honored:
the 12 pure tests were written first and failed with AttributeError (helpers absent) before the GREEN implementation.

## Honest notes

- **Ruff:** my delta introduces **zero** new violations. The file carries 4 pre-existing `E702` (the harness's
  established `cur.execute(...); rc = ...` compact idiom); `benchmarks/` is **outside the CI ruff scope** (CI runs
  `ruff check theodb_bench tests`), so these were never gated and are left untouched (out of M164 scope, no restyle churn).
- **Sizing constants** (`EST_TSV_BYTES_PER_ROW=745`, `EST_INDB_BYTES_PER_ROW=800`) are advisory estimates derived from
  the published ClickBench `hits` size ÷ row count, documented inline — the guard is a coarse safety net, not exact.

## Review fixes (council-benchmark + cross-validation → NEEDS_FIXES → addressed)

Two independent reviewers found real defects; all fixed and re-validated (20 tests green, ruff-clean):

- **HIGH-1 (both):** `sample_is_fresh` was permanently stale for `--n > HITS_TOTAL_ROWS` — the canonical `--n 100000000`
  materializes 99,997,497 rows (the corpus max), so `100M <= 99997497` was False → re-streamed ~74 GB every run (the M162
  pain #2 the guard exists to prevent). **Fix:** clamp the freshness target to `min(n_rows, HITS_TOTAL_ROWS)`; test
  `test_sample_is_fresh_accepts_full_corpus_for_over_dataset_request`. Same clamp added to `preflight_sizing`.
- **HIGH-2 (council-benchmark):** `classify_ab` was fed `columnar_customscan` (`"theodb_columnar_agg" in plan OR "Custom
  Scan" in plan`) — but `theodb.enable_projection` defaults ON, so every columnar query renders a `Custom Scan
  (theodb_columnar_project)`; the broad signal was ~always True and a declined agg still classified as `routed_identical`
  → `no_pushdown_exercised` was effectively dead. **Fix:** new `plan_shows_agg_pushdown` keys on `theodb_columnar_agg`
  specifically; `_bench_query` sets `columnar_agg_routed`; `classify_ab` consumes THAT. Proof:
  `test_plan_shows_agg_pushdown_distinguishes_projection_from_agg`.
- **MEDIUM (cross-val):** a `TRIVIAL` verdict overwrote `DIVERGENCE` when `--agg` + all-declined + a real divergence.
  **Fix:** extracted `decide_ab_verdict` — DIVERGENCE outranks TRIVIAL; `no_pushdown_exercised` requires `ab_diverged==0`.
  Tests `test_decide_ab_verdict_*`.
- **MEDIUM (council):** disk-BLOCK sized only the sample file, not the heap+columnar tables the load writes. **Fix:**
  `EST_DISK_BYTES_PER_ROW` sums sample + heap + columnar; documented as a coarse net biased toward a (safe) false-BLOCK.
- **LOW/INFO:** unit label 745→800 B/row (GiB); re-stream branch now has an integration test
  (`test_ensure_sample_restreams_stale_cache`, monkeypatched — no network); 4 pre-existing `E702` split so the file is
  now **fully ruff-clean** (DoD literally satisfied).

## Gates

- `/plan-confidence`: SHIPPABLE 98 (intrinsic).
- `/code-quality`: FAIL_SOFT HARD=0 (env-only; M164 touches no Rust).
- `/review`: council-benchmark + cross-validation, NEEDS_FIXES → all findings addressed + re-validated (20 tests green).
- Tests: `benchmarks/test_m164_harness_guards.py` — **20 green** (was 13; +7 for the HIGH/MEDIUM regressions).
