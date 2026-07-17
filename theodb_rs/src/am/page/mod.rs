//! Page persistence for the vector index AM (M26 Phase 1). Stores ONE serialized index blob (from
//! `IvfflatIndex::to_bytes`) across the index relation's pages, WAL-logged via `GenericXLog` (crash-safe):
//!
//! - block 0 = meta item `[magic u32, version u32, blob_len u64, nchunks u32]`
//! - blocks 1..=nchunks = one blob chunk each (≤ `CHUNK` bytes), reassembled in order by `read_blob`.
//!
//! This is the minimal correct persistence for the measurement-first AM (the blueprint's anti-sunk-cost stance):
//! build once at `CREATE INDEX`, read back per scan — never rebuild per query. Primitives ported from
//! pgvectorscale (pgrx =0.16.1): `util/page.rs` (GenericXLog lifecycle), `util/buffer.rs` (extend/read), and the
//! `PageGetItem`/`PageGetItemId` macros pgrx does not expose (`util/ports.rs`).
use pgrx::pg_sys;

// M104 (Boundaries): the IVF/AQ on-disk format encode/decode cluster lives in `ivf.rs` (a
// descendant module — it can call the private page primitives in this file directly). Re-exported
// flat so every existing `page::write_ivf_*` / `page::read_ivf_*` call site is unchanged.
mod ivf;
pub(crate) use ivf::*;
mod symqg; // E2 — theodb_symqg co-located page layout (reaches the private helpers via `use super::*`)
pub(crate) use symqg::*;

const META_MAGIC: u32 = 0x5449_4D45; // "TIME" (Theodb Index MEta)
const META_VERSION: u32 = 1;
/// Max blob bytes per data page. BLCKSZ 8192 − page header − item-id − item alignment slack. 8000 is safe.
pub(crate) const CHUNK: usize = 8000;

/// Write `blob` across freshly-extended, WAL-logged pages of the (empty) index relation `rel`, in fork `fork`
/// (`MAIN_FORKNUM` for `ambuild`; `INIT_FORKNUM` for `ambuildempty` on unlogged indexes). The fork is assumed to
/// have 0 blocks (a fresh index).
pub(crate) unsafe fn write_blob(rel: pg_sys::Relation, fork: pg_sys::ForkNumber::Type, blob: &[u8]) {
    let nchunks = blob.len().div_ceil(CHUNK).max(1);
    // Meta item first (block 0).
    let mut meta = Vec::with_capacity(20);
    meta.extend_from_slice(&META_MAGIC.to_le_bytes());
    meta.extend_from_slice(&META_VERSION.to_le_bytes());
    meta.extend_from_slice(&(blob.len() as u64).to_le_bytes());
    meta.extend_from_slice(&(nchunks as u32).to_le_bytes());
    extend_page_with_item(rel, fork, &meta);
    // Data chunks (blocks 1..=nchunks). An empty blob still writes one empty data page for a uniform read path.
    if blob.is_empty() {
        extend_page_with_item(rel, fork, &[]);
    } else {
        for chunk in blob.chunks(CHUNK) {
            extend_page_with_item(rel, fork, chunk);
        }
    }
}

/// Read the blob back. Returns an empty Vec when the index has no blocks (an unbuilt/empty index).
///
/// DEPRECATED (M104): the M26 single-blob layout is legacy — superseded by the structured IVF (M31, v3–v7) and
/// HNSW (M35) layouts. Retained for read/VACUUM back-compat with pre-M31 indexes (REINDEX migrates them). New
/// builds never write a blob (see `write_ivf_structured` / `hnsw_page::write_structured`).
pub(crate) unsafe fn read_blob(rel: pg_sys::Relation) -> Result<Vec<u8>, String> {
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    if nblocks == 0 {
        return Ok(Vec::new());
    }
    let meta = read_page_item(rel, 0)?;
    if meta.len() < 20 {
        return Err("theodb am: truncated meta page".into());
    }
    let magic = u32::from_le_bytes(meta[0..4].try_into().unwrap());
    if magic != META_MAGIC {
        return Err("theodb am: bad meta page magic".into());
    }
    let blob_len = u64::from_le_bytes(meta[8..16].try_into().unwrap()) as usize;
    let nchunks = u32::from_le_bytes(meta[16..20].try_into().unwrap()) as usize;
    // Do NOT trust blob_len for the allocation — cap at what the declared chunks could physically hold, so a
    // corrupt meta page cannot trigger a multi-GB reserve before the per-page reads validate the real length.
    let mut blob = Vec::with_capacity(blob_len.min(nchunks.saturating_mul(CHUNK)));
    for i in 1..=nchunks {
        if (i as u32) >= nblocks {
            return Err("theodb am: missing data page".into());
        }
        // M38: one copy (append into `blob`) instead of the old two-copy `extend_from_slice(&read_page_item(...))`.
        read_page_item_into(rel, i as pg_sys::BlockNumber, &mut blob)?;
    }
    if blob.len() != blob_len {
        return Err("theodb am: blob length mismatch (corrupt index)".into());
    }
    Ok(blob)
}

/// Extend the given fork by one page and write `data` as its single item, WAL-logged. Returns the `BlockNumber` the
/// extension actually received (P_NEW), so a caller building a directory records the real blocks rather than assuming
/// contiguity from a pre-read `nblocks` — the latter races another backend's concurrent extend (council-index-storage,
/// M99). Existing call sites simply ignore the return value.
/// (M99: reused by the columnar TAM to create its metapage — block 0 — on relation creation.)
pub(crate) unsafe fn extend_page_with_item(
    rel: pg_sys::Relation,
    fork: pg_sys::ForkNumber::Type,
    data: &[u8],
) -> pg_sys::BlockNumber {
    debug_assert!(data.len() < CHUNK + 1);
    // Extend: serialize extension with the relation-extension lock (pgvectorscale util/buffer.rs:62).
    pg_sys::LockRelationForExtension(rel, pg_sys::ExclusiveLock as pg_sys::LOCKMODE);
    let buf = pg_sys::ReadBufferExtended(
        rel,
        fork,
        pg_sys::InvalidBlockNumber, // == P_NEW: extend by one page
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_EXCLUSIVE as i32);
    pg_sys::UnlockRelationForExtension(rel, pg_sys::ExclusiveLock as pg_sys::LOCKMODE);

    let state = pg_sys::GenericXLogStart(rel);
    let page = pg_sys::GenericXLogRegisterBuffer(state, buf, 0);
    pg_sys::PageInit(page, pg_sys::BLCKSZ as usize, 0);
    let off = pg_sys::PageAddItemExtended(
        page,
        data.as_ptr() as pg_sys::Item,
        data.len(),
        pg_sys::InvalidOffsetNumber,
        0,
    );
    assert!(off != pg_sys::InvalidOffsetNumber, "theodb am: PageAddItem failed (chunk too large?)");
    let blkno = pg_sys::BufferGetBlockNumber(buf);
    pg_sys::MarkBufferDirty(buf);
    pg_sys::GenericXLogFinish(state);
    pg_sys::UnlockReleaseBuffer(buf);
    blkno
}

/// The block where the pending region starts, read from the meta page: `1 + nchunks` (block 0 = meta, blocks
/// `1..=nchunks` = the main blob). Returns `(pending_start, nblocks)`. `nchunks==0` when the index is unbuilt.
unsafe fn pending_layout(rel: pg_sys::Relation) -> Result<(u32, u32), String> {
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    if nblocks == 0 {
        return Ok((0, 0));
    }
    // Pending starts right after the MAIN index pages — format-aware (blob or structured IVFFlat, M31).
    Ok((main_index_pages(rel)?, nblocks))
}

/// Number of pending pages `[pending_start, nblocks)`. Returns 0 (⇒ do-not-fold) for any index whose meta
/// cannot be read — a legacy M26 blob, an unbuilt index, or a torn meta. A routine VACUUM must NEVER abort on
/// this, so the `Err` is swallowed to 0 with a server WARN (the EC-3 fail-safe applied to the maintenance path,
/// per D3 "v1 blob → skip with WARN"), not propagated.
pub(crate) unsafe fn pending_page_count(rel: pg_sys::Relation) -> u32 {
    match pending_layout(rel) {
        Ok((pstart, nblocks)) if pstart > 0 && nblocks > pstart => nblocks - pstart,
        Ok(_) => 0,
        Err(e) => {
            pgrx::log!("theodb am: pending-fold skipped (meta unreadable, REINDEX to upgrade): {e}");
            0
        }
    }
}

/// Encode one pending entry: `[tid i64, dim u32, f32×dim]`.
fn encode_pending(tid: i64, vec: &[f32]) -> Vec<u8> {
    let mut e = Vec::with_capacity(12 + vec.len() * 4);
    e.extend_from_slice(&tid.to_le_bytes());
    e.extend_from_slice(&(vec.len() as u32).to_le_bytes());
    for x in vec {
        e.extend_from_slice(&x.to_le_bytes());
    }
    e
}

/// Append one `(tid, vector)` to the pending region — O(1) amortized, NO index rebuild (M26 Phase 5, ADR-2).
/// Adds to the last pending page if the item fits, else extends a new page. Requires a built index (meta page).
pub(crate) unsafe fn append_pending(rel: pg_sys::Relation, tid: i64, vec: &[f32]) -> Result<(), String> {
    let (pstart, nblocks) = pending_layout(rel)?;
    if pstart == 0 {
        return Err("theodb am: aminsert before build".into());
    }
    let item = encode_pending(tid, vec);
    // Try the last pending page first (if any) — modify it in place under WAL.
    if nblocks > pstart {
        let last = nblocks - 1;
        if try_add_to_page(rel, last, &item) {
            return Ok(());
        }
    }
    // Otherwise extend a fresh pending page.
    extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, &item);
    Ok(())
}

/// Add `item` to an existing page under WAL; returns false if it does not fit (caller extends a new page).
unsafe fn try_add_to_page(rel: pg_sys::Relation, block: pg_sys::BlockNumber, item: &[u8]) -> bool {
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_EXCLUSIVE as i32);
    let state = pg_sys::GenericXLogStart(rel);
    let page = pg_sys::GenericXLogRegisterBuffer(state, buf, 0);
    let free = pg_sys::PageGetFreeSpace(page);
    if free < item.len() + 8 {
        pg_sys::GenericXLogAbort(state);
        pg_sys::UnlockReleaseBuffer(buf);
        return false;
    }
    let off = pg_sys::PageAddItemExtended(
        page,
        item.as_ptr() as pg_sys::Item,
        item.len(),
        pg_sys::InvalidOffsetNumber,
        0,
    );
    if off == pg_sys::InvalidOffsetNumber {
        pg_sys::GenericXLogAbort(state);
        pg_sys::UnlockReleaseBuffer(buf);
        return false;
    }
    pg_sys::MarkBufferDirty(buf);
    pg_sys::GenericXLogFinish(state);
    pg_sys::UnlockReleaseBuffer(buf);
    true
}

/// Read every pending `(tid, vector)` appended since the build (M26 Phase 5). Empty when there is no pending.
pub(crate) unsafe fn read_pending(rel: pg_sys::Relation) -> Result<Vec<(i64, Vec<f32>)>, String> {
    let (pstart, nblocks) = pending_layout(rel)?;
    let pending_pages = if pstart > 0 && nblocks > pstart { nblocks - pstart } else { 0 };
    // Runtime metric (wiring pillar c, opt-in THEODB_SCAN_PROFILE=1): the O(pending) linear scan cost that
    // `pages_read` (graph-only) misses. Logged on EVERY scan (incl. 0) so the T3.1 fold win (N→0) is observable.
    if std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1") {
        pgrx::log!("theodb am pending scan: pending_pages={pending_pages}");
    }
    if pending_pages == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for block in pstart..nblocks {
        for item in read_all_page_items(rel, block)? {
            // A well-formed pending item is EXACTLY `[tid i64, dim u32, f32×dim]`. Validate the length with `==`
            // (not `<`): a crash mid-fold that extended the relation (#47, base=tail) leaves orphan body pages in
            // the old pending range `[pending_start, nblocks)`; decoded as pending they yield a bogus dim. Fail
            // LOUD (typed REINDEX error) rather than feed a garbage vector to the scan — the fail-loud crash
            // window closed fully (no REINDEX) only by M55 (ADR 0014); never silent corruption.
            if item.len() < 12 {
                return Err("theodb am: corrupt pending item (too short for header) — REINDEX".into());
            }
            let tid = i64::from_le_bytes(item[0..8].try_into().unwrap());
            let dim = u32::from_le_bytes(item[8..12].try_into().unwrap()) as usize;
            if item.len() != 12 + dim * 4 {
                return Err("theodb am: corrupt pending item (bad length — likely a post-crash orphan page) — REINDEX".into());
            }
            let mut v = Vec::with_capacity(dim);
            for i in 0..dim {
                let o = 12 + i * 4;
                v.push(f32::from_le_bytes(item[o..o + 4].try_into().unwrap()));
            }
            out.push((tid, v));
        }
    }
    Ok(out)
}

/// Read ALL items on a page (share-locked). Used for the multi-item pending pages.
pub(crate) unsafe fn read_all_page_items(rel: pg_sys::Relation, block: pg_sys::BlockNumber) -> Result<Vec<Vec<u8>>, String> {
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_SHARE as i32);
    let page = pg_sys::BufferGetPage(buf);
    let max_off = page_get_max_offset(page);
    let mut out = Vec::with_capacity(max_off);
    for off in 1..=max_off {
        let item_id = page_get_item_id(page, off as pg_sys::OffsetNumber);
        let len = (*item_id).lp_len() as usize;
        if len == 0 {
            continue;
        }
        let ptr = page_get_item(page, item_id) as *const u8;
        out.push(std::slice::from_raw_parts(ptr, len).to_vec());
    }
    pg_sys::UnlockReleaseBuffer(buf);
    Ok(out)
}

/// M56 fase 2: like [`read_all_page_items`] but returns each item's `OffsetNumber` alongside its bytes, so a
/// caller (the in-place-insert slot finder) can build the element `Addr` `(block, off)` of a reusable slot.
pub(crate) unsafe fn read_all_page_items_with_off(
    rel: pg_sys::Relation,
    block: pg_sys::BlockNumber,
) -> Result<Vec<(u16, Vec<u8>)>, String> {
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_SHARE as i32);
    let page = pg_sys::BufferGetPage(buf);
    let max_off = page_get_max_offset(page);
    let mut out = Vec::with_capacity(max_off);
    for off in 1..=max_off {
        let item_id = page_get_item_id(page, off as pg_sys::OffsetNumber);
        let len = (*item_id).lp_len() as usize;
        if len == 0 {
            continue;
        }
        let ptr = page_get_item(page, item_id) as *const u8;
        out.push((off as u16, std::slice::from_raw_parts(ptr, len).to_vec()));
    }
    pg_sys::UnlockReleaseBuffer(buf);
    Ok(out)
}

/// M56: edit the items of ONE page IN PLACE under a single `GenericXLog` (crash-safe, per-page — no advisory
/// index lock, no O(N) rebuild). For each item, `f` receives the item's mutable bytes and returns `true` iff it
/// modified them (the item size MUST NOT change — this is a fixed-offset byte edit, e.g. the tombstone flag).
/// If nothing changed the WAL record is aborted (no dirtying). Returns the count of items `f` modified. Takes
/// `BUFFER_LOCK_EXCLUSIVE` for the page only — concurrent scans (SHARE on other pages) never stall.
pub(crate) unsafe fn modify_items_under_wal(
    rel: pg_sys::Relation,
    block: pg_sys::BlockNumber,
    mut f: impl FnMut(&mut [u8]) -> bool,
) -> u32 {
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_EXCLUSIVE as i32);
    let state = pg_sys::GenericXLogStart(rel);
    // The registered page is GenericXLog's working copy; mutating it is the intended in-place edit path.
    let page = pg_sys::GenericXLogRegisterBuffer(state, buf, 0);
    let max_off = page_get_max_offset(page);
    let mut changed = 0u32;
    for off in 1..=max_off {
        let item_id = page_get_item_id(page, off as pg_sys::OffsetNumber);
        let len = (*item_id).lp_len() as usize;
        if len == 0 {
            continue;
        }
        let ptr = page_get_item(page, item_id) as *mut u8;
        let item = std::slice::from_raw_parts_mut(ptr, len);
        if f(item) {
            changed += 1;
        }
    }
    if changed > 0 {
        pg_sys::MarkBufferDirty(buf);
        pg_sys::GenericXLogFinish(state);
    } else {
        pg_sys::GenericXLogAbort(state);
    }
    pg_sys::UnlockReleaseBuffer(buf);
    changed
}

/// M56 fase 2: edit exactly ONE item at `(block, off)` in place under a single `GenericXLog` (crash-safe). `f`
/// receives the item's mutable bytes and returns `true` iff it modified them (item size MUST NOT change). Returns
/// whether the edit applied. The single-slot counterpart of [`modify_items_under_wal`], for the in-place insert
/// that rewrites ONE reused element / neighbor slot (not a whole-page sweep). `BUFFER_LOCK_EXCLUSIVE` on the page.
pub(crate) unsafe fn modify_item_at(
    rel: pg_sys::Relation,
    block: pg_sys::BlockNumber,
    off: u16,
    f: impl FnOnce(&mut [u8]) -> bool,
) -> bool {
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_EXCLUSIVE as i32);
    let state = pg_sys::GenericXLogStart(rel);
    let page = pg_sys::GenericXLogRegisterBuffer(state, buf, 0);
    let mut changed = false;
    let item_id = page_get_item_id(page, off as pg_sys::OffsetNumber);
    let len = (*item_id).lp_len() as usize;
    if len > 0 {
        let ptr = page_get_item(page, item_id) as *mut u8;
        changed = f(std::slice::from_raw_parts_mut(ptr, len));
    }
    if changed {
        pg_sys::MarkBufferDirty(buf);
        pg_sys::GenericXLogFinish(state);
    } else {
        pg_sys::GenericXLogAbort(state);
    }
    pg_sys::UnlockReleaseBuffer(buf);
    changed
}

/// Overwrite the whole relation with a fresh blob (VACUUM fold/rebuild, M26 Phase 5). Reinitializes existing
/// pages in place under WAL, then extends/uses pages for the new blob; surplus old pages are emptied. Simpler
/// than physical truncation and correct (empty trailing pages are ignored by `read_blob`/`read_pending`).
pub(crate) unsafe fn rewrite_blob(rel: pg_sys::Relation, blob: &[u8]) {
    // The simplest correct fold: append a brand-new blob after the current contents would grow the relation
    // unboundedly across vacuums. Instead, reinit block 0.. in place. We reuse write-by-reinit on each needed
    // block and empty any leftover pages.
    let nchunks = blob.len().div_ceil(CHUNK).max(1);
    let mut meta = Vec::with_capacity(20);
    meta.extend_from_slice(&META_MAGIC.to_le_bytes());
    meta.extend_from_slice(&META_VERSION.to_le_bytes());
    meta.extend_from_slice(&(blob.len() as u64).to_le_bytes());
    meta.extend_from_slice(&(nchunks as u32).to_le_bytes());

    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    let mut blocks: Vec<Vec<u8>> = Vec::with_capacity(1 + nchunks);
    blocks.push(meta);
    if blob.is_empty() {
        blocks.push(Vec::new());
    } else {
        for chunk in blob.chunks(CHUNK) {
            blocks.push(chunk.to_vec());
        }
    }
    for (i, data) in blocks.iter().enumerate() {
        let b = i as u32;
        if b < nblocks {
            reinit_page_with_item(rel, b, data);
        } else {
            extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, data);
        }
    }
    // Empty any leftover trailing pages (old pending / larger old blob).
    for b in (blocks.len() as u32)..nblocks {
        reinit_page_with_item(rel, b, &[]);
    }
}

/// Reinit an existing block to hold exactly one `data` item, WAL-logged.
/// M35 — reinit `block` in place with ALL `items` (offsets 1..=items.len()), WAL-logged. Empties the page when
/// `items` is empty. The multi-item counterpart of [`reinit_page_with_item`], used by the structured HNSW VACUUM
/// rewrite to replace the graph in place without growing the relation.
pub(crate) unsafe fn reinit_page_with_items(
    rel: pg_sys::Relation,
    block: pg_sys::BlockNumber,
    items: &[Vec<u8>],
) {
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_EXCLUSIVE as i32);
    let state = pg_sys::GenericXLogStart(rel);
    let page = pg_sys::GenericXLogRegisterBuffer(state, buf, 0);
    pg_sys::PageInit(page, pg_sys::BLCKSZ as usize, 0);
    for it in items {
        let off = pg_sys::PageAddItemExtended(
            page,
            it.as_ptr() as pg_sys::Item,
            it.len(),
            pg_sys::InvalidOffsetNumber,
            0,
        );
        assert!(off != pg_sys::InvalidOffsetNumber, "theodb am: reinit PageAddItem failed");
    }
    pg_sys::MarkBufferDirty(buf);
    pg_sys::GenericXLogFinish(state);
    pg_sys::UnlockReleaseBuffer(buf);
}

/// Pivot the fixed meta page (block 0) to a new generation — the LAST write of a crash-safe fold (M48 #47).
/// Registers block 0 with `GENERIC_XLOG_FULL_IMAGE` (nbtree/GIN meta-full-record discipline, blueprint §Q1/§Q4):
/// the whole meta is carried, torn-page-proof on redo — a delta over a torn base page would corrupt it.
pub(crate) unsafe fn pivot_meta_page(rel: pg_sys::Relation, meta: &[u8]) {
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        0,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_EXCLUSIVE as i32);
    let state = pg_sys::GenericXLogStart(rel);
    let page = pg_sys::GenericXLogRegisterBuffer(state, buf, pg_sys::GENERIC_XLOG_FULL_IMAGE as i32);
    pg_sys::PageInit(page, pg_sys::BLCKSZ as usize, 0);
    let off = pg_sys::PageAddItemExtended(
        page,
        meta.as_ptr() as pg_sys::Item,
        meta.len(),
        pg_sys::InvalidOffsetNumber,
        0,
    );
    assert!(off != pg_sys::InvalidOffsetNumber, "theodb am: pivot PageAddItem failed");
    pg_sys::MarkBufferDirty(buf);
    pg_sys::GenericXLogFinish(state);
    pg_sys::UnlockReleaseBuffer(buf);
}

unsafe fn reinit_page_with_item(rel: pg_sys::Relation, block: pg_sys::BlockNumber, data: &[u8]) {
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_EXCLUSIVE as i32);
    let state = pg_sys::GenericXLogStart(rel);
    let page = pg_sys::GenericXLogRegisterBuffer(state, buf, 0);
    pg_sys::PageInit(page, pg_sys::BLCKSZ as usize, 0);
    if !data.is_empty() {
        let off = pg_sys::PageAddItemExtended(
            page,
            data.as_ptr() as pg_sys::Item,
            data.len(),
            pg_sys::InvalidOffsetNumber,
            0,
        );
        assert!(off != pg_sys::InvalidOffsetNumber, "theodb am: reinit PageAddItem failed");
    }
    pg_sys::MarkBufferDirty(buf);
    pg_sys::GenericXLogFinish(state);
    pg_sys::UnlockReleaseBuffer(buf);
}

// ---------------------------------------------------------------------------------------------------------------
// M31 — structured IVFFlat layout for partial-page reads. Instead of one monolithic blob, the index is laid out
// as: meta page (block 0: dim, metric, nlists, centroid page count, + a per-list directory) · centroid pages ·
// per-list pages. A scan reads the meta + centroids (small, ∝ nlists) then ONLY the probed lists' pages (∝ probes),
// never the whole index. One item per page (chunked at CHUNK), reusing `extend_page_with_item`/`read_page_item`.
// ---------------------------------------------------------------------------------------------------------------


/// Peek block 0's leading magic to dispatch the scan/maintenance path (structured IVF vs M26 blob). Returns 0
/// for an unbuilt index (0 blocks).
pub(crate) unsafe fn peek_magic(rel: pg_sys::Relation) -> Result<u32, String> {
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    if nblocks == 0 {
        return Ok(0);
    }
    let m = read_page_item(rel, 0)?;
    if m.len() < 4 {
        return Ok(0);
    }
    Ok(u32::from_le_bytes(m[0..4].try_into().unwrap()))
}

/// Number of pages the MAIN index occupies before the pending region — format-aware (blob: `1 + nchunks`;
/// structured: `1 + centroid_npages + Σ list npages`). Used to locate the pending region for either layout.
unsafe fn main_index_pages(rel: pg_sys::Relation) -> Result<u32, String> {
    let m = read_page_item(rel, 0)?;
    if m.len() < 4 {
        return Err("theodb am: truncated meta page".into());
    }
    let magic = u32::from_le_bytes(m[0..4].try_into().unwrap());
    if magic == IVF_STRUCT_MAGIC {
        if m.len() < 25 {
            return Err("theodb am: truncated structured meta".into());
        }
        // Version-gate BEFORE parsing offsets — a v1 index (M31/M31b, magic identical) has a different header
        // layout, so reading `dir_npages`/`centroid_npages` at the v2/v3 offsets would misparse and yield a bogus
        // pending offset (silently dropping INSERTed rows). v2 (M34) has an implicit gen_base of 1; v3 (M48)
        // carries it explicitly. Reject anything else with the REINDEX error.
        let ver = u32::from_le_bytes(m[4..8].try_into().unwrap());
        if ver == 4 {
            // v4 (M77 AQ): pending starts after meta(gen_base) + dir + codebook + centroids + Σ list pages.
            if m.len() < 37 {
                return Err("theodb am: truncated v4 meta".into());
            }
            let nlists4 = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
            let codebook_npages = u32::from_le_bytes(m[21..25].try_into().unwrap());
            let dir_npages4 = u32::from_le_bytes(m[25..29].try_into().unwrap());
            let centroid_npages4 = u32::from_le_bytes(m[29..33].try_into().unwrap());
            let gen_base4 = u32::from_le_bytes(m[33..37].try_into().unwrap());
            let dbytes4 = read_chunked(rel, gen_base4, dir_npages4)?;
            if dbytes4.len() < nlists4 * 12 {
                return Err("theodb am: truncated v4 directory".into());
            }
            let mut total4 = gen_base4
                .saturating_add(dir_npages4)
                .saturating_add(codebook_npages)
                .saturating_add(centroid_npages4);
            for i in 0..nlists4 {
                let o = i * 12 + 4; // np field within the 12-byte dir entry
                total4 = total4.saturating_add(u32::from_le_bytes(dbytes4[o..o + 4].try_into().unwrap()));
            }
            return Ok(total4);
        }
        if ver == 5 || ver == 7 {
            // v5 (M83 storage-separated) AND v7 (M90 label-aware): identical page accounting — the label region
            // lives INSIDE the per-list code pages (already counted in code_np), and the meta/dir shape is the same.
            // pending starts after meta(gen_base) + dir(20B) + codebook + centroids + Σ(code pages + vec pages).
            if m.len() < 37 {
                return Err("theodb am: truncated v5/v7 meta".into());
            }
            let nlists5 = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
            let codebook_npages5 = u32::from_le_bytes(m[21..25].try_into().unwrap());
            let dir_npages5 = u32::from_le_bytes(m[25..29].try_into().unwrap());
            let centroid_npages5 = u32::from_le_bytes(m[29..33].try_into().unwrap());
            let gen_base5 = u32::from_le_bytes(m[33..37].try_into().unwrap());
            let dbytes5 = read_chunked(rel, gen_base5, dir_npages5)?;
            if dbytes5.len() < nlists5 * 20 {
                return Err("theodb am: truncated v5 directory".into());
            }
            let mut total5 = gen_base5
                .saturating_add(dir_npages5)
                .saturating_add(codebook_npages5)
                .saturating_add(centroid_npages5);
            for i in 0..nlists5 {
                let o = i * 20;
                total5 = total5.saturating_add(u32::from_le_bytes(dbytes5[o + 4..o + 8].try_into().unwrap()));
                total5 = total5.saturating_add(u32::from_le_bytes(dbytes5[o + 12..o + 16].try_into().unwrap()));
            }
            return Ok(total5);
        }
        if ver == 6 || ver == 8 {
            // v6 (M85 SQ8-refine) AND v8 (E1 RaBitQ-refine): byte-identical page accounting — the refine codebook
            // npages sits at m[37..41] (SQ8 for v6, RaBitQ for v8) and the dir-entry shape is the same 20B
            // (code_fb, code_np, refine_fb, refine_np, cnt). pending starts after meta(gen_base) + dir(20B) +
            // AQ codebook + refine codebook + centroids + Σ(code pages + refine pages).
            if m.len() < 41 {
                return Err("theodb am: truncated v6/v8 meta".into());
            }
            let nlists6 = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
            let aq_codebook_npages6 = u32::from_le_bytes(m[21..25].try_into().unwrap());
            let dir_npages6 = u32::from_le_bytes(m[25..29].try_into().unwrap());
            let centroid_npages6 = u32::from_le_bytes(m[29..33].try_into().unwrap());
            let gen_base6 = u32::from_le_bytes(m[33..37].try_into().unwrap());
            let sq8_codebook_npages6 = u32::from_le_bytes(m[37..41].try_into().unwrap());
            let dbytes6 = read_chunked(rel, gen_base6, dir_npages6)?;
            if dbytes6.len() < nlists6 * 20 {
                return Err("theodb am: truncated v6 directory".into());
            }
            let mut total6 = gen_base6
                .saturating_add(dir_npages6)
                .saturating_add(aq_codebook_npages6)
                .saturating_add(sq8_codebook_npages6)
                .saturating_add(centroid_npages6);
            for i in 0..nlists6 {
                let o = i * 20;
                total6 = total6.saturating_add(u32::from_le_bytes(dbytes6[o + 4..o + 8].try_into().unwrap()));
                total6 = total6.saturating_add(u32::from_le_bytes(dbytes6[o + 12..o + 16].try_into().unwrap()));
            }
            return Ok(total6);
        }
        if ver != 2 && ver != 3 {
            return Err(format!(
                "theodb am: unsupported structured format v{ver} — REINDEX to upgrade to the M48 relocatable generation (v3)"
            ));
        }
        let nlists = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
        let dir_npages = u32::from_le_bytes(m[17..21].try_into().unwrap());
        let centroid_npages = u32::from_le_bytes(m[21..25].try_into().unwrap());
        let gen_base = if ver == 3 {
            if m.len() < 29 {
                return Err("theodb am: truncated v3 meta (missing gen_base)".into());
            }
            u32::from_le_bytes(m[25..29].try_into().unwrap())
        } else {
            1
        };
        // The directory is on its own pages — read it to sum the per-list npages. Pending starts right after the
        // generation body (gen_base + dir + centroids + Σ list pages), which is the true tail for an append fold.
        let dbytes = read_chunked(rel, gen_base, dir_npages)?;
        if dbytes.len() < nlists * 12 {
            return Err("theodb am: truncated directory".into());
        }
        let mut total = gen_base.saturating_add(dir_npages).saturating_add(centroid_npages);
        for i in 0..nlists {
            let o = i * 12 + 4; // np field within the 12-byte dir entry
            total = total.saturating_add(u32::from_le_bytes(dbytes[o..o + 4].try_into().unwrap()));
        }
        Ok(total)
    } else if magic == crate::am::hnsw_page::HNSW_STRUCT_MAGIC {
        // M35 structured HNSW: pending starts right after the neighbor page range (nbr_first + nbr_npages).
        Ok(crate::am::hnsw_page::decode_meta(&m)?.pending_start())
    } else if magic == symqg::SYMQG_MAGIC {
        // E2: pending starts after gen_base + rotation codebook + directory + tids + Σ(row pages). Read the
        // directory (8B entries: first_block:u32, npages:u32) and sum the row page counts — the true tail.
        let meta = symqg::SymqgMeta::decode(&m)?;
        let dir_base = meta.gen_base.saturating_add(meta.rot_codebook_npages);
        let dbytes = read_chunked(rel, dir_base, meta.dir_npages)?;
        if dbytes.len() < meta.n as usize * 8 {
            return Err("theodb am: truncated symqg directory".into());
        }
        let mut total = meta
            .gen_base
            .saturating_add(meta.rot_codebook_npages)
            .saturating_add(meta.dir_npages)
            .saturating_add(meta.tids_npages);
        for i in 0..meta.n as usize {
            let o = i * 8 + 4; // npages field within the 8-byte dir entry
            total = total.saturating_add(u32::from_le_bytes(dbytes[o..o + 4].try_into().unwrap()));
        }
        Ok(total)
    } else {
        // blob (M26 legacy / old HNSW): 1 meta + nchunks data pages.
        if m.len() < 20 {
            return Err("theodb am: truncated blob meta".into());
        }
        Ok(1 + u32::from_le_bytes(m[16..20].try_into().unwrap()))
    }
}


/// Number of CHUNK-sized pages needed to store `nbytes` (min 1 — an empty list still gets one page so the
/// directory's `first_block` always points at a real page).
pub(crate) fn npages_for(nbytes: usize) -> u32 {
    (nbytes.div_ceil(CHUNK)).max(1) as u32
}

/// Read `npages` chunk-items starting at `first_block` and concatenate them.
pub(crate) unsafe fn read_chunked(rel: pg_sys::Relation, first_block: u32, npages: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for b in first_block..first_block + npages {
        // M38: append each chunk's bytes DIRECTLY into `out` (one copy) — was `extend_from_slice(&read_page_item(...))`
        // (two copies + intermediate realloc); the copy dominates the `reads` scan phase (M36 profiler).
        read_page_item_into(rel, b, &mut out)?;
    }
    Ok(out)
}




// (M48) The old in-place `rewrite_ivf_structured` was replaced by the crash-safe `fold::fold` (meta-pivot) via
// `ivf_structured_items` — the in-place rewrite wrote block 0 first, corrupting the index on a mid-vacuum crash (#47).




// ============================================================================================================
// M77 — IVF-AQ v4 structured layout (pg_scann): per-list AQ codes (block32) + f32 (rerank) + persisted codebook.
// Isolated from the v3 f32 path (read_ivf_meta rejects v4) so the ~134 existing IVF tests are untouched. The scan
// (scan.rs::scan_ivf_aq) reads this via read_ivf_aq_meta + read_ivf_list_bytes. Layout from gen_base=1:
//   [block 0 meta v4] · dir pages · codebook pages · centroid pages · per-list pages
// per-list bytes = [ids i64×n][f32 (dim×4)×n][AQ codes: ceil(n/32)·pairs·32 block32-transposed].
// ============================================================================================================





// ============================================================================================================
// M83 — IVF-AQ v5 STORAGE-SEPARATED layout (Roadmap v7 D3 spike): per-list codes and f32 live on DISTINCT page
// ranges so the scan reads ONLY the compact codes for the whole list (Stage 1 AH prune), then random-reads the
// f32 for the `over_fetch` survivors ONLY (Stage 2 rerank). Fixes the v4 interleaving that made M82 I/O-bound
// (ADR-0037). Behind `WITH (separate_storage=1)`; v3/v4 untouched. Layout from gen_base=1:
//   [block 0 meta v5] · dir(20B/list) · codebook · centroid · (per-list: CODE pages then VECTOR pages)
// CODE bytes = [ids i64×n][AQ codes ceil(n/32)·pairs·32 block32]; VECTOR bytes = [f32 (dim×4)×n].
// dir entry = (code_fb u32, code_np u32, vec_fb u32, vec_np u32, cnt u32).
// ============================================================================================================








/// M89 — write one page item (mirrors the pre-M89 `extend_page_with_item` call site).
#[inline]
unsafe fn write_item(rel: pg_sys::Relation, data: &[u8]) {
    extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, data);
}

/// M89 — write `data` as CHUNK-sized page items (byte-identical to the pre-M89 `push_chunks` split), streaming
/// directly to pages without an intermediate `Vec<Vec<u8>>`. An empty region still writes one empty item.
#[inline]
unsafe fn write_chunks(rel: pg_sys::Relation, data: &[u8]) {
    if data.is_empty() {
        write_item(rel, &[]);
    } else {
        for chunk in data.chunks(CHUNK) {
            write_item(rel, chunk);
        }
    }
}





// ============================================================================================================
// M85 — IVF-AQ v6 SQ8-REFINE layout (Roadmap v7): like v5 (storage-separated) but the per-list rerank region is
// SQ8 codes (`dim` B/vec) instead of raw f32 (`dim·4` B/vec) — Stage-2 survivor reads shrink 4× (ADR-0037 lever
// pushed to the high-recall frontier, M84). Behind `WITH (separate_storage=1, refine=sq8)`; v3/v4/v5 untouched.
// Layout from gen_base=1:
//   [block 0 meta v6] · dir(20B/list) · AQ codebook · SQ8 codebook · centroid · (per-list: CODE pages, SQ8 pages)
// CODE bytes = [ids i64×n][AH codes block32]; SQ8 bytes = [sq8 code dim×n]. dir = (code_fb, code_np, sq8_fb, sq8_np, cnt).
// Meta v6 header adds sq8_codebook_npages at [37..41] (41 bytes vs v4/v5's 37).
// ============================================================================================================









/// Read the single item stored on `block` (share-locked, no WAL). Copies the bytes out into an owned Vec.
unsafe fn read_page_item(rel: pg_sys::Relation, block: pg_sys::BlockNumber) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    read_page_item_into(rel, block, &mut out)?;
    Ok(out)
}

/// M38 — append `block`'s single item DIRECTLY into `out` (one copy), share-locked. This is the single item-read
/// implementation `read_page_item` (fresh Vec) and `read_chunked` (reassembly) both delegate to — eliminating the
/// double-copy of the old `read_chunked` (`read_page_item(...).to_vec()` then `extend_from_slice`), which the M36
/// profiler showed dominates the `reads` scan phase (~44% vs ~15% for the SIMD distance). Recall-zero-risk: the
/// bytes copied out are identical; only the number of memcpies changes.
unsafe fn read_page_item_into(
    rel: pg_sys::Relation,
    block: pg_sys::BlockNumber,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    // Bounds guard (M95 review HIGH-2): mirror `read_page_item_at` — a torn/concurrently-folded meta page can name
    // a block past end-of-relation; `ReadBufferExtended(RBM_NORMAL)` on it raises a C `ereport(ERROR)` longjmp,
    // which from a planner hook (cost.rs / customscan.rs meta reads) aborts ALL query planning. A typed `Err`
    // degrades to the fail-safe path instead.
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    if block >= nblocks {
        return Err(format!("theodb am: page {block} out of range (nblocks={nblocks})"));
    }
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_SHARE as i32);
    let page = pg_sys::BufferGetPage(buf);
    let max_off = page_get_max_offset(page);
    if max_off < 1 {
        pg_sys::UnlockReleaseBuffer(buf);
        return Ok(()); // empty data page — append nothing
    }
    let item_id = page_get_item_id(page, 1);
    let len = (*item_id).lp_len() as usize;
    let ptr = page_get_item(page, item_id) as *const u8;
    out.extend_from_slice(std::slice::from_raw_parts(ptr, len));
    pg_sys::UnlockReleaseBuffer(buf);
    Ok(())
}

/// M35 — read the item at (`block`, `offno`) — generalizes [`read_page_item`] (which reads offset 1) to the
/// arbitrary-offset addressing the on-demand HNSW traversal needs. Share-locked; copies the bytes out.
/// `offno` is 1-based (Postgres `OffsetNumber`). Fail-fast typed `Err` on an out-of-range offset.
pub(crate) unsafe fn read_page_item_at(
    rel: pg_sys::Relation,
    block: pg_sys::BlockNumber,
    offno: pg_sys::OffsetNumber,
) -> Result<Vec<u8>, String> {
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    if block >= nblocks {
        return Err(format!("theodb am: page {block} out of range (nblocks={nblocks})"));
    }
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_SHARE as i32);
    let page = pg_sys::BufferGetPage(buf);
    let max_off = page_get_max_offset(page);
    if (offno as usize) < 1 || (offno as usize) > max_off {
        pg_sys::UnlockReleaseBuffer(buf);
        return Err(format!("theodb am: offset {offno} out of range (max={max_off}) on page {block}"));
    }
    let item_id = page_get_item_id(page, offno);
    let len = (*item_id).lp_len() as usize;
    let ptr = page_get_item(page, item_id) as *const u8;
    let out = std::slice::from_raw_parts(ptr, len).to_vec();
    pg_sys::UnlockReleaseBuffer(buf);
    Ok(out)
}

/// M41 — copy-free variant of [`read_page_item_at`]: pin+share-lock the page, call `f` with the item bytes
/// **borrowed directly from the pinned page** (no `to_vec` alloc/memcpy), then unlock+unpin. `nblocks` is passed
/// in so a hot traversal reads `RelationGetNumberOfBlocksInFork` ONCE instead of once-per-item. `f` MUST NOT
/// leak the slice past its return (the buffer is unpinned after `f`); it returns an OWNED value (score/decoded
/// addrs). This removes the per-node alloc+copy that made the on-demand HNSW scan pay a fixed cost per vector
/// (vs theodb_ivfflat amortizing over a whole page). Same share-lock + bounds discipline as `read_page_item_at`.
pub(crate) unsafe fn with_page_item<T>(
    rel: pg_sys::Relation,
    block: pg_sys::BlockNumber,
    offno: pg_sys::OffsetNumber,
    nblocks: pg_sys::BlockNumber,
    f: impl FnOnce(&[u8]) -> Result<T, String>,
) -> Result<T, String> {
    if block >= nblocks {
        return Err(format!("theodb am: page {block} out of range (nblocks={nblocks})"));
    }
    let buf = pg_sys::ReadBufferExtended(
        rel,
        pg_sys::ForkNumber::MAIN_FORKNUM,
        block,
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_SHARE as i32);
    // RAII: release (unlock + unpin) on EVERY exit path — incl. an early `?`, an `Err` from `f`, OR a Rust panic
    // inside `f`. `f` now runs decode+score under the pin (a wider critical section than the old `to_vec`), so
    // structural release matters (mirrors pgvectorscale's `LockedBufferShare` Drop guard). M41 hardening.
    let _pin = SharePin(buf);
    let page = pg_sys::BufferGetPage(buf);
    let max_off = page_get_max_offset(page);
    if (offno as usize) < 1 || (offno as usize) > max_off {
        return Err(format!("theodb am: offset {offno} out of range (max={max_off}) on page {block}"));
    }
    let item_id = page_get_item_id(page, offno);
    let len = (*item_id).lp_len() as usize;
    let ptr = page_get_item(page, item_id) as *const u8;
    let bytes = std::slice::from_raw_parts(ptr, len);
    f(bytes) // score / decode INSIDE the pin — the borrow ends here; `_pin` releases the buffer on drop
}

/// RAII guard for a pinned + share-locked buffer: `UnlockReleaseBuffer` on drop, so the release is panic-safe by
/// construction (the unwind runs the destructor). Mirrors pgvectorscale's `LockedBufferShare` pattern.
struct SharePin(pg_sys::Buffer);
impl Drop for SharePin {
    fn drop(&mut self) {
        unsafe { pg_sys::UnlockReleaseBuffer(self.0) }
    }
}

/// M41 — read the MAIN-fork block count once (a hot traversal caches this instead of per-item).
pub(crate) unsafe fn main_fork_nblocks(rel: pg_sys::Relation) -> pg_sys::BlockNumber {
    pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM)
}

/// M35 — extend `fork` by one page and write ALL `items` onto it (offsets 1..=items.len()), WAL-logged. The
/// caller (the HNSW packer) has pre-assigned every item's `(blkno, offno)`, so this writer is dumb: it appends in
/// order. Mirrors the [`extend_page_with_item`] WAL scaffold for the multi-item case.
pub(crate) unsafe fn extend_page_with_items(
    rel: pg_sys::Relation,
    fork: pg_sys::ForkNumber::Type,
    items: &[Vec<u8>],
) {
    pg_sys::LockRelationForExtension(rel, pg_sys::ExclusiveLock as pg_sys::LOCKMODE);
    let buf = pg_sys::ReadBufferExtended(
        rel,
        fork,
        pg_sys::InvalidBlockNumber, // P_NEW
        pg_sys::ReadBufferMode::RBM_NORMAL,
        std::ptr::null_mut(),
    );
    pg_sys::LockBuffer(buf, pg_sys::BUFFER_LOCK_EXCLUSIVE as i32);
    pg_sys::UnlockRelationForExtension(rel, pg_sys::ExclusiveLock as pg_sys::LOCKMODE);

    let state = pg_sys::GenericXLogStart(rel);
    let page = pg_sys::GenericXLogRegisterBuffer(state, buf, 0);
    pg_sys::PageInit(page, pg_sys::BLCKSZ as usize, 0);
    for it in items {
        let off = pg_sys::PageAddItemExtended(
            page,
            it.as_ptr() as pg_sys::Item,
            it.len(),
            pg_sys::InvalidOffsetNumber,
            0,
        );
        assert!(off != pg_sys::InvalidOffsetNumber, "theodb am: PageAddItem failed (item too large / page full?)");
    }
    pg_sys::MarkBufferDirty(buf);
    pg_sys::GenericXLogFinish(state);
    pg_sys::UnlockReleaseBuffer(buf);
}

// --- Page macros pgrx does not expose (reimplemented from pgvectorscale util/ports.rs:47-92) ---

#[allow(non_upper_case_globals)]
const SIZE_OF_PAGE_HEADER: usize = std::mem::offset_of!(pg_sys::PageHeaderData, pd_linp);

unsafe fn page_get_item_id(page: pg_sys::Page, offset: pg_sys::OffsetNumber) -> pg_sys::ItemId {
    let header = page.cast::<pg_sys::PageHeaderData>();
    (*header).pd_linp.as_mut_ptr().add((offset - 1) as usize)
}

unsafe fn page_get_item(page: pg_sys::Page, item_id: pg_sys::ItemId) -> *mut std::os::raw::c_char {
    page.cast::<std::os::raw::c_char>().add((*item_id).lp_off() as usize)
}

unsafe fn page_get_max_offset(page: pg_sys::Page) -> usize {
    let header = page.cast::<pg_sys::PageHeaderData>();
    if (*header).pd_lower as usize <= SIZE_OF_PAGE_HEADER {
        0
    } else {
        ((*header).pd_lower as usize - SIZE_OF_PAGE_HEADER)
            / std::mem::size_of::<pg_sys::ItemIdData>()
    }
}
