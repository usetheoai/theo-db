//! Crash-safe VACUUM fold (M48 / issue #47) — the meta-pivot lifecycle.
//!
//! The old `rewrite_*` path rewrote the index **in place, meta (block 0) FIRST**, one `GenericXLog` record per
//! page. `GenericXLog` has no multi-record atomicity, so a crash mid-fold left a mixed state: the new meta
//! pointing at pages that still held old-generation bytes (worst case: the scan scored stale bytes as vectors —
//! a silently wrong result). This module fixes that by composing the two upstream primitives the discovery
//! blueprint anchored (`m48-am-crash-safety` §Q3/§Q4):
//!
//!   1. **GIN order** (`ginfast.c:766-772`): write the new generation to FRESH pages FIRST — inert while the
//!      fixed meta page (block 0) still points at the old generation.
//!   2. **nbtree meta-full-record** (`nbtxlog.c:81-130`): flip block 0 LAST, in its own record. Block 0 is the
//!      pivot page; a single-page `GenericXLog` record is atomic in replay, so the flip is all-or-nothing.
//!
//! Crash BEFORE the pivot ⇒ old generation intact (block 0 unchanged). Crash AFTER the pivot ⇒ new generation
//! intact (all body pages were committed before the meta record). The layout of the body pages is opaque to this
//! module (blueprint restriction, anti-rework M51): the caller's serializer produces `meta` + `body` with all
//! internal pointers already resolved relative to `base` (HNSW: `elem_first`/`nbr_first`; IVF: `gen_base`).
use crate::am::page;
use pgrx::pg_sys;

/// Where the new generation body starts. T2.1 always appends at the tail (block 0 is the fixed meta/pivot page,
/// so a body base is always ≥ 1); contiguous-region reuse (bounded growth) is T2.2's `free_region`.
pub(crate) unsafe fn tail_base(rel: pg_sys::Relation) -> u32 {
    pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM).max(1)
}

/// Write `body` (one `Vec<item-bytes>` per page) at `base..`, then pivot block 0 to `meta` — LAST, alone.
///
/// Invariant (the #47 fix): no block in `[0, base)` is modified before the pivot. When `base == tail` every body
/// page is a fresh extend; when `base < tail` (T2.2 region reuse) the pages are reinit'd in place — still safe,
/// because those blocks are NOT part of the live generation the current meta points at (the caller guarantees the
/// region is free via `free_region`).
pub(crate) unsafe fn fold(rel: pg_sys::Relation, meta: &[u8], body: &[Vec<Vec<u8>>], base: u32) {
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    // 1. shadow-write the body — inert (block 0 still points at the old generation).
    for (i, page_items) in body.iter().enumerate() {
        let b = base + i as u32;
        if b < nblocks {
            page::reinit_page_with_items(rel, b, page_items);
        } else {
            page::extend_page_with_items(rel, pg_sys::ForkNumber::MAIN_FORKNUM, page_items);
        }
        // T2.3 wires the crash-injection hook here (after page `i+1`, before the pivot).
    }
    // 2. pivot — flip the fixed meta page LAST, in its own record, as a FULL IMAGE (D2 / blueprint §Q1/§Q4:
    // a delta over a torn base page would corrupt the meta; the full image is torn-page-proof on redo).
    page::pivot_meta_page(rel, meta);
}
