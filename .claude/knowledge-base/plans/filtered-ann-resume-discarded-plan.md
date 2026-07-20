---
slug: filtered-ann-resume-discarded
milestone_id: M118
created_at: 2026-07-20
goal: Replace the iterative-scan re-search-with-doubled-ef by resume-from-discarded in the page-native HNSW traverse, closing the measured selective-filter QPS gap to ≤1.2× pgvector 0.8.5 at matched recall.
---

# Plan: M118 — Filtered ANN resume-from-discarded

## Goal

Replace the iterative HNSW scan's **re-search-with-doubled-ef** (M52) with **resume-from-discarded** in the page-native `traverse`, so the selective (1%) filtered-ANN case runs at **≤ 1.2× the QPS latency of pgvector 0.8.5** (down from the measured ~3× / 42.8 vs 14.6 ms) **at matched recall** (theodb recall ≥ current, within the 0.01 parity gate), proven by a multi-seed droplet benchmark.

Single metric: **selective-case latency ratio theodb/pgvector ≤ 1.2×** at matched recall (multi-seed mean, `run_m52_filtered_ann.py` on a quiet droplet).

## Context

`docs/benchmarks/m52-filtered-ann.md:25` measured theodb ~3× slower than pgvector 0.8.5 on the selective filtered case (42.8 vs 14.6 ms) because the M52 iterative scan (ADR-1, KISS) **re-searches the whole graph with a doubled `ef`** each exhaustion, while pgvector 0.8.5 **resumes from a `discarded` candidate set** (`hnswscan.c::ResumeScanItems`). Recall is already at parity; this is a pure QPS cost. Blueprint: `.claude/knowledge-base/discoveries/blueprints/filtered-ann-resume-discarded-blueprint.md`. Honest framing: this is a QPS optimization on an already-recall-parity path — **not a v1.0 blocker** (backlog priority MEDIUM), executed as a full cycle per the milestone.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC | Last commit | Why it exists |
|---|---|---|---|
| `theodb_rs/src/ann/scan_core.rs` | — | (FU-1 seam) | **The resumable core.** `ground_search_nodes` holds the exact pgvector-shaped state: `cands: BinaryHeap<Reverse<Ranked>>` (= `so->discarded` frontier), `visited: HashSet<u64>` (= `so->v`, sparse — NOT O(N)), `result` (the ef beam). Pure fn behind the `NeighborSource` seam (prod `PageNeighborSource` / bench `MemNeighborSource`); **drops the frontier+visited on return** — the state to make owned/resumable. |
| `theodb_rs/src/am/hnsw_page.rs` | 3351 | `7a5d798` 2026-07-16 | Page-native HNSW: `traverse` (L1502) does the greedy upper-layer descent then calls `scan_core::ground_search_nodes` (L1613); becomes the one-shot wrapper (`init(...).run()`). |
| `theodb_rs/src/am/scan.rs` | 1304 | `4f9c38c` 2026-07-18 | Index-AM scan: `ScanState` (L43) holds the M52/M87 iterative state; `gather_hnsw_candidates` (L217) calls `hnsw_page::traverse(...)` and, on exhaustion, re-searches with a grown `ef`. |
| `benchmarks/run_m52_filtered_ann.py` | — | (existing harness) | The filtered-ANN benchmark that measured the ~3× gap; extended here for multi-seed 1%/10%/50%. |
| `theodb_rs/src/am/guc.rs` | — | (existing) | GUC registration; adds the frontier memory-ceiling GUC. |

### Current callers / dependents

- `hnsw_page::traverse` — **single production caller**: `theodb_rs/src/am/scan.rs::gather_hnsw_candidates` (L217). No other production caller (grep `::traverse(` — only scan.rs). Making it resumable is internal to the am/ layer.
- `ScanState` — consumed by `amgettuple` / `amrescan` (scan.rs L108) — the executor pulls tuples one at a time; `amrescan` resets the state (L118-122).
- `max_scan_tuples()` GUC (scan.rs L186) — already bounds the iterative scan; the resume path reuses it as the tuple cap.

### Domain glossary

- **beam (`w`/`result`)** — the current top-`ef` nearest candidates.
- **frontier (`cand`)** — the min-heap-by-distance of explored-but-not-expanded candidates; **the resumable state** (pgvector's `discarded`).
- **visited** — the set of node-ids already scored; prevents re-scoring on resume.
- **iterative scan** — under a selective `WHERE`, the executor keeps pulling after the beam exhausts; M52 re-searches, M118 resumes.
- **ef** — search breadth; larger = more recall, more cost.

### Architecture boundaries affected

`am/` is the index-access-method layer (`rules/architecture.md` — infrastructure adapter implementing the PG IndexAmRoutine contract). `traverse` and `ScanState` are internal to this layer; no boundary crossed, no new public export. The in-memory `ann/hnsw.rs` is the build-time graph and is **out of scope** (not the scan hot path).

## Prior Art & Related Work

- **pgvector 0.8.5** (`knowledge-base/references/pgvector/src/hnswscan.c`, PostgreSQL License): `ResumeScanItems` (L59-86) pops the best `ef_search` candidates from the `so->discarded` pairing heap as resume entry points and continues `HnswSearchLayer(initial=false)`; `so->discarded` + `so->v` persist in the scan opaque state, bounded by `hnsw_max_scan_tuples` + `work_mem` (L259).
- Internal blueprint: `.claude/knowledge-base/discoveries/blueprints/filtered-ann-resume-discarded-blueprint.md`.
- Internal prior: M52 (`docs/benchmarks/m52-filtered-ann.md` — the measured gap + the ADR-1 re-search trade-off), M87 (IVF iterative cursor — same shape for IVF, out of scope here: HNSW only).

## Objective

Make `hnsw_page::traverse` resumable (persist frontier + visited across `amgettuple` calls in `ScanState`), replace the M52 re-search with a resume, keep recall at parity, bound the retained state's memory, and prove the QPS gap closes on a quiet droplet.

## Dependencies

(none — **no new dependency**. Reuses `std::collections::BinaryHeap` for the frontier and a sparse visited set (stdlib `HashSet<i64>` or the existing bitset helper). Parsimony ladder rung 2/4 (stdlib / already-present). No new crate → `/deps-audit` PASS by construction; no CVE surface added.)

## ADRs

- **ADR-1 — Resume-from-discarded over re-search.** Chosen: persist the traverse frontier and resume. Rejected alternative: keep M52 re-search-with-doubled-ef (simpler, KISS) — rejected because `docs/benchmarks/m52-filtered-ann.md` **measured** it at ~3× the pgvector latency on the selective case; the cost is structural, not tuning. Mirrors pgvector 0.8.5 `ResumeScanItems` (Rule 9 — learn the technique from the permissive peer).
- **ADR-2 — State lives in `ScanState`, traverse gets a resumable variant.** Chosen: a `ResumableTraverse` struct owned by `ScanState`; the existing one-shot `traverse` becomes a thin wrapper (`run to completion`) over it (no caller breakage). Rejected: implement resume in `ann/hnsw.rs::search_layer` — rejected because that is the in-memory build graph, NOT the scan hot path (`scan.rs:217` calls `hnsw_page::traverse`). Per `architecture.md` (SRP at module level).
- **ADR-3 — Bounded frontier + fail-safe.** Chosen: cap the retained frontier/visited by a GUC memory ceiling (`theodb.hnsw_resume_max_mb`) mirroring pgvector's `work_mem` guard; on overflow, stop resuming and return what is held (correctness preserved — the executor's MVCC recheck + `max_scan_tuples` cap already bound emission). Rejected: unbounded frontier — rejected as an OOM risk at 1M selective (same failure class the M104 fold-cap guards).

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Resumable traverse breaks the pure-function simplicity of `traverse` (harder to reason about). | MEDIUM | Keep the one-shot `traverse` as a thin wrapper over the resumable core (ADR-2); the pure path stays for build/non-iterative scans. | impl |
| Retained frontier + visited grow unbounded on a 1M selective query → OOM. | MEDIUM | Bounded ceiling GUC + fail-safe return (ADR-3); test at 1M selective on the droplet. | impl |
| Resume changes candidate visitation order → subtly different top-k → recall regression. | HIGH | Same-index ablation test: the union of resumed batches MUST equal the single-large-ef traverse top-k (recall-neutral by construction); assert byte-identical ordered top-k. | impl |
| The QPS win on our page-native on-demand-I/O layout may be smaller than pgvector's buffer-cached graph. | MEDIUM | UNBENCHMARKED until the droplet run; the DoD gate is the measured ratio, not an assumption. | impl |

## Unresolved Questions

- The real QPS gain depends on our page-native on-demand page-reads (vs pgvector's in-buffer graph) — **UNBENCHMARKED** until the Phase 3 droplet run. If the measured ratio does not reach ≤1.2×, the plan halts honest (no false PASS) and the milestone re-scopes (e.g., page-cache warming) rather than shipping an unproven claim.

## Dependency Graph

```
Phase 1 (resumable traverse core) ──▶ Phase 2 (wire into ScanState + bound) ──▶ Phase 3 (droplet bench evidence) ──▶ Integration Validation
```

Phase 1 blocks Phase 2 (the state must exist before wiring). Phase 2 blocks Phase 3 (bench needs the wired behavior). No parallelism (single strand).

## Phase 1: Resumable page-native traverse

### T1.1 — Extract a resumable `traverse` core

#### Objective
Refactor `hnsw_page::traverse` into a `ResumableTraverse { frontier: BinaryHeap<Reverse<Cand>>, visited: <sparse set>, ... }` that exposes `next_batch(n) -> Vec<(i64,f64)>` and retains its frontier/visited between calls; the existing `traverse(rel,meta,q,ef)` becomes `ResumableTraverse::new(...).run_to(ef)`.

#### Why this step (action + reasoning)
Action: split the one-shot walk into a stepped searcher owning the frontier. Reasoning: the frontier is exactly pgvector's `so->discarded` (blueprint Corner 4); today `traverse` allocates and drops it each call (`hnsw_page.rs:1502`), which is why M52 must re-search. Owning it enables resume (ADR-1). A thin wrapper preserves the single existing caller (`scan.rs:217`) — no breakage (ADR-2, Baseline callers).

#### Evidence
`hnsw_page.rs:1502` (traverse discards frontier); `hnswscan.c:59-86` (ResumeScanItems shape); `docs/benchmarks/m52-filtered-ann.md:25` (the 3× cost).

#### Files to edit
- `theodb_rs/src/ann/scan_core.rs` (extract `ResumableGround<N>` from `ground_search_nodes`: `{ visited, cands, result, ef, m0, exhausted }`; `init<S>(src, entry, ef, m0, presize)` + `expand<S>(&mut self, src: &S) -> Vec<(N, f64)>` that pops/expands the retained `cands` frontier for the next batch; `ground_search_nodes` becomes `init(...).run()`). Keep < 500 LoC delta.
- `theodb_rs/src/am/hnsw_page.rs` (`traverse` stays the one-shot wrapper: descent + `ResumableGround::init(...).run()`).

#### Deep file dependency analysis
`ground_search_nodes` (scan_core.rs) is generic over `S: NeighborSource`; `S::Node` carries the page handle. The resumable struct holds `S::Node` in its heaps (already the case) and takes `&S` per `expand` call (avoids threading a lifetime into `ScanState`). `visited` is ALREADY a sparse `HashSet<u64>` (not `vec![false; N]`) — no O(N) allocation to fix; it is exactly `so->v`. `traverse` is called only by `scan.rs::gather_hnsw_candidates` (Baseline callers); the SBQ/AQ rerank paths call `ground_search_nodes` directly and must keep the one-shot behavior via the wrapper.

#### TDD
- RED: `resumed_batches_union_equals_single_ef_traverse` — a `ResumableTraverse` stepped in batches of `b` until `N` candidates equals `traverse(..., ef=N)` top-N (ordered, byte-identical) → proves recall-neutral (Risk HIGH mitigation).
- RED: `resume_does_not_revisit` — no node scored twice across batches (visited honored).
- RED (EC-1, edge-case MUST-FIX): `resume_exhausts_when_frontier_empty` — when every reachable node is visited before the caller stops, `next_batch` sets `exhausted=true` and returns `[]` (no spin, no re-search fallback). This is the highly-selective case M118 targets.
- RED (EC-3, SHOULD-TEST): `resume_single_node_index_ef1` — a 1-node index with `ef=1`: first batch = the node, second batch = `[]` exhausted (smallest-graph boundary).

#### Concurrency tests (only when applicable)
(none — single-threaded) — a scan runs in one backend; `ScanState` is per-scan, not shared. MVCC visibility is the executor's heap recheck, not this layer's concern (no shared mutable state, no atomics/locks introduced).

#### Acceptance Criteria
- `ResumableTraverse::new(...).run_to(ef)` is byte-identical to the old `traverse(...,ef)` for all `ef` (regression-safe wrapper).
- Resumed union == single-large-ef top-k (recall-neutral).

#### DoD
- `cargo pgrx test` (droplet) green for the two RED tests; the one-shot wrapper passes the existing traverse tests unchanged.

## Phase 2: Wire resume into the scan + bound memory

### T2.1 — Replace M52 re-search with resume in `ScanState`

#### Objective
Store the `ResumableTraverse` in `ScanState`; on exhaustion under a selective `WHERE`, pull the next batch from the retained frontier instead of re-calling `traverse` with a doubled `ef`. Reset on `amrescan`.

#### Why this step (action + reasoning)
Action: swap the re-search branch (scan.rs:217 + the growing-ef path) for `state.resumable.next_batch(...)`. Reasoning: this is where the 3× is paid (Context); the resumable core from T1.1 makes the swap local. `amrescan` reset (scan.rs:118-122) must clear the resumable state to avoid cross-rescan bleed (Baseline callers).

#### Evidence
`scan.rs:217` (gather + re-search), `scan.rs:108,118-122` (amrescan reset), `scan.rs:186` (max_scan_tuples arming).

#### Files to edit
- `theodb_rs/src/am/scan.rs` (ScanState field + amgettuple resume branch + amrescan reset).

#### Deep file dependency analysis
`emitted: HashSet<i64>` (scan.rs:60) still dedups final emission; the resumable `visited` prevents re-scoring (distinct concern — visited is graph-level, emitted is TID-level). Both reset on amrescan.

**EC-6 note (graph stability):** the retained frontier holds node-ids across `amgettuple` calls. A concurrent VACUUM compaction fold takes the advisory **EXCLUSIVE** lock (`am/lock.rs:24`), which waits for the scan's **SHARE** lock — so the page-native graph cannot be compacted out from under an in-flight scan; concurrent tombstones are filtered by the scan + the executor's MVCC heap recheck. The existing M26 lock discipline makes the frontier's node-ids stable for the scan's lifetime — no extra work; do not re-flag in review.

#### TDD
- RED: `self_join_no_skip_or_dup` — a nested-loop / self-join over the index emits each live TID exactly once per outer row (no skip, no dup across rescans).
- RED: `resume_terminates_at_max_scan_tuples` — with `max_scan_tuples=5`, the scan stops after 5 distinct emitted, frontier retained or dropped, no infinite resume.
- RED (EC-4, SHOULD-TEST): `resume_disarmed_when_max_scan_tuples_zero` — with `max_scan_tuples=0`, resume is NOT armed; the scan returns at most `ef_search` (byte-identical to the pre-M52 non-iterative path).

#### Concurrency tests (only when applicable)
(none — single-threaded).

#### Acceptance Criteria
- Selective `WHERE ... ORDER BY emb` returns the same rows as M52 re-search (recall parity) with resume instead of re-search.
- `amrescan` fully resets resumable + emitted + visited.

#### DoD
- `cargo pgrx test` (droplet) green; existing filtered-scan tests unchanged.

### T2.2 — Bounded frontier + fail-safe (GUC)

#### Objective
Add `theodb.hnsw_resume_max_mb` (default e.g. 64) capping the retained frontier+visited; on overflow, stop resuming and return what is held (fail-safe, correctness preserved). **EC-2 (MUST-FIX) — `0 = disabled` (unbounded, no cap), consistent with the sibling GUCs `max_scan_tuples` / `vacuum_fold_max_mb`; documented in the registration comment.**

#### Why this step (action + reasoning)
Action: track retained-state bytes; stop at the ceiling. Reasoning: mirrors pgvector's `work_mem`/`maxMemory` guard (hnswscan.c:259) and the M104 fold-cap pattern (ADR-3) — turns a possible 1M-selective OOM into a documented safe deferral.

#### Evidence
`hnswscan.c:259` (memory guard); `build.rs:620-626` (M104 fold-cap precedent).

#### Files to edit
- `theodb_rs/src/am/guc.rs` (register the GUC), `theodb_rs/src/am/scan.rs` (enforce the ceiling).

#### Deep file dependency analysis
Reuses the GUC registration pattern already in `guc.rs` (e.g., `max_scan_tuples`, `vacuum_fold_max_mb`) — no new dependency.

#### TDD
- RED: `resume_stops_at_memory_ceiling` — with a tiny `hnsw_resume_max_mb`, a large selective query stops resuming without OOM and returns the held candidates (no crash, no wrong-count panic).

#### Concurrency tests (only when applicable)
(none — single-threaded).

#### Acceptance Criteria
- Overflow path returns cleanly (typed, no panic across the C boundary); default ceiling never triggers on the benchmark sizes.

#### DoD
- `cargo pgrx test` (droplet) green; GUC visible via `SHOW theodb.hnsw_resume_max_mb`.

## Phase 3: Droplet benchmark evidence

### T3.1 — Multi-seed filtered-ANN benchmark vs pgvector 0.8.5

#### Objective
Extend `run_m52_filtered_ann.py` to report mean±std over seeds [42,99,7] of the latency ratio theodb/pgvector at selectivities 1%/10%/50%, at matched recall (0.01 parity gate), on a quiet droplet; regenerate `docs/benchmarks/m52-filtered-ann.{md,json}` (or an m118 sibling).

#### Why this step (action + reasoning)
Action: run the real harness on a dedicated box. Reasoning: the dev box is polluted (m52 doc alerts variance); the DoD metric (≤1.2× ratio) is only meaningful on a quiet box (Rule 5 — performance is a claim, not opinion). UNBENCHMARKED until this runs.

#### Evidence
`benchmarks/run_m52_filtered_ann.py` (existing), `docs/benchmarks/m52-filtered-ann.md:31` (variance + parity gate caveats).

#### Files to edit
- `benchmarks/run_m52_filtered_ann.py` (multi-seed loop), `docs/benchmarks/m118-resume-discarded.{md,json}` (NEW — the verdict).

#### TDD
- The harness is a measurement script; its "test" is the reproducible artifact (committed json + md with methodology). A dry-run smoke (tiny N) asserts the script runs end-to-end before the droplet full-scale run.

#### Failure scenarios (external I/O)
(none — no external network I/O; the benchmark drives a local PG on the droplet.)

#### Acceptance Criteria
- Committed `m118-resume-discarded.{md,json}` shows selective-case ratio ≤ 1.2× at matched recall, multi-seed mean±std, with the exact reproduction command + droplet spec.
- If the ratio does NOT reach ≤1.2×: **HALT honest** — record the measured number, do NOT claim success, re-scope the milestone.

#### DoD
- Benchmark artifact committed; verdict states measured ratio + methodology; no perf claim without the artifact (`public-copy.md`).

## Coverage Matrix

| DoD requirement | Task(s) |
|---|---|
| (1) Resumable page-native traverse | T1.1 |
| (2) Recall parity vs current (same-index ablation) | T1.1 (union==single-ef), T2.1 (same rows) |
| (3) MVCC/rescan correct (self-join no skip/dup) | T2.1 |
| Bounded memory / fail-safe (no OOM) | T2.2 |
| (4) MEASURED multi-seed 1%/10%/50% closes the gap | T3.1 |

100% — every DoD item maps to ≥1 task.

## Global Definition of Done

- [ ] All phase tasks' DoD green.
- [ ] `cargo pgrx test` (droplet) green (unit + isolation for the scan).
- [ ] Recall parity proven (same-index ablation), no regression.
- [ ] Benchmark artifact committed with measured ratio + methodology (or honest HALT if the metric is not met).
- [ ] CHANGELOG `[Unreleased]` updated.
- [ ] No new dependency (stdlib only — parsimony rung 2/4).
- [ ] File-size budget respected (< 500 LoC delta per file).
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merged.

## Failure scenarios (when I/O external)

(none — no external I/O touched; the traverse reads index pages via the PG buffer manager, not an external service. The benchmark drives a local PG on the droplet.)

## Final Phase: Integration Validation (MANDATORY)

### Execution
1. Provision quiet droplet; `cargo pgrx install`; load the extension.
2. `cargo pgrx test` (unit + the new RED tests) — all green.
3. Run the isolation/self-join scan tests (MVCC/rescan correctness).
4. Run `run_m52_filtered_ann.py` multi-seed at scale; collect `m118-resume-discarded.{md,json}`.

### Acceptance Criteria
- Full test suite green on the droplet.
- Measured selective-case ratio ≤ 1.2× at matched recall (else HALT honest).
- Recall not regressed at any selectivity.

### If Validation Fails
- Recall regressed → the resume order differs; fix the frontier/visited semantics (T1.1) before proceeding.
- Ratio not met → record the measured number, do NOT claim success, re-scope (page-cache warming or accept the honest partial), surface to human.
