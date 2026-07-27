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

## Gates

- `/plan-confidence`: SHIPPABLE 98 (intrinsic).
- `/code-quality`, `/review`, `/release`: next.
