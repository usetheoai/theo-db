# Review — M91 selectivity-adaptive probing (adaptive-filter-strategy)

**Date:** 2026-07-13 · **Slug:** adaptive-filter-strategy · **Milestone:** M91 · **Verdict:** `READY_TO_MERGE`

Three specialist councils (the M90 signing set) reviewed the M91 diff (commit `1d1cd5f` + benchmark artifacts) in parallel. All returned READY_TO_MERGE.

## Severity matrix

| Council | Verdict | BLOCKER | HIGH | MEDIUM | LOW/INFO |
|---|---|---:|---:|---:|---:|
| council-index-storage | READY_TO_MERGE | 0 | 0 | 0 | 3 INFO |
| council-rust-pgrx | READY_TO_MERGE | 0 | 0 | 1 | 1 LOW |
| council-benchmark | READY_TO_MERGE | 0 | 0 | 1 | 2 LOW |

## Key confirmations

- **No-filter path byte-identical** (index-storage Q1): `probed >= probes && (!filtering || …)` collapses to `probed >= probes` when not filtering; the loop executes exactly `probes` bodies — same iteration set/order as the old `.take(probes)`. `v7_non_filter_scan_unchanged` corroborates.
- **M87 iterative composition correct** (index-storage Q2): the adaptive loop honors a grown `state.probes` as a floor; the two growth mechanisms are orthogonal (M87 grows the floor across `amgettuple` calls; M91 extends above it within one call). `last_total` saturation still terminates. No double-growth.
- **No panic across the C-unwind boundary** (rust-pgrx Q1–Q5): `meta.dir[ci]` in-bounds by construction (`dir` and `centroids` share `nlists`, `page.rs:1341/1356/1364`); empty index / huge `rerank_pool` / `probed` overflow all safe; `pgrx::log!` is LOG-level (no longjmp), env-gated.
- **Measurement honest & traceable** (benchmark): every shipped number traces to a committed raw log; recall metric sound (exact seqscan-filtered GT, same query set); no double-timing bug; honest cost (QPS 147→72.7 @ 0.01%) stated plainly; paradigm-ceiling boundary retained.

## Findings dispositioned

| Finding | Council | Severity | Disposition |
|---|---|---|---|
| Synthetic tie-density numbers were prose-only (not in a committed log) | benchmark | MEDIUM | **FIXED** (commit `23ea191`) — synthetic sweep+diag logs committed; Finding 2 now cites them |
| Provenance log filenames wrong | benchmark | LOW | **FIXED** (`23ea191`) — corrected to the 5 real filenames |
| "loose QPS within noise" understated the −10.5% @ 10% | benchmark | LOW | **FIXED** (`23ea191`) — re-labelled as a bounded −2% to −11% per-list-check overhead (not noise) |
| Selective-filter full-`cd` sweep is a latency cliff at extreme selectivity | rust-pgrx | MEDIUM | **ACCEPTED (non-gating)** — bounded + correct + documented (ADR M91-3, `docs/benchmarks/m91-adaptive-filter.md` boundary). A GUC probe-ceiling is an explicit follow-up only if p99 under adversarial selective filters matters. |
| Optional O(lists)-bound note in M31 partial-read blueprint | index-storage | INFO | Deferred (non-gating documentation nicety) |

## Hard gates (cycle-review)

- Full suite **255 pg_tests green, 0 failed** (droplet `cargo pgrx test pg17`), incl. the 2 new M91 tests.
- No commits to `main`; no `Co-Authored-By` trailer; CHANGELOG `[Unreleased]` updated; working on `develop`.
- No new secrets. No `page.rs`/format change (no REINDEX).

## Verdict

`READY_TO_MERGE`. The one MEDIUM benchmark finding and both LOWs are fixed; the rust-pgrx MEDIUM is an accepted, documented, bounded latency trade with a named follow-up. Proceed to `/release`.
