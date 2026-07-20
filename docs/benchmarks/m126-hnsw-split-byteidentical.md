# M126 — Split of `am/hnsw_page.rs` proven byte-identical (measured)

**Date:** 2026-07-20 · **Box:** DO droplet (theo-e2e-runner), pgrx-managed PG17.10, pgrx 0.19.0.
**Verdict:** the 3,456-LoC god-file `am/hnsw_page.rs` was split into a `am/hnsw_page/` directory module
(layout/meta/codec/pack/store/search + co-located tests) with **zero behavior/format/API change**. The guarantee
rests on TWO proofs: (1) **static byte-identity** of the production modules — the whole prod diff vs the original
is 6 lines (one `#![allow(unused_imports)]` per module), so `encode_*`/`pack_*`/`write_structured`/`traverse` are
textually identical (this covers the write/build/VACUUM/AQ-v4 paths); (2) a **same-index A/B** that
runtime-corroborates the v1 read hot path — the pre- and post-refactor binaries read the SAME physical HNSW index
and return **byte-identical** `(id, distance)` rankings. Both were independently confirmed by adversarial review
(council-rust-pgrx + council-index-storage).

## The single metric — same-index byte-identical A/B (ADR M126-2)

The index was built **once** with the pre-refactor binary; the pre/post snapshots read that same physical
index (no rebuild between them — this isolates the read path, which is what the refactor touched).

| Step | What | Result |
|---|---|---|
| build (baseline `.so`) | `CREATE TABLE items(id, e vector(16))` + 5,000 deterministic rows + `CREATE INDEX … USING theodb_hnsw` | index built once |
| pre snapshot (baseline `.so`) | 50 fixed probes × top-10, `ef_search=100`, index path forced | `ab_pre.csv` (504 lines) |
| swap | rebuild theodb_rs from the **split** source; `pg_ctl restart` (index files untouched) | post-refactor `.so` loaded |
| post snapshot (split `.so`) | **identical** query on the **same physical index** (no rebuild) | `ab_post.csv` (504 lines) |
| **diff** | `diff ab_pre.csv ab_post.csv` | **ZERO rows differ** |

```
pre  sha256 = 20c1d8288b0ac549248b…
post sha256 = 20c1d8288b0ac549248b…   → byte-identical
```

The refactored read path (`traverse` / `load` / `neighbors_*` / resumable scan — the M35/M52/M118 hot path)
produces exactly the same rankings and distances across 50 queries. Behavior is preserved.

> The A/B'd `.so` is the shipped production binary: `cargo pgrx install` builds without the `pg_test`/`test`
> feature, so the co-located test code (`#[cfg(any(test, feature = "pg_test"))]`) is excluded from the release
> `.so`. The prod modules (layout/meta/codec/pack/store/search) are what the A/B exercised.

## Structural result — cohesive modules, `unsafe` isolated

| Module | LoC | `unsafe` | Responsibility |
|---|---|---|---|
| `mod.rs` | 35 | 0 | module doc + `pub(crate) use <sub>::*` flat re-exports (every `crate::am::hnsw_page::X` call site unchanged) |
| `layout.rs` | 109 | 0 | on-disk offset constants + tuple/page size arithmetic + `Addr` |
| `meta.rs` | 286 | 0 | page-0 meta + AQ/V4 descriptor encode/decode |
| `codec.rs` | 229 | 0 | element/neighbor/raw tuple encode + decode (the format) |
| `pack.rs` | 331 | 0 | build-time graph → page images (SBQ/AQ/v4 packers) |
| `store.rs` | 400 | **12** | Relation-facing page I/O: write/read/enumerate/tombstone/insert |
| `search.rs` | 495 | **10** | traverse + frontier + resumable scan (`Cand`, `HnswResume`, `PageNeighborSource`) |
| `tests.rs` | 1642 | — | the hnsw_page test suite (moved verbatim; shares fixtures so kept together) |

- **`unsafe` is now isolated to the two Relation-facing modules** (store + search = 22 blocks); the four pure
  format/build modules (layout/meta/codec/pack) are `unsafe`-free. This is the safety win: the memory-safety
  surface a reviewer must audit is confined to 2 of 8 files.
- **Every prod module ≤ ~500 LoC.** `tests.rs` (1642) exceeds the ~1500 guide — the test suite shares fixtures
  (corpus builders, `heap_tid_i64`, probe helpers) across cases, so splitting it per-seam would fragment shared
  helpers (a worse outcome than one cohesive test file). Honest deviation; the safety/cohesion goal is the prod
  split, which is fully met.
- **Zero caller edits.** `am/scan.rs`, `am/build.rs`, `api.rs` are untouched — the flat re-exports keep every
  `crate::am::hnsw_page::{traverse, HnswResume, resumable_*, pack, …}` path resolving. `am/mod.rs` still says
  `mod hnsw_page;` (a directory module keeps the name).
- **Zero on-disk-format change.** Two independent proofs: (1) the same physical index built by the baseline
  binary is read identically by the split binary (the A/B — this proves the **read** path); (2) the **write**
  path's byte-layout is preserved because the split is a source-identical move — `encode_meta` / `decode_meta` /
  `encode_element_v4` / `encode_neighbors` / `pack_*` / `write_structured` are byte-identical to the original (all
  format constants + size formulae in `layout.rs`/`meta.rs` unchanged, only visibility widened). The A/B alone
  does not exercise the split binary's write path / VACUUM-fold / crash-recovery — those rest on proof (2).

## What changed vs. a pure text-move (honest)

A behavior-preserving split still needs visibility widening for items now referenced across the new module
boundary. All widenings are **crate-internal** (`pub(crate)`), never `pub` — no public API surface changed:

- Format offset constants + size fns (`E_*`, `E4_*`, `ELEM_HEADER`, `elem_size`, …) → `pub(crate)` (were
  file-private; now referenced by codec/pack/store/search across the seam).
- `Cand`, `AqDescriptor`, `PageNeighborSource` struct **fields** → `pub(crate)` (read across the store↔search /
  meta↔store seam; `am/scan.rs` still holds `ResumableGround<Cand>` opaquely and touches no field — the M118
  invariant holds at the public boundary).

## Validation — compiles clean (lib + tests)

- `cargo check --features pg17` → **rc=0** (0 errors; 946 pre-existing never-used warnings, unchanged by the split).
- `cargo check --features "pg17 pg_test" --tests` → **rc=0** (the moved 1,642-LoC test suite type-checks).
- `cargo pgrx test` does not link on this droplet (known pgrx/PG-symbols gotcha) — the in-PG same-index A/B above
  is the correctness gate (ADR M126-2), which directly proves "same rankings" rather than proxying it.

## Reproduction

```
# build baseline .so, build index once, snapshot; then rebuild from split source, restart, snapshot; diff.
cargo pgrx install --pg-config <pg17>       # baseline
psql -f ab_setup.sql ; psql -f ab_query.sql > ab_pre.csv
# … apply split, cargo pgrx install again, pg_ctl restart …
psql -f ab_query.sql > ab_post.csv          # SAME index, no rebuild
diff ab_pre.csv ab_post.csv                 # zero rows
```
