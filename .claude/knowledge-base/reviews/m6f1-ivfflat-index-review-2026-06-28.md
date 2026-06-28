# Review — M9 IVFFlat / IVF index validated + benchmarked

**Slug:** m6f1-ivfflat-index · **Milestone:** M9 · **Date:** 2026-06-28
**Verdict:** READY_TO_MERGE (after fixes)
**Plan:** `.claude/knowledge-base/plans/m6f1-ivfflat-index-plan.md` (plan-confidence **SHIPPABLE 98.0**)
**Discovery:** satisfied by existing `vector-recall-benchmark-harness-blueprint.md` (SHIPPABLE_WITH_CAVEATS — scope "pgvector HNSW/IVFFlat")
**Code-quality:** `.claude/knowledge-base/audits/m6f1-ivfflat-index-code-quality-2026-06-28.md` (**PASS** 100)
**Report:** `docs/benchmarks/m9-ivfflat.md`
**Commits:** `8b73643` (impl T1.1/T2.1) · `936666b` (review fixes)

## Process

4 specialist agents in parallel (test-auditor · cross-validation · measurement-honesty+architecture ·
vector-index domain). Tally: test-auditor + cross-validation + measurement-honesty = READY_TO_MERGE;
domain = NEEDS_FIXES (1 HIGH). All actionable findings fixed + re-verified live. No BLOCKER.

## ROADMAP M9 DoD — met (honestly)

| DoD | Status | Evidence |
|---|---|---|
| `_ivfflat_spec` + `--index ivfflat`/`all`; no regression | ✅ | `__main__.py` `_ivfflat_spec`; build_config additive; hnsw/diskann/`both` unchanged; 4 index integration tests green |
| integration test: recall@10 ∈ [0,1], index used (`enable_seqscan=off`) | ✅ | `test_harness_measures_ivfflat` (monotone recall across probes + max≥0.90; `assert_index_used` per sweep point) |
| measured report IVFFlat vs HNSW recall×QPS | ✅ | `docs/benchmarks/m9-ivfflat.md` (n=5000 dim=16 l2): IVFFlat ~4× smaller index, recall 0.57→1.0 across probes; HNSW higher QPS at recall 1.0 |
| specs 03/04 annotated validated | ✅ | both carry ✅-Validado banner linking the report; 04 documents IVF≡pgvector IVFFlat |

## Findings & resolution

| # | Sev | Finding | Resolution | Verify |
|---|---|---|---|---|
| 1 | HIGH (domain) | `probes` de-duped BEFORE clamp → at default n=5000 the `probes=10` row silently ran `probes=5` (mislabeled duplicate operating point — measurement-honesty defect) | Clamp each probe to `lists` BEFORE dedup: `sorted({min(p,lists) for p in (1,10,lists)})`; session sets the clamped value directly (label == executed) | Fresh `--index all` now emits exactly 2 ivfflat rows (probes=1, probes=5); unit test asserts labels==`["probes=1","probes=5"]` |
| 2 | MEDIUM (test) | New `_ivfflat_spec` / `--index ivfflat`/`all` had no unit test; `lists=0` floor untested | Added `test_build_config_ivfflat_only`, `_lists_floored_to_one_for_small_n`, `_all_includes_hnsw_and_ivfflat` | `pytest -m 'not integration'` 72 passed |
| 3 | MEDIUM (cross-val) | Report "~6.9× faster build (157 vs 1078ms)" did not reproduce (build-time is machine-load dependent; saw 1.8×–8×) | Dropped the fixed multiplier; report states per-run numbers + stable direction + load-dependence caveat; CHANGELOG likewise | `grep` report: no fixed build multiplier; direction-only |
| 4 | MEDIUM (domain) | Degenerate `lists=1` (n<1000) = exact full scan dressed as ANN | Documented in report Caveats (lists floored at 1, never 0; lists=1 = exact-via-fullscan, not an ANN point); unit test covers the floor | report `## Caveats`; `test_..._lists_floored_to_one` |
| 5 | LOW (cross-val) | Plan AC grep `faster than|outperforms` self-matched the report's own disclaimer | Reworded disclaimer ("no speed-superiority claim") + build bullet to avoid the literal "faster than" | `grep -ciE 'faster than|outperforms'` report → 0 |
| 6 | LOW (arch) | `--index all` excludes diskann (misnomer) | help string now states "all (hnsw+ivfflat — excludes diskann; use 'both'/'diskann')" | `--help` |
| 7 | LOW (arch) | "best-of-N mean" methodology phrasing ambiguous | Report now states exact aggregation (recall once/build; QPS=1/best mean latency; p95 client-side) | report `## Methodology` |
| 8 | LOW/INFO | Monotone assertion degenerate at test n=2000 (2 distinct probes); INFO confirmations (build-after-load PASS; opclass support OK; probes>lists safe no-op; seqscan-off honest) | Accepted — test asserts the real invariant; deep-dive confirmations recorded | live runs green |

## Hard gates (cycle-review)

| Gate | Status |
|---|---|
| Tests passing on branch | PASS — 72 unit + 4 index integration green; lint (ruff) + dead-code (vulture) clean |
| No secrets committed | PASS — `sk-proj` staged = 0 |
| No direct commit to `main` | PASS — develop |
| No Co-Authored-By trailer | PASS |
| CHANGELOG updated | PASS — `[Unreleased] § Added` M9 entry (honest, no fixed perf multiplier) |
| No unbenchmarked perf claim | PASS — report numbers measured; build-multiplier dropped; `faster than|outperforms` = 0 |
| No new dependency (Rule 9) | PASS — reuses shipped pgvector IVFFlat; `requirements.txt` untouched |
| Backward compatibility | PASS — hnsw/diskann/`both` paths byte-for-byte preserved |

## Verdict

**READY_TO_MERGE.** M9 closes features 03 (IVFFlat) and 04 (IVF ≡ pgvector IVFFlat) with measured,
reproducible evidence: IVFFlat is now a first-class index in the recall@k harness, its recall×QPS curve
is honest (label == executed probes after the clamp-before-dedup fix), and the trade-off (smaller index +
faster build vs lower QPS at high recall) is reported as measured with load-dependence caveats. The one
HIGH (measurement-honesty of the probes label) and all worthwhile MEDIUM/LOW findings are fixed and
re-verified live. No new dependency; full backward compatibility.
