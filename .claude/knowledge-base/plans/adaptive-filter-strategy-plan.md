---
slug: adaptive-filter-strategy
milestone_id: M91
created_at: 2026-07-13
goal: Add selectivity-adaptive probing to the v7 INLINE scan so filtered recall@10 on SIFT1M recovers from 0.741 to ≥0.97 at 0.01% label selectivity while staying within noise of default QPS at ≥1% selectivity.
---

# M91 — selectivity-adaptive probing on the v7 INLINE filtered scan

## Context

M90 shipped the v7 INLINE label filter (`theodb_ivfflat (e, lbl)`): the label lives co-located in the Stage-1 code
pages and non-overlapping candidates are skipped before the rerank (`xs_recheck=true` for correctness). It delivered
recall 1.00 vs the M87 post-filter's 0.52 at ~1% selectivity.

The M91 discovery (`docs/benchmarks/m91-adaptive-filter.md`, SIFT1M, real neighbors) re-scoped this milestone with
three MEASURED findings: (1) INLINE dominates POST at every selectivity — no strategy crossover, so an INLINE⇄POST
adaptive switch is falsified; (2) the earlier synthetic "collapse" was a tie-density artifact (vanishes on SIFT);
(3) the real adaptive axis is **probes** — INLINE's only weakness is ultra-selective (≤0.1%: recall 0.741 @ 0.01%),
and cranking probes recovers it decisively (0.01% sel, probes 64→500 lifts recall 0.741→1.000). This plan implements
the one measured change: the v7 scan probes more lists when the label filter is selective, self-tuning on the
matching-candidate count.

## Goal

Add selectivity-adaptive probing to the v7 INLINE scan so filtered recall@10 on SIFT1M recovers from 0.741 to ≥0.97
at 0.01% label selectivity while staying within noise of default QPS at ≥1% selectivity, proven by the
`benchmarks/m91_filter_bench.py` sweep.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Role |
|---|---:|---|---|
| `theodb_rs/src/am/scan.rs` | 942 | `c178baf` (M90 v7 inline scan) | the AM scan; holds `scan_ivf_aq_split_v7` (`:560`), `amrescan` (`:108`), `amgettuple` (`:805`), `ScanState` (`:43`) |

### Current callers / dependents (from code read)

- `scan_ivf_aq_split_v7(rel, query, probes, rerank_pool, query_labels)` — `theodb_rs/src/am/scan.rs:560`. Stage-1
  loop today: `for &(_, ci) in cd.iter().take(probes)` (`:595`) reads each probed list's code page, AH-scores,
  inline-skips non-overlapping labels via `v7_label_overlaps` (`:668`), pushes matching candidates to `cands`. Stage-2
  reranks the `rerank_pool` best by exact f32 (`:629`). Called ONLY from `scan_ivf_structured` (`:253`, the `ivf_is_v7`
  branch).
- `scan_ivf_structured(rel, query, probes, rerank_pool, query_labels)` — `:243`. Called from `amrescan` (`:180`) with
  `probes0 = guc::probes()` and `pool0 = 64 * guc::over_fetch()`, and from `amgettuple`'s IVF iterative re-search
  (`:882`) with grown probes.
- `ScanState` — `:43`: carries `query_labels: Vec<i16>` (`:66`) and `filtering: bool` (`:71`), set in `amrescan`
  (`:135`,`:142`).
- The scan `profile` log — `:341` (`theodb scan profile: cand=... probes=... reads=...`) is the existing runtime
  observability line, gated on the profile GUC.

### Domain glossary

- **probes** — number of nearest IVF lists the scan visits (`theodb_ivfflat.probes` GUC, default via `guc::probes()`).
- **rerank_pool** — Stage-2 exact-rerank budget (`64 * over_fetch()`); a matching-candidate target proxy.
- **matching candidate** — a Stage-1 candidate whose stored label set overlaps the query label set (survives the inline skip).
- **selectivity** — fraction of rows matching the label predicate; low = few matches (selective), high = many (loose).

### Architecture boundaries affected

`scan.rs` is the interface layer (Postgres AM `IndexAmRoutine` callbacks). The change is local to one scan function
+ its ScanState; no new modules, no cross-layer imports, no page-format change (`page.rs` untouched → no REINDEX).

## Prior Art & Related Work

- Internal: `docs/benchmarks/m91-adaptive-filter.{json,md}` (the design-driving measurement), M90 blueprint
  `knowledge-base/discoveries/blueprints/inline-filter-pushdown-blueprint.md`, M91 blueprint
  `knowledge-base/discoveries/blueprints/adaptive-filter-strategy-blueprint.md` (§ MEASURED VERDICT).
- Internal mechanism reused: the M87 IVF iterative re-search (`scan.rs:882`) — same "probe more lists to recover
  filtered recall" principle, but triggered on heap-underflow; M91 triggers on the matching-candidate count inside
  the v7 scan (the gap the measurement identified).
- External (study, Rule 9): pgvectorscale ships a single non-adaptive strategy (per M91 blueprint) — adaptive probing
  is our own addition, ahead of it.

## ADRs

### ADR M91-1 — selectivity-adaptive probing (probe-until-match-count) over a strategy switch

**Decision:** implement adaptive filtered search as **more probes when the filter is selective** on the existing v7
INLINE path, self-tuning on the accumulated matching-candidate count.

**Rejected alternatives:**
- *INLINE⇄POST strategy switch* — FALSIFIED by measurement (`docs/benchmarks/m91-adaptive-filter.md` Finding 1): INLINE dominates
  POST at every selectivity, so there is no regime to switch to. Building a selector would be dead code.
- *A new PRE strategy (whole-index compact-code scan for the tiny match set)* — the blueprint deferred this to
  "only if measured needed"; the probe-recovery data (Finding 3) shows probes recover ultra-selective recall to ~1.0,
  so PRE is YAGNI and tensions the partial-read invariant.
- *Fix the M87 iterative trigger (amgettuple) to fire on filtered under-recall* — considered; rejected as more
  invasive: the amgettuple loop grows probes globally across re-search rounds and would need the matching-candidate
  signal plumbed up from the scan anyway. Doing it inside the scan is smaller and self-contained (KISS, per
  `rules/parsimony-ladder.md` rung 6).

### ADR M91-2 — self-tuning on the match count, no threshold GUC

**Decision:** the stop condition is data-true — keep probing until `matching candidates ≥ rerank_pool` OR all lists
probed. No selectivity-threshold GUC to tune.

**Rejected alternative:** *a `SET theodb_ivfflat.filter_selectivity_threshold` GUC that flips a high-probes mode* —
rejected (YAGNI, `rules/parsimony-ladder.md` rung 1): the match count is measured directly from the scan, so a
user-tuned threshold adds a knob nobody asked for and can be wrong. A GUC ceiling is a follow-up only if a gate shows
the extreme-selectivity full-scan is a real problem (ADR M91-3).

### ADR M91-3 — bound at total lists; document the extreme-selectivity boundary

**Decision:** the adaptive loop is naturally bounded by the total list count (`cd.len()`). At extreme selectivity
(matches ≪ rerank_pool) it degenerates toward a near-full-list scan — bounded and correct.

**Rejected alternative:** *a hard probe ceiling now* — rejected (YAGNI): finding those few rows' true NN has no
cheaper path, and 95 QPS at 100% recall (measured @ 0.01%) is acceptable. The honest boundary is documented in the
benchmark + `## Drawbacks & Risks`; a GUC ceiling ships only if measured needed.

## Dependency Graph

```
Phase 1 (adaptive probe loop) ──> Phase 2 (observability metric) ──> Phase 3 (integration validation on droplet)
```

Phase 2 depends on Phase 1 (it observes the loop's effective probes). Phase 3 depends on both.

## Phase 1 — adaptive probe loop in `scan_ivf_aq_split_v7`

### Task T1.1 — probe past the default until the matching-candidate target is met

#### Why this step

**Action:** replace the fixed `for &(_, ci) in cd.iter().take(probes)` Stage-1 loop with a loop over all lists
(nearest-first) that breaks once `probed >= probes AND (!filtering OR cands.len() >= rerank_pool)`.

**Reasoning:** Finding 3 (`docs/benchmarks/m91-adaptive-filter.md`) measured that ultra-selective recall recovers only by visiting
more lists; the matching-candidate count is the data-true stop signal (ADR M91-1/M91-2). Non-filter and loose-filter
queries break at exactly `probed >= probes` (target already met or no filter), so behavior is byte-identical where the
default suffices — that is the no-regression guarantee.

#### Files to edit

- `theodb_rs/src/am/scan.rs` — `scan_ivf_aq_split_v7` Stage-1 loop (`:595`).

#### Deep file dependency analysis

`scan_ivf_aq_split_v7` is called only from `scan_ivf_structured` (`:253`); `cd` is the centroid-distance-sorted list
vector (`:589`); `cands` accumulates matching candidates (`:594`+); `rerank_pool` is already a parameter (`:593`).
No caller signature changes. `amgettuple`'s iterative re-search still calls with grown probes — the new loop honors a
grown `probes` floor identically (it only ever probes MORE, never fewer, than `probes`).

#### TDD

```
test_v7_adaptive_probing_recovers_selective_recall (pg_test):
  GIVEN a v7 index on clustered data with a rare label (selectivity ~0.5%) spread across many lists
  WHEN a filtered query `WHERE lbl && '{rare}' ORDER BY e <-> q LIMIT 10` runs at the DEFAULT probes
  THEN filtered recall@10 vs exact seqscan-filtered ≥ 0.9
   AND the same query with the label made LOOSE (every row) returns in ≤ the same probed-list count as an
       unfiltered scan (the loop breaks at `probes` — no extra I/O when not selective).
test_v7_non_filter_scan_unchanged (pg_test):
  GIVEN a v7 index, an UNFILTERED vector query
  THEN the result set is identical to the pre-M91 fixed-`take(probes)` scan (byte-identical top-k) — the adaptive
       loop must not alter the no-filter path.
```

#### Concurrency tests

(none — single-threaded: the scan reads per-backend `ScanState`; no shared mutable state is added. The loop mutates
only local `cands`/`probed`.)

#### Acceptance criteria

- `test_v7_adaptive_probing_recovers_selective_recall` asserts filtered recall@10 ≥ 0.9 at ~0.5% selectivity (oracle: exact seqscan-filtered top-10), where the fixed `.take(probes)` scan measured < 0.9.
- `test_v7_non_filter_scan_unchanged` asserts the unfiltered top-k equals the pre-change fixed-scan top-k for ≥ 20 queries (oracle: assert_eq on the returned tid list).
- The adaptive loop terminates in ≤ `cd.len()` iterations (oracle: the loop condition `probed >= probes && (...)` plus the `for` over `cd.iter()` — asserted by the tests completing without timeout).

#### DoD

- `cargo pgrx test` reports both new pg_tests passing and 0 failures (run on droplet — local box cannot compile pgrx).
- `git diff --stat` shows `page.rs` unchanged (no on-disk format change → no REINDEX).

## Phase 2 — observability (wiring-triad runtime metric)

### Task T2.1 — emit effective probes + match count in the scan profile

#### Why this step

**Action:** extend the existing `theodb scan profile:` log line (`scan.rs:341` and the v7 path) to include, when
`filtering`, the effective probed-list count and the matching-candidate count.

**Reasoning:** the wiring triad requires a runtime metric so ops can SEE the adaptive behavior fire in production
(`rules/cycle-implement.md § Wiring triad`). The profile log already exists; this extends it (DRY — no new metric
subsystem). It is the observable proof the loop adapted (effective probes > default ⇒ a selective filter was detected).

#### Files to edit

- `theodb_rs/src/am/scan.rs` — the profile log path used by the v7 scan.

#### Deep file dependency analysis

The profile log is gated on the profile GUC (`:299`) — zero cost in production when off. Emitting the effective probe
count requires the v7 loop to track `probed` (already introduced in T1.1). No new GUC.

#### TDD

```
test_v7_profile_reports_effective_probes (pg_test):
  GIVEN the profile GUC on and a selective filtered v7 query
  THEN the emitted profile line reports an effective-probes value > the default probes (the adaptive loop grew it).
```

(If capturing the log line in a pg_test is impractical, assert instead on a returned/queryable counter exposed for
the test; the metric MUST be observable by the integration validation either way.)

#### Concurrency tests

(none — single-threaded.)

#### Acceptance criteria

- `test_v7_profile_reports_effective_probes` asserts the emitted profile value for effective-probes is strictly > the default probes on a selective filtered query (oracle: parse the logged/returned counter), and equals the default on an unfiltered query.

#### DoD

- `cargo pgrx test` reports `test_v7_profile_reports_effective_probes` passing, OR the Phase 3 integration validation shows the effective-probes metric > default at 0.01% selectivity (wiring pillar (c) — metric observed non-zero/adapted).

## Failure scenarios

- **Index page read error mid-adaptive-probe** (`page::read_ivf_list_bytes` returns `Err`): the existing path
  propagates via `pg_sys::error!` (`scan.rs:606`) — the adaptive loop inherits this fail-loud behavior unchanged
  (no swallowed error). Test: covered by the existing error path; the new loop adds no new I/O call site (same
  `read_ivf_list_bytes`), only more iterations.
- **Empty match set** (label matches zero rows): the loop probes all lists, `cands` stays empty, Stage-2 reranks
  nothing, the scan returns the pending region only — correct (an empty filtered result), bounded by `cd.len()`.

(No external I/O — HTTP/DB-driver/queue/RPC/object-store — is touched; all reads are internal buffer-manager access.)

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Recover ultra/selective recall via adaptive probes (Finding 3) | T1.1 | probe-until-matching-candidate-target loop in `scan_ivf_aq_split_v7` |
| 2 | No regression on the no-filter path (byte-identical top-k) | T1.1 | break at `probed >= probes` when `!filtering`; `test_v7_non_filter_scan_unchanged` |
| 3 | No regression on loose selectivity (break at default probes) | T1.1 | loose-label assertion in `test_v7_adaptive_probing_recovers_selective_recall` |
| 4 | Observability of the adaptive behavior (wiring metric) | T2.1 | effective-probes + match-count in the scan profile log |
| 5 | Envelope proof (recall ≥0.97 @ 0.01%, QPS ≈ default @ ≥1%) | T1.1 (validated in Phase 3) | droplet SIFT sweep adaptive vs fixed |

**Coverage: 5/5 gaps covered (100%)**

## Drawbacks & Risks

| # | Risk | Severity | Mitigation | Owner |
|---|---|---|---|---|
| 1 | Extreme selectivity (matches ≪ rerank_pool) → near-full-list scan (O(lists) I/O), tensioning the partial-read invariant | MEDIUM | bounded by `cd.len()`; measured 95 QPS @ 0.01% is acceptable; documented boundary; GUC ceiling deferred (ADR M91-3) | impl |
| 2 | The match-count target (`rerank_pool`) could over- or under-probe on data unlike SIFT | MEDIUM | the target is the same rerank budget already tuned by `over_fetch`; the droplet sweep (Phase 3) validates the envelope before release; honest-negative is a valid terminal per the M91 gate | impl |
| 3 | Loop refactor could subtly change the no-filter top-k | HIGH | `test_v7_non_filter_scan_unchanged` asserts byte-identical no-filter results; the break condition is `!filtering ⇒ break at probes` | impl |

## Unresolved Questions

- Should the match-count target be `rerank_pool` exactly or a fraction/multiple? Resolved-at-plan: start at
  `rerank_pool` (the measured probes=500→1.0 @ 0.01% corresponds to ~50 matches ≥ default pool 64 is NOT reached, so
  the loop probes all lists there — matching the measured full recovery); the Phase 3 sweep confirms or tunes it.
  Any tuning stays within T1.1, not a new GUC.

## Global DoD

- Both phases' pg_tests green on the droplet (`cargo pgrx test`); full suite ≥ 253 tests, 0 failed (no regression).
- No on-disk format change (no REINDEX); `page.rs` untouched.
- CHANGELOG `[Unreleased]` updated.
- File size: `scan.rs` stays < 1000 LoC (currently 942; the change is a loop refactor + a log field, net ~+20 LoC).

## Final Phase: Integration Validation (Phase 3)

Re-run `benchmarks/m91_filter_bench.py` on the droplet with the adaptive v7 build:

- **Envelope gate:** adaptive recall@10 ≥ 0.97 at 0.01% selectivity (was 0.741 fixed), ≥ 0.95 at 0.1–1%, and QPS at
  ≥1% selectivity within noise (±10%) of the fixed-probes=64 baseline (no regression where the default suffices).
- **Observability:** the scan profile shows effective probes > default at 0.01% and ≈ default at ≥1% (the loop adapts).
- Persist the adaptive-vs-fixed result to `docs/benchmarks/m91-adaptive-filter.json` (append `adaptive_validated` block)
  and update the `.md` verdict. Honest-negative (adaptive does not ride the envelope) is a valid terminal that blocks
  the release and loops back to `/to-plan`.
