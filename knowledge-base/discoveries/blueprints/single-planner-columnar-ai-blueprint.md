# Blueprint: Single-Planner In-Postgres Columnar + Vectorized Execution + AI (AlloyDB-class HTAP)

**Verdict:** `SHIPPABLE` (discovery) · **Recommendation:** **GO-CONDITIONAL** — feasible + paradigm-distinct from the shipped pg_duckdb ceiling, gated on a pgrx-upgrade spike (Q6) and the FFI-safety seam. **Honest ceiling locked:** DuckDB/Photon-class (15–30× on columnar-resident data) — capability-MATCH AlloyDB, NEVER superiority over its in-core in-memory engine (M73/M97 discipline).

**Date:** 2026-07-14 · **Slug:** `single-planner-columnar-ai` · **Cycle:** discover-execute (4 councils, file:line-grounded) · **Source plan:** `knowledge-base/discoveries/plans/single-planner-columnar-ai-plan.md` · **Dossier:** `knowledge-base/discoveries/single-planner-columnar-ai-research-dossier.md`

## Context

The owner directed a long-horizon bet (time NOT a constraint) to match AlloyDB's model — one engine, one planner,
columnar in-core — so AI + vector + columnar compose in a SINGLE query plan. The shipped route (pg_duckdb, M61/M62)
is two-engine (ADR-0023: `DuckDB execution not supported inside functions`). This discovery read the reference impls
to answer whether a permissive (Apache-2.0/MIT) single-planner path is buildable in pgrx PG17, and at what honest
ceiling. It also **corrects a factual error in ADR-0041 (M97): Hydra columnar is Apache-2.0, not AGPL** — a permissive
columnar TAM the M97 DEFER wrongly barred, and the DataFusion-CustomScan single-planner route M97 never examined.

## Objective

Deliver the grounded HOW (file:line from the reference clones) for the four load-bearing pieces + an honest
GO/NO-GO/DEFER + a scope ladder a multi-milestone roadmap can consume. Success = 9 research questions answered with
≥1 reference citation each, 4 corners populated, feasibility + ceiling verdict locked.

## Coverage Corner 1 — Integration Tests

**(Q9 — the correctness gate.)** Three test layers the references converge on, which TheoDB MUST reproduce:

1. **Result-equivalence (columnar path == row-store).** Run every analytical query twice — columnar-scan on vs heap
   baseline — and diff. Oracle: Hydra `columnar/src/test/regress/sql/columnar_fallback_scan.sql:15-28`
   (`count/min/max/avg` identical across the fallback toggle) + `columnar_vectorization.sql:2-8` + delete/update
   surviving-set `columnar_update_delete.sql:4-11` (all Apache-2.0).
2. **Concurrent-MVCC isolation (non-optional).** pgisolation `session/step/permutation` specs pinning the visible-row
   set per interleaving — the ONLY tool that catches a bug in the stripe-visibility / row_number→stripe search under
   concurrency. Pattern: `citus/src/test/regress/spec/columnar_write_concurrency.spec:15-90` [AGPL-DESIGN-ONLY]
   (uncommitted stripe invisible; committed becomes visible; REPEATABLE READ holds the old snapshot).
3. **VACUUM-vs-writer + crash-safety.** `citus/.../columnar_vacuum_vs_insert.spec:14-47` [AGPL-DESIGN-ONLY] (reclaim
   never corrupts an in-flight txn) + Hydra `columnar_vacuum.sql:11-43` (stripe-merge compacts) + TheoDB's existing
   index-AM WAL-replay invariant extended: insert stripes → restart → identical scan; abort-mid-write → restart → no
   partial stripe visible. Rust harness: `#[pg_test]` + the paradedb `#[rstest]`+sqlx `fetch_collect`+`assert_eq!`
   shape (`paradedb/tests/tests/query_json.rs:25-52` [AGPL-DESIGN-ONLY]).

Honest: result-equivalence + single-writer VACUUM are cheap; the **concurrent-MVCC isolation permutations are the
fragile gate** — "MVCC-correct columnar" without them green is M97 over-claiming (`plan-confidence-golden-rule.md`
concurrency-tests cap; `testing.md § 4.1`).

## Coverage Corner 2 — Dependencies

**(Q6 version matrix + Q7 TableAmRoutine FFI — the GO/NO-GO gates.)**

**Q6 — the sharp finding (a prerequisite, not a blocker):** pg_search proves the vectorized stack works, but at
**pgrx `=0.19.0`** (`paradedb/Cargo.toml:42`) with **datafusion `54` + arrow `58.1`** (`paradedb/pg_search/Cargo.toml:29-32,79`).
**TheoDB is on pgrx `=0.16.1`** (`theodb_rs/Cargo.toml:25`). So the datafusion+arrow coexistence is proven at 0.19.0,
NOT at 0.16.1 → the honest matrix:

| Component | pg_search (proven) | TheoDB (today) | Gate |
|---|---|---|---|
| pgrx | =0.19.0 | =0.16.1 | **pgrx 0.16.1→0.19.0 upgrade likely a PREREQUISITE milestone** (touches all AM code) |
| datafusion | 54 | — | adopt (Apache-2.0) |
| arrow | 58.1 (datafusion-main churns to 59.1 — `datafusion/Cargo.toml:92`) | — | version-fragile; pin + own a thin shim (DIP) |
| Rust | ≥1.88 (datafusion MSRV) | **1.91.0** (`rust-toolchain.toml`) | **already satisfied** ✓ |

The Rust-version fear is RESOLVED (1.91 ≥ 1.88). Coexistence of datafusion-54+arrow-58 with pgrx-0.16.1-or-0.19.0 in
ONE crate is **UNPROVEN until a `cargo tree`/build spike** — never asserted from pins (Rule 5, ADR D3). Use UPSTREAM
`apache/datafusion`, not pg_search's Apache-2.0 `datafusion-distributed` fork (`pg_search/Cargo.toml:81` — supply-chain hygiene, Rule 9).

**Q7 — `TableAmRoutine` FFI: FEASIBLE.** The binding is fully exposed in pgrx 0.16.1: `pub struct TableAmRoutine`
with 46 callback slots (`pgrx-pg-sys-0.16.1/src/include/pg17.rs:21046`), all `Option<unsafe extern "C-unwind" fn>` —
the exact discipline TheoDB already uses for `IndexAmRoutine` (`theodb_rs/src/am/mod.rs:32-97`, `PgBox::alloc_node`).
The WAL FFI Hydra needs is present too: `GenericXLogStart/RegisterBuffer/Finish` (`pg17.rs:37352-37358`), `log_newpage`
(`pg17.rs:37121`) — and TheoDB already drives that lifecycle with the abort path (`am/page.rs:87-99,174,330-367`).
Buildable append-only-first; the hard parts are semantic (MVCC/TID/vacuum), not the FFI.

## Coverage Corner 3 — Tools

**(Q8 — the build/test toolchain we mirror.)** pg_search builds as a standard pgrx extension: `pgrx.workspace`
feature-gated `pg15..pg18` (`pg_search/Cargo.toml:13-16`), `pg_test = ["dep:proptest"]` (`:17`), integration tests
under `pg_search/tests/` using `#[rstest]`+sqlx + pgrx `#[pg_test]` for the in-backend path. This is **byte-for-byte
TheoDB's existing `cargo pgrx test pg17` flow** (the droplet toolchain used across M83-M96) + `proptest` for the
columnar edge cases. No new toolchain — the delta is adding `datafusion`/`arrow` deps + the CustomScan test harness.
The correctness-critical addition: the pgisolation `permutation` specs (Corner 1) run via PG's `isolationtester`,
which TheoDB does not yet wire — a tools gap to close for the MVCC gate.

## Coverage Corner 4 — Techniques

### Q1 — the single-planner CustomScan ↔ Arrow ↔ DataFusion seam [pattern proven by pg_search, AGPL-DESIGN-ONLY]

A pgrx `CustomScan` is a sink-and-source bridge: at plan time 3 hooks (`set_rel_pathlist_hook`,
`set_join_pathlist_hook`, `create_upper_paths_hook`) register a `CustomPath` (`customscan/hook.rs:80/196/241`); at
exec time the callback lazily builds a DataFusion physical plan, `block_on`s the `SendableRecordBatchStream` one
`RecordBatch` at a time, and projects each Arrow row into `tts_values` (`aggregatescan/mod.rs:1540-1674`,
`datafusion_project.rs:41-155`). The reverse leaf implements DataFusion's `TableProvider`/`ExecutionPlan` to pull PG
data as `RecordBatch` (`scan/execution_plan.rs:646-792`, `table_provider.rs:675-748`). **This is ONE PG plan, ONE
backend — not two engines.** The seam is what TheoDB builds own-code; DataFusion is adopted.

**The highest-value safety artifact (own from day one):** `mpp/interrupt.rs:18-131` — a `CHECK_FOR_INTERRUPTS`
inside `block_on` runs `proc_exit` which does NOT unwind Rust → the live tokio runtime drops → process abort.
Mitigation = `HeldInterrupts` RAII (`HOLD_INTERRUPTS`/`RESUME_INTERRUPTS`) around the synchronous `block_on` + a
safe-point `check_for_interrupts!()` after. Plus: a `work_mem` `MemoryPool` that returns `ResourcesExhausted` instead
of panicking (`datafusion/memory.rs:28-113`), and `unsafe impl Send` on PG pointers justified ONLY by
single-thread-per-backend (`execution_plan.rs:80-83`) — which becomes a real data-race the moment DataFusion
multi-partition parallelism is used. TheoDB must convert pg_search's residual `panic!`s (`datafusion_project.rs:93,143`)
to `pg_sys::error!` (never-panic-across-C invariant).

### Q2 — Hydra columnar TAM: storage + MVCC/WAL/vacuum [Apache-2.0, transcription-safe]

Append-optimized analytical TAM. Layout: metapage (block 0) → logical byte-stream mapped `LogicalToPhysical`
(`columnar_storage.c:56-135`); **stripe** (150k rows, `columnar.c:29`) → **chunk_group** (10k, `:30`) → per-column
chunk; pluggable compression `none|pglz|lz4|zstd` per-column-per-chunk (`columnar.c:54-62`, `columnar_writer.c:399`);
**min/max skip nodes** per chunk for chunk-group pruning (`columnar_metadata.c:490-518`). **TID = synthetic
row_number bit-packed into (block,offset)** (`columnar_tableam.c:420-446`), resolved to a stripe by binary search
over the `columnar.stripe` heap catalog (`columnar_metadata.c:1195-1247`).

**The elegant MVCC trick:** visibility is resolved at STRIPE granularity through ordinary heap MVCC on the catalog —
`columnar_tuple_satisfies_snapshot` just does `FindStripeByRowNumber(rel, rowNumber, snapshot)` (`columnar_tableam.c:857-863`),
delegating to `systable_beginscan_ordered(..., snapshot, ...)` on `columnar.stripe`. No per-row xmin/xmax in data
pages. Deletes = a `columnar.row_mask` bitmap under a relation-level advisory lock (`columnar_tableam.c:1090-1112`);
updates = delete-old + append-new (`:1118-1160`, update-hostile). WAL = `GenericXLog` full-image per data-page write
(`columnar_storage.c:748`). VACUUM truncates trailing offsets; VACUUM FULL merges stripes. **Honest scope:
append-mostly analytical — no in-place update, writers serialize on an advisory lock, parallel/bitmap/sample/BRIN/FK
unsupported (`columnar_tableam.c:476-490,2135,2619-2627`). Claiming updatable HTAP from this base is over-claiming.**

### Q3 — DataFusion `ExecutionPlan` + `Expr` model [Apache-2.0, the adopt half]

The `ExecutionPlan` trait (`datafusion/physical-plan/src/execution_plan.rs:98`): `execute(partition, ctx) →
SendableRecordBatchStream` (`:518`) — pull-based boxed async stream; `properties()` (schema/partitioning/ordering),
`children()`, `with_new_children()` for optimizer rewrites. A leaf returns `children()=[]`; a unary op (`FilterExec`,
`filter.rs:520`) wraps the child stream. Programmatic build: high-level `DataFrame.aggregate/filter/...` →
`into_optimized_plan()` → `build_physical_plan()` → `.execute(0, ctx)`. `Expr` enum (`expr/src/expr.rs:326`):
`Column`/`Literal`/`BinaryExpr`/`AggregateFunction`/… A PG qual → `Expr` is a `NodeTag` walk: `OpExpr` → `BinaryExpr`
gated on `schema=="pg_catalog"` (else a user-defined `=` mistranslates) + type-coercion on the Const/Column boundary
(unhandled → return `None` and fall back to a non-pushed qual — the honest failure mode). **Difficulty MEDIUM — this
is the low-risk adopt half; own a thin translation shim behind our interface (DIP) to contain DataFusion's major-version churn.**

### Q4 — Lance: vector index natively in a columnar substrate [Apache-2.0]

Lance's insight: **an index IS just more columnar files.** A V3 vector index = 3 axes (IVF cluster / FLAT-or-HNSW
sub-index / PQ-SQ-RQ quant) → two Lance files: `index.idx` (HNSW graph as columns `__vector_id`/`__neighbors`/`_distance`)
+ `auxiliary.idx` (quantized codes as `__pq_code`/`__sq_code` columns) (`vector/index.md:85-189`). IVF is a
partition→contiguous-row-range map (`ivf/storage.rs:143-147`), centroids in the global buffer. Read path: scalar
predicate → (optional scalar index) → `RowAddrMask` loaded in background (`prefilter.rs:26-50`) → `IvfSubIndex.search(storage,
prefilter)` over only the probed partitions' code columns (`v3/subindex.rs:41-49`). Quantized vectors arrive as Arrow
`RecordBatch` columns (`flat/storage.rs:50-83`). **KEY insight for TheoDB (ε rung):** our IVF is already a
partition→postings map (`ann/ivf.rs`) and our AQ/SQ codes are already page-resident (ADR-0037); as Arrow columns
partitioned by row-range, filtered-vector + analytics share one columnar reader with the scalar prefilter as a
first-class `RowAddrMask`. **Honest limit: Lance is a FILE FORMAT, not a PG Table-AM (no MVCC/WAL/immutable segments)
→ lakehouse/FDW integration, NOT AM replacement.** It improves cost/scale/composability (the M83-M89 out-of-RAM line),
**NOT recall and NOT the ScaNN QPS gap (M73/M74 verdict stands).**

### Q5 — LOTUS + Palimpzest: AI operators as optimizable plan nodes [Apache-2.0 / MIT]

LOTUS = the semantic-operator algebra with sample-learned accuracy guarantees: `sem_filter` uses a proxy/oracle
**cascade** — a cheap proxy (small-LM logprobs OR embedding sim) scores all rows; only the uncertain band hits the
oracle LM; thresholds learned from a sample to hit a **recall/precision target with bounded failure probability**
(`sem_ops/sem_filter.py:446-603,139-235`). `sem_join` = embedding-sim blocker (`sem_join.py:343-372`); `sem_topk` =
LLM-comparator quickselect ~`2K+2N·logN` (`sem_topk.py:347-486`). Palimpzest = a Cascades optimizer costing each AI
op on **3 axes — cost/time/quality + selectivity — learned from sample execution** (`optimizer/cost_model.py:80-153,228-264`);
each physical implementation (model choice / code-synth / token-reduction) is a costed Pareto point; a `Policy` picks
the frontier (`policy.py:13-96`). The load-bearing rewrite: `PushDownFilter` moves a cheap relational predicate below
an expensive AI op ONLY when dependency-safe (`depends_on ∩ generated_fields = ∅`, `optimizer/rules.py:245-345`).
**KEY insight for TheoDB (δ rung):** make `ai.generate`/`AI.IF` a set-oriented `CustomScan`/table-func (not per-row
plpgsql) with a 3-axis cost hook on `am/cost.rs` + the dependency-safe push-down + optional LOTUS cascade. **Honest:
orthogonal to vector recall — a composability/cost win with STATISTICAL accuracy that must be reported with the
recall target + sample methodology (never "AI.IF is fast" without the quality point).**

## Cross-cutting Comparison

| Axis | AlloyDB (closed SOTA) | pg_duckdb (shipped, two-engine) | **This bet (DataFusion-CustomScan single-planner)** | Native columnar TAM (Hydra-model) |
|---|---|---|---|---|
| Planner | one, in-core | two (ADR-0023 ceiling) | **ONE** (PG plans, CustomScan executes vectorized) | one (TAM) but no vectorized exec |
| Columnar exec | in-memory auto | DuckDB (separate) | **DataFusion (Arrow, adopted Apache-2.0)** | PG row-at-a-time over columnar pages |
| Storage | in-memory cache over heap | Parquet (external) | Arrow cache (γ) OR native TAM (α) | native TAM pages |
| AI + vector + columnar in one plan | yes | no (functions can't call DuckDB) | **yes (CustomScan seam + AI-op nodes)** | partial (no vectorized AI) |
| License | proprietary | MIT | **Apache-2.0/MIT own-code glue** | Apache-2.0 (Hydra) |
| MVCC | heap-authoritative | external | heap-authoritative (γ) / append-only (α) | append-mostly |
| Honest ceiling | in-core in-memory | 15–23× (M97) | **DuckDB/Photon-class 15–30× on columnar-resident** | compression + skip only (~2–5×) |

The single-planner DataFusion-CustomScan route is **paradigm-distinct** from the shipped pg_duckdb ceiling and is the
option M97/ADR-0041 never examined. It is capability-competitive with AlloyDB's columnar layer, **never superiority**
over its in-core in-memory engine.

## ADRs

### D1 — GO-CONDITIONAL on the single-planner DataFusion-CustomScan route; supersede ADR-0041's DEFER

**Decision:** proceed to a multi-milestone roadmap for the single-planner columnar+AI bet, gated on the Q6 pgrx-upgrade
+ coexistence spike. This SUPERSEDES ADR-0041 (M97 DEFER) on two grounds: (a) **factual — Hydra columnar is Apache-2.0,
not AGPL** (`gh api repos/hydradatabase/columnar → Apache-2.0`; local top-level LICENSE is Apache 2.0), so a permissive
columnar TAM exists; (b) **scope — the DataFusion-CustomScan single-planner route was never examined by M97** (which
only looked at pg_duckdb/pg_mooncake/Hydra/Citus as two-engine-or-AGPL). **Rejected alternatives:** keep DEFER
(rejected — the two grounds above are new, decisive evidence); GO-unconditional (rejected — Q6 coexistence + the FFI
seam are real, spike-gated risks; Rule 1 95%-confidence forbids committing code before the spike).

### D2 — Honest ceiling locked from day one (M73/M97 discipline)

**Decision:** every performance claim gates on a `docs/benchmarks/` artifact; the target is **DuckDB/Photon-class
15–30× on columnar-RESIDENT data** (M97 measured DuckDB 15–23×; Photon SIGMOD 2022), capability-MATCH AlloyDB's
columnar layer, NEVER superiority over its in-core in-memory auto-tuned engine. Row-heap vectorization LOSES
(M61 0.63–0.89×) → the win requires columnar-resident data. **Rejected:** "faster than AlloyDB" framing — barred by
`public-copy.md` + the M73 vector precedent.

### D3 — Study-only boundary + own-code glue scope

**Decision:** paradedb/pg_search + Citus columnar are **DESIGN study only** (AGPL — pattern + file:line captured, code
NEVER transcribed); Hydra (Apache-2.0) + DataFusion/Arrow/Lance/LOTUS/Palimpzest (Apache/MIT) are reusable. The
own-code glue = the FFI seam (planner hooks + `#[pg_guard]` exec shims + the `block_on`/`HeldInterrupts` interrupt
discipline + Arrow→slot copy-out + `work_mem` MemoryPool + single-thread `Send` pinning) — small in LoC, high in
`unsafe`-density, where every TheoDB safety invariant applies at once. **Rejected:** adopt pg_search's code (AGPL —
D1 bars distribution); adopt the `datafusion-distributed` fork (third-party fork — Rule 9 supply-chain).

## Recommendations

1. **M-0 (prerequisite spike, GATE): pgrx 0.16.1→0.19.0 upgrade + datafusion-54/arrow-58 coexistence `cargo build`
   proof.** Until this is green, α/β are blocked (Q6, ADR D3). Measure: the extension builds + all 277 existing tests
   pass on 0.19.0 with datafusion+arrow linked.
2. **M-α (low risk): append-only native columnar TAM (Hydra-model, Apache-2.0 study).** `TableAmRoutine` (Q7) +
   stripe/chunk/skip + catalog-snapshot visibility + `GenericXLog` (reuse `am/page.rs`). Skip in-place update /
   parallel / bitmap / sample (Hydra's own NULL/ERROR set). Gate: result-equivalence + MVCC isolation permutations (Q9).
3. **M-β: DataFusion `CustomScan` vectorized executor over the columnar/Arrow batch.** The seam (Q1) + `ExecutionPlan`/`Expr`
   translation (Q3) + the interrupt/MemoryPool/Send safety discipline. Measure vs pg_duckdb + vs row-heap (honest 15–30×
   on columnar-resident, D2).
4. **M-γ: heap-authoritative Arrow columnar CACHE (AlloyDB model) — MVCC-correct HTAP over live heap;** manual
   "columnarize these columns" pragma first (auto-populate/evict is the ambitious tail).
5. **M-δ: AI operators as plan nodes** (`AI.IF`/`sem_filter` as a pushable CustomScan predicate) — 3-axis cost hook +
   dependency-safe push-down + optional cascade (Q5). Requires revisiting per-row HTTP inference (ADR-0007) for batching.
6. **M-ε: vector + columnar shared substrate** (Lance-inspired layout, Q4) — filtered vector + analytics in one scan
   (FDW/columnar side-store, NOT AM replacement; cost/scale win, not recall).
7. **Cross-cutting:** council-security reviews any NL→SQL/AI-operator surface; a watch-item on moonlink's BSL license
   (if it relicenses, auto-sync becomes obtainable).

## Prior Art & Related Work

- Dossier: `knowledge-base/discoveries/single-planner-columnar-ai-research-dossier.md` (source table + papers).
- ADRs superseded/amended: `docs/adr/0041-m97-columnar-defer.md` (Hydra-license error + unexamined route), `docs/adr/0023-m64-rag-unified-not-columnar-planner.md` (the two-engine ceiling this breaks), `docs/adr/0021-m62-htap-codegen-surface.md`.
- TheoDB prior art (transcription-safe): `theodb_rs/src/am/{mod.rs,page.rs,tid.rs,customscan.rs,cost.rs}` (IndexAmRoutine raw-FFI + GenericXLog + CustomScan seam + cost surface — the bet is NOT greenfield).

## Drawbacks & Risks

| # | Risk | Severity | Mitigation |
|---|---|---|---|
| 1 | pgrx 0.16.1 ≠ pg_search's 0.19.0 → coexistence unproven | HIGH | M-0 prerequisite spike (gate before α/β) |
| 2 | FFI seam: panic/proc_exit across a live tokio runtime = backend crash | HIGH | own the `HeldInterrupts` discipline + MemoryPool-errors-not-panics from day one (Q1) |
| 3 | `unsafe impl Send` on PG ptrs breaks under DataFusion parallel exec | HIGH | pin all partitions to the one backend thread; no multi-partition until proven |
| 4 | MVCC isolation bugs surface only under concurrency | HIGH | pgisolation permutation specs non-optional (Q9) |
| 5 | DataFusion/arrow major-version churn | MEDIUM | pin + thin translation shim behind our interface (DIP, Q3) |
| 6 | Multi-year effort vs uncertain differentiation | MEDIUM | scope ladder ships value incrementally (α analytical TAM is useful alone); honest-negative at any rung is a valid terminal |

## Unresolved Questions

- Does datafusion-54 + arrow-58 + pgrx-0.19.0 actually link + pass TheoDB's 277 tests in ONE crate? → M-0 spike (the GO/NO-GO gate; cannot be answered by reading, only by building — EC-2).
- Is the append-only TAM's analytical win (α, without vectorized exec) already worth shipping before β? → measured at M-α.

## Global Definition of Done

- [x] 9 research questions answered with ≥1 reference citation each (file:line)
- [x] 4 coverage corners populated
- [x] AGPL/BSL findings are DESIGN-only (no copied code — D3)
- [x] ADR amends ADR-0041's Hydra-license error + records the unexamined DataFusion-CustomScan route
- [x] honest ceiling locked (capability-match, not superiority)
- [ ] `/discover-confidence` verdict recorded (next)
