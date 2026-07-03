# Review — M45 rigorous mean±std Pareto claim (theodb_hnsw vs pgvector hnsw, SIFT1M)

**Date:** 2026-07-03
**Slug:** m45-rigorous-pareto-claim
**Verdict:** READY_TO_MERGE
**Scope:** benchmark-only (Python) — `benchmarks/{m45_pareto.py, m45_report.py, run_m45_pareto.py, tests/test_run_m45_pareto.py}` + artifact `docs/benchmarks/m45-pareto-sift1m.{md,json}` + honesty retractions (`docs/benchmarks/sift1m-carrier-verdict.md`, `ROADMAP.md`) + CHANGELOG. **Zero product-code (`theodb_rs`/`theodb_bench`) change.**

## The outcome (honest negative)

The milestone set out to convert the M42 vector-superiority *signal* into a rigorous *claim*. Under proper mean±std measurement (500 queries, ≥3 timed runs, exact GT, matched build params, shared ef grid), the verdict is **PARITY** — the M42 "~1.7–2.8× superiority" does **NOT** reproduce (it was best-of-N + 200-query + warm-cache; two runs gave INFERIOR→PARITY, within noise). **theodb_hnsw is competitive, not superior, vs pgvector hnsw at 1M.** The M42 claim is retracted in three surfaces. North Star P0 (vector superiority vs the reachable SOTA) is **NOT met — it is parity**; next lever is theodb scan latency+variance.

## Agents & findings

Three independent specialist agents (benchmark-rigor, test-auditor, cross-validation). All findings below the BLOCKER line were addressed in commit `ff0c5b7` before this verdict.

| Agent | Verdict | Findings |
|---|---|---|
| council-benchmark (rigor/honesty) | PASS | 0 BLOCKER/HIGH. Verdict reproduces bit-exactly from raw json; retraction complete; no data degeneracy (matched build, identical query subset, exact GT, index isolation, single machine disclosed). 2 MEDIUM (latent methodology debt) → **FIXED**. |
| test-auditor | fixes needed | 1 HIGH + 2 MEDIUM + 2 LOW — all on the refactored internals' test coverage → **FIXED**. |
| cross-validation | READY_TO_MERGE | All DoD/AC met; ADRs D1–D4 honored; no product code; no Co-Authored-By; CHANGELOG present; PARITY handled honestly. |

### Resolved findings (commit `ff0c5b7`)

- **HIGH (test-auditor) — effect>variance gate never isolated.** The gate that licenses the claim's honesty had no test that would fail if it stopped blocking a false SUPERIOR. FIXED: `test_verdict_parity_when_margin_exceeds_tol_but_gap_within_variance` (margin 1.1× but gap 10 < std 80 → PARITY).
- **MEDIUM-1 (council-benchmark) — nearest-point std proxy.** At an interpolated shared level blended from a noisy bracket, the gate used the nearest (possibly quiet) point's std → latent false-SUPERIOR vector for a future non-interleaving dataset. FIXED: `_margin_at` now uses the frac-weighted **interpolated** std (`_interp_field(..., "qps_std")`); 2 tests prove a noisy bracket blocks a false SUPERIOR.
- **MEDIUM (test-auditor) — `_classify` disagreement branch + `_margin_at` qp==0 branch untested.** FIXED: `test_verdict_parity_when_levels_disagree`, `test_verdict_parity_when_pgvector_qps_zero` (asserts the typed reason).
- **LOW (test-auditor) — equal-recall oracle under-specified.** FIXED: pinned to 200.0.
- **MEDIUM-2 (council-benchmark) — doc caveat overstated ef=400 isolation.** FIXED: doc now states the point is excluded as a shared level but still bounds the top interpolation, with the interpolated-std note.

Verdict recomputed from the stored real frontier after the interpolated-std fix: **PARITY, zero flag changes** — the artifact stays consistent.

## Hard gates

- Tests green (17/17 unit; integration structure test green against a container earlier). No secrets. On `develop`. **No `Co-Authored-By`** in any M45 commit. CHANGELOG `[Unreleased]` updated. No product-code change. Complexity ≤10; file-size budget respected (m45_pareto 103≤120, run_m45_pareto 200≤200, m45_report 58).

## Verdict rationale

0 BLOCKER, 0 unresolved HIGH. The one HIGH (gate isolation) and both MEDIUMs are fixed and covered by new tests; the interpolated-std change hardens the honesty gate for future datasets without altering the current (PARITY) verdict. The milestone is a **model honest-negative**: it measured, got parity instead of the hoped-for superiority, published the parity, and retracted the prior over-claim (Rule 3). **READY_TO_MERGE.**

## Release note

An honest measurement milestone that refutes a prior superiority signal is a legitimate release (like M36/M38/M39/M40). Human decides.
