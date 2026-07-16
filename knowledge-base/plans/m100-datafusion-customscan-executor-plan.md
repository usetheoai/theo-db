---
slug: m100-datafusion-customscan-executor
milestone_id: M100
created_at: 2026-07-16
goal: Ship a DataFusion CustomScan vectorized executor over the theodb_columnar TAM whose aggregate/GROUP BY result-matches a row-store byte-for-byte, proven by result-equivalence pg_tests + the interrupt/MemoryPool/Send safety discipline tested + a measured OLAP benchmark vs heap and pg_duckdb.
---

# M100 — DataFusion CustomScan vectorized executor over `theodb_columnar`

## Goal

Ship a single-planner DataFusion `CustomScan` vectorized executor over the M99 `theodb_columnar` TAM whose
aggregate/GROUP BY over a columnar table is **result-identical** to the same data in a row-store heap table — proven
by result-equivalence pg_tests, the interrupt/MemoryPool/Send safety discipline exercised by tests, and a measured
OLAP benchmark (`docs/benchmarks/m100-datafusion-executor.{md,json}`) columnar-vectorized vs heap vs pg_duckdb on
columnar-resident data. Metric: **the full M100 test set (equivalence + safety) GREEN on pg17 + a measured benchmark
artifact showing the vectorized path's speed-up over the M99 row-at-a-time seqscan on columnar-resident data.**

## Context

M99 (v0.87.0) shipped the columnar storage substrate: an own-code `theodb_columnar` TAM with column-major TCS1
stripes (per-column zstd chunks + a min/max skip directory) and MVCC delegated to the `columnar.stripe` heap catalog.
The M99 benchmark proved the honest limit: a plain seqscan of a columnar table is ~26× SLOWER than heap because it
decodes every column of every chunk group and reconstructs full heap tuples (form→deform), with no projection/skip/
vectorization. **M100 is where the scan speed finally comes from** — the DataFusion `CustomScan` that batches columnar
stripes into Arrow `RecordBatch`es, pushes projection + min/max pruning into the leaf, and runs a vectorized
`ExecutionPlan` in ONE plan (unlike pg_duckdb's two-engine ceiling, ADR-0023).

This is the pillar's **most dangerous FFI seam** (blueprint Drawback #2, HIGH): a synchronous `block_on` drives an
async tokio runtime inside a synchronous C executor callback. A query-cancel firing mid-`block_on` longjmps PAST the
live runtime → process abort. The safety discipline (`HeldInterrupts`, a `work_mem` `MemoryPool` that errors not
panics, single-thread `Send` pinning) is non-negotiable from day one — it is the own-code glue the blueprint scopes
(D3).

## Baseline Context

### Files that will be touched

| File | LoC today | Role | Change |
|---|---|---|---|
| `theodb_rs/src/am/customscan.rs` | 962 | the M92-95 vecfilter CustomScan (PATH/SCAN/EXEC methods, `pathlist_hook`, `plan_custom_path`, begin/exec/end) | REUSE the machinery; ADD a columnar-analytical CustomScan variant OR a sibling module |
| `theodb_rs/src/am/datafusion_probe.rs` | 108 | M98 smoke: `block_on` a DataFusion aggregate over a 3-row Arrow batch under `HeldInterrupts` | REUSE the `HeldInterrupts` pattern; the probe is the seed of the real executor |
| `theodb_rs/src/am/columnar.rs` | ~1240 | the columnar TAM (stripe catalog, TCS1 decode, min/max directory) | ADD a stripe→Arrow reader path (or expose the decode primitives to the new module) |
| `theodb_rs/src/am/columnar_codec.rs` | ~470 | pure column-major codec + min/max | REUSE `decode_column` + the `ChunkDirEntry` min/max for Arrow array build + pruning |
| `theodb_rs/src/am/df_executor.rs` | (NEW) | the DataFusion executor: `ColumnarTableProvider`, `ExecutionPlan`, `block_on` driver, MemoryPool, Send pinning | NEW — the milestone core |
| `theodb_rs/Cargo.toml` | ~60 | deps | `datafusion=54` + `arrow=58` + `tokio` (rt) already present (M98) — no new dep |
| `docs/benchmarks/m100-datafusion-executor.{md,json}` | (NEW) | benchmark artifact | NEW |

### Current callers / prior art in this repo (reuse, not greenfield)

- `am/customscan.rs:161-181` — `PATH_METHODS`/`SCAN_METHODS`/`EXEC_METHODS` + `pathlist_hook` (`:226`) already register a
  CustomScan and install `set_rel_pathlist_hook`. M100 adds a columnar-analytical path that wins for a scan of a
  `theodb_columnar` relation (detected by `rel->rd_tableam` == the columnar handler / `relam`).
- `am/datafusion_probe.rs:29-70` — the `HeldInterrupts` RAII + `rt.block_on(async { … DataFusion aggregate … })`
  pattern, PROVEN to run in a real PG backend (M98 `m98_datafusion_runs_in_backend` GREEN). The M100 executor
  generalizes this over real columnar Arrow batches.
- `am/columnar.rs` `read_visible_stripes` / `decode_stripe` / `ChunkDirEntry.min_bits/max_bits` — the visible-stripe
  enumeration + per-column decode + the STORED min/max the M100 leaf consumes for projection + skip-pruning.
- `am/customscan.rs` xact/subxact cleanup callbacks (`:189`,`:209`) — the leak-bounding discipline the new node reuses.

### Glossary

- **RecordBatch** — an Arrow columnar batch (a set of equal-length column arrays); the unit DataFusion vectorizes over.
- **TableProvider** — DataFusion's leaf trait; `ColumnarTableProvider::scan` returns an `ExecutionPlan` that yields
  `RecordBatch`es from the columnar stripes (projection + pruning pushed in).
- **`block_on`** — synchronously drive an async `SendableRecordBatchStream` to completion on the current thread.
- **`HeldInterrupts`** — RAII holding off `ProcessInterrupts` (`HOLD_INTERRUPTS`/`RESUME_INTERRUPTS`) so a mid-flight
  query-cancel cannot siglongjmp past the live tokio runtime and abort the backend.
- **MemoryPool** — DataFusion's memory accountant; bounded to `work_mem`, returns `ResourcesExhausted` (a typed error)
  instead of OOM-panicking.

### Architecture boundaries

Per `rules/architecture.md`: the CustomScan callbacks are the *interface* layer (called from the C executor);
`ColumnarTableProvider`/`ExecutionPlan` are the *application/engine* layer; the stripe decode reuses the M99 storage
adapter. No callback panics across C (`error-handling.md` + Rule 8) — every fallible path returns a typed error →
`pg_sys::error!`; DataFusion errors (incl. `ResourcesExhausted`) map to `ereport(ERROR)`, never a panic across the
tokio boundary.

## Prior Art & Related Work

- **Pillar blueprint (SHIPPABLE 98.8):** `knowledge-base/discoveries/blueprints/single-planner-columnar-ai-blueprint.md`
  — Q1 (the CustomScan↔Arrow↔DataFusion seam + the `HeldInterrupts`/MemoryPool/Send discipline, pg_search AGPL-design-
  only), Q3 (DataFusion `ExecutionPlan`/`Expr` model, Apache-2.0 adopt-half), D3 (own-code glue scope).
- **AGPL study-only (Rule 9):** `paradedb/pg_search/src/postgres/customscan/` — design literature only; copy no source.
- **Apache-2.0 adopt:** `apache/datafusion` (the `ExecutionPlan`/`TableProvider`/`MemoryPool` traits), `arrow-rs`.
- **TheoDB own prior art:** `theodb_rs/src/am/{customscan.rs,datafusion_probe.rs,columnar.rs,columnar_codec.rs,cost.rs}`
  — the CustomScan seam + the M98 block_on proof + the M99 columnar decode + min/max. NOT greenfield.
- **Honest ceiling precedent:** `[[m99-columnar-tam-shipped]]`, `[[goto-p0-vector-superiority]]` — DuckDB/Photon-class,
  capability-match not superiority; gain ONLY on columnar-resident data (M61 measured 0.63-0.89× on heap-resident).

## ADRs

### D1 — Own-code DataFusion CustomScan glue; pg_search (AGPL) as design literature only

**Decision:** build the columnar `CustomScan` executor from scratch (planner hooks + `#[pg_guard]` exec shims + the
`block_on`/`HeldInterrupts` discipline + Arrow→slot copy-out + `work_mem` MemoryPool + single-thread `Send` pinning);
study pg_search's customscan as design literature; copy no AGPL source; adopt only Apache-2.0 `datafusion`/`arrow`.
**Alternatives:** (A) adopt pg_search's customscan directly — REJECTED, AGPLv3 barred by D1 license gate. (B) keep
pg_duckdb (shipped) — REJECTED for this pillar, its two-engine ceiling (ADR-0023 `ERROR: DuckDB execution not
supported inside functions`) is paradigm-blocked from a single plan. **Rationale:** the FFI glue is small-LoC/high-
risk own code (blueprint D3); the vectorized engine is a solved Apache-2.0 library to reuse (Rule 9).

### D2 — Interrupt discipline: hold across each batch, not the whole `block_on`; safe-point between batches

**Decision:** the executor drives the `SendableRecordBatchStream` batch-by-batch; `HeldInterrupts` wraps each
`block_on(stream.next())` poll, with a `check_for_interrupts!()` safe-point BETWEEN batches — so a long analytical
scan stays cancellable while never longjmp-ing past the live runtime mid-poll (the M98 probe note: holding across the
WHOLE block_on is fine for 3 rows but wrong for a real scan).
**Alternatives:** hold across the whole scan — REJECTED, an uncancellable multi-second scan is a DoS/ops hazard.
**Rationale:** `error-handling.md` (fail-fast, recoverable) + the M98 review H1 finding.

### D3 — `work_mem` MemoryPool that errors, single-thread Send pinning (no multi-partition until proven)

**Decision:** the DataFusion `RuntimeEnv` uses a `GreedyMemoryPool`/custom pool capped at `work_mem` returning
`ResourcesExhausted`; all PG-pointer-bearing structs are `unsafe impl Send` justified ONLY by pinning every partition
to the one backend thread (`target_partitions=1`); no DataFusion multi-partition parallelism until a later milestone
proves it safe.
**Alternatives:** unbounded pool (panics on OOM) — REJECTED (Rule 8); multi-partition now — REJECTED (Drawback #3,
HIGH: `Send` on PG ptrs races under parallel exec). **Rationale:** blueprint Q1/D3 + Drawback #3.

### D4 — Honest ceiling: gain ONLY on columnar-resident data; NOT superiority vs AlloyDB in-core

**Decision:** the benchmark measures the vectorized path vs the M99 row-at-a-time seqscan vs heap vs pg_duckdb, on
COLUMNAR-RESIDENT data; the claim is DuckDB/Photon-class capability, never superiority over AlloyDB's in-core engine.
**Alternatives:** claim generic OLAP superiority — REJECTED (M73/M97 discipline; M61 measured no gain on heap-resident).
**Rationale:** `public-copy.md` §4 + Rule 5.

## Dependency Graph

```
Phase A (block_on executor over a real columnar Arrow batch, standalone #[pg_extern]) ── gates ──▶ Phase B
Phase B (ColumnarTableProvider: stripes→Arrow + projection + min/max pruning)          ── gates ──▶ Phase C
Phase C (planner CustomScan integration: EXPLAIN node + result-equivalence)            ── gates ──▶ Phase D
Phase D (safety hardening: per-batch interrupts + MemoryPool-errors + Send pin; benchmark)
```

Phases are strictly sequential (each de-risks the next). Phase A de-risks the async-in-C seam over REAL columnar data
BEFORE the planner wiring — the same "prove the dangerous FFI first" discipline as M99 Phase A.

## Phase A — `block_on` executor over a real columnar Arrow batch (de-risk the seam)

### Task A1 — read a columnar table's stripes into an Arrow `RecordBatch` and run a DataFusion aggregate under `HeldInterrupts`

#### Why this step
The single riskiest thing in the pillar is driving an async tokio runtime inside a sync C callback over REAL data. We
de-risk it in isolation (a `#[pg_extern]` test fn, no planner) exactly as `datafusion_probe.rs` de-risked the 3-row
case and M99 Phase A de-risked TAM registration. If `block_on` over a real multi-batch columnar stream is safe under
`HeldInterrupts` + a `work_mem` MemoryPool, the planner wiring (Phase C) is a known-safe extension.

#### Files to edit
- `theodb_rs/src/am/df_executor.rs` (NEW) — `columnar_to_record_batches(rel, projection) -> Vec<RecordBatch>` (reuse
  `columnar::read_visible_stripes` + `decode_column` → build Arrow `ArrayRef` per column), a `RuntimeEnv` with a
  `work_mem`-bounded MemoryPool, the `HeldInterrupts` RAII (moved/shared from `datafusion_probe.rs`), and a
  `#[pg_extern] theodb_df_columnar_agg(rel_oid, sql) -> String` test driver that `block_on`s a DataFusion aggregate.
- `theodb_rs/src/am/mod.rs` — `mod df_executor;`.

#### TDD
- RED: `test_df_columnar_agg_matches_heap` — insert the same 50k rows into a `theodb_columnar` table and a heap table;
  `theodb_df_columnar_agg(rel, "SELECT count(*), sum(measure) FROM t")` equals the heap's `SELECT count(*), sum(measure)`.
  Fails before the executor exists.
- GREEN: build Arrow arrays from decoded columns; `block_on` a DataFusion `SessionContext` aggregate under `HeldInterrupts`.
- REFACTOR: extract per-PG-type → Arrow `DataType` mapping behind a `col_to_arrow` helper (SRP).

#### Concurrency tests
`#### Concurrency tests` — the async runtime is single-threaded pinned (`target_partitions=1`, D3). Test: a scan that
holds interrupts and completes without a second thread; the true cancel-mid-scan safe-point is Phase D. No shared
mutable PG state crosses a thread boundary (Send pinning) — asserted structurally (single-thread runtime).

#### Failure scenarios
`## Failure scenarios` — (a) a DataFusion `ResourcesExhausted` (MemoryPool cap hit) must surface as a typed
`ereport(ERROR)`, never a panic across the tokio boundary; test by capping `work_mem` tiny and asserting a clean SQL
error. (b) a decode error on a corrupt stripe → typed error, no runtime drop.

#### Acceptance criteria
- Aggregate over columnar == heap for ≥ 50k rows across int/bigint/float8/text; a tiny `work_mem` yields a clean
  `ResourcesExhausted` SQL error (not a crash); `HeldInterrupts` wraps the `block_on` (no proc_exit-past-runtime).

#### DoD
- `cargo pgrx test pg17 df_columnar_agg` GREEN on the droplet.

## Phase B — `ColumnarTableProvider`: stripes → Arrow + projection + min/max pruning

### Task B1 — a DataFusion `TableProvider`/`ExecutionPlan` that pulls only projected columns + prunes chunk groups by min/max

#### Why this step
The columnar gain (over the M99 seqscan) comes from (a) decoding ONLY projected columns and (b) SKIPPING chunk groups
whose stored min/max excludes the scan's filter — the consumption the M99 milestone stored but a plain seqscan could
not use. A `TableProvider` receives the projection + filters from DataFusion, so this is where projection + pruning
finally land.

#### Files to edit
- `theodb_rs/src/am/df_executor.rs` — `ColumnarTableProvider` (impl `TableProvider`) + `ColumnarExec` (impl
  `ExecutionPlan`) streaming `RecordBatch`es; consume `projection: Option<&Vec<usize>>` (decode only those columns) and
  `filters: &[Expr]` (map simple `col op const` to a min/max chunk-group prune via `ChunkDirEntry`).
- `theodb_rs/src/am/columnar.rs` — expose `pub(super)` decode primitives (directory + per-chunk decode) to `df_executor`.

#### TDD
- RED: `test_projection_reads_fewer_columns` — a `SELECT sum(measure)` over a wide columnar table decodes ONLY the
  `measure` column (assert via a decode counter / instrumentation), result still correct.
- RED: `test_min_max_prune_skips_chunk_groups` — a `WHERE id > K` that excludes whole chunk groups reads fewer groups
  (assert via a pruned-group counter) while returning the correct rows.
- GREEN: projection-pushed decode + min/max prune-decision (reuse the stored `min_bits/max_bits` + a per-PG-type
  comparator from the typecache B-tree cmp, the M99-council pattern).
- REFACTOR: a `PruneDecision` helper per Arrow `DataType`.

#### Concurrency tests
`#### Concurrency tests` — (none — single-threaded) — the provider runs on the pinned backend thread; no cross-thread state.

#### Failure scenarios
`## Failure scenarios` — a filter type the pruner cannot evaluate → fail-SAFE (do not prune, read the group) never
fail-wrong (the M99 `has_minmax=0` fallback discipline).

#### Acceptance criteria
- Projection decodes only referenced columns; a range filter skips ≥ 1 chunk group with correct results; unsupported
  filters fall back to full read (never wrong).

#### DoD
- `cargo pgrx test pg17 df_projection` + `df_prune` GREEN.

## Phase C — planner CustomScan integration (EXPLAIN node + result-equivalence)

### Task C1 — a CustomScan path that wins for a columnar-table scan, executing the DataFusion plan and projecting RecordBatch → TupleTableSlot

#### Why this step
The headline DoD: `SELECT`/aggregate over a columnar table runs through the vectorized DataFusion executor in ONE plan
(EXPLAIN shows the node), result-identical to a row-store. This wires Phase A/B into the executor via the existing
`customscan.rs` machinery.

#### Files to edit
- `theodb_rs/src/am/customscan.rs` (or a new `columnar_customscan.rs`) — a `pathlist_hook` branch that, when the scanned
  relation's `relam` is `theodb_columnar` AND the query is analytical (aggregate/projection), adds a `CustomPath`;
  `plan_custom_path` → `CustomScan`; `begin/exec/end` drive the `ColumnarExec` stream and project each `RecordBatch`
  column → `TupleTableSlot` (Arrow array → PG datum copy-out).
- `theodb_rs/src/am/df_executor.rs` — the `RecordBatch` → `TupleTableSlot` projection (Arrow `ArrayRef` value → PG Datum).

#### TDD
- RED: `test_columnar_customscan_matches_rowstore` — same 100k-row dataset in a columnar + heap table; `SELECT
  count(*), sum(a), avg(a), min(b), max(b) FROM t` IDENTICAL; `EXPLAIN` over the columnar table shows the CustomScan node.
- GREEN: pathlist hook + plan + exec projecting Arrow→slot.
- REFACTOR: gate the path behind a `theodb.enable_columnar_vectorize` GUC (default on for columnar rels; off = M99 seqscan).

#### Concurrency tests
`#### Concurrency tests` — (none — single-threaded) — `target_partitions=1`; the CustomScan runs on the backend thread.

#### Failure scenarios
`## Failure scenarios` — a query shape the executor cannot handle (unsupported agg/expr) → fall back to the M99 seqscan
path (never wrong, never crash); tested by an unsupported expression asserting the correct result via fallback.

#### Acceptance criteria
- EXPLAIN shows the CustomScan node for a columnar analytical query; aggregates + ordered results are byte-identical to
  the heap for ≥ 100k rows across all supported types; unsupported shapes fall back correctly.

#### DoD
- `cargo pgrx test pg17 columnar_customscan_matches` GREEN.

## Phase D — safety hardening + measured benchmark (the gate)

### Task D1 — per-batch interrupt safe-points + MemoryPool-errors-not-panics + Send-pinning, all tested

#### Why this step
The FFI seam's safety is the DoD's hardest item: a mid-scan cancel must NOT longjmp past the runtime; a work_mem
overflow must ERROR not panic; the `unsafe impl Send` must be sound under `target_partitions=1`. These are proven by
adversarial tests, not assumed.

#### Files to edit
- `theodb_rs/src/am/df_executor.rs` — batch-by-batch `block_on` with `HeldInterrupts` per poll + `check_for_interrupts!()`
  safe-point between batches (D2); the `work_mem` MemoryPool wired into the `RuntimeEnv`; `unsafe impl Send` with a
  single-thread invariant comment.

#### TDD
- RED: `test_work_mem_overflow_errors_cleanly` — a query that exceeds a tiny `work_mem` returns a clean
  `ResourcesExhausted` SQL error, backend stays alive (a second query on the same connection succeeds).
- RED: `test_cancel_between_batches_is_safe` — a scan that is cancelled at a between-batch safe-point returns a clean
  cancel error, backend survives (simulated via the interrupt flag; the true cancel is an isolation/manual proof).
- GREEN: per-batch interrupt discipline + MemoryPool wiring.
- REFACTOR: the interrupt/pool discipline behind a `run_vectorized(plan) -> Result` driver (single choke point).

#### Concurrency tests
`#### Concurrency tests` — this task IS the concurrency/interrupt proof: the between-batch safe-point + `HeldInterrupts`
per poll are race-aware by construction; a cancel firing mid-poll is held, a cancel at the safe-point is honored. The
`Send` soundness is asserted by `target_partitions=1` (no second thread touches PG pointers).

#### Failure scenarios
`## Failure scenarios` — (a) work_mem overflow → `ResourcesExhausted` → clean SQL error, backend alive; (b) cancel
mid-scan → held during poll, honored at safe-point → clean cancel, backend alive; (c) a DataFusion internal error →
typed `ereport(ERROR)`, runtime not dropped mid-flight.

#### Acceptance criteria
- work_mem overflow errors cleanly (backend survives a follow-up query); cancel is honored at safe-points without a
  backend crash; no `unsafe impl Send` is exercised across a real second thread.

#### DoD
- `cargo pgrx test pg17 df_safety` GREEN.

### Task D2 — OLAP benchmark columnar-vectorized vs heap vs pg_duckdb (measured, honest)

#### Why this step
The DoD's measured artifact: the vectorized path's speed-up over the M99 row-at-a-time seqscan on columnar-resident
data, with the honest DuckDB/Photon-class ceiling (never superiority vs AlloyDB in-core).

#### Files to edit
- `theodb_rs/isolation/bench_m100.sh` (NEW) — the benchmark harness (reuse the M99 harness shape); measures the
  vectorized CustomScan vs the M99 seqscan (GUC off) vs heap vs pg_duckdb (if installed), same data, ≥ 3 runs, mean±stddev.
- `docs/benchmarks/m100-datafusion-executor.{md,json}` (NEW) — the measured artifact.

#### TDD
- RED: the benchmark harness runs and emits numbers (no assertion — a benchmark, not a unit test); result-equivalence
  cross-check (vectorized == heap) inside the harness gates the run.
- GREEN: the harness measures + emits the artifact with methodology + honest ceiling note.
- REFACTOR: fixed seed, ≥ 3 runs, mean±stddev (council-benchmark rules).

#### Failure scenarios
`## Failure scenarios` — pg_duckdb not installed → the harness runs without it and notes the gap honestly (never fabricate
a pg_duckdb number).

#### Acceptance criteria
- The artifact shows the vectorized path's measured speed-up vs the M99 seqscan on columnar-resident data, result-
  equivalent to heap, with the honest ceiling note (DuckDB/Photon-class, NOT AlloyDB-in-core superiority).

#### DoD
- `bash theodb_rs/isolation/bench_m100.sh` produces `docs/benchmarks/m100-datafusion-executor.{md,json}` with measured numbers.

## Coverage Matrix

| Requirement (ROADMAP M100 DoD) | Task(s) |
|---|---|
| (1) CustomScan DataFusion over the M99 TAM, single plan (EXPLAIN shows the node) | C1 |
| (2) result-equivalence vs row-store | A1, C1 |
| (3) interrupt/MemoryPool/Send discipline implemented + tested (crash-under-interrupt does not kill the backend) | A1 (seed), D1 (proof) |
| (4) measured OLAP benchmark vs pg_duckdb + heap, honest DuckDB/Photon ceiling | D2 |
| (5) sign-off council-rust-pgrx (FFI/panic-across-C) + council-benchmark | Review phase |
| projection pushdown (decode only needed columns) | B1 |
| min/max skip-pruning CONSUMPTION (the M99-stored directory) | B1 |
| honest boundary (gain only on columnar-resident data; not AlloyDB-in-core superiority) | D4 (ADR) enforced in D2 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| FFI seam: a cancel/panic/proc_exit across a live tokio runtime crashes the backend | HIGH | `HeldInterrupts` per poll + between-batch safe-point (D2); every panic → `pg_sys::error!`; MemoryPool errors not panics (D3); proven in D1 tests | impl |
| `unsafe impl Send` on PG pointers races under DataFusion parallel exec | HIGH | `target_partitions=1`, single-thread pinning; no multi-partition until a later milestone proves it (D3) | impl |
| Over-claiming OLAP superiority (gain mistaken for AlloyDB-beating) | MEDIUM | benchmark labels the ceiling: DuckDB/Photon-class on columnar-resident only; no "faster than AlloyDB" (D4, public-copy.md) | impl |
| DataFusion 54 API churn on later bumps | MEDIUM | a thin shim behind our `run_vectorized` interface (DIP); pin datafusion=54 (already pinned) | impl |
| Arrow→slot copy-out overhead eats the vectorization gain | MEDIUM | measure it (D2); copy only projected columns; the honest-negative is a valid terminal (blueprint Unresolved) | impl |

## Unresolved Questions

- **Arrow→slot copy-out cost vs gain:** whether the per-row Arrow→Datum copy-out at the CustomScan boundary eats the
  vectorized aggregate gain is an empirical question resolved by the D2 benchmark; honest-negative (no net gain over
  heap) is a valid terminal per the blueprint (the gain would then require pushing the aggregate result up, not rows).
- **Which query shapes to accept in v1:** aggregates + projections + simple filters; complex joins/window functions
  fall back to the M99 seqscan. The exact accepted-shape set is finalized at C1 by what maps cleanly to a DataFusion
  `Expr` (gate `schema=="pg_catalog"`).

## Failure scenarios

- **work_mem overflow** (D1) — DataFusion `ResourcesExhausted` → typed `ereport(ERROR)`, backend survives a follow-up query.
- **query-cancel mid-scan** (D1) — held during a poll, honored at the between-batch safe-point → clean cancel, backend alive.
- **corrupt stripe / unsupported filter** (A1, B1) — typed error / fail-safe fallback, never a panic across the runtime.
- **pg_duckdb absent** (D2) — benchmark runs without it, gap noted honestly.

## Global DoD

- All Phase A–D tasks' `cargo pgrx test pg17` GREEN on the droplet (result-equivalence + safety).
- `docs/benchmarks/m100-datafusion-executor.{md,json}` present with measured numbers, methodology, ≥ 3 runs, honest ceiling.
- No callback panics across C; no proc_exit/cancel past the live runtime (HeldInterrupts + safe-points); MemoryPool errors, never panics.
- CHANGELOG `[Unreleased]` updated; no commits to main; no Co-Authored-By trailer.
- Files respect the ~500 LoC budget (split `df_executor.rs` if it grows; keep the FFI shim thin).
- Sign-off: council-rust-pgrx + council-benchmark (review phase).

## Final Phase — Integration Validation

- Full `cargo pgrx test pg17` suite GREEN (no regression on the M99 + M98 tests + the new M100 tests).
- The vectorized path result-equivalent to heap across all supported types + query shapes.
- Benchmark artifact reproducible from a documented command; honest ceiling stated.
- council-rust-pgrx + council-benchmark review = READY_TO_MERGE before `/release`.
