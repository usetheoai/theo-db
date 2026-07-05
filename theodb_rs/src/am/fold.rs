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

/// Choose the base block for the next generation body (T2.2 — bounded growth). PURE (no `Relation`) so the
/// data-loss-sensitive arithmetic is unit-testable in isolation.
///
/// A tail-append fold leaves the layout `0=meta | [1, cur_gen_start)=dead old gen | [cur_gen_start, …)=live gen
/// | pending`. The reclaimable contiguous free region is exactly `[1, cur_gen_start)`. Lowest-fit: reuse it when
/// the new generation (`need` pages) fits — the body then lands entirely inside the dead region, never touching
/// block 0, the live generation, or pending — else extend at the tail. Reusing the low region caps the relation
/// at ~two generations across the low/high alternation instead of growing every fold.
pub(crate) fn free_region(cur_gen_start: u32, nblocks: u32, need: u32) -> u32 {
    if cur_gen_start > 1 && cur_gen_start - 1 >= need {
        1 // fits in the dead low region [1, cur_gen_start): base+need ≤ cur_gen_start, so no live page is touched
    } else {
        nblocks.max(1) // extend at the tail (block 0 is the fixed meta page, so a body base is always ≥ 1)
    }
}

/// The current live generation's first block, read from the meta — the input `free_region` reclaims BEFORE.
/// Same pointer the scan follows (single source of truth: HNSW `elem_first`, IVF v3 `gen_base`); a divergence
/// here would let the reclaim overwrite live data. Falls back to 1 (contiguous) for an unbuilt/legacy meta.
pub(crate) unsafe fn cur_gen_start(rel: pg_sys::Relation) -> u32 {
    match page::peek_magic(rel) {
        Ok(m) if m == crate::am::hnsw_page::HNSW_STRUCT_MAGIC => {
            crate::am::hnsw_page::read_meta(rel).map(|meta| meta.elem_first).unwrap_or(1)
        }
        Ok(m) if m == page::IVF_STRUCT_MAGIC => page::ivf_gen_base(rel).unwrap_or(1),
        _ => 1,
    }
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
        // T4.1/D4: the fold runs inside VACUUM — `vacuum_delay_point` applies the cost-based throttle AND checks
        // for interrupts per page, so a VACUUM of a huge index responds to `pg_cancel_backend` (and to the cost
        // limit) mid-fold, not only at batch boundaries of the rebuild.
        pg_sys::vacuum_delay_point();
        // T2.3 crash injection: after this body page's WAL record is committed (reinit/extend already ran
        // GenericXLogFinish), before the pivot. Default GUC 0 ⇒ no-op in production.
        crate::am::guc::maybe_crash_after_body_page(i as u32 + 1);
    }
    // 2. pivot — flip the fixed meta page LAST, in its own record, as a FULL IMAGE (D2 / blueprint §Q1/§Q4:
    // a delta over a torn base page would corrupt the meta; the full image is torn-page-proof on redo).
    page::pivot_meta_page(rel, meta);
    // T2.3 crash injection: right after the pivot is committed, before reclaim — proves the new generation is
    // intact on recovery (block 0 already points at it).
    crate::am::guc::maybe_crash_at_phase(crate::am::guc::CRASH_PHASE_POST_PIVOT);
    // 3. reclaim (T2.2 — bounded growth): reinit every page AFTER the new generation to EMPTY, so the pending
    // range `[gen_end, nblocks)` reads clean (0 entries) — this makes a reused-low-region fold NOT grow the
    // relation, and turns a dead tail generation back into free space for the next fold. `read_pending` fails
    // LOUD (typed Err → REINDEX) on any non-empty stale page, so a crash mid-reclaim is fail-loud, never silent
    // corruption. Fully-atomic crash-safe reclaim (no REINDEX window) is deferred to M55 (fold incremental vs
    // in-place) — see ROADMAP § M55; this is the conservative, non-corrupting bound.
    let gen_end = base + body.len() as u32;
    let nblocks_now = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    for (reclaimed, b) in (gen_end..nblocks_now).enumerate() {
        page::reinit_page_with_items(rel, b, &[]);
        // T2.3 crash injection: after the first leftover page is emptied — the moment that proves the
        // crash-mid-reclaim window is FAIL-LOUD (read_pending → typed REINDEX error), never silent corruption.
        if reclaimed == 0 {
            crate::am::guc::maybe_crash_at_phase(crate::am::guc::CRASH_PHASE_MID_RECLAIM);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::free_region;

    #[test]
    fn free_region_reuses_low_dead_region_when_it_fits() {
        // dead low region [1, 100) has 99 pages; a 50-page generation fits → reuse at base 1.
        assert_eq!(free_region(100, 200, 50), 1);
    }

    #[test]
    fn free_region_extends_when_low_region_too_small() {
        // dead low region [1, 10) has 9 pages; a 50-page generation does not fit → extend at the tail.
        assert_eq!(free_region(10, 200, 50), 200);
    }

    #[test]
    fn free_region_extends_for_a_contiguous_index_with_no_dead_region() {
        // cur_gen_start == 1 → no dead low region at all → always extend.
        assert_eq!(free_region(1, 100, 5), 100);
    }

    #[test]
    fn reused_base_never_touches_the_live_generation() {
        // Property (guard EC-2 reframed): whenever free_region reuses the low region (returns 1), the body
        // `[1, 1+need)` stays strictly below cur_gen_start — it can never overwrite block 0, the live gen, or
        // pending. When it extends, the base is ≥ nblocks (past everything). Checked across a range of inputs.
        for cur in 1u32..40 {
            for need in 0u32..40 {
                for nblocks in cur..80 {
                    let base = free_region(cur, nblocks, need);
                    if base == 1 {
                        assert!(1 + need <= cur, "reuse must fit strictly below the live gen (cur={cur} need={need})");
                        assert!(cur > 1, "reuse requires a non-empty dead low region");
                    } else {
                        assert!(base >= nblocks.max(1), "extend must be at/after the tail");
                    }
                }
            }
        }
    }
}
