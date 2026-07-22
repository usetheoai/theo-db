---
slug: ambuild-tuplesort-streaming
milestone_id: M96
created_at: 2026-07-13
goal: Replace the corpus-materializing IVF-AQ build with a tuplesort spool so peak build RSS is O(maintenance_work_mem + sample) independent of N — measured at 30M as a ≤0.5× base-dataset peak (vs the M88 4.21× OOM baseline) with byte-identical recall on the ≤1M A/B path.
---

# M96 — tuplesort-streaming ambuild (bounded-memory IVF build)

## Context

M89 (ADR-0039) cut the build peak from ~4.21× base (M88 OOM at 30M) to ~1.28×(v5)/1.50×(v6) by MOVING the corpus
into the index (`build_owned`) and streaming the per-list page writes. But the peak still holds **1× the corpus**
(`idx.vectors`, ~15.4 GB at 30M / ~51 GB at 100M) → 100M does not fit in commodity RAM. The M88 out-of-RAM
crossover measurement stayed "direcional-não-provada" for lack of a 100M-capable build.

The blueprint (`ambuild-streaming-blueprint.md`, SHIPPABLE — council-index-storage, feasibility-verified against the
pgrx 0.16.1 pg17 bindings) is Option B: mirror pgvector's `ivfbuild.c` — never materialize the corpus. Two heap
scans (sample-train, then stream-assign into a `tuplesort`), sort by list#, read back grouped and stream-write
pages. Peak = `O(sample + centroids + maintenance_work_mem)`, independent of N.

## Goal

Replace the corpus-materializing IVF-AQ build with a tuplesort spool so peak build RSS is O(maintenance_work_mem +
sample) independent of N — measured at 30M as a ≤0.5× base-dataset peak (vs the M88 4.21× OOM baseline) with
byte-identical recall on the ≤1M A/B path.

## Baseline Context

### Files that will be touched

| File | LoC today | Last touch | Role |
|---|---|---|---|
| `theodb_rs/src/am/build.rs` | ~1450 | M89/M90 | `ambuild` + `collect_corpus` + the build-scan callback (`corpus.push`) |
| `theodb_rs/src/am/build_stream.rs` | 0 (NEW) | — | the tuplesort spool: begin/put/performsort/get FFI + the streaming pipeline |
| `theodb_rs/src/ann/ivf.rs` | ~330 | M89 | `IvfflatIndex::build_owned`; add a `from_streamed` constructor that takes pre-assigned list-grouped data |

### Current callers / dependents (from code read)

| Symbol | Defined | Called by | Notes |
|---|---|---|---|
| `collect_corpus` | `build.rs:63` | `ambuild` (`build.rs:114`) | drives `table_index_build_scan` → `corpus.push((tid,v))` (`build.rs:342`) — the 1× materialization M96 replaces |
| `IvfflatIndex::build_owned` | `ivf.rs:35` | `ambuild` (`build.rs:130`) | kmeans + parallel assign; holds `vectors` 1× |
| `page::write_ivf_aq_split*` | `page.rs` | `ambuild` per layout (v5/v6/v7) | consume `positions/ids/vecs/codes` arrays — M96 feeds them per-list from the sorter |
| `pack_block32_codes` / `Sq8Quantizer` / `AqQuantizer` | `build.rs`/`sq8.rs`/`aq.rs` | `ambuild` | train on a bounded sample (already capped); encode reads vectors — M96 reads them from the sorted tuple |

### Domain glossary

- **tuplesort spool** — Postgres external merge sort (`tuplesort_begin_heap`, spills to temp files past `maintenance_work_mem`); the caller `puttupleslot`s rows and reads them back sorted, holding only `O(mwm)` in RAM.
- **stream-assign** — in the build-scan callback, compute each vector's nearest centroid inline and `puttupleslot((list# i32, tid i64, vector bytea))`, dropping the vector immediately (never accumulate).
- **fast-path** — when `N × dim × 4 ≤ maintenance_work_mem` the in-RAM `build_owned` path is kept (byte-identical for ≤1M tests/benchmarks); streaming activates only for large N.
- **spill-forcing** — set `maintenance_work_mem` low so the sorter spills at a MODERATE N — the deterministic proof the mechanism bounds memory without needing a 100M box.

### Architecture boundaries affected

`am/build.rs` (interface — the AM build entrypoint) + a new `am/build_stream.rs` module (the FFI spool) +
`ann/ivf.rs` (a streamed constructor). The on-disk page format is UNCHANGED (same `write_ivf_*` writers, same
bytes) → **no magic bump, no REINDEX**. Crash-safety unchanged (pages via `GenericXLog`).

## Prior Art & Related Work

- **Blueprint `ambuild-streaming-blueprint.md`** (SHIPPABLE) — the pgvector `ivfbuild.c` pipeline mapped line-by-line (`:162,213,216,272,435,479,614,1024`), the FFI feasibility (all tuplesort symbols bound in pgrx 0.16.1), the A+B decision, the SOAR/parallel/small-N scope caveats.
- **pgvector `ivfbuild.c`** (PostgreSQL License, `knowledge-base/references/pgvector/`) — the reference pipeline (study, own code; Rule 9).
- **M89 (`docs/adr/0039`)** — the streaming writers + `build_owned` move this extends; the 1× wall it left.

## ADRs

### ADR M96-1 — tuplesort spool over hand-rolled temp-file spill

**Decision:** use `tuplesort_begin_heap` (what pgvector and btree builds use) for the assign→sort→read-back spool.

**Rejected alternatives:** (a) *hand-rolled disk spill* — REJECTED: reinvents Postgres external merge sort (Rule 9). (b) *clone-elimination only* — REJECTED: the 1× corpus (51 GB at 100M) is the wall; only never-materializing clears it. This is the blueprint's A-vs-B; M89 already shipped A.

### ADR M96-2 — two heap scans (sample-train, then stream-assign)

**Decision:** scan 1 reservoir-samples for kmeans + AQ + SQ8 training (bounded sample, already capped at 1.1M/50k); scan 2 (`table_index_build_scan`) assigns each vector to its trained centroid inline and puts it in the sorter.

**Rejected alternative:** *single scan holding a sample buffer* — REJECTED: kmeans needs the centroids BEFORE assignment can compute list#; pgvector uses the same two-pass shape (`ComputeCenters` then the assign scan). The second scan re-reads the heap (bounded I/O cost the blueprint's R3 measures), never re-materializing.

### ADR M96-3 — keep the in-RAM fast-path for small N

**Decision:** when `N × dim × 4 ≤ maintenance_work_mem`, keep the existing `build_owned` in-RAM path (byte-identical); the streaming path activates only above that threshold.

**Rejected alternative:** *stream always* — REJECTED: the tuplesort has fixed overhead + the ≤1M recall/persist tests must stay byte-identical (the M46 A/B invariant). KISS: don't pay the spool cost when the corpus fits.

## Dependency Graph

```
Phase 1 (SPIKE — tuplesort put/get roundtrip, de-risk the FFI) ──> Phase 2 (streaming pipeline: sample-train + stream-assign + read-back write) ──> Phase 3 (memory-bound measurement + byte-identical recall) ──> Phase 4 (review + release)
```

## Phase 1 — SPIKE: de-risk the tuplesort FFI

### Task T1.1 — a minimal `(i32,i64,bytea)` put/get roundtrip

#### Why this step

**Action:** in a new `am/build_stream.rs`, write `fn tuplesort_roundtrip(rows: Vec<(i32,i64,Vec<f32>)>) -> Vec<(i32,i64,Vec<f32>)>` that builds a 3-column TupleDesc (int4 list#, int8 tid, bytea vector), `tuplesort_begin_heap` sorting by column 1, fills a virtual slot per row (`tts_values`/`tts_isnull` + `ExecStoreVirtualTuple` — `ExecClearTuple` is unbound per the blueprint), `puttupleslot`, `performsort`, `gettupleslot` back, decode. A pg_test asserts the output is the input sorted by list# with the vectors byte-identical.

**Reasoning:** the blueprint's risk (a) mandates spiking the put/get cycle BEFORE the pipeline — the FFI (virtual-slot fill, the unbound `ExecClearTuple`, the bytea encode of an f32 vector) is the one genuinely new/unproven surface. A green roundtrip de-risks all of Phase 2. Spike-first is the M92 lesson.

#### Files to edit

- `theodb_rs/src/am/build_stream.rs` (NEW) — the roundtrip fn + test.
- `theodb_rs/src/am/mod.rs` — `mod build_stream;`.

#### Deep file dependency analysis

New module; no existing caller until Phase 2. Uses `pg_sys` tuplesort/slot symbols (confirmed bound). The bytea encode/decode of `Vec<f32>` must round-trip exactly (little-endian bytes) — the recall-critical invariant.

#### TDD

```
test_m96_tuplesort_roundtrip_sorts_and_preserves_vectors (pg_test):
  GIVEN rows [(2, 20, [2.0,2.0]), (1, 10, [1.0,1.0]), (1, 11, [1.5,1.5])]
  WHEN tuplesort_roundtrip(rows)
  THEN output == the rows sorted by list# ascending, each tid + vector byte-identical.
test_m96_tuplesort_spills_under_low_workmem (pg_test):
  GIVEN 50k rows + maintenance_work_mem set to 64kB (forces external spill)
  WHEN roundtrip
  THEN all 50k rows return correctly sorted (the spill path works, not just in-memory).
```

#### Concurrency tests

(none — the tuplesort is leader-only, `coordinate=NULL` — serial build, no parallel workers in this milestone per the blueprint's deferral.)

#### Failure scenarios

- **Malformed slot (nvalid/flags not set) → `puttupleslot` reads garbage:** the roundtrip test's byte-identical assertion catches any slot-fill error (wrong tid/vector = test fail). The unbound `ExecClearTuple` is worked around by direct `tts_flags`/`tts_nvalid` set per the blueprint.
- **bytea alignment/length mismatch on decode:** the vector-byte-identical assertion catches a wrong length/endianness.

#### Acceptance criteria

- `test_m96_tuplesort_roundtrip_sorts_and_preserves_vectors` asserts `output == sorted_input` (oracle: `assert_eq!` on the decoded Vec).
- `test_m96_tuplesort_spills_under_low_workmem` asserts all 50k rows return sorted after a forced spill (oracle: len + sorted-order check).

#### DoD

- Both pg_tests green (droplet). No accumulation: the roundtrip holds only the sorter (the input Vec is the test's, not the mechanism's).

## Phase 2 — the streaming build pipeline

### Task T2.1 — sample-train + stream-assign + read-back write

#### Why this step

**Action:** add `ambuild_streaming` in `build_stream.rs`: (1) scan-1 reservoir-sample (reuse the existing capped samplers) → train kmeans centroids + AQ codebook + SQ8; (2) scan-2 `table_index_build_scan` callback computes nearest centroid inline over the small centroids and `puttupleslot((list#, tid, vector))`; (3) `performsort`; (4) read back grouped by list#, pack AQ (+SQ8) codes per list from the tuple's vector bytes, and write via the existing `write_ivf_aq_split*` writers ONE list at a time (a per-list buffer, freed after each list). `ambuild` dispatches to `ambuild_streaming` when `N × dim × 4 > maintenance_work_mem` (ADR M96-3), else the existing `build_owned` path.

**Reasoning:** ADRs M96-1/2/3 — this is the pipeline that never holds the corpus. Reading vectors back from the sorted stream (not `idx.vectors`) is what drops the 1× wall. The writers are unchanged (same bytes) so the format is byte-identical.

#### Files to edit

- `theodb_rs/src/am/build_stream.rs` — `ambuild_streaming`.
- `theodb_rs/src/am/build.rs` — dispatch in `ambuild`; a per-list writer entry (or reuse the existing writers fed per-list).
- `theodb_rs/src/ann/ivf.rs` — a `from_streamed` helper if the writers need an `IvfflatIndex`-shaped view (centroids + per-list positions) without the vectors.

#### Deep file dependency analysis

The writers (`write_ivf_aq_split*`) currently take `positions: &[Vec<usize>]` indexing into a `vecs` array. Streaming supplies one list's `(tid, vector, code)` at a time. Either (a) add a per-list writer entry the streaming loop calls, or (b) buffer one list's vectors, call the existing writer with a single-list slice, append its pages. Option (b) is smaller (reuses the writers) and still O(largest list) not O(N) — acceptable since a list is N/lists (≈ balanced). SOAR (`soar_lambda>0`) is NOT supported on the streaming path (blueprint caveat) → explicit path selection: `soar_lambda>0` forces the in-RAM path (or errors loud if N too big — never silent wrong).

#### TDD

```
test_m96_streaming_build_recall_matches_inram (pg_test):
  GIVEN a dataset small enough for both paths but with maintenance_work_mem forced low so streaming activates
  WHEN built via streaming vs built in-RAM (same seed/data)
  THEN the streamed index's set-equal recall vs seqscan is within the same band as the in-RAM build (byte-identical
       centroids/order → identical lists → identical pages).
test_m96_streaming_build_crash_restart_scan_identical (pg_test):
  GIVEN a streamed build
  WHEN the build commits and a scan runs after
  THEN the scan returns the same top-k as immediately post-build (crash-safety via GenericXLog unchanged).
test_m96_soar_forces_inram_path (pg_test):
  GIVEN soar_lambda>0 + a size that would stream
  THEN the build uses the in-RAM path (explicit selection) OR errors loud — never a silent wrong (SOAR unsupported streaming).
```

#### Concurrency tests

(none — serial leader-only build; `assign_all_parallel` is deferred/unused on the streaming path, or the parallel assignment is over the trained centroids with disjoint output — no shared mutable state, same as M88.)

#### Failure scenarios

- **maintenance_work_mem = 0 / unreadable:** default to the in-RAM path (fail-safe — never a zero-size sorter).
- **Empty relation (N=0):** `ambuildempty` path unchanged; streaming not entered.
- **A list larger than one buffer:** the per-list buffer is O(list size) = O(N/lists); at 100M/1000 lists ≈ 100k vectors × 128 × 4 ≈ 51 MB — bounded, acceptable (the blueprint's "one list in flight").

#### Acceptance criteria

- `test_m96_streaming_build_recall_matches_inram` asserts the streamed recall equals the in-RAM recall on the same data (oracle: both set-equal vs seqscan, same band).
- `test_m96_streaming_build_crash_restart_scan_identical` asserts post-build scan == a second scan (oracle: `assert_eq!` top-k).
- `test_m96_soar_forces_inram_path` asserts explicit path selection (oracle: no silent wrong result).

#### DoD

- The 3 pg_tests green; the ≤1M in-RAM path stays byte-identical (existing v5/v6/v7 recall tests unchanged).

## Phase 3 — memory-bound measurement

### Task T3.1 — peak-RSS measurement (spill-forced + 30M)

#### Why this step

**Action:** measure peak build RSS (`/usr/bin/time -v` Max RSS OR cgroup peak) for the streaming build vs the M89 in-RAM build at 30M×128 (the M88 OOM baseline: 4.21×/64.7 GB). Assert the streaming peak is `≤ 0.5×` base (target O(mwm+sample) ≈ 2-3 GB). Then, if the box + wall-clock allow, attempt the 100M build and report completion + peak (honest: MEASURED if it completes, else the constant-memory shape is documented from the 30M + spill-forced points, with the 100M projection labeled as projection, not measurement — Rule 5).

**Reasoning:** the milestone's purpose (M88/ADR-0038) is 100M-buildable-on-commodity-RAM. The 30M measurement reproduces the OOM baseline and proves the bound; the spill-forced test proves the mechanism at any N. 100M is the stretch — measured if feasible, honestly projected otherwise.

#### Files to edit

- `benchmarks/m96_streaming_build_bench.py` (NEW) — build at N, capture peak RSS, compare old vs new.

#### TDD

```
(benchmark, not a unit test — the gate is the measured peak artifact)
Assertion: streaming peak(30M) ≤ 0.5× base; the old in-RAM build reproduces ~4.21× (the M88 baseline).
Emits docs/benchmarks/m96-streaming-build.{md,json}.
```

#### Concurrency tests

(none — measurement harness.)

#### Failure scenarios

- **100M build exceeds feasible wall-clock:** report the 30M + spill-forced measured points + the honest projection; do NOT fabricate a 100M number (Rule 5). Honest-partial is a valid terminal for the stretch.

#### Acceptance criteria

- `docs/benchmarks/m96-streaming-build.{md,json}` with real peak-RSS numbers (Rule 5): streaming ≤ 0.5× base at 30M, old build ~4.21×; 100M measured-or-honestly-projected.

#### DoD

- Benchmark artifact committed; the memory bound is proven at 30M (the DoD's measurable core); 100M measured or honestly labeled projection.

## Phase 4: Integration Validation

- Full `cargo pgrx test pg17` GREEN (≥ 276 tests) on the droplet; the ≤1M in-RAM path byte-identical.
- The 30M peak-RSS measurement proves the bound (≤0.5× base vs the M88 4.21× baseline).
- Review: council-index-storage (crash-safety / page invariants) + council-rust-pgrx (the tuplesort FFI surface) + council-benchmark (the peak measurement honesty); findings fixed before `/release`.

## Failure scenarios

The build reads the heap (Postgres storage) and the tuplesort (temp files) — the external I/O surface.

| Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|
| tuplesort spill to temp files under low `maintenance_work_mem` | `test_m96_tuplesort_spills_under_low_workmem` (64kB mwm, 50k rows) | all rows return correctly sorted — the external merge path works, memory stays bounded |
| `maintenance_work_mem` 0 / unreadable | default-to-in-RAM guard | fail-safe: the in-RAM path, never a zero-size sorter |
| SOAR requested on a streaming-size build | `test_m96_soar_forces_inram_path` | explicit path selection (in-RAM) or loud error — never a silent wrong index |
| 100M build exceeds feasible wall-clock | T3.1 measurement | report measured 30M + spill points + honest projection; never fabricate the 100M number (Rule 5) |

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | De-risk the tuplesort FFI (put/get/spill) before the pipeline | T1.1 | spike roundtrip + spill test (ADR M96-1) |
| 2 | Never materialize the corpus (drop the 1× wall) | T2.1 | stream-assign into the sorter; read back grouped (ADR M96-2) |
| 3 | Bounded per-list write (one list in flight) | T2.1 | per-list buffer O(N/lists), reuse the existing writers |
| 4 | Byte-identical ≤1M in-RAM path (no regression) | T2.1 | fast-path dispatch (ADR M96-3) |
| 5 | Crash-safety unchanged (build→restart→scan-identical) | T2.1 | writers via GenericXLog unchanged; crash test |
| 6 | SOAR unsupported streaming → explicit selection, never silent wrong | T2.1 | soar_lambda>0 forces in-RAM / loud error |
| 7 | Memory bound proven (≤0.5× base at 30M vs 4.21× baseline) | T3.1 | peak-RSS measurement |
| 8 | 100M buildable (measured or honestly projected) | T3.1 | measure if feasible; honest projection otherwise (Rule 5) |
| 9 | sign-off council-index-storage + council-rust-pgrx + council-benchmark | T3.1 | review before release |

**Coverage: 9/9 gaps covered (100%)**

## Drawbacks & Risks

| # | Risk | Severity | Mitigation | Owner |
|---|---|---|---|---|
| 1 | tuplesort FFI (virtual-slot fill, unbound `ExecClearTuple`, bytea round-trip) is the new/unproven surface | HIGH | spike-first (Phase 1) de-risks it BEFORE the pipeline; the byte-identical roundtrip test is the proof | impl |
| 2 | Two heap scans double the read I/O | MEDIUM | measured in T3.1; the gate is memory-bound (caber em RAM), not build-speed; bounded read is acceptable vs an OOM | impl |
| 3 | 100M wall-clock (assignment O(N·k·d)) may exceed feasible measurement time | MEDIUM | the 30M measurement proves the memory bound (the DoD's core); 100M is the stretch, measured-or-projected honestly (Rule 5, never fabricated) | impl |
| 4 | SOAR / parallel-assign deferred (blueprint caveats) | LOW | explicit path selection (never silent wrong); documented follow-ups, no regression (λ=0 default) | impl |

## Unresolved Questions

- Whether the 100M build completes within feasible droplet wall-clock is resolved at measurement time (T3.1) — the plan commits to the 30M measured bound as the DoD core and an honest 100M measured-or-projected result, never a fabricated number.

## Global DoD

- Full suite ≥ 276 tests, 0 failed (droplet); ≤1M in-RAM path byte-identical.
- No page-format change (no REINDEX); crash-safety unchanged.
- `build.rs` + `build_stream.rs` each < 800 LoC; the tuplesort `unsafe` is bounded and commented.
- CHANGELOG `[Unreleased]` updated; benchmark artifact committed.
