# Blueprint — M89 ambuild streaming (tuplesort/spool, bounded-memory IVF build)

**Date:** 2026-07-12 · **Milestone:** M89 · **Verdict source:** council-index-storage deep research (code+web-grounded, R0) + feasibility verification against pgrx 0.16.1 pg17 bindings.

## Problem (measured — M88 / ADR-0038)

`theodb_ivfflat` `ambuild` peaks at ~4× the base dataset in anon-rss → 2 OOM-kills at 30M on a 62 GB box. Root cause (code trace):
- `theodb_rs/src/am/build.rs:46` `collect_corpus` → `corpus: Vec<(i64,Vec<f32>)>` (1× base).
- `theodb_rs/src/ann/ivf.rs:28` `IvfflatIndex::build` **clones** the corpus into `self.vectors` (→ 2× base).
- `theodb_rs/src/am/build.rs:135` SQ8/v6 path **clones again** into `corpus_vecs` (→ 3× base) + buffers all `codes` (`build.rs:126`) + all `sq8_codes` (`build.rs:137`) before flushing (→ ~4× peak).

## Prior art — pgvector IVFFlat build (PostgreSQL License, permissive; `knowledge-base/references/pgvector/`)

pgvector **never materializes the corpus**. Pipeline (`src/ivfbuild.c`):
1. **Sample-train** — `ComputeCenters` (`ivfbuild.c:435`) reservoir-samples `lists*50` rows, runs k-means, then `VectorArrayFree(samples)` (`ivfbuild.c:479`). Guarded by `IvfflatCheckMemoryUsage` vs `maintenance_work_mem`.
2. **Stream-assign into a sorter** — `tuplesort_begin_heap(tupdesc, 1 key=list#, …, maintenance_work_mem, …)` (`ivfbuild.c:614`); `table_index_build_scan` drives `AddTupleToSort` (`ivfbuild.c:162`) which computes closest center inline and `tuplesort_puttupleslot((list#, tid, vector))` (`ivfbuild.c:216`). Header contract: *"Input data is always copied; the caller need not save it"* (`ivfbuild.c:213`) → each tuple flows through a per-callback temp context and is dropped. **Nothing accumulates in the backend heap.**
3. **Sort by list#** — `tuplesort_performsort` (`ivfbuild.c:1024`).
4. **Stream-write pages** — `InsertTuples` (`ivfbuild.c:272`) reads back sorted (`tuplesort_gettupleslot`), forms each `IndexTuple`, writes each list's run to sequential pages, `pfree`ing every tuple right after `PageAddItem`. Only one list is in flight.

**Peak-memory shape: `O(sample + centroids + maintenance_work_mem)`, independent of N** (external merge sort spills to temp files). ~2–3 GB at 30M *and* at 100M.

## Feasibility — pgrx 0.16.1 pg17 bindings (VERIFIED against `~/.cargo/.../pgrx-pg-sys-0.16.1/src/include/pg17.rs`)

ALL Stage-2 symbols are bound: `tuplesort_begin_heap`, `tuplesort_puttupleslot`, `tuplesort_gettupleslot`, `tuplesort_performsort`, `tuplesort_end`, `MakeSingleTupleTableSlot`, `ExecStoreVirtualTuple`, `CreateTemplateTupleDesc`, `TupleDescInitEntry`, `index_form_tuple`, `TTSOpsVirtual`, `Int4LessOperator`/`Int8LessOperator`/`TIDLessOperator`. `maintenance_work_mem` = the `workMem` arg (KB).
**FFI rough edges (bounded, known):** `ExecClearTuple` is `static inline` (unbound) → fill `slot.tts_values`/`tts_isnull` directly + `ExecStoreVirtualTuple` (also mark `tts_flags`/`tts_nvalid`). `table_index_build_scan` we already call successfully (`build.rs:52`).

## Decision (parsimony + honesty)

| Option | Peak @30M | Peak @100M | Change size | Hits DoD (≤1.5×@30M)? | Makes 100M buildable? |
|---|---|---|---|---|---|
| **A — clone-elimination only** (borrow corpus, drop 2 clones) | ~1.15× (~18 GB) | ~1.1× (~55 GB > 64 GB) | small, no FFI | ✅ | ✗ (1× corpus = 51 GB is the wall) |
| **B — tuplesort spool** (never hold corpus) | ~0.15× (~2–3 GB) | ~0.06× (~2–3 GB) | larger, FFI | ✅ | ✅ |

**M89 = A + B.** Option A alone technically passes the *literal* 30M/1.5× DoD, but the milestone is named "streaming" and its purpose (per M88/ADR-0038 + the M90 gate = the ≥100M measurement) is to make 100M+ buildable on commodity RAM — which **only B achieves**. Shipping A and calling it "streaming" would be a mislabel (the goal forbids workarounds). A is the natural first commit (un-regresses today, ~2× peak); B is the milestone.

## Architecture for M89

Mirror the pgvector pipeline inside `am/build.rs`:
1. **Sample-train kmeans** — reuse `IvfflatIndex::kmeanspp` on a bounded heap sample (already 1.1M-capped in `ivf.rs:13`); do NOT hold the full corpus.
2. **Assign-into-sorter** — in the `table_index_build_scan` callback, compute nearest centroid inline (per-vector, read-only over the small centroids) and `tuplesort_puttupleslot((list# i32, tid i64, vector bytea))`. Replaces `state.corpus.push` (`build.rs:281`).
3. **performsort → stream-write** — read back grouped by list#, pack AQ/SQ8 codes per list, write via the existing `page::write_ivf_*` writers (unchanged on-disk format → **no magic bump, no REINDEX**).

Crash-safety unchanged (pages via `GenericXLog`); the build→restart→scan-identical invariant is the acceptance test.

## Scope caveats (honest)

- **SOAR under streaming is DEFERRED.** `IvfflatIndex::with_soar_spill` (`ivf.rs:88`) needs the primary residual per vector (a second look at the vector) → doesn't fit the single-stream model cheaply. Scope M89 to the plain f32 (v5) + SQ8 (v6) path; SOAR (λ=0 default → off by default, no regression) stays on the in-RAM build path OR is a documented M89-follow-up. Must assert: `soar_lambda>0` + streaming = explicit path selection, never silent wrong result.
- **Parallel workers deferred.** pgvector parallelizes assignment via `Sharedsort`; a **serial** tuplesort build (leader-only, `coordinate=NULL`) is the minimal correct B and still bounds memory. Parallel is a follow-up optimization, not the DoD.
- **Small-N fast-path.** When `N·base ≤ maintenance_work_mem` the current in-RAM build is fine and byte-identical; keep it (fast-path) so ≤1M tests + benchmarks stay byte-identical. The streaming path activates for large N.

## Coverage corners

- **Techniques:** pgvector tuplesort/spool pipeline (`ivfbuild.c:162,216,272,614,1024`); Postgres external merge sort (bounded by `maintenance_work_mem`).
- **Dependencies:** none new — `tuplesort`/`TupleTableSlot` are core Postgres via `pg_sys` (Rule 9). pgrx 0.16.1 bindings confirmed.
- **Tools:** droplet memory-peak measurement (`/usr/bin/time -v` max RSS OR cgroup peak) at 16M/30M, old-build vs new-build.
- **Integration tests:** build→restart→scan-identical (crash-safety); byte-identical recall ≤1M A/B same-data (M46); pg_test on the streaming path at a size that forces spill.

## ADR

**ADR M89-1 — tuplesort spool over hand-rolled disk spill.** Alternatives rejected: (a) manual temp-file spill (reinvents external merge sort — Rule 9); (b) clone-elimination only (doesn't make 100M buildable — fails the milestone purpose). Chosen: Postgres `tuplesort_begin_heap` (what pgvector/btree use), FFI confirmed available. Cost: bounded `unsafe` for the virtual-slot fill.

## Citations

- pgvector `knowledge-base/references/pgvector/src/ivfbuild.c:162,213,216,272,435,479,614,1024`.
- Our code: `theodb_rs/src/am/build.rs:46,52,126,135,137,281`; `theodb_rs/src/ann/ivf.rs:13,26,28,88`.
- Bindings: `pgrx-pg-sys-0.16.1/src/include/pg17.rs` (tuplesort_begin_heap etc. present).
- M88 verdict: `docs/benchmarks/m88-billion-scale-verdict.md`, `docs/adr/0038-m88-billion-scale-regime-verdict.md`.
