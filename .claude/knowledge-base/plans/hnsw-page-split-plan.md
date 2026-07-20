---
slug: hnsw-page-split
milestone_id: M126
created_at: 2026-07-20
goal: Split the 3456-LoC hnsw_page.rs into a directory module with zero behavior/format/API change proven byte-identical
---

# Plan — M126 Split `am/hnsw_page.rs` god-file (behavior-preserving)

## Goal

Split `theodb_rs/src/am/hnsw_page.rs` (3,456 LoC) into a `am/hnsw_page/` directory module (layout/meta/codec/pack/
store/search + co-located tests, each ≤ ~500 LoC) with `pub(crate) use` re-exports, so every caller path resolves
unchanged and recall is **byte-identical** — proven by a same-index in-PG A/B (zero-row rank+distance diff).

**Single metric:** a same-index A/B on a fixed dataset returns identical `(tid, distance)` for a fixed query set
pre- and post-refactor (`SELECT … EXCEPT SELECT …` both ways = 0 rows), with the build compiling clean at each cut.

## Context

Consumes `.claude/knowledge-base/discoveries/blueprints/hnsw-page-split-blueprint.md`. `hnsw_page.rs` is TheoDB's
page-native HNSW: on-disk layout/codec + graph traverse/frontier + scan/resume + build-time pack. It is the largest
file (top LoC), the highest-churn hot-path (M35/M52/M118), and concentrates `unsafe`. The `/analysis` report
(`knowledge-base/audits/2026-07-20-analysis.md`) flagged it as the top maintainability/safety risk.

## Baseline Context

Repo state: git sha `5a72be3`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `theodb_rs/src/am/hnsw_page.rs` | 3456 | single-file: layout+meta+codec+pack+store+search+tests | Convert to `am/hnsw_page/` directory module (7 files). |
| `theodb_rs/src/am/mod.rs` | — | `mod hnsw_page;` at :29 | Unchanged (directory module keeps the same `mod` name). |

### Current callers / dependents (verified `file:line`)

- `theodb_rs/src/am/scan.rs:76,:215,:217,:280` — `crate::am::hnsw_page::{HnswResume, traverse, resumable_init, resumable_next}` — MUST keep resolving (via `mod.rs` re-exports).
- `theodb_rs/src/am/build.rs` — calls `hnsw_page::{pack, ...}` (build path).
- `theodb_rs/src/am/page/mod.rs` (981 LoC) — the buffer/WAL layer `hnsw_page` calls (`with_page_item`) — **untouched**.
- `theodb_rs/src/ann/hnsw.rs:7` — `HnswIndex` (the build graph consumed by `encode_*`/`pack_*`).
- Verified: **0** `#[pg_extern]`/`extension_sql!`/`#[pg_schema]` in `hnsw_page.rs` → the pgrx SQL-gen concern does not apply.

### Domain glossary

- **directory module** — `foo/{mod,a,b}.rs` where `mod.rs` re-exports submodule items; the project already uses it at `am/page/{mod,ivf,symqg}.rs`.
- **seam** — a clean cut line between responsibilities with minimal cross-references (blueprint enumerates 7).
- **same-index A/B** — build the index ONCE, run identical queries on the pre/post binaries against the same physical index; the refactor must not touch build output.

### Architecture boundaries affected

Per `rules/architecture.md`: the split sharpens module cohesion (SRP) — layout/meta/codec are pure (`unsafe`-free); store/search hold 100% of the file's `unsafe` (Relation-facing). The buffer/WAL layer (`am/page/`) is the boundary below and is untouched. No layer direction changes; the composition/entry points (`am/mod.rs`, `build.rs`, `scan.rs`) are untouched. Zero public-API and zero on-disk-format change.

## Prior Art & Related Work

- Blueprint (web-evidenced, read the real file): pgvector splits HNSW by AM-verb + shared `hnswutils.c` codec + `hnsw.h` header (`references/pgvector/src/hnsw*`); hnswlib separates algorithm/distance-space/visited-set; Lucene separates the on-disk codec package from the graph algorithm (even filtered-search is its own class = our M118 resume). pgvectorscale (Rust pgrx AM) splits node/storage/graph/meta_page. All converge on the 7-seam decomposition.

## ADRs

### ADR M126-1 — directory module with `pub(crate) use` re-exports (no caller edits)

**Decision:** convert `hnsw_page.rs` → `am/hnsw_page/{mod,layout,meta,codec,pack,store,search}.rs`; `mod.rs`
re-exports every currently-`pub(crate)` symbol via `pub(crate) use <sub>::*`, so external paths
(`crate::am::hnsw_page::X`) are byte-identical and no caller changes.

**Rationale (cites blueprint + `rules/architecture.md` SRP):** the project already uses this exact pattern
(`am/page/{mod,ivf,symqg}.rs`); it keeps `mod hnsw_page;` in `am/mod.rs` unchanged and needs zero SQL-gen changes
(no `#[pg_extern]` here). The one widening is the format offset consts (private `const` → `pub(crate) const`) —
compile-time integers, not behavior.

**Alternatives rejected:**
- **Split into sibling top-level files** (`am/hnsw_layout.rs`, …) — REJECTED: changes caller paths + the `mod`
  tree; the directory module keeps `crate::am::hnsw_page::*` intact.
- **Extract to a new crate** — REJECTED: over-engineering (YAGNI); an intra-crate directory module is the minimal move.

### ADR M126-2 — same-index in-PG A/B as the byte-identical oracle

**Decision:** the primary correctness gate is a same-index A/B (build once, run a fixed query set on the pre/post
binaries against the same physical index, assert zero-row `(tid,distance)` diff both ways). Plus the pure modules'
unit tests under `cargo build`/`cargo test`.

**Rationale (cites blueprint):** `cargo pgrx test` does not link on the droplet (known gotcha, M118/M120/M122); the
same-index A/B isolates the read path (traverse/load/resume) from any build variance and directly proves "same
rankings" (the DoD). A write-path block digest is a secondary check (valid only if build is deterministic —
`AQ_BUILD_SEED=0x5943_4E41`).

**Alternatives rejected:** relying on `cargo pgrx test` — REJECTED (does not link on the droplet); rebuild-diff as
the only oracle — REJECTED (needs unverified build determinism; same-index A/B does not).

## Dependencies

No new dependency (pure refactor). `## Dependencies`: **none** — no crate added; `theodb_rs/Cargo.toml` unchanged.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Split into ≤500-LoC cohesive modules, `unsafe` isolated | T1 (layout/meta), T2 (codec/pack), T3 (store/search) |
| Every caller path resolves unchanged (re-exports) | T1–T3 (mod.rs re-exports), compile-between-each |
| Tests co-located, still see module-privates | T4 (co-locate tests) |
| Byte-identical recall (same rankings) | T5 (same-index A/B) |

## Phase 1 — pure modules (layout, meta)

### T1.1 — extract `layout.rs` + `meta.rs` (unsafe-free)

#### Why this step
Leaf-first: layout (constants + size math, seam a, ~110 LoC) and meta (page-0 codec, seam b, ~270 LoC) have the
fewest deps — a mistake is caught immediately. Reasoning: create `hnsw_page/mod.rs` with re-exports, move seam (a)
to `layout.rs` (widen offset consts to `pub(crate) const`), seam (b) to `meta.rs`; compile.

#### Files to edit
- `theodb_rs/src/am/hnsw_page/mod.rs` (NEW — module doc + `mod` decls + `pub(crate) use {layout,meta}::*`), `layout.rs` (NEW, seam a), `meta.rs` (NEW, seam b). Delete the corresponding ranges from the old file (which becomes the remaining seams until fully split).

#### TDD
- RED: `cargo build --features pg17` fails until the re-exports + widened consts resolve every `layout`/`meta` reference.
- GREEN: the move + `pub(crate) const` widening compiles clean.
- REFACTOR: move the meta round-trip unit tests into `meta.rs`'s `#[cfg(test)] mod tests`.

#### Concurrency tests
(none — single-threaded) — a pure text-move; no new concurrent path, no shared state introduced.

#### Acceptance criteria
- `cargo build --features pg17` EXIT=0; `layout.rs`/`meta.rs` `unsafe`-free (grep); no caller edited.

#### DoD
- Build clean; `grep -c unsafe layout.rs meta.rs` = 0.

## Phase 2 — codec + pack

### T2.1 — extract `codec.rs` (seam c) + `pack.rs` (seam d)

#### Why this step
codec (tuple encode+decode, ~220 LoC) and pack (build-time graph→pages, ~325 LoC) are the next layer up
(deps: layout, meta, HnswIndex). Reasoning: keep encode+decode together (Lucene principle: the format lives in
one place); pack depends on codec::encode.

#### Files to edit
- `am/hnsw_page/codec.rs` (NEW, seam c + `mark_tombstone_in_place`), `pack.rs` (NEW, seam d); update `mod.rs` re-exports; delete the ranges from the old file.

#### TDD
- RED: build fails until codec/pack references resolve.
- GREEN: move compiles.
- REFACTOR: move the tuple round-trip + `mod m56_tombstone_layout` tests into `codec.rs`'s test module.

#### Concurrency tests
(none — single-threaded) — pure text-move.

#### Acceptance criteria
- Build clean; `codec.rs`/`pack.rs` `unsafe`-free (they are serialization, not Relation I/O).

#### DoD
- `cargo build` EXIT=0; the pure round-trip unit tests compile.

## Phase 3 — unsafe modules (store, search)

### T3.1 — extract `store.rs` (seam e) + `search.rs` (seam f)

#### Why this step
The two `unsafe` Relation-facing modules LAST (highest risk — the M35/M52/M118 hot path). Reasoning: by now the
re-export glue is proven by 4 clean compiles, so moving the recall-bearing code is the only remaining risk.
`Cand` + `HnswResume` co-locate in `search.rs` (the alias names `Cand`).

#### Files to edit
- `am/hnsw_page/store.rs` (NEW, seam e — page I/O + insert helpers), `search.rs` (NEW, seam f — `Cand`, `score`, `load`, `neighbors_*`, `traverse`, `HnswResume`, `resumable_*`, `PageNeighborSource`); update `mod.rs`; the old `hnsw_page.rs` file is deleted (all seams moved).

#### TDD
- RED: build fails until `scan.rs`/`build.rs` resolve `hnsw_page::{HnswResume,traverse,resumable_*,pack}` via the re-exports.
- GREEN: move compiles; `pub(crate) unsafe fn` signatures kept verbatim.
- REFACTOR: co-locate the store/search tests into their modules' test modules.

#### Concurrency tests
(none — single-threaded) — the traverse/scan logic is moved verbatim; no concurrency semantics change (a pure move). The existing scan-concurrency behavior is unchanged by construction.

#### Acceptance criteria
- Build clean; 100% of the original `unsafe` blocks live in `store.rs`+`search.rs`; `layout/meta/codec/pack` `unsafe`-free.
- No file > ~1500 LoC (prod + co-located tests).

#### DoD
- `cargo build --features pg17` EXIT=0; the old `hnsw_page.rs` no longer exists (fully moved).

## Phase 4 — tests + byte-identical validation

### T4.1 — same-index A/B proves byte-identical recall

#### Why this step
The DoD's single metric. Reasoning: build the index ONCE on the fixed vector benchmark dataset (pre-refactor
binary), snapshot `(tid, distance)` for a fixed query set; after the refactor, run the identical queries against
the SAME physical index (post-refactor binary); assert a zero-row set-diff both ways.

#### Files to edit
- `docs/benchmarks/m126-hnsw-split-byteidentical.md` (NEW) — the A/B methodology + the zero-diff result + the LoC-per-module table (proving the split).

#### TDD
- RED: `SELECT id,dist FROM <pre> EXCEPT SELECT id,dist FROM <post>` (and the reverse) MUST be non-empty if the move changed any ranking/distance — the test is designed to catch a mis-moved constant/offset.
- GREEN: after a correct pure move, both diffs are zero rows (identical recall).

#### Concurrency tests
(none — single-threaded) — deterministic query replay.

#### Acceptance criteria
- Zero-row `(tid,distance)` diff both ways on the fixed query set at fixed `ef_search`; the pure unit tests (offset math, meta/tuple round-trip) pass under `cargo test`.

#### DoD
- `docs/benchmarks/m126-hnsw-split-byteidentical.md` shows the zero-diff A/B + the per-module LoC table (all ≤ ~1500).

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Refactor of a 3456-LoC hot-path with `unsafe` may introduce a subtle bug (mis-moved offset/const, lost `unsafe` invariant) | MEDIUM | Pure text-move (no logic edit); compile between each cut; same-index A/B byte-identical gate is mandatory; council-rust-pgrx review | implementer |
| `cargo pgrx test` does not link on the droplet (known gotcha) | MEDIUM | Pure unit tests (offset/round-trip) run under `cargo test`; Relation-touching correctness via the in-PG same-index A/B (ADR M126-2) | implementer |
| Test co-location may lose visibility of module-private helpers | LOW | Co-locate `#[cfg(test)] mod tests` INSIDE each submodule (blueprint gotcha), not a central tests.rs | implementer |

## Unresolved Questions

- Is `ann/hnsw.rs` build deterministic (for the write-path digest oracle)? Resolved at plan time: **the same-index
  read A/B is the primary gate and does NOT need build determinism**; the write-digest is a secondary check used
  only if build is confirmed deterministic (`AQ_BUILD_SEED` fixed). The milestone does not block on it.
- (none other — every decision is resolved at plan time.)

## Global DoD

- `am/hnsw_page/` directory module: `mod.rs` + layout/meta/codec/pack/store/search, each ≤ ~1500 LoC (prod+tests);
  `unsafe` only in store/search; `layout/meta/codec/pack` `unsafe`-free.
- Zero caller edit (re-exports); zero public-API change; zero on-disk-format change (index files still readable).
- Byte-identical recall proven by the same-index A/B (`docs/benchmarks/m126-hnsw-split-byteidentical.md`).
- `cargo build --features pg17` clean; pure unit tests green. No new dependency. CHANGELOG `[Unreleased]`. `/code-quality` ∉ {FAIL_HARD, INVALID}.
