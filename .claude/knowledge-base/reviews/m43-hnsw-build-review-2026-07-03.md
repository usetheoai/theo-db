# Review — M43 theodb_hnsw build-time optimization

**Date:** 2026-07-03
**Slug:** m43-hnsw-build
**Milestone:** M43
**Verdict:** READY_TO_MERGE
**Scope:** Rust production code — `theodb_rs/src/{vec.rs, ann/mod.rs, ann/hnsw.rs}` (SIMD build distance + unsafe
reinterpret cast).

## Change

The theodb_hnsw in-memory graph build now uses the SIMD AVX2+FMA kernel (`l2_distance_simd`, reusing `simd_x86::l2_sq`
via an f32→bytes reinterpret) instead of scalar L2, routed through `Metric::dist_simd` at 7 `ann/hnsw.rs` sites.
`l2_distance` (pgvector-parity: operators/scan-rerank/knn) untouched.

## Findings by dimension

| Dimension | Result | Evidence |
|---|---|---|
| **unsafe reinterpret soundness** | PASS (SOUND) | Focused rust-pgrx audit: `&[f32]→&[u8]` view is align-valid (4→1 downcast), size `len*4` cannot overflow (live slice ≤ isize::MAX), read-only (no aliasing), lifetime does not escape. Mirrors `l2_dist_from_bytes`. |
| **Endianness portability** | PASS + HARDENED | Audit flagged a dormant big-endian gap (reinterpret feeds native-BE bytes to a LE decoder → silently wrong on s390x/ppc64be — no shipped target). **Adopted the reviewer's `#[cfg(target_endian="little")]` guard + scalar fallback on BE.** x86_64/LE behavior byte-identical (the LE block is the prior code); BE now correct. |
| **Numeric / recall parity** | PASS | 8/8 `test_index_am.py` green on `theo-db:m43`; recall IDENTICAL @ 200k (0.9825), parity @ 1M (0.9725). The `neighbor_slice_matches_in_memory_graph_every_layer` invariant holds (build+persistence use the same graph). Exact paths (operators/rerank/knn) untouched. |
| **Build/scan consistency** | PASS (improved) | Audit: the structured scan was ALREADY SIMD (`hnsw_page.rs:418-420`); M43 aligns the build to the same kernel — REMOVING a prior build-scalar/scan-SIMD mismatch, not adding one. |
| **Performance (the goal)** | PASS | A/B rigorous 3-sample @ 200k: build 2.20× (m41 200±23s vs m43 91±3s, std bands separated), recall identical. @ 1M: 24min → 8.4min (~2.86×), recall parity. `docs/benchmarks/m43-hnsw-build.md`. |
| **Safe-signature encapsulation** | PASS | `l2_distance_simd` is `pub(crate) fn` (not `unsafe fn`) — the unsafe precondition is fully established internally from `&[f32]` + `check_dims`, no caller obligation. Correct pgrx idiom. |

## Hard gates

- Failing tests → none (8/8 AM tests green). No secrets. On `develop`. No `Co-Authored-By`. CHANGELOG updated.
  Build compiles clean (release install, guard version).

## Benchmark requirement (standing directive)

Satisfied: A/B build-time with data (3-sample mean±std @ 200k std-bands-separated + 1M confirmation), recall
parity — the honest oracle. Directly addresses the "24min@1M" goal (→ 8.4min).

## Verdict rationale

No BLOCKER. The unsafe reinterpret is SOUND; the endianness trap is closed (guard adopted, LE unchanged); recall
preserved (8/8 tests, identical @ 200k); build ~2.2–2.9× faster. A real, measured, recall-preserving product win.
**READY_TO_MERGE.**

## Release recommendation

Product code (Rust) with a measured, recall-preserving build-time win — a legitimate release candidate. Human decides.
