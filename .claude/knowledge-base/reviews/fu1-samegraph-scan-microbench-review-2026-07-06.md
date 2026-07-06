# /review — M47 / FU-1 same-graph scan-allocation micro-benchmark

Date: 2026-07-06 · Slug: `fu1-samegraph-scan-microbench` · milestone_id: M47

## Verdict: READY_TO_MERGE (after fix commit)

M47 is a measurement milestone: the code (pure `ann/scan_core` layer + `NeighborSource` seam + `benches/scan_hot_path.rs`)
shipped in v0.38.0; the remaining Task 4.1 was running the criterion micro-bench and producing the honest artifact.
Domain review by **council-benchmark** (the proportional specialist for a benchmark-artifact milestone).

## Reviewer + findings

**council-benchmark: NEEDS_FIXES → READY_TO_MERGE** (all resolved, independently re-verified):
- **[HIGH] H1 — fabricated methodology:** the first draft described the bench traversing a real
  `HnswIndex::build(seeded_corpus)` via `MemNeighborSource`, but the bench actually builds a synthetic
  random-regular `BenchGraph` (`build_graph(50k,128,m0=32,seed=42)`) and calls `ground_search` directly
  (`MemNeighborSource`/`HnswIndex` are `#[cfg(pg_test)]`-excluded). **FIXED** — rewrote the graph description to
  reality + re-inherited the code's "representative-for-allocation-not-recall; recall proven separately on a real
  HnswIndex by the pg_test oracle" caveat. Honest process win: the review caught a provenance error I introduced by
  transcribing from the equivalence test without reading the bench.
- **[MEDIUM] M1 — `cargo bench --no-run` unevidenced:** **FIXED** — added the link-gate command + captured PASS
  ("Finished bench profile; Executable benches/scan_hot_path.rs") to the artifact (Coverage #7). Reviewer
  independently confirmed the `#[path]`-included region is pg_sys-free.
- **[LOW] L1 — swing understated:** **FIXED** — corrected to -37%/+39% (ef=100 presized widest), per-cell swing added.
- **[polish] L2:** **DONE** — noted the tight run-1 CIs are within-run precision, not run-to-run reproducibility.
- **[INFO residual, non-blocking]** json `git_sha` is the artifact commit vs the release SHA — harmless (bench byte-identical).

## Result (the milestone's product)

Honest verdict **HONEST_NEGATIVE_WITHIN_NOISE**: the M46 pre-size scratch is directionally faster on the mean at
every ef (-2.6/-6.7/-4.1%), but the shared-box run-to-run noise (-37%/+39%) exceeds the effect and the direction
flips at ef=100/400 — only ef=200 is presized-faster in all 3 runs. Valid per plan EC-2 ("o pre-size pode ser ruído
mesmo isolado — resultado válido"). EC-2 upper-bound caveat explicit; no product/QPS claim. Also closes the M48
honest gap (`cargo bench --no-run` validated).

## Hard gates
- Failing tests: NONE (doc-check `test_fu1_report.py` 3 passed). No secrets; no main commit; no Co-Authored-By; CHANGELOG updated.

**Verdict:** READY_TO_MERGE
