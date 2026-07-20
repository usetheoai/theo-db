---
slug: hybrid-beir-significance
milestone_id: M123
created_at: 2026-07-20
goal: Add a paired significance test (permutation p + bootstrap CI) to the hybrid-BEIR benchmark and report the honest verdict on SciFact
---

# Plan — M123 Paired significance of hybrid vs vector (BEIR)

## Goal

Add a paired per-query significance test (permutation p-value + bootstrap 95% CI + t-test cross-check) over the hybrid vs vector-only nDCG@10 arrays the BEIR harness already produces, and report the honest measured verdict on SciFact — a p-value + effect size + CI + wins/losses/ties, with "parity" declared if not significant.

**Single metric:** `run_m53_hybrid_beir.py` emits a `significance` block with a reproducible permutation p-value (fixed seed + B) + bootstrap 95% CI on Δ̄(nDCG@10) for hybrid−vector, validated by a deterministic unit test (null → not significant; clear shift → significant).

## Context

Consumes `.claude/knowledge-base/discoveries/blueprints/hybrid-beir-significance-blueprint.md`. The M53 harness (`benchmarks/run_m53_hybrid_beir.py`) reports MEAN nDCG@10/Recall@100 per retriever but no significance test; `docs/benchmarks/m53-hybrid-beir.md` §4 lists this as the open follow-up ("+0.004 é determinístico entre runs mas não testado para significância entre queries"). The per-query arrays already flow (`theodb_bench/hybrid.py:83` `per_query` behind `return_per_query=True`) but the driver calls `run_three_retrievers` (`:124`) without capturing them.

## Baseline Context

Repo state: git sha `7b93170`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `benchmarks/theodb_bench/significance.py` | 0 | (NEW) | Paired permutation p + bootstrap CI + t-test + wins/losses/ties (scipy). |
| `benchmarks/run_m53_hybrid_beir.py` | 183 | run() aggregates means; calls `run_three_retrievers` at `:124` without `return_per_query` | Capture `_per_query`; compute significance hybrid vs vector; add to JSON + print. |
| `benchmarks/requirements.txt` | 6 | numpy/psycopg2/… (no scipy) | Add `scipy` (BSD-3, dev-only). |
| `benchmarks/theodb_bench/test_significance.py` | 0 | (NEW) | Deterministic unit tests (null / shift / ties). |
| `docs/benchmarks/m123-hybrid-significance.md` | 0 | (NEW) | The honest measured report. |

### Current callers / dependents (verified `file:line`)

- `benchmarks/run_m53_hybrid_beir.py:124` calls `run_three_retrievers(...)` (`theodb_bench/hybrid.py`) — must pass `return_per_query=True` and read `out["_per_query"]`.
- `benchmarks/theodb_bench/hybrid.py:83` emits `per_query[name] = {"qids", "ndcg10", "recall100"}`.
- `_RETRIEVERS = ("vector", "fts", "hybrid")` (`run_m53_hybrid_beir.py:37`) — the paired test compares `hybrid` vs `vector` (primary).
- `run()` aggregates via `_aggregate_runs` (`:51`); the significance block is added alongside `per_retriever` in the returned dict (`:135-146`).

### Domain glossary

- **nDCG@10** — BEIR's primary graded-ranking metric at cutoff 10; one score per query.
- **paired permutation test** — sign-flip the per-query difference under label exchangeability; p = fraction of permutations with |mean| ≥ |observed|.
- **paired bootstrap CI** — resample the per-query differences with replacement; 95% percentile interval on Δ̄.
- **Δ̄ (effect size)** — mean per-query nDCG@10 difference (hybrid − vector).

### Architecture boundaries affected

`benchmarks/` is a dev/CI harness (NOT shipped in the theo-db image) — per `rules/architecture.md` the significance module is pure computation over arrays (no DB/network), so it is unit-testable offline; the real numbers come from the harness's existing DB+embedding boundary. New dep scipy is dev-only (the PRD permissive-licence gate covers distributed deps, not a dev-only harness dep).

## Prior Art & Related Work

- Blueprint (web-evidenced): Smucker/Allan/Carterette CIKM 2007 (randomization test preferred; Wilcoxon/sign rejected); Urbano SIGIR 2013 (bootstrap/t also fine — report together); BEIR NeurIPS 2021 (nDCG@10 primary); pytrec_eval significance example uses `ttest_rel`.
- `docs/benchmarks/m53-hybrid-beir.md` (the artifact M123 upgrades).

## ADRs

### ADR M123-1 — permutation p + bootstrap CI + t-test cross-check (not Wilcoxon)

**Decision:** headline = paired permutation p-value (`scipy.stats.permutation_test`, `permutation_type='samples'`, fixed seed, B declared) + paired bootstrap 95% CI on Δ̄ (`scipy.stats.bootstrap`); paired t-test (`scipy.stats.ttest_rel`) reported as an agreeing cross-check.

**Rationale (cites blueprint + Rule 9):** Smucker CIKM 2007 recommends randomization and rejects Wilcoxon/sign (they discard magnitude + drop ties); Urbano SIGIR 2013 shows bootstrap/t also perform well → report all three. scipy implements them (do not reimplement statistical tests — Rule 9); it is BSD-3 dev-only.

**Alternatives rejected:**
- **Wilcoxon signed-rank / sign test** — REJECTED: discards per-query magnitude, must drop tied (Δ=0) queries (biases n); Smucker says "should no longer be used".
- **Hand-rolled numpy permutation** — REJECTED as the primary (Rule 9: scipy is battle-tested; BCa bootstrap is error-prone by hand). Kept only as a documented KISS fallback if the scipy pin is unavailable.

### ADR M123-2 — pre-declared single endpoint (anti p-hack)

**Decision:** the primary endpoint is nDCG@10 on SciFact, pre-declared. No k-sweep, no dataset-shopping; if not significant, report "parity" and stop.

**Alternatives rejected:** sweeping k∈{1,3,5,10,100} or multiple datasets and headlining the significant one — REJECTED as p-hacking (blueprint anti-p-hack contract); if multiple are tested, Holm/Bonferroni correction is applied and stated.

## Dependencies

Adds `scipy` (BSD-3, dev-only — the CVE/permissive-licence gate applies to distributed deps, not this dev-only CI harness). `## Dependencies`: `scipy>=1.11` (permutation_test/bootstrap/ttest_rel). Verified not already in `benchmarks/requirements.txt`. numpy already present (rung 4 reuse for array ops).

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Paired permutation p + bootstrap CI + t-test over per-query arrays | T1 (significance module) |
| Harness captures per-query arrays + emits a `significance` block | T2 (wire into run_m53) |
| Deterministic correctness (null/shift/ties) | T3 (unit test) |
| Honest measured verdict on SciFact (or parity) | T4 (real run + report) |
| scipy dependency declared | T1 (requirements.txt) |

## Phase 1 — significance module

### T1.1 — `paired_significance(a, b, seed, n_resamples)`

#### Why this step
The reusable, offline-testable core: given two equal-length per-query score arrays (hybrid, vector), compute the paired permutation p, bootstrap 95% CI on Δ̄, t-test p, and wins/losses/ties. Reasoning: isolating it from the DB/embedding harness lets it be unit-tested deterministically (Rule 9 — scipy does the stats), and the harness just calls it.

#### Files to edit
- `benchmarks/theodb_bench/significance.py` (NEW) — `paired_significance(a: list[float], b: list[float], *, seed: int = 20260720, n_resamples: int = 100_000) -> dict` returning `{n, mean_diff, ci95_low, ci95_high, p_permutation, p_ttest, wins, losses, ties, cohens_dz, seed, n_resamples}`. Validate equal length + n ≥ 2 (typed `ValueError` otherwise). Uses `scipy.stats.permutation_test` (samples), `scipy.stats.bootstrap`, `scipy.stats.ttest_rel`.
- `benchmarks/requirements.txt` — add `scipy>=1.11`.

#### TDD
- RED: `test_paired_significance_shapes` — assert the returned dict has all keys + `wins+losses+ties == n`; asserts a `ValueError` on unequal-length / n<2 input.
- GREEN: implement with scipy.
- REFACTOR: single code path for ndcg/recall (metric-agnostic arrays).

#### Concurrency tests
(none — single-threaded) — pure array computation, no shared state, no threads.

#### Acceptance criteria
- Deterministic: two calls with the same seed produce identical `p_permutation` + CI.
- `wins/losses/ties` sum to `n`; unequal-length input raises a typed `ValueError`.

#### DoD
- Unit test green; `paired_significance` importable from `theodb_bench.significance`.

## Phase 2 — wire into the harness

### T2.1 — capture per-query arrays + emit the `significance` block

#### Why this step
The harness must feed the real hybrid/vector per-query nDCG@10 arrays into `paired_significance`. Reasoning: pass `return_per_query=True` to `run_three_retrievers`, use the LAST run's `_per_query` (all runs are identical-by-design — the harness already asserts determinism), align hybrid vs vector by qid, and add a `significance` block to the returned dict + the printout.

#### Files to edit
- `benchmarks/run_m53_hybrid_beir.py` — at `:124` pass `return_per_query=True`; capture `_per_query`; after `_aggregate_runs`, call `paired_significance(hybrid_ndcg, vector_ndcg)` (aligned by qid) and add `"significance": {...}` to the return dict (`:135`); print p + CI + wins/losses/ties in `main()`.

#### TDD
- RED: `test_run_emits_significance_block` — a stubbed `run_three_retrievers` returning fixed per-query arrays makes `run()` produce a `significance` dict with the expected keys (no DB/network needed — inject the stub).
- GREEN: wire the capture + call.
- REFACTOR: keep the per-query alignment (by qid) in one helper.

#### Concurrency tests
(none — single-threaded) — the harness runs retrievers sequentially; no new concurrency.

#### Acceptance criteria
- `run()` output contains `significance` with `p_permutation`, `ci95_low/high`, `wins/losses/ties`, `seed`.
- The mean Δ̄ in `significance` matches `per_retriever[hybrid].ndcg10 − per_retriever[vector].ndcg10` (± rounding).

#### DoD
- Stubbed `run()` test green.

## Phase 3 — deterministic correctness

### T3.1 — unit tests for the statistical properties

#### Why this step
Prove the test is CORRECT before trusting its verdict on real data. Reasoning: on synthetic arrays with known properties — identical arrays (null), a clear uniform shift, and heavy ties — the p-value + CI must behave as the theory predicts; this is deterministic (fixed seed), no BEIR needed.

#### Files to edit
- `benchmarks/theodb_bench/test_significance.py` (NEW).

#### TDD
- RED then GREEN: `test_null_not_significant` (a==b → p≈1.0, CI straddles 0, wins==losses==0, ties==n); `test_clear_shift_significant` (b = a + 0.2 for all → p < 0.05, CI strictly > 0, wins==n); `test_ties_counted` (mixed equal/unequal → ties == count of equal pairs). Deterministic via the fixed seed.
- REFACTOR: parametrize the three cases.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- All three tests green, deterministic across runs (fixed seed).

#### DoD
- `pytest benchmarks/theodb_bench/test_significance.py` green.

## Phase 4 — real measurement + report

### T4.1 — run on SciFact + write the honest report

#### Why this step
The DoD's measured verdict: run the harness on SciFact (OPENAI_API_KEY from `.env`, gitignored) and report the p-value + effect + CI + wins/losses/ties honestly — significant lift OR parity. Reasoning: "performance is a claim, not an opinion" (TheoDB rule 5); the honest-negative (parity) is an accepted outcome.

#### Files to edit
- `docs/benchmarks/m123-hybrid-significance.md` (NEW) — the measured report + the SciFact license flag (CC BY-NC per BEIR paper; CI-internal use only, not redistributed).

#### TDD
- RED: the report must cite real numbers from a real run (n=SciFact test queries, Δ̄, CI, p, wins/losses/ties) — the "test" is the reproduction command + the artifact; a placeholder-only report FAILs the honesty gate.
- GREEN: run `python3 benchmarks/run_m53_hybrid_beir.py --dataset scifact` with OPENAI_API_KEY set; capture the `significance` block into the report.

#### Concurrency tests
(none — single-threaded).

#### Failure scenarios (external I/O — OpenAI API, PG, BEIR download)
- **OPENAI_API_KEY absent / API 5xx/timeout:** the harness already emits `status=UNBENCHMARKED` (`:85-88`) — the report is flagged UNBENCHMARKED, never fabricated numbers.
- **BEIR SciFact download fails (network):** the loader raises; the run aborts with a typed error — no partial/fabricated result.
- **PG/theodb unavailable:** `VectorDB.connect()` raises; run aborts. No silent empty result.

#### Acceptance criteria
- The report contains real per-query n, Δ̄, 95% CI, permutation p (with seed + B), wins/losses/ties; the verdict is stated honestly (significant lift or parity); the SciFact license is flagged.

#### DoD
- `docs/benchmarks/m123-hybrid-significance.md` present with measured numbers + reproduction command; OR flagged UNBENCHMARKED with the honest reason if the environment cannot run it.

## Failure scenarios

External I/O = OpenAI embeddings API + PostgreSQL + the BEIR dataset download (all in T4; the significance module in T1 touches no I/O):

- OpenAI API key absent or the embeddings endpoint returns 5xx/timeout — the harness emits `status=UNBENCHMARKED` (`run_m53_hybrid_beir.py:85-88`); the report is flagged UNBENCHMARKED, never fabricated numbers.
- BEIR SciFact download fails on the network — the loader raises a typed error and the run aborts; no partial or fabricated result is written.
- PostgreSQL / theodb is unavailable — `VectorDB.connect()` raises and the run aborts loudly; no silent empty result reaches the report.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Result may be NOT significant (parity) — the +0.004 mean gain M53 saw may be noise | MEDIUM | Accepted honest-negative: report "parity" and stop; never sweep k/dataset to manufacture significance (ADR M123-2) | implementer |
| SciFact license conflict (BEIR paper CC BY-NC vs HF cc-by-sa) | LOW | CI-internal use only (the permissive-licence gate covers distributed deps); flag the conflict in the report; do not redistribute the corpus | implementer |
| Candidate-set parity (M53 §3 `@@` trap dropped ~93% relevant) could confound hybrid vs vector | MEDIUM | Verify the vector leg + hybrid's vector component use the same corpus/top-N before fusion; note the caveat if unresolved | implementer |

## Unresolved Questions

- Should the report also cover hybrid vs fts, or only hybrid vs vector? Resolved at plan time: **hybrid vs vector is the pre-declared primary** (ADR M123-2); hybrid vs fts is secondary and, if reported, carries a multiple-comparison note.
- (none other — every decision is resolved at plan time.)

## Global DoD

- `paired_significance` (permutation p + bootstrap CI + t-test + wins/losses/ties) implemented + unit-tested deterministically (null/shift/ties).
- Harness emits a `significance` block; the printout shows p + CI + wins/losses/ties.
- Real SciFact run reported honestly in `docs/benchmarks/m123-hybrid-significance.md` (significant OR parity), with n/Δ̄/CI/p/seed + the license flag — or flagged UNBENCHMARKED with the honest reason.
- scipy declared (dev-only). No production-code change. `pytest` green.
