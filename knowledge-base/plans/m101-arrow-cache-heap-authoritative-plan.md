---
slug: m101-arrow-cache-heap-authoritative
milestone_id: M101
created_at: 2026-07-16
goal: Ship a heap-authoritative in-memory Arrow columnar cache that the M100 DataFusion executor reads for analytical aggregates, kept MVCC-correct by invalidate-on-write + a snapshot-compatibility gate, proven by result-equivalence pg_tests + pg_isolation MVCC permutations + a measured HTAP benchmark.
---

# M101 — Heap-authoritative Arrow columnar cache (MVCC-correct HTAP)

## Goal

Ship an in-memory Arrow columnar cache **derived from a heap row-store table** (the heap stays the source of truth,
so the cache is MVCC-correct by construction: it is only consulted when it is a faithful, snapshot-compatible copy of
the committed state) that the M100 DataFusion `CustomScan` reads for a simple analytical aggregate — with
invalidate-on-write and a snapshot-compatibility gate, the planner falling back to the heap otherwise. Metric: **the
M101 test set (result-equivalence heap-vs-cache + MVCC isolation permutations + a measured HTAP benchmark) GREEN on
pg17**, with the cache proven to never return a snapshot-incorrect answer.

## Context

M100 (v0.88.0) shipped the DataFusion vectorized `CustomScan` over the *columnar* TAM (9.89× over the M99 seqscan).
M101 brings the same vectorized acceleration to ordinary **heap** tables via the AlloyDB pattern made permissive: the
heap remains authoritative; an Arrow columnar cache is a DERIVED, in-memory replica the executor reads. The hard
problem is MVCC — a cache that returned rows a reader's snapshot must not see would be a correctness disaster. The
heap-authoritative design sidesteps re-implementing MVCC: the cache is a snapshot-tagged copy of committed rows,
**invalidated on any write**, and consulted ONLY when the reader's snapshot is compatible (else the native heap plan
runs). First cut is a MANUAL pragma (`columnarize`); workload-driven auto-populate/evict (what AlloyDB does) is an
explicit follow-up, not this milestone.

## Baseline Context

### Files that will be touched

| File | LoC today | Role | Change |
|---|---|---|---|
| `theodb_rs/src/am/arrow_cache.rs` | (NEW) | the in-memory Arrow cache: build from heap, per-table state, snapshot tag, invalidation | NEW — the milestone core |
| `theodb_rs/src/am/columnar_agg.rs` | ~330 | the M100 `create_upper_paths_hook` + CustomScan for columnar aggregates | EXTEND: also admit a HEAP table WITH a valid, snapshot-compatible cache; run the aggregate over the cache batch |
| `theodb_rs/src/am/df_executor.rs` | ~230 | the DataFusion executor (`run_columnar_aggs`, `build_arrow`, `HeldInterrupts`, MemoryPool) | REUSE: run the aggregate over the cache's Arrow batch (a `run_aggs_on_batch` split out of `run_columnar_aggs`) |
| `theodb_rs/src/api.rs` or a new SQL surface | — | `extension_sql!` DDL surface | ADD `columnar.columnarize(regclass, text[])` pragma + `columnar.cache_state` catalog + the invalidation trigger |
| `theodb_rs/isolation/` | — | pg_isolation harness (M99/M100 pattern) | ADD `arrow_cache_mvcc.spec` + `bench_m101.sh` |
| `docs/benchmarks/m101-arrow-cache.{md,json}` | (NEW) | HTAP benchmark artifact | NEW |

### Current callers / prior art in this repo (reuse, not greenfield)

- `am/columnar_agg.rs` — the M100 `create_upper_paths_hook` + `admit()` + CustomScan that emits an aggregate as one
  tuple. M101 adds a heap-with-cache admission branch alongside the columnar-table branch.
- `am/df_executor.rs` — `build_arrow` (PG type → Arrow), `run_columnar_aggs` (decode → batch → DataFusion aggregate
  under `HeldInterrupts` + MemoryPool). M101 reuses the batch→aggregate half over the cached batch.
- `am/columnar.rs` `decode_columns` — the columnar decode; for M101 the cache is built from the HEAP (a seqscan), not
  decoded from columnar stripes — a different source, same Arrow-array build (`build_arrow`).
- M62 materialized-view pattern (prior art for the invalidate-on-write + non-interference discipline).

### Glossary

- **Arrow cache** — an in-memory `RecordBatch` (projected columns) built from a heap table's committed rows.
- **Build snapshot** — the MVCC snapshot the cache was materialized under; the cache is a faithful copy of the
  committed set as of this snapshot.
- **Snapshot-compatibility gate** — a read may use the cache IFF the cache is valid AND the reader's snapshot would
  see exactly the committed set the cache captured (conservatively: the cache is valid and current); else fall back.
- **Invalidate-on-write** — any INSERT/UPDATE/DELETE on the cached table marks its cache invalid (a trigger flips the
  `columnar.cache_state.valid` flag); the next read rebuilds or falls back to the heap.

### Architecture boundaries

Per `rules/architecture.md`: `arrow_cache.rs` is the application/engine layer; the invalidation trigger + catalog
are the persistence adapter; the CustomScan admission is the interface layer. No panic across C (Rule 8) — every
fallible path is a typed error, and a planner-hook admission failure is always a fail-safe fallback to the heap.

## Prior Art & Related Work

- **Pillar blueprint (SHIPPABLE 98.8):** `knowledge-base/discoveries/blueprints/single-planner-columnar-ai-blueprint.md`
  Q4 / D-γ — the heap-authoritative derived Arrow cache read zero-copy by the DataFusion executor.
- **AlloyDB columnar engine** (proprietary — design study only): the auto-maintained in-memory columnar cache over a
  row-store; M101 is the MANUAL-pragma permissive subset (NOT auto-tuned — declared honestly).
- **TheoDB own prior art:** `theodb_rs/src/am/{columnar_agg.rs,df_executor.rs}` (the M100 executor + CustomScan),
  M62 (the materialized non-interference pattern), `[[m99-columnar-tam-shipped]]` (isolation + benchmark harness).

## ADRs

### D1 — Heap-authoritative + invalidate-on-write + snapshot-gate (NOT re-implement MVCC)

**Decision:** the cache is a snapshot-tagged read-only copy of committed heap rows; a write invalidates it; a read
consults it only when valid AND snapshot-compatible, else the native heap plan runs. The cache NEVER carries its own
xmin/xmax visibility logic over individual rows (that would re-implement MVCC).
**Alternatives:** (A) per-row visibility in the cache (custom xmin/xmax) — REJECTED (re-implements MVCC, Rule 9 + the
parsimony ladder; the exact trap M99 D2 avoided). (B) no invalidation, TTL-only — REJECTED (a stale cache returns
wrong answers). **Rationale:** the heap's MVCC is the truth; the cache is a cache — correctness = "use only when
provably faithful", the same discipline as a query cache.

### D2 — Manual `columnarize` pragma (not workload auto-tuning)

**Decision:** an operator calls `columnar.columnarize('t', ARRAY['a','b'])` to build a cache of chosen columns;
auto-populate/evict by workload is a follow-up milestone.
**Alternatives:** auto-tune from workload stats now — REJECTED (that IS AlloyDB's proprietary engine; over-scoping;
YAGNI for M101). **Rationale:** the manual pragma proves the mechanism + MVCC correctness; auto-tuning is a separable,
larger bet (honest boundary — this is NOT AlloyDB's auto-maintained engine).

### D3 — Cache invalidation via a statement-level trigger on the heap table

**Decision:** `columnarize` installs an AFTER INSERT/UPDATE/DELETE/TRUNCATE statement trigger that flips
`columnar.cache_state.valid = false` for the table; the read path rebuilds on next use (or falls back).
**Alternatives:** invalidate from a TAM hook — REJECTED (heap has no per-table extension hook we own); logical
decoding — REJECTED (heavyweight, async, out of scope). **Rationale:** a statement trigger is the permissive, exact,
synchronous invalidation point; it fires within the writing xact so the flag flips atomically with the write.

## Dependency Graph

```
Phase A (cache build from heap + Arrow batch, standalone) ── gates ──▶ Phase B
Phase B (invalidate-on-write trigger + cache_state catalog + snapshot-gate) ── gates ──▶ Phase C
Phase C (CustomScan admits heap-with-valid-cache → vectorized aggregate; planner cost) ── gates ──▶ Phase D
Phase D (pg_isolation MVCC permutations + HTAP benchmark)
```

## Phase A — Build the Arrow cache from a heap table

### Task A1 — `columnar.columnarize(table, cols)` builds an in-memory Arrow cache; a test aggregate over it matches the heap

#### Why this step
The cache substrate — read a heap table's projected columns into an Arrow `RecordBatch` (via a seqscan under a build
snapshot) and run a DataFusion aggregate over it. De-risks the heap→Arrow path (distinct from M99 columnar decode)
before the MVCC machinery.

#### Files to edit
- `theodb_rs/src/am/arrow_cache.rs` (NEW) — `build_cache(rel, cols) -> RecordBatch` (heap seqscan → `build_arrow`),
  a per-backend/shared cache store keyed by relid, `run_aggs_on_batch` (split from `df_executor::run_columnar_aggs`).
- `theodb_rs/src/am/df_executor.rs` — extract `run_aggs_on_batch(batch, aggs)` so both the columnar and cache paths
  share the DataFusion-aggregate-under-HeldInterrupts+MemoryPool half.

#### TDD
- RED: `test_cache_agg_matches_heap` — `columnarize('h', ARRAY['measure'])`; a `count(*)`/`sum(measure)` over the
  cache equals the heap aggregate for 50k rows. Fails before the cache exists.
- GREEN: heap seqscan → Arrow arrays → cache batch; aggregate over it.
- REFACTOR: `run_aggs_on_batch` shared with the M100 columnar path (DRY).

#### Concurrency tests
`#### Concurrency tests` — (none — single-threaded) — the build runs under one backend's snapshot; cross-xact
visibility is Phase B/D. The cache store's access is single-backend in this slice.

#### Failure scenarios
`## Failure scenarios` — an unsupported column type in the pragma → typed error (not a panic); a work_mem overflow in
the aggregate → `ResourcesExhausted` clean error (reused M100 discipline).

#### Acceptance criteria
- `columnarize` builds a cache; an aggregate over it equals the heap for the supported types.

#### DoD
- `cargo pgrx test pg17 cache_agg` GREEN on the droplet.

## Phase B — Invalidate-on-write + snapshot-compatibility gate (the MVCC substrate)

### Task B1 — `columnar.cache_state` catalog + invalidation trigger + a snapshot-compatibility check

#### Why this step
The correctness core: a write invalidates the cache; a read uses it only when valid AND snapshot-compatible. Without
this, the cache returns snapshot-incorrect answers — the one thing a database must never do.

#### Files to edit
- `theodb_rs/src/am/arrow_cache.rs` — `extension_sql!` for `columnar.cache_state (relid oid PK, valid bool,
  built_xid xid8, ncols)` + the `columnar.columnarize` function that installs an AFTER INSERT/UPDATE/DELETE/TRUNCATE
  statement trigger flipping `valid=false`; `cache_is_usable(rel, snapshot) -> bool` (valid AND the reader's snapshot
  sees exactly the built committed set — conservative: valid AND reader snapshot ≥ built_xid horizon).
- The build tags the cache with the build snapshot's xid horizon.

#### TDD
- RED: `test_write_invalidates_cache` — build the cache, INSERT one row, the cache is marked invalid (the next read
  rebuilds or falls back → the new row is visible). Fails before the trigger exists.
- RED: `test_stale_snapshot_falls_back` — a reader whose snapshot predates the (rebuilt) cache does NOT use the cache
  (snapshot-gate) — proven via snapshot manipulation (the true cross-xact proof is Phase D).
- GREEN: the trigger + `cache_state` + the snapshot gate.
- REFACTOR: a `CacheGate` decision struct.

#### Concurrency tests
`#### Concurrency tests` — the single-process snapshot tests here do NOT prove race-freedom; the cross-xact proof is
the Phase D `pg_isolation_regress` permutation (a writer invalidating while a reader is mid-decision).

#### Failure scenarios
`## Failure scenarios` — the trigger fails to install (permissions) → `columnarize` errors typed, no half-built cache;
a `cache_state` row missing at read → treat as invalid (fail-safe, use the heap).

#### Acceptance criteria
- A write flips `valid=false`; a read after a write sees the new row (rebuild or fallback); an incompatible-snapshot
  reader falls back.

#### DoD
- `cargo pgrx test pg17 cache_invalidate` + `cache_snapshot` GREEN.

## Phase C — CustomScan admits heap-with-valid-cache

### Task C1 — the M100 aggregate CustomScan uses a valid, compatible cache for a heap table; planner falls back otherwise

#### Why this step
Wire the cache into the vectorized path: an admitted aggregate over a heap table WITH a usable cache runs over the
Arrow cache batch (vectorized); otherwise the native heap plan runs. This delivers the HTAP acceleration.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — extend `admit()`: a heap base rel is admissible IFF `cache_is_usable`;
  `begin` runs the aggregate over the cache batch (not a columnar decode). The columnar-table branch (M100) is
  unchanged.

#### TDD
- RED: `test_heap_cache_customscan_matches_heap` — with a valid cache + GUC on, `SELECT count(*), sum(measure) FROM h`
  is a CustomScan (EXPLAIN) and equals the native heap aggregate; after an INSERT (cache invalid) it falls back to the
  heap plan and still returns the correct (updated) result.
- GREEN: the heap-with-cache admission + exec-over-cache.
- REFACTOR: unify the columnar-table and heap-cache begin paths behind a `batch_source` enum.

#### Concurrency tests
`#### Concurrency tests` — (none — single-threaded here) — cross-xact is Phase D.

#### Failure scenarios
`## Failure scenarios` — cache invalidated between plan and exec → begin re-checks `cache_is_usable`; if now invalid,
error to force a re-plan OR (better) fall back — tested by invalidating mid-scan.

#### Acceptance criteria
- EXPLAIN shows the CustomScan when the cache is usable; result-equivalence with and without the cache; a write
  correctly routes the next read to the heap.

#### DoD
- `cargo pgrx test pg17 heap_cache_customscan` GREEN.

## Phase D — MVCC isolation permutations + HTAP benchmark (the gate)

### Task D1 — `pg_isolation_regress` MVCC permutations + HTAP benchmark

#### Why this step
"MVCC-correct cache" is over-claiming without concurrency permutations. And the HTAP claim (OLAP accelerated + OLTP
p95 not degraded) is a measured artifact, not an opinion.

#### Files to edit
- `theodb_rs/isolation/specs/arrow_cache_mvcc.spec` (+ expected) — a writer commits/aborts while a concurrent reader
  aggregates: the reader must see the snapshot-correct answer (never a stale cache row, never a phantom); an RR reader
  holds its snapshot across a cache rebuild.
- `theodb_rs/isolation/bench_m101.sh` + `docs/benchmarks/m101-arrow-cache.{md,json}` — OLAP-accelerated (cache) vs heap
  aggregate + OLTP p95 under a concurrent write load (non-interference, the M62 pattern), ≥ 3 runs.

#### TDD
- RED: the spec fails before the snapshot-gate + invalidation are correct; the benchmark harness runs.
- GREEN: the permutations pass (snapshot-correct under concurrency); the benchmark emits the artifact.
- REFACTOR: reproducibility (fixed seed, ≥ 3 runs, mean±stddev).

#### Concurrency tests
`#### Concurrency tests` — this task IS the concurrency proof: permutations covering writer-commits-mid-read,
writer-aborts-mid-read, and RR-reader-holds-snapshot-across-rebuild. Race-aware by construction (isolationtester).

#### Failure scenarios
`## Failure scenarios` — a writer commits between the reader's snapshot and its cache-decision → the reader must NOT
use a cache that would show the new row inconsistently with its snapshot (fall back); tested by the permutation.

#### Acceptance criteria
- `make check-isolation` GREEN for the MVCC spec; the benchmark shows OLAP acceleration + non-degraded OLTP p95,
  honest ceiling (manual pragma, not auto-tuned; refresh cost noted).

#### DoD
- Isolation spec GREEN; `docs/benchmarks/m101-arrow-cache.{md,json}` present with measured numbers.

## Coverage Matrix

| Requirement (ROADMAP M101 DoD) | Task(s) |
|---|---|
| (1) Arrow cache derived + refresh/invalidation on write | A1, B1 |
| (2) planner chooses cache vs heap by cost | C1 |
| (3) pg_isolation MVCC permutations green | D1 |
| (4) HTAP benchmark (OLAP accelerated + OLTP p95 non-degraded) | D1 |
| (5) sign-off council-index-storage + council-benchmark | Review phase |
| honest boundary (manual pragma, not auto-tuned; refresh cost) | D2 (ADR) enforced in the benchmark note |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Cache returns a snapshot-incorrect answer under concurrent write | HIGH | invalidate-on-write (statement trigger, fires in the writing xact) + a conservative snapshot-compatibility gate (fall back when in doubt); proven by D1 permutations | impl |
| Re-implementing MVCC in the cache (the M99 D2 trap) | HIGH | D1 ADR: heap-authoritative, no per-row visibility in the cache — it is consulted only when provably faithful | impl |
| Expensive refresh on hot tables | MEDIUM | the manual pragma leaves the decision to the operator (D2); invalidation is a flag flip, rebuild is lazy on next read | impl |
| OLTP interference from cache build/read | MEDIUM | the cache is read-only + built under a normal snapshot; D1 benchmark measures OLTP p95 non-degradation (the M62 pattern) | impl |
| Over-claiming AlloyDB parity (auto-tuned engine) | MEDIUM | honest boundary: manual pragma, NOT AlloyDB's auto-maintained engine; benchmark labels it | impl |

## Unresolved Questions

- **Cache residency (per-backend vs shared memory):** a per-backend cache is simplest + safe but not shared; a shared
  cache (dsm/dsa) is the AlloyDB model but adds concurrency complexity. Slice-1 uses a per-backend cache (resolved at
  A1); shared-memory residency is a follow-up (honest scope).
- **Snapshot-gate strictness:** the exact "snapshot-compatible" predicate (conservative fall-back vs precise) is
  finalized at B1 against the D1 permutations; the conservative rule (valid AND current) is correct but may fall back
  more than strictly necessary — an acceptable, honest trade.

## Failure scenarios

- **Concurrent write mid-read** (D1) — invalidation + snapshot-gate → the reader falls back to the heap, snapshot-correct.
- **Unsupported column type in the pragma** (A1) — typed error, no half-built cache.
- **cache_state row missing** (B1) — treated as invalid → the heap plan runs (fail-safe).

## Global DoD

- All Phase A–D tasks' `cargo pgrx test pg17` GREEN on the droplet (result-equivalence + MVCC).
- `make check-isolation` GREEN for the MVCC permutation spec.
- `docs/benchmarks/m101-arrow-cache.{md,json}` present with measured numbers, methodology, ≥ 3 runs, honest ceiling.
- No callback panics across C; the cache never returns a snapshot-incorrect answer.
- CHANGELOG `[Unreleased]` updated; no commits to main; no Co-Authored-By trailer.
- Files respect the ~500 LoC budget.
- Sign-off: council-index-storage + council-benchmark (review phase).

## Final Phase — Integration Validation

- Full `cargo pgrx test pg17` suite GREEN (no regression on M99 + M100 + the new M101 tests).
- The cache path result-equivalent to the heap; the MVCC permutations green.
- Benchmark artifact reproducible; honest ceiling stated (manual pragma, not auto-tuned).
- council-index-storage + council-benchmark review = READY_TO_MERGE before `/release`.
