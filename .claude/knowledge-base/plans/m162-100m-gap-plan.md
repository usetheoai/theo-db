---
slug: m162-100m-gap
milestone_id: M162
created_at: 2026-07-27
goal: produce the honest larger-than-RAM ClickBench gap number at 100M (TheoDB columnar vs ClickHouse, same box) and issue a measurement-first verdict on whether I/O/decode became the bottleneck — building a type-specific encoding ONLY if the measurement justifies it
---

# M162 — measure the 100M larger-than-RAM ClickBench gap (measurement-first)

## Goal

Run ClickBench at **100M rows** (larger-than-RAM: ~22GB columnar > 15GB RAM box) with the SAME reproducible harness M159
used at 1M, measure the TheoDB-vs-ClickHouse gap per-query + overall, and issue an honest verdict: **which query classes
widen at 100M vs 1M, and whether I/O/decode became the bottleneck** (the M162 question). A lightweight type-specific
encoding (delta / dictionary / RLE / frame-of-reference) is added **ONLY IF** the measurement shows decode/I/O is the
lever — never on a guess (Unbreakable Rule 5; ROADMAP M162 risk (a)).

## Context

M159 measured the gap at **1M** (fits in RAM → `shared_blks_read≈0`, I/O is not the signal): 7.54× covered-class /
19.4× overall vs ClickHouse. M159 explicitly left the 100M number as `[NEEDS-100M]` and flagged the 1M ratio as a
LOWER bound — at 100M the non-covered precipice likely widens and I/O/decode may dominate. This milestone produces that
number. Deep-dive: `knowledge-base/discoveries/blueprints/columnar-improvement-deepdive-blueprint.md` (Lever C).

## Baseline Context

| File | Role |
|---|---|
| `benchmarks/run_m128_clickbench.py` | TheoDB harness: streams `hits.tsv.gz`, systematic-samples `--n`, COPY→`hits_heap`, INSERT→columnar `hits`, runs 43 queries ×3 with per-query A/B vs `hits_heap` (byte-identity oracle). `--agg` enables the M131+ columnar-agg pushdown. |
| `benchmarks/m159_clickhouse_run.sh` | ClickHouse side: `clickhouse local --path` MergeTree over the SAME sample.tsv, 43 ch-dialect queries ×3, server-side `--time` (min-of-3). Conservative (overstates the gap). |
| `docs/benchmarks/m159-clickhouse-gap-verdict.md` | the `[NEEDS-100M]` marker + the 1M numbers. |

Box: DO droplet (15GB RAM, 8 vCPU, 280GB disk) — 100M columnar ~22GB > RAM ⇒ genuine larger-than-RAM. Git sha: v0.152.0.

## ADRs

### ADR-1 — measurement-first; encoding is CONDITIONAL
**Decision:** land the 100M measurement + verdict as the milestone core. Build a type-specific encoding ONLY if the
verdict shows I/O/decode-bound. **Rationale:** ROADMAP M162 + Rule 5 — a persistent-format encoding on a guess is
accidental complexity; if the 100M gap is CPU-bound (compute, not bytes-read), an encoding does not help and shipping it
would be dishonest. **Alternatives:** *build encoding now* — REJECTED (guess before measure).

### ADR-2 — if encoding IS justified, it is a FORMAT subsystem, not a side script
**Decision:** any encoding change bumps the columnar magic + needs REINDEX/upgrade (M137 subsystem) + crash-safety +
rollback + A/B byte-identical. **Rationale:** ROADMAP M162 risk (c). Given the size of that change, it is likely a
FOLLOW-UP milestone (M163) rather than folded into M162 — M162's honest deliverable is the number + the verdict that
scopes it.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Produce the 100M TheoDB number (load + 43 queries + A/B) | T1.1 |
| Produce the 100M ClickHouse number (same box, same sample) | T1.2 |
| Honest verdict: classes that widen + I/O-vs-CPU bottleneck | T2.1 |
| Encoding ONLY IF justified (else honest-negative) | T2.2 |

## Phase 1 — measure

### T1.1 — TheoDB 100M load + benchmark
Run `run_m128_clickbench.py --n 99997497 --agg` on the box → columnar `hits` + `hits_heap`, 43 queries ×3 with per-query
A/B (`diverged=0` enforced). Capture `pg_stat_statements` `shared_blks_read`/`shared_blks_hit` per query to quantify I/O.
#### Acceptance criteria
- [ ] 100M loaded (`count = 99,997,497`); per-query A/B byte-identical (any divergence is a BLOCKER, not a perf note).

### T1.2 — ClickHouse 100M (same box)
Install `clickhouse`, load the SAME `hits_sample.tsv` into a `clickhouse local --path` MergeTree, run the 43 ch-dialect
queries ×3 (server-side `--time`, min-of-3).
#### Acceptance criteria
- [ ] ClickHouse 100M timings captured; count matches.

## Phase 2 — verdict

### T2.1 — honest gap verdict
Compute per-query + overall geomean ratio TheoDB/ClickHouse at 100M; compare to the 1M ratio (7.54× / 19.4×). Classify
which query classes widened. Use `shared_blks_read` to answer: is TheoDB I/O-bound (decode/read the lever → encoding
helps) or CPU-bound (encoding does NOT help → honest-negative)?
#### Acceptance criteria
- [ ] `docs/benchmarks/m162-100m-gap-verdict.md`: per-query table, overall ratio, 1M-vs-100M delta, I/O-vs-CPU verdict, honest non-flips.

### T2.2 — encoding decision (conditional)
IF T2.1 shows I/O/decode-bound: scope a lightweight type-specific encoding (delta/dict/RLE/FOR) as a follow-up (ADR-2 —
format subsystem). IF CPU-bound: honest-negative — document that an encoding does not close this gap, no code shipped.
#### Acceptance criteria
- [ ] Explicit decision recorded with the measured evidence that drives it.

## Drawbacks & Risks

- **Infra cost / idle box (medium):** the 100M box is billed — destroy at the end, never leave idle (ROADMAP risk b). Owner: implementer.
- **Load time (medium):** INSERT-SELECT columnar of 100M is slow; the run is multi-hour. Owner: implementer. Mitigation: nohup + poll.
- **Encoding-on-a-guess (high):** the whole point is to NOT build encoding before the measurement justifies it. Owner: honest verdict.

## Unresolved Questions

- Does the covered-class gap (7.54× @1M) widen at 100M, or does the vectorized pushdown hold? Resolved at T2.1 (measure).
- Is TheoDB's 100M cost I/O (bytes decoded) or CPU (per-row materialization, M148)? Resolved at T2.1 via `shared_blks_read`.

## Global Definition of Done

- [ ] 100M TheoDB + ClickHouse numbers measured, same box, A/B byte-identical.
- [ ] Honest verdict (widen classes + I/O-vs-CPU) in `docs/benchmarks/m162-100m-gap-verdict.md`.
- [ ] Encoding shipped ONLY IF justified; else honest-negative documented.
- [ ] `/code-quality` ∉ {FAIL_HARD, INVALID}; CHANGELOG `[Unreleased]`; released + M162 checkbox flipped.

## Final Phase: Integration Validation

- [ ] The 100M artifact + verdict committed to `docs/benchmarks/`; box destroyed.
