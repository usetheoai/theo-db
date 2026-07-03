---
slug: m44-hnsw-parallel-build
milestone_id: M44
created_at: 2026-07-03
goal: Parallelize the theodb_hnsw graph build with std::thread::scope + per-node RwLock, recall parity, build speedup.
---

# Plan: theodb_hnsw parallel graph build (std::thread + per-node RwLock)

> **Version 1.0** — Parallelize the in-memory HNSW graph construction across CPU cores using `std::thread::scope`
> (borrow the read-only corpus without Arc; panics propagate on join) + a per-node `RwLock` on the neighbor lists.
> The persisted-AM build is L2-only, pure-Rust (no PG calls in the graph loop), so worker threads never touch PG.
> Small corpora build sequentially (deterministic, tests unchanged); large corpora build in parallel. Gate: recall
> PARITY + a measured build speedup + race-freedom.

## Goal

> "Parallelize the theodb_hnsw graph build with std::thread::scope + per-node RwLock so that a large-corpus build
> is meaningfully faster on multi-core, measured by an A/B build-time speedup (≥3 samples mean±std, parallel vs
> sequential on the same SIFT subset) at recall PARITY (within tolerance) and 8/8 `test_index_am.py` green."

## Context

M43 cut the theodb_hnsw build ~2.2× via SIMD distance; M42 showed the build (24min@1M → 8.4min after M43) is the
carrier's remaining weakness. The build is CPU-bound (distance-dominated `search_layer`) and pure-Rust over an
in-memory corpus — parallelizable. pgvector's HNSW is `amcanbuildparallel=true` (parallel workers); we take the
tractable std-thread path (no PG-parallel-worker machinery, no new dependency). Owner-approved (2026-07-03) to
accept a NON-DETERMINISTIC build (racy insert order) since no test asserts build determinism and recall parity is
the gate.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit | Why it exists | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/ann/hnsw.rs` | 383 | `682be07` (2026-07-03) | The in-memory HNSW graph (build + search); `HnswIndex` | `HnswIndex` fields + `search`/`pack`/`entries` API unchanged; sequential `build` result shape identical |
| `theodb_rs/src/ann/hnsw_parallel.rs` (NEW) | 0 | — | (the parallel build worker + per-node-lock structure) | — |
| `theodb_rs/src/ann/mod.rs` | ~110 | `682be07` | `mod` decls + `Metric` | add `mod hnsw_parallel;` |
| `CHANGELOG.md` | — | — | Rule 6 | `[Unreleased]` entry |

### Current callers / dependents

- **Symbol:** `HnswIndex::build(corpus, m, ef_construction, metric, seed)` in `ann/hnsw.rs:23` — callers:
  `theodb_rs/src/am/build.rs` (`ambuild` for theodb_hnsw). Signature UNCHANGED (the parallel path is internal).
- **Symbol:** `search_layer`/`greedy_descend`/`select_from` — private to `hnsw.rs`; the parallel builder
  re-implements the concurrent variants (reads under read-lock). `select_from` is pure (vectors only) → reusable.
- `HnswIndex` fields (`vectors/ids/levels/neighbors/entry/max_level`) consumed by `pack` (`am/hnsw_page.rs`) +
  `search` — the parallel path produces the SAME final `HnswIndex`, so these are untouched.

### Domain glossary

- **ef_construction** — the candidate-list width during build (recall/build-cost knob).
- **per-node RwLock** — one `RwLock` per graph node guarding its neighbor lists; readers (search) share, writers
  (link/prune) are exclusive.
- **std::thread::scope** — stable scoped threads that may borrow non-`'static` data (the corpus `&[Vec<f32>]`)
  and join (propagating panics) at scope end — no `Arc`, no `'static` bound.
- **racy insert** — parallel inserts see a graph missing each other's in-flight links → a slightly different but
  recall-equivalent graph; non-deterministic across runs.

### Architecture boundaries affected

`ann/hnsw_parallel.rs` is a new sibling of `ann/hnsw.rs` in the domain `ann` module; it depends inward on
`crate::vec` (SIMD distance) + `crate::ann::Metric`, never on PG (the graph build is pure Rust). No inner→outer
import. File-size budget 500 LoC.

## Prior Art & Related Work

- **Internal blueprint:** none (first parallel build); the discover analysis is in this plan's Context + the M43
  blueprint `m43-hnsw-build-qps-blueprint.md` (which named parallel as the next lever).
- **Reference project:** `.claude/knowledge-base/references/pgvector/src/hnswbuild.c` — `amcanbuildparallel=true`
  (PG parallel-worker HNSW build; we take the std-thread path instead, ADR D1).
- **External:** hnswlib parallel build (per-node locks, N threads, non-deterministic graph, recall-equivalent) —
  the pattern this mirrors. Malkov & Yashunin 2016 (HNSW) — the base algorithm.

## Objective

- [ ] `HnswIndex::build` dispatches: corpus < threshold → sequential (current code, deterministic); ≥ threshold → parallel.
- [ ] Parallel builder: `std::thread::scope` workers pull nodes via an `AtomicUsize`, insert with per-node `RwLock`.
- [ ] `entry`/`max_level` guarded by a `RwLock`; read at insert start, written when a higher-level node inserts.
- [ ] Panic in a worker propagates (scope join) → surfaced as a PG error, never a silent-wrong graph.
- [ ] The parallel path produces the SAME final `HnswIndex` shape (extract neighbors from the RwLocks).
- [ ] Recall PARITY (AM tests + a SIFT-subset A/B) + a measured build speedup (≥3 samples mean±std).

## ADRs

### D1 — std::thread::scope + per-node RwLock; threshold-dispatched; no new dependency

**Decision:** Parallelize with `std::thread::scope` (stdlib, no rayon) + a `Vec<RwLock<Vec<Vec<usize>>>>` per-node
neighbor lock + a `RwLock<(entry, max_level)>`. Dispatch on corpus size: `< PARALLEL_BUILD_THRESHOLD` (e.g. 4096)
builds sequentially (current deterministic code; tiny test corpora unaffected), `≥` builds in parallel.

**Rationale:** `std::thread::scope` (stable 1.63) borrows the read-only corpus without `Arc`/`'static` and joins
with panic propagation — the safest stdlib primitive (parsimony rung 3, no dep). Per-node RwLock is the standard
hnswlib pattern (fine-grained, low contention). The threshold keeps small builds deterministic + overhead-free and
lets the AM tests exercise the unchanged sequential path.

**Alternatives considered:** (a) PG parallel workers (`amcanbuildparallel`, pgvector) — rejected (huge DSM/LWLock
pgrx undertaking, multi-cycle); (b) rayon — rejected (new dep for what `std::thread::scope` does); (c) global lock
on the whole graph — rejected (serializes everything, no speedup); (d) lock-free — rejected (YAGNI, unproven).

### D2 — Accept a non-deterministic build (racy insert)

**Decision:** the parallel build is non-deterministic (insert order races → a different graph each run). Level
assignment stays deterministic (sequential RNG upfront); only linking races.

**Rationale:** no test asserts build determinism (verified: only `hnsw_roundtrip_bytes_reproduces_search`, a
persistence test). Recall PARITY is the correctness gate (the graph is approximate). Owner-approved. Reproducibility
regresses — documented honestly.

**Alternatives considered:** deterministic-parallel via partition+merge — rejected (changes the algorithm + recall).

### D3 — Benchmark-gated + race-freedom gated

**Decision:** merges only if (a) recall PARITY (within tolerance on a SIFT subset + 8/8 AM tests), (b) a measured
build speedup (≥3 samples mean±std, parallel vs sequential same subset), (c) no data race — validated by a
stress build (large corpus, repeated) with the AM recall gate + a `#[pg_test]` that builds above the threshold and
asserts a valid, searchable graph. If the speedup is marginal or recall regresses, revert honestly.

**Rationale:** measurement-first (M36/M41 lesson); concurrency correctness is non-negotiable (Rust's type system +
RwLock give data-race freedom by construction; the test confirms logical correctness + no deadlock).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Deadlock (a thread holding two node locks) | High | Locks taken ONE at a time (never nested across nodes); linking locks node then each nb sequentially, releasing between — no lock cycle | dev |
| Lock contention limits speedup (Amdahl) | Medium | Fine-grained per-node locks; measure the real speedup (D3); accept whatever the A/B shows honestly | dev |
| Non-deterministic build (reproducibility loss) | Medium | Documented (D2); level assignment stays deterministic; recall parity gated | dev |
| Worker panic corrupts state / crosses C boundary | High | `std::thread::scope` joins + re-panics on the main thread; the graph build is pure Rust (no PG calls in workers); the re-panic is caught by pgrx's ambuild boundary as a normal error | dev |
| Parallel path untested by the tiny AM corpora | Medium | Add a `#[pg_test]` that builds a corpus ABOVE the threshold + asserts recall; the SIFT benchmark exercises it at scale | dev |

## Unresolved Questions

- Q1 — What threshold best balances thread-overhead vs speedup? (resolved empirically; default 4096, the A/B may tune it.)
- Q2 — Does lock contention cap the speedup below the core count? (measured by the A/B; accepted honestly, whatever it is.)

## Dependency Graph

```
Phase 1 (thread-safe parallel builder, hnsw_parallel.rs) ──▶ Phase 2 (dispatch in build + tests) ──▶ Phase 3 (A/B benchmark) ──▶ Phase 4 (integration validation)
```

Sequential — the builder must exist before build() dispatches to it; the benchmark needs both.

---

## Phase 1: Parallel builder (thread-safe)

### T1.1 — `hnsw_parallel.rs`: per-node-lock parallel insert via std::thread::scope

#### Objective
A `build_parallel(vectors, ids, levels, m, m0, ef_construction, metric) -> (neighbors, entry, max_level)` that
inserts all nodes concurrently with per-node RwLocks and returns the plain neighbor lists.

#### Why this step (action + reasoning)
1. **What this step does** — new module with the concurrent insert (search under read-locks, link under
   write-locks, entry/max_level under a RwLock), driven by `std::thread::scope` + an `AtomicUsize` work counter.
2. **Why it is necessary now** — this is the parallel engine; Phase 2 dispatches to it. Isolating it in its own
   file keeps `hnsw.rs`'s immutable `HnswIndex` + sequential build untouched (lower blast radius).

#### Evidence
`theodb_rs/src/ann/hnsw.rs:55-199` (the sequential insert/search/select to mirror concurrently); pgvector
`hnswbuild.c` (the parallel-build reference); `.claude/knowledge-base/references/pgvector/src/hnsw.c:291`
(`amcanbuildparallel=true`).

#### Files to edit
```
theodb_rs/src/ann/hnsw_parallel.rs (NEW) — build_parallel + concurrent insert/search + #[pg_test]
theodb_rs/src/ann/mod.rs — add `mod hnsw_parallel;`
```

#### Deep file dependency analysis
- `hnsw_parallel.rs` reuses `crate::vec` (SIMD dist via `Metric::dist_simd`) + `Cand`/`select_from` logic (pure,
  vectors-only — can be a shared free fn or duplicated ~15 lines). Reads `vectors`/`levels` by `&` (scope borrow).
- `mod.rs` gains one `mod` line.

#### Deep Dives
- Structures: `neighbors: Vec<RwLock<Vec<Vec<usize>>>>` (init per node to `vec![Vec::new(); level+1]`);
  `state: RwLock<(usize /*entry*/, usize /*max_level*/)>` (init to node 0).
- Concurrent search (`search_layer`/`greedy_descend`): to read node `x`'s neighbors, take `neighbors[x].read()`,
  clone the layer slice, drop the guard (short critical section — the distance work happens lock-free after).
- Concurrent link (in insert): `neighbors[node].write()` = selected; for each nb: `neighbors[nb].write()` push +
  prune (locks taken one node at a time, released before the next → no deadlock).
- entry/max_level: read `state.read()` at insert start; if `level > max_level` after linking, `state.write()`.
- Determinism: levels pre-assigned by the sequential RNG (deterministic); insert order racy (D2).
- Panic-safety: `std::thread::scope` re-panics on join; pure-Rust workers → no PG state to corrupt.

#### Pseudo-code / Signatures
```pseudocode
fn build_parallel(vectors: &[Vec<f32>], levels: &[usize], m, m0, ef, metric) -> (Vec<Vec<Vec<usize>>>, usize, usize)
  neighbors = (0..n).map(|i| RwLock::new(vec![vec![]; levels[i]+1]))
  state = RwLock::new((0, levels[0]))         # node 0 is entry
  counter = AtomicUsize::new(1)               # node 0 already placed
  scope(|s| for _ in 0..nthreads: s.spawn(|| loop {
      node = counter.fetch_add(1); if node>=n break
      insert_node(node, vectors, levels, &neighbors, &state, m, m0, ef, metric)   # read/write locks
  }))                                          # join + propagate panic
  return (neighbors.map(into_inner), state.0, state.1)
```

#### Tasks
1. Create `hnsw_parallel.rs`; add `mod hnsw_parallel;` to `mod.rs`.
2. Port `search_layer`/`greedy_descend` to read-lock the neighbor slices; reuse/duplicate `select_from` (pure).
3. Implement `insert_node` (search + link under locks + state update).
4. Implement `build_parallel` (scope + atomic counter + extract).

#### TDD
```
RED:  parallel_build_produces_valid_searchable_graph() — build a 5000-node (> threshold) corpus in parallel; assert
      the returned graph searches (top-1 of a corpus point is itself) and every node has ≤ m0 ground neighbors
RED:  parallel_build_recall_matches_sequential_within_tol() — same corpus built parallel vs sequential; recall@10
      over 50 queries within ±0.03 (parity, not identity — racy graph)
GREEN: implement build_parallel + insert_node
REFACTOR: extract shared select_from if duplicated
VERIFY: cargo pgrx test --package theodb_rs hnsw_parallel
```

#### Concurrency tests (only when applicable)
```
race-freedom: Rust's RwLock gives data-race freedom by construction (no &mut aliasing across threads). The
parallel_build_produces_valid_searchable_graph test builds > threshold with N threads and asserts a valid graph
(no lost links, no panic, no deadlock — the test would hang on deadlock / fail on corruption). Run repeatedly
(the #[pg_test] builds a fresh parallel graph each run) as a stress signal.
```

#### Acceptance Criteria
- [ ] `parallel_build_produces_valid_searchable_graph` + `parallel_build_recall_matches_sequential_within_tol` pass.
- [ ] No deadlock (test completes); no `unsafe` (RwLock/scope are safe); `hnsw_parallel.rs` ≤ 500 LoC.

#### DoD
- [ ] `cargo pgrx test` green; commit referencing T1.1.

---

## Phase 2: Dispatch + threshold

### T2.1 — `HnswIndex::build` dispatches on corpus size

#### Objective
`build` builds sequentially below `PARALLEL_BUILD_THRESHOLD`, parallel at/above; the parallel result is wrapped in
the same `HnswIndex`.

#### Why this step (action + reasoning)
1. **What this step does** — split the current `build` into `build_sequential` (unchanged body) + a dispatcher
   that calls `build_parallel` for large corpora and assembles the `HnswIndex`.
2. **Why it is necessary now** — wires the Phase-1 engine into the AM build path; the threshold keeps tiny test
   corpora deterministic (sequential) and the AM tests unchanged.

#### Evidence
`theodb_rs/src/ann/hnsw.rs:23-54` (current `build`); `am/build.rs` (the `ambuild` caller).

#### Files to edit
```
theodb_rs/src/ann/hnsw.rs — split build → build_sequential + dispatcher; pre-assign levels; wrap parallel result
```

#### Deep file dependency analysis
- `build`'s signature is unchanged (callers in `am/build.rs` untouched). Internally it now pre-assigns levels
  (sequential RNG, deterministic) then dispatches. `build_sequential` is the current loop verbatim.

#### Deep Dives
- Level pre-assignment: `levels[i] = ((-(rng.next_f64().ln())*ml) as usize).min(HNSW_MAX_LEVEL)` for all i,
  BEFORE dispatch (so both paths use the same deterministic levels).
- Assemble: `HnswIndex { metric, m, m0, ef_construction, vectors, ids, levels, neighbors, entry, max_level }`.
- Threshold const `PARALLEL_BUILD_THRESHOLD = 4096` (tunable; below it thread overhead ≥ benefit).

#### Tasks
1. Extract `build_sequential` (current loop).
2. Pre-assign `levels` deterministically.
3. Dispatch: `< threshold` → sequential; else → `hnsw_parallel::build_parallel` + assemble `HnswIndex`.

#### TDD
```
RED:  build_below_threshold_is_sequential_deterministic() — a 100-node corpus built twice with the same seed →
      byte-identical neighbors (the sequential path stays deterministic)
RED:  build_above_threshold_uses_parallel_and_recalls() — a 5000-node corpus builds + recalls ≥ 0.9 (parallel path)
GREEN: implement the dispatcher
REFACTOR: None expected
VERIFY: cargo pgrx test --package theodb_rs hnsw
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```
The dispatcher is single-threaded control flow; the parallel work is in build_parallel (Phase 1).
```

#### Acceptance Criteria
- [ ] Both tests pass; the 8 existing `test_index_am.py` stay green (they use tiny corpora → sequential path).
- [ ] `build` signature unchanged; `am/build.rs` untouched.

#### DoD
- [ ] `cargo pgrx test` green; commit referencing T2.1; CHANGELOG updated.

---

## Phase 3: A/B benchmark (build speedup)

### T3.1 — parallel-vs-sequential build-time A/B on SIFT

#### Objective
Measure the build speedup (parallel vs sequential, same SIFT subset, ≥3 samples mean±std) at recall parity.

#### Why this step (action + reasoning)
1. **What this step does** — reuse the M43 A/B harness pattern: build the same SIFT subset with a small
   corpus (sequential, < threshold via a forced flag OR the M43 scalar-vs-… no — build parallel vs a forced
   sequential) and compare build wall-clock + recall.
2. **Why it is necessary now** — the D3 gate: prove the speedup is real (≥3 samples) and recall holds.

#### Evidence
`docs/benchmarks/m43-hnsw-build.md` (the A/B pattern to mirror); the SIFT dataset `benchmarks/.datasets/sift-128-euclidean.hdf5`.

#### Files to edit
```
benchmarks/run_m44_parallel_build.py (NEW) — build-time parallel-vs-sequential A/B + recall parity
docs/benchmarks/m44-parallel-build.md (NEW) — results (after the run)
```

#### Deep file dependency analysis
- New standalone Python; builds `theodb_hnsw` over a SIFT subset ABOVE the threshold (parallel) and a forced
  sequential baseline (a corpus below threshold scaled, OR a GUC/env to force sequential — see Deep Dives), times
  `CREATE INDEX`, compares recall.

#### Deep Dives
- To A/B parallel-vs-sequential at the SAME scale, the build needs a way to force sequential. Add a session GUC
  `theodb_hnsw.parallel_build` (default on) that, when off, forces the sequential path regardless of size (mirrors
  `max_parallel_maintenance_workers=0`). This is the honest apples-to-apples knob.
- Metric: build wall-clock (≥3 samples mean±std) + recall@10 vs exact GT on the subset.
- Gate: parallel meaningfully faster (effect > variance) at recall within ±0.03 of sequential.

#### Tasks
1. Add a `theodb_hnsw.parallel_build` GUC (on/off) gating the dispatch (off → force sequential).
2. Write `run_m44_parallel_build.py`: build parallel vs sequential (GUC off) over a SIFT subset, ≥3 samples, recall.
3. Emit JSON + a PASS/FAIL D3 verdict.

#### TDD
```
RED:  test_run_m44_emits_build_times_and_recall() — on a small synthetic corpus the harness returns parallel +
      sequential build-time + recall for both (structure + non-degeneracy)
GREEN: implement the harness + the GUC
REFACTOR: reuse the recall helper
VERIFY: cd benchmarks && python3 -m pytest tests/test_run_m44_parallel_build.py
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```
Single-threaded benchmark driver; the concurrency under test is inside the Rust build.
```

#### Acceptance Criteria
- [ ] `test_run_m44_emits_build_times_and_recall` passes.
- [ ] A real SIFT run emits build speedup (mean±std) + recall parity + a D3 verdict → `docs/benchmarks/m44-parallel-build.md`.

#### DoD
- [ ] Harness test green; a real run recorded; commit referencing T3.1.

---

## Phase 4: Integration Validation

### T4.1 — Full validation + honest verdict

#### Objective
`cargo pgrx test` (all hnsw + hnsw_parallel) green + extension installs + AM tests + the A/B recorded.

#### Why this step (action + reasoning)
1. **What this step does** — the "eat your own cooking" gate: parallel build works end-to-end, recall preserved,
   speedup recorded.
2. **Why it is necessary now** — the plan is not done until the full chain passes + the D3 verdict is honest.

#### Evidence
`.claude/rules/cycle-implement.md` (Integration Validation mandatory).

#### Files to edit
```
docs/benchmarks/m44-parallel-build.md — final verdict
CHANGELOG.md — final [Unreleased] entry
```

#### Deep file dependency analysis
- Docs/CHANGELOG only.

#### Deep Dives
- If PASS (speedup real + recall parity): proceed to review/release.
- If FAIL (marginal speedup or recall regression): revert honestly (the lock contention capped it — an honest
  negative), record the finding.

#### Tasks
1. `cargo pgrx test` full green (hnsw + hnsw_parallel).
2. Extension installs; `theodb_hnsw` index builds via the parallel path at scale.
3. `run_m44_parallel_build.py` on real SIFT; record speedup + recall + verdict.
4. CHANGELOG entry.

#### TDD
```
RED:  (integration — no new unit test) the D3 verdict line present in the JSON
GREEN: run the chain; record
REFACTOR: None expected
VERIFY: cargo pgrx test && cd benchmarks && python3 -m pytest tests/test_run_m44_parallel_build.py
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```
Validation orchestration; the concurrency is validated in Phase 1.
```

#### Acceptance Criteria
- [ ] All hnsw/hnsw_parallel `cargo pgrx test` green; extension installs; parallel build works at SIFT scale.
- [ ] `run_m44_parallel_build.py` produces build speedup mean±std + recall parity + a D3 verdict.
- [ ] `docs/benchmarks/m44-parallel-build.md` records the honest verdict; CHANGELOG reflects it.

#### DoD
- [ ] Integration chain green; D3 verdict recorded; ready for `/review`.

## Coverage Matrix

| Goal/Objective claim | Task(s) |
|---|---|
| Parallel builder (std::thread::scope + per-node RwLock) | T1.1 |
| Recall parity vs sequential | T1.1, T3.1 |
| build dispatches on threshold (small→sequential deterministic) | T2.1 |
| Sequential path stays deterministic (tiny corpora) | T2.1 |
| entry/max_level RwLock-guarded | T1.1 |
| Panic propagation (no silent-wrong graph) | T1.1 |
| Build-speedup A/B (≥3 samples mean±std) | T3.1 |
| parallel_build GUC (force-sequential for the A/B) | T3.1 |
| Race-freedom / no deadlock | T1.1 |
| Integration chain green | T4.1 |

**Coverage: 10/10 claims mapped (100%).**

## Failure scenarios

External I/O touched: none new — the graph build is pure in-memory Rust; the corpus is already read from SPI by
the AM (`am/build.rs`) before `HnswIndex::build` is called. The workers make no PG/HTTP/queue calls.

- **Worker panic** — `std::thread::scope` re-raises the panic on join; the pgrx `ambuild` boundary converts it to a
  PG error (fail-loud, no silent-wrong graph). Covered by the pure-Rust-workers design (no PG state to corrupt).
- **Deadlock** — prevented by construction (one node lock at a time, no nesting); the `parallel_build_produces_valid_searchable_graph`
  test would hang → caught.

## Global Definition of Done

- [ ] All tasks' DoD checked; Coverage Matrix 100%.
- [ ] `cargo pgrx test` green; every new file ≤ 500 LoC; no `unsafe` added (RwLock/scope are safe).
- [ ] 8/8 `test_index_am.py` green (sequential path unchanged for tiny corpora).
- [ ] No new dependency (`std::thread::scope`, `RwLock`, `AtomicUsize` are stdlib).
- [ ] `run_m44_parallel_build.py` produces build speedup mean±std + recall parity + D3 verdict on real SIFT.
- [ ] `docs/benchmarks/m44-parallel-build.md` records the honest outcome; no perf claim without the benchmark (`public-copy.md`).
- [ ] CHANGELOG `[Unreleased]` updated (Rule 6).
- [ ] `/code-quality` verdict ∉ {FAIL_HARD, INVALID}.
