# Review — M126 split `am/hnsw_page.rs` god-file

**Date:** 2026-07-20 · **Slug:** hnsw-page-split · **Milestone:** M126 · **Commit:** be00b86
**Verdict:** READY_TO_MERGE

## Scope

Two adversarial specialist reviews of the behavior-preserving split of the 3,456-LoC `am/hnsw_page.rs` into
`am/hnsw_page/{layout,meta,codec,pack,store,search,tests}.rs`. Both reviewers reconstructed the original via
`git show be00b86~1:…` and proved byte-identity by **static normalized diff of the entire prod surface** — not by
trusting the A/B.

## Consolidated findings

| # | Severity | Finding | Resolution |
|---|---|---|---|
| 1 | LOW (both reviewers) | The evidence doc's headline over-attributed the full behavior guarantee to the same-index A/B, which runtime-exercises only the **v1 read hot path** (index built once by the baseline `.so`; write/build/VACUUM/AQ-v4 paths not run). | **FIXED** — doc reworded to credit TWO proofs: static byte-identity of the prod modules (covers write/pack/VACUUM/AQ-v4) + the A/B corroborating the read hot path. Lines 5–8 and 57–58 tightened. |

No BLOCKER, no HIGH, no MEDIUM.

## council-rust-pgrx (unsafe / FFI / lifetime / WAL) — CLEAN PURE MOVE

- **Entire prod diff = 6 lines** (one `#![allow(unused_imports)]` per module). All 90+ top-level items present
  exactly once, in original order, identical signatures + bodies.
- All **22 `unsafe` blocks** moved verbatim (store 12, search 10). No FFI signature changed, no
  panic-across-`C-unwind` altered, no lifetime touched (`ElementView<'a>`, `PageNeighborSource<'a>` intact), no
  buffer/WAL/GenericXLog sequencing rearranged.
- **No `pub ` (public) leak** — every widened item is `pub(crate)`; carrier types are `pub(crate)`, so no field
  escapes the crate. M118 boundary invariant holds (`am/scan.rs` holds `ResumableGround<Cand>` opaquely).
- **No SQL-entity change** — zero `#[pg_extern]`/`#[pg_test]`/`extern "C"` in the prod section; the single prod
  `cfg` gate preserved; test suite (42 `pg_test` + 3 `test`) moved intact.

## council-index-storage (on-disk format / MVCC / WAL) — CLEAN MOVE

- **No on-disk constant/offset/formula/codec byte-layout changed** — verified 4 ways: 24 layout constants
  identical (magic `0x5448_5353`, all `E_*`/`E4_*` offsets, `USABLE`, version tags, `AQ_BUILD_SEED`); normalized
  whole-module diff = zero logic lines changed (+14 scaffolding only); order-preserving function-body diffs of
  `encode_meta`/`decode_meta`/`encode_element_v4`/`decode_element_v4`/`encode_neighbors` byte-identical; 61
  production functions, none added/removed.
- **Module boundary coherent** — format constant + its size formula stay together in `layout.rs` (highest drift
  risk avoided); each constant has exactly one definition (no shadowing); `cargo check` rc=0 ⇒ no glob ambiguity.
- **Crash-safety / on-disk compatibility structurally unchanged** — the M35 page format, GenericXLog write path,
  pending region, VACUUM/tombstone codec are byte-identical moves.

## Independent evidence (this cycle)

- Same-index A/B: `docs/benchmarks/m126-hnsw-split-byteidentical.md` — zero-diff `(id,distance)` over 50 queries ×
  top-10 on the same physical index (pre/post sha `20c1d8288b0ac549248b`, both).
- `cargo check --features pg17` rc=0 (lib) and `cargo check --features "pg17 pg_test" --tests` rc=0.
- Structural: prod modules ≤ ~500 LoC; `unsafe` isolated to store+search; four pure modules `unsafe`-free.

## Gate check

Per `cycle-review.md`: no BLOCKER, ≤ 2 HIGH → **READY_TO_MERGE**. CHANGELOG `[Unreleased]` updated. No secrets.
No Co-Authored-By. Zero caller edits (behavior-preserving refactor).

## Verdict

**READY_TO_MERGE.** Two independent specialist reviews converged on "clean pure move" and proved byte-identity
statically; the one LOW (evidence wording) is fixed. The top maintainability/safety risk from the 2026-07-20
trajectory analysis (the 3,456-LoC god-file with concentrated `unsafe`) is resolved.
