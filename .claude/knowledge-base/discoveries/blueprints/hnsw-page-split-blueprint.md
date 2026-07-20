# Blueprint — M126 split `am/hnsw_page.rs` (3,456 LoC) into cohesive modules

Date: 2026-07-20 · Source: council-index-storage discover (read the real file + web-evidenced pgvector/hnswlib/Lucene/pgvectorscale).

## Bottom line

A **pure text-move** into a directory module `am/hnsw_page/` with `pub(crate) use <sub>::*` re-exports in `mod.rs`
→ every `crate::am::hnsw_page::X` path in `scan.rs`/`build.rs` resolves **unchanged** (zero public-API edit, zero
caller change). **No `#[pg_extern]`/`extension_sql!`/`#[pg_schema]` in this file (verified grep)** → the pgrx
SQL-gen concern does NOT apply. A clean safe/unsafe fault line bisects the file at ~line 948 (a–d pure, e–f unsafe).

## The 7 seams (verified file:line in `hnsw_page.rs`)

| Seam | Lines | LoC | unsafe | → target file |
|---|---|---|---|---|
| (a) layout constants + size math | 20–127 | ~110 | no | `layout.rs` |
| (b) meta (page-0) codec | 129–401 | ~270 | no | `meta.rs` |
| (c) tuple codec (element/neighbor/raw, encode+decode) | 401–621 | ~220 | no | `codec.rs` |
| (d) build-time pack/train (graph→pages) | 622–947 | ~325 | no | `pack.rs` |
| (e) page I/O + insert helpers | 948–1341 | ~394 | **yes** | `store.rs` |
| (f) search hot path: `Cand`/score/load/neighbors/`traverse`/`resumable_*` | 1342–1825 | ~460 | **yes** | `search.rs` |
| (g) tests (`mod tests` 1826–3405 + `mod m56_tombstone_layout` 3406–3456) | 1826–3456 | ~1630 | mixed | co-located per submodule |

All prod files ≤ ~460 LoC (< 1500 ✓). The two `unsafe` modules (`store`, `search`) hold 100% of the file's `unsafe`.

## Target decomposition

Convert `am/hnsw_page.rs` → `am/hnsw_page/{mod,layout,meta,codec,pack,store,search}.rs` (the project already uses
this exact directory-module pattern at `am/page/{mod,ivf,symqg}.rs`). Keep `mod hnsw_page;` in `am/mod.rs:29`
unchanged. `mod.rs` = module doc + `mod` declarations + `pub(crate) use <sub>::*` re-exports of EVERY currently
`pub(crate)` symbol (so external paths are byte-identical).

## Cut order (safest-first, compile between each)

1. `layout.rs` (zero deps) → build. 2. `meta.rs` (deps layout). 3. `codec.rs` (deps layout/meta/HnswIndex).
4. `pack.rs` (deps codec/meta). 5. `store.rs` (first unsafe). 6. `search.rs` (LAST — the M35/M52/M118 hot path,
highest risk). 7. co-locate tests. Doing the unsafe hot path last means the re-export glue is already proven.

## Gotchas

- **SQL-gen non-issue for this file** (no `#[pg_extern]`/`extension_sql!`). The `IndexAmRoutine`/`#[pg_guard]`
  `extern "C-unwind"` entry points live in `am/mod.rs`/`build.rs`/`scan.rs` — none move.
- **One visibility widening:** the format offset consts in seam (a) (`E_*`/`E4_*`/`R_*`/`N_*`/`ELEM_HEADER*`/
  `NBR_HEADER`/`SLOT`) are private `const` → make `pub(crate) const` so codec/store/search resolve them. Exposes
  compile-time integers, not behavior. Note in CHANGELOG.
- **`Cand` + `HnswResume` co-locate** in `search.rs` (`pub(crate) type HnswResume = ResumableGround<Cand>` at
  :1668; `scan.rs:76,215` uses `crate::am::hnsw_page::HnswResume`) — re-export both from `mod.rs`.
- **Tests co-located per submodule, NOT centralized** — today's tests see module-private items (`score`, `load`,
  `encode_element`, `elem_size`, offset consts); a single `tests.rs` doing `use super::*` would only see the
  `mod.rs` re-exports. Put `#[cfg(test)] mod tests { use super::*; }` inside each submodule (pgvectorscale
  `sbq/tests.rs` pattern). Route `mod m56_tombstone_layout` into `codec.rs`'s test module.
- Lifetimes carry over verbatim (`ElementView<'a>`, `ElementViewV4<'a>`) — mechanical move.

## Byte-identical validation (the DoD)

`cargo pgrx test` doesn't link on the droplet (known gotcha, M118/M120/M122) → the gate is **in-PG A/B**:
- **Read-path / ranking identity (primary oracle):** **same-index A/B** — build the index ONCE (the refactor must
  not touch build output), then run the identical query set (fixed `ef_search`, `enable_seqscan=off`) on the pre-
  and post-refactor binaries against the SAME physical index; record `(tid, distance)` per query; assert
  `SELECT … EXCEPT SELECT …` both ways = zero rows. Isolates the `traverse`/`load`/`resume` read path.
- **Write-path digest (if build deterministic — `AQ_BUILD_SEED=0x5943_4E41` at :688):** `CREATE INDEX` on a fixed
  dataset, checksum every block (pageinspect `get_raw_page` / `md5(string_agg)`), pre vs post → identical digest.
  Caveat: confirm `ann/hnsw.rs` build has no unseeded RNG before relying on rebuild-diff.
- Belt+suspenders: the `unsafe`-free modules' pure unit tests (offset math, meta/tuple round-trip) run under plain
  `cargo test` and catch a mis-moved constant instantly.

## Prior art (convergent)

pgvector splits by AM-verb + one shared `hnswutils.c` (1431 LoC, codec+distance+read/write) + `hnsw.h` header
(`references/pgvector/src/hnsw*.{c,h}`). hnswlib: algorithm ≠ distance-space ≠ visited-set. Lucene: on-disk codec
package fully separate from the graph algorithm; even filtered-search is its own class (`FilteredHnswGraphSearcher`
= our M118 resume path). pgvectorscale (the closest — Rust pgrx AM): node/storage/graph/meta_page split. All four
converge: separate (1) tuple codec, (2) meta, (3) graph+neighbors, (4) search, (5) build/pack; distance kernels
already external (our `crate::vec`/`sbq`/`ah`).

## Flags

- Per-module test LoC split not pre-computed — partition the 1,630 test LoC by which module each `#[test]`
  exercises during implementation (only the two test-module boundaries 1826/3406 are verified).
- Build determinism of `ann/hnsw.rs` unverified — the write-path rebuild-diff oracle is valid only if build is
  deterministic; the same-index read A/B does NOT need this assumption (primary gate).

## Local anchors

- Refactor target: `theodb_rs/src/am/hnsw_page.rs`. Callers: `am/scan.rs` (:76,:215,:217,:280), `am/build.rs`.
- Buffer/WAL layer (already isolated, untouched): `am/page/mod.rs`. Directory-module precedent: `am/page/{mod,ivf,symqg}.rs`.
- `HnswIndex` (build graph consumed by pack/encode): `ann/hnsw.rs:7`. Prior art vendored: `references/pgvector/src/hnsw*`, `references/pgvectorscale/.../access_method/`.
