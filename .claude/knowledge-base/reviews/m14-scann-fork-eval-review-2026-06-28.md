# Review — M14 ScaNN-quality / fork-trigger evaluation (feature 05)

**Slug:** m14-scann-fork-eval · **Milestone:** M14 · **Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m14-scann-fork-eval-plan.md` (plan-confidence **SHIPPABLE 100.0**)
**Discovery:** satisfied by `vector-recall-benchmark-harness-blueprint.md` + `alloydb-vector-ai-implementation-blueprint.md`.
**Code-quality:** `.claude/knowledge-base/audits/m14-scann-fork-eval-code-quality-2026-06-28.md` (**PASS** 100)
**Report:** `docs/benchmarks/m14-scann-fork-decision.md` · **Decision:** `docs/adr/0004-scann-fork-decision.md` (NO-FORK, provisional)
**Commits:** `f7ce221` (impl) · `064ec37` (review fixes)

## Process

4 specialist agents in parallel (measurement-honesty · cross-validation · architecture/parsimony/decision-soundness ·
test-auditor), all with live verification against container `m14-it`. Tally: measurement-honesty + cross-validation +
test-auditor = READY_TO_MERGE; architecture/decision-soundness = NEEDS_FIXES (H1 + MEDIUMs). All actionable findings
fixed + re-verified live. No BLOCKER.

## ROADMAP M14 DoD — met (honestly)

| DoD | Status | Evidence |
|---|---|---|
| reproducible DiskANN vs ScaNN-quality benchmark | ✅ | `benchmarks/scann_fork_eval.sh` (runs=3, seed=14, per-index `--out`) + `docs/benchmarks/m14-scann-fork-decision.md` |
| fork/no-fork ADR anchored on evidence | ✅ | `docs/adr/0004` — **NO-FORK (provisional)** + two re-open gates |
| honesty: DiskANN substitute, theodb_scann gated; no "ScaNN done" | ✅ | spec 05 note + ADR + CHANGELOG; overclaim grep = 0 |

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | HIGH (decision) | ADR 0004 "only path back" / "Until then NO-FORK" **narrowed the LOCKED ADR 0002** (which keeps ScaNN-as-PG-AM as a benchmark-gated *superiority* bet, not only recall-rescue) | Added re-open gate #2 (north-star superiority per ADR 0002 — native AM authorized on a benchmark showing a GAIN over DiskANN); reworded "only path back"; ADR no longer narrows the LOCKED one | ADR §Re-open gates (2 gates) |
| 2 | MEDIUM (decision) | "ScaNN-quality" collapsed to recall@10; ScaNN's AH-quantizer / multi-level trees (memory/compression) un-addressed | scoped explicitly: recall is the bar; AH/compression = a distinct axis (DiskANN uses SBQ), named out-of-scope-by-design + a memory-at-recall re-open trigger | ADR §Consequences + Caveats |
| 3 | MEDIUM (decision) | evidence is synthetic gaussian dim=32 + runs=2; Status "Accepted" overstated | Status → **Accepted (provisional — pending real-dataset `--hdf5`)**; dim=32/synthetic caveat foregrounded in report + spec 05 | ADR Status; report §Caveats |
| 4 | MEDIUM (rigor) | runs=2 below the project's ≥3-runs bar | `scann_fork_eval.sh` RUNS default → 3; re-ran (DiskANN 0.934/0.978) | report (runs=3) |
| 5 | MEDIUM (test) | `scann_fork_eval.sh` clobbered its own JSON (3 indexes → same stem → only last survived) | per-index `--out "${OUT}/${idx}"`; verified 3 artifacts survive | live `ls` shows diskann/hnsw/ivfflat JSON |
| 6 | MEDIUM (test) | pre-existing `test_harness_measures_diskann` depended on test-ordering for `vectorscale` | made self-sufficient (`CREATE EXTENSION IF NOT EXISTS vectorscale`); passes alone after dropping the extension | test green standalone |
| 7 | LOW | script pinned no seed (table not bit-reproducible); `psql` undeclared dep | added `--seed ${SEED:-14}` + a `command -v psql` guard | bash -n ok |
| 8 | LOW (honesty) | report/ADR disclaimers tripped the overclaim grep ("ScaNN delivered/shipped") | reworded the negations to avoid the literal pattern | overclaim grep = 0 |

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — ScaNN-quality bar + diskann/hnsw/ivfflat green; ruff + `bash -n` clean |
| No secrets committed | PASS — `sk-proj` staged = 0 |
| No direct commit to `main` | PASS — develop |
| No Co-Authored-By trailer | PASS |
| CHANGELOG updated | PASS — `[Unreleased] § Added` M14 entry |
| No unbenchmarked perf claim / no "ScaNN done" overclaim | PASS — overclaim grep = 0; QPS-superiority marked UNBENCHMARKED; numbers carry source |
| No new dependency (Rule 9) | PASS — runs the shipped DiskANN; harness code unchanged (`__main__.py`/`harness.py` absent from diff) |
| Measurement-first / anti-sunk-cost | PASS — no native AM built; decision evidence-gated with two explicit re-open paths |

## Verdict

**READY_TO_MERGE.** M14 honestly closes feature 05 as a **measurement + decision** milestone (PRD fork-gate
policy): the reproducible benchmark (runs=3, seed=14) shows DiskANN crosses the ScaNN-quality recall bar
(0.934 at sls=500, 0.978 at sls=1000 ≥ 0.90), and ADR 0004 records a **NO-FORK (provisional)** decision with
**two** evidence-gated re-open paths — reconciled with the LOCKED ADR 0002 superiority bet (the HIGH fix). The
decision is honestly bounded (synthetic dim=32 + recall-only scope + provisional status disclosed); no native
ScaNN AM is built (anti-sunk-cost); the harness is unchanged; no new dependency. All review findings fixed and
re-verified live.
