# Review — M41 theodb_hnsw scan QPS optimization

**Date:** 2026-07-03
**Slug:** m41-hnsw-qps
**Milestone:** M41
**Verdict:** READY_TO_MERGE
**Scope:** Rust production code — `theodb_rs/src/am/{page,hnsw_page}.rs` (unsafe pgrx buffer path).

## Change

The on-demand HNSW `traverse` now decodes + scores each visited node inside the pinned page scope
(`page::with_page_item` — no per-node `to_vec` alloc/memcpy) and caches `RelationGetNumberOfBlocksInFork` once per
query. Motivated by M40 (theodb_hnsw 3–5× slower than theodb_ivfflat at matched recall).

## Findings by dimension

| Dimension | Result | Evidence |
|---|---|---|
| **unsafe / pgrx buffer safety** | PASS (SOUND) | Focused rust-pgrx audit: no buffer leak, no borrow-escape (`T` not lifetime-parameterized → closure cannot return `&[u8]`), `Err`-from-closure still releases. Every exit path releases. |
| **Panic-safety across C boundary** | PASS + HARDENED | The audit flagged that the refactor widened the pinned critical section to include decode+score (residual: a future panic inside `f` would leak the pin until abort). **Adopted the reviewer's RAII-guard recommendation** (`SharePin` with `impl Drop { UnlockReleaseBuffer }`) — release is now panic-safe by construction, mirroring pgvectorscale's `LockedBufferShare`. The live path was already safe (dim-mismatch panic closed upstream at `scan.rs:127`). |
| **Recall preservation (correctness)** | PASS | 8/8 `benchmarks/tests/test_index_am.py` green on `theo-db:m41` (incl. `test_hnsw_am_persists_pushes_down_and_recalls`); recall byte-identical at every ef in the A/B benchmark. |
| **Performance (the goal)** | PASS (honest) | A/B, rigorous 4-sample mean±std: QPS **1.2–1.5×** at identical recall (ef=200: 385±30 → 562±23 — std bands separated, significant; the win grows with ef). A single cross-session run falsely showed 2.4–3.0× — corrected as CPU-throttling variance (M38/M40 lesson). `docs/benchmarks/m41-hnsw-qps.md`. |
| **nblocks caching safety** | PASS | Audit: `nblocks` is only a corruption bounds-check; traverse visits only build-time-encoded addrs (< nblocks); the pending region is read separately (`scan.rs:136`). A stale (too-small) nblocks cannot falsely reject a traversed block. |
| **Parsimony / no new dep** | PASS | Reuses the M31b `l2_dist_from_bytes` (already scores off `&[u8]`); zero new dependency; layout + traversal + top-k unchanged. |
| **CHANGELOG / ROADMAP** | PASS | `[Unreleased] § Changed` + ROADMAP M41 `[x]` with the A/B numbers + recall-identical note. |

## Hard gates

- Failing tests → none (8/8 AM tests green post-hardening rebuild). No secrets. On `develop`. No `Co-Authored-By`.
  CHANGELOG updated. Build compiled clean (release install).

## Benchmark requirement (standing directive)

Satisfied: A/B benchmark with data (n=50k, 4 alternating samples mean±std, recall byte-identical), a real
**1.2–1.5×** QPS win (significant at ef=200) — end-to-end, recall-controlled, variance-honest.

## Verdict rationale

No BLOCKER. The unsafe path is SOUND and now panic-safe by construction (reviewer recommendation adopted). Recall
preserved (8/8 tests), QPS improved a real **1.2–1.5×** (rigorous 4-sample A/B; significant at ef=200). Modest but
honest — the inflated 2.4–3.0× first number was corrected as CPU variance. **READY_TO_MERGE.**

## Release recommendation

This is product code (Rust) with a measured, recall-preserving performance win — a legitimate release candidate
(unlike the measurement-only milestones). Human decides bump/timing.
