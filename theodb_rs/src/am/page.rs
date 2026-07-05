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

const META_MAGIC: u32 = 0x5449_4D45; // "TIME" (Theodb Index MEta)
const META_VERSION: u32 = 1;
/// Max blob bytes per data page. BLCKSZ 8192 − page header − item-id − item alignment slack. 8000 is safe.
const CHUNK: usize = 8000;

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

/// Extend the given fork by one page and write `data` as its single item, WAL-logged.
unsafe fn extend_page_with_item(rel: pg_sys::Relation, fork: pg_sys::ForkNumber::Type, data: &[u8]) {
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
    pg_sys::MarkBufferDirty(buf);
    pg_sys::GenericXLogFinish(state);
    pg_sys::UnlockReleaseBuffer(buf);
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
    if pstart == 0 || nblocks <= pstart {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for block in pstart..nblocks {
        for item in read_all_page_items(rel, block)? {
            // A well-formed pending item is `[tid i64, dim u32, f32×dim]`. A short item means page corruption —
            // fail loud with a typed Err rather than silently dropping a row (error-handling discipline).
            if item.len() < 12 {
                return Err("theodb am: corrupt pending item (too short for header)".into());
            }
            let tid = i64::from_le_bytes(item[0..8].try_into().unwrap());
            let dim = u32::from_le_bytes(item[8..12].try_into().unwrap()) as usize;
            if item.len() < 12 + dim * 4 {
                return Err("theodb am: corrupt pending item (truncated vector)".into());
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
/// Registers block 0 with `GENERIC_XLOG_FULL_IMAGE` so the record carries the whole rewritten meta rather than
/// a delta: the meta is replaced in full, and a full image is torn-page-proof on redo (the nbtree/GIN
/// meta-full-record discipline, blueprint §Q1/§Q4 — a delta applied over a torn base page would corrupt it).
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

pub(crate) const IVF_STRUCT_MAGIC: u32 = 0x5449_5653; // "TIVS" — structured IVFFlat (M31)

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
    } else {
        // blob (M26 legacy / old HNSW): 1 meta + nchunks data pages.
        if m.len() < 20 {
            return Err("theodb am: truncated blob meta".into());
        }
        Ok(1 + u32::from_le_bytes(m[16..20].try_into().unwrap()))
    }
}

/// One list's entries encoded as `[tid i64, vector f32×dim]×count`.
fn encode_list(entries: &[(i64, Vec<f32>)]) -> Vec<u8> {
    let mut b = Vec::new();
    for (tid, v) in entries {
        b.extend_from_slice(&tid.to_le_bytes());
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
    b
}

/// Number of CHUNK-sized pages needed to store `nbytes` (min 1 — an empty list still gets one page so the
/// directory's `first_block` always points at a real page).
fn npages_for(nbytes: usize) -> u32 {
    (nbytes.div_ceil(CHUNK)).max(1) as u32
}

/// Read `npages` chunk-items starting at `first_block` and concatenate them.
unsafe fn read_chunked(rel: pg_sys::Relation, first_block: u32, npages: u32) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for b in first_block..first_block + npages {
        // M38: append each chunk's bytes DIRECTLY into `out` (one copy) — was `extend_from_slice(&read_page_item(...))`
        // (two copies + intermediate realloc); the copy dominates the `reads` scan phase (M36 profiler).
        read_page_item_into(rel, b, &mut out)?;
    }
    Ok(out)
}

/// Build the ordered page-item sequence for the structured layout (meta · centroid chunks · per-list chunks).
/// Each item is one page. The meta's directory references the sequential block numbers (identical whether the
/// items are then extended into a fresh relation or reinit-ed in place).
fn structured_page_items(
    base: u32,
    dim: u32,
    metric_tag: u8,
    centroids: &[Vec<f32>],
    lists: &[Vec<(i64, Vec<f32>)>],
) -> Vec<Vec<u8>> {
    let nlists = centroids.len() as u32;
    let mut cbytes = Vec::with_capacity(centroids.len() * dim as usize * 4);
    for c in centroids {
        for x in c {
            cbytes.extend_from_slice(&x.to_le_bytes());
        }
    }
    let centroid_npages = npages_for(cbytes.len());
    let encoded: Vec<Vec<u8>> = lists.iter().map(|l| encode_list(l)).collect();

    // The per-list directory lives on its OWN chunked page range (M34) — NOT inline on the meta page — so `lists`
    // is no longer bounded by a single page (CHUNK=8000 → ~665 lists at 12 B/entry). M48 (v3): the generation body
    // starts at `base` (block 0 is the fixed meta/pivot page; `base==1` for the initial contiguous build, `base`
    // == tail/reclaimed-region for a crash-safe fold). Layout: [block 0 meta] · gen_base: dir pages · centroid
    // pages · per-list pages. The dir's per-list first_block cursors are ABSOLUTE, resolved from `base` here.
    let dir_npages = npages_for(nlists as usize * 12);
    let mut cursor = base + dir_npages + centroid_npages;
    let mut dir: Vec<(u32, u32, u32)> = Vec::with_capacity(lists.len());
    for (i, enc) in encoded.iter().enumerate() {
        let np = npages_for(enc.len());
        dir.push((cursor, np, lists[i].len() as u32));
        cursor += np;
    }
    let mut dirbytes = Vec::with_capacity(dir.len() * 12);
    for (fb, np, cnt) in &dir {
        dirbytes.extend_from_slice(&fb.to_le_bytes());
        dirbytes.extend_from_slice(&np.to_le_bytes());
        dirbytes.extend_from_slice(&cnt.to_le_bytes());
    }

    // Meta header (block 0), fixed 29 bytes: magic · ver=3 · metric · dim · nlists · dir_npages · centroid_npages
    // · gen_base (M48). v3 adds gen_base so the generation body is relocatable for the crash-safe fold; v2 (M34)
    // is still readable with an implicit gen_base of 1 (auto-migrated to v3 on the first VACUUM fold).
    let mut meta = Vec::with_capacity(29);
    meta.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    meta.extend_from_slice(&3u32.to_le_bytes()); // format v3 — relocatable generation (M48 issue #47)
    meta.push(metric_tag);
    meta.extend_from_slice(&dim.to_le_bytes());
    meta.extend_from_slice(&nlists.to_le_bytes());
    meta.extend_from_slice(&dir_npages.to_le_bytes());
    meta.extend_from_slice(&centroid_npages.to_le_bytes());
    meta.extend_from_slice(&base.to_le_bytes());

    // One page-item per page: meta · dir chunks · centroid chunks · each list's chunks.
    let mut items: Vec<Vec<u8>> = vec![meta];
    let push_chunks = |items: &mut Vec<Vec<u8>>, data: &[u8]| {
        if data.is_empty() {
            items.push(Vec::new());
        } else {
            for chunk in data.chunks(CHUNK) {
                items.push(chunk.to_vec());
            }
        }
    };
    push_chunks(&mut items, &dirbytes);
    push_chunks(&mut items, &cbytes);
    for enc in &encoded {
        push_chunks(&mut items, enc);
    }
    items
}

/// Persist the IVFFlat index in the structured layout (M31), extending a FRESH (0-block) relation.
pub(crate) unsafe fn write_ivf_structured(
    rel: pg_sys::Relation,
    dim: u32,
    metric_tag: u8,
    centroids: &[Vec<f32>],
    lists: &[Vec<(i64, Vec<f32>)>],
) {
    // Initial build: contiguous generation right after the meta page (base = block 1).
    for item in structured_page_items(1, dim, metric_tag, centroids, lists) {
        extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, &item);
    }
}

/// Build the IVFFlat structured page items for a generation based at `base` (M48 crash-safe fold). The caller
/// (`fold::fold`) writes item 0 (meta, carrying gen_base) to block 0 LAST and items 1.. to `base..`.
pub(crate) fn ivf_structured_items(
    base: u32,
    dim: u32,
    metric_tag: u8,
    centroids: &[Vec<f32>],
    lists: &[Vec<(i64, Vec<f32>)>],
) -> Vec<Vec<u8>> {
    structured_page_items(base, dim, metric_tag, centroids, lists)
}

// (M48) The old in-place `rewrite_ivf_structured` was replaced by the crash-safe `fold::fold` (meta-pivot) via
// `ivf_structured_items` — the in-place rewrite wrote block 0 first, corrupting the index on a mid-vacuum crash (#47).

/// The parsed structured meta: dim, metric tag, centroids, and the per-list directory `(first_block, npages, count)`.
pub(crate) struct IvfMeta {
    pub dim: u32,
    pub metric_tag: u8,
    pub centroids: Vec<Vec<f32>>,
    pub dir: Vec<(u32, u32, u32)>,
}

/// Read the meta page + centroid region (small — ∝ nlists, NOT ∝ N). Typed `Err` on corruption.
pub(crate) unsafe fn read_ivf_meta(rel: pg_sys::Relation) -> Result<IvfMeta, String> {
    let m = read_page_item(rel, 0)?;
    if m.len() < 25 {
        return Err("theodb ivf: truncated structured meta".into());
    }
    if u32::from_le_bytes(m[0..4].try_into().unwrap()) != IVF_STRUCT_MAGIC {
        return Err("theodb ivf: bad structured meta magic".into());
    }
    // v2 (M34) is read with an implicit gen_base of 1 (contiguous from block 1); v3 (M48) carries an explicit
    // gen_base so the generation can live at a relocated offset after a crash-safe fold. Anything else → REINDEX.
    let ver = u32::from_le_bytes(m[4..8].try_into().unwrap());
    if ver != 2 && ver != 3 {
        return Err(format!(
            "theodb ivf: unsupported structured format v{ver} — REINDEX to upgrade to the M48 relocatable generation (v3)"
        ));
    }
    let metric_tag = m[8];
    let dim = u32::from_le_bytes(m[9..13].try_into().unwrap());
    let nlists = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
    let dir_npages = u32::from_le_bytes(m[17..21].try_into().unwrap());
    let centroid_npages = u32::from_le_bytes(m[21..25].try_into().unwrap());
    let gen_base = if ver == 3 {
        if m.len() < 29 {
            return Err("theodb ivf: truncated v3 meta (missing gen_base)".into());
        }
        u32::from_le_bytes(m[25..29].try_into().unwrap())
    } else {
        1 // v2: directory implicitly at block 1
    };
    // Directory region: blocks gen_base..=+dir_npages, chunked (no longer inline on the meta page).
    let dbytes = read_chunked(rel, gen_base, dir_npages)?;
    if dbytes.len() < nlists * 12 {
        return Err("theodb ivf: truncated list directory".into());
    }
    let mut dir = Vec::with_capacity(nlists);
    for i in 0..nlists {
        let o = i * 12;
        dir.push((
            u32::from_le_bytes(dbytes[o..o + 4].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 4..o + 8].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 8..o + 12].try_into().unwrap()),
        ));
    }
    // Centroid region: blocks gen_base+dir_npages ..= +centroid_npages.
    let cbytes = read_chunked(rel, gen_base + dir_npages, centroid_npages)?;
    let d = dim as usize;
    if d == 0 || cbytes.len() < nlists * d * 4 {
        if nlists == 0 {
            return Ok(IvfMeta { dim, metric_tag, centroids: Vec::new(), dir });
        }
        return Err("theodb ivf: truncated centroid region".into());
    }
    let mut centroids = Vec::with_capacity(nlists);
    for i in 0..nlists {
        let mut c = Vec::with_capacity(d);
        for j in 0..d {
            let o = (i * d + j) * 4;
            c.push(f32::from_le_bytes(cbytes[o..o + 4].try_into().unwrap()));
        }
        centroids.push(c);
    }
    Ok(IvfMeta { dim, metric_tag, centroids, dir })
}

/// Read ONE list's raw page bytes (`npages` chunks from `first_block`) — the hot scan path scores entries directly
/// off these bytes with a reused scratch buffer (M31), avoiding a `Vec<f32>` allocation per entry.
pub(crate) unsafe fn read_ivf_list_bytes(
    rel: pg_sys::Relation,
    first_block: u32,
    npages: u32,
) -> Result<Vec<u8>, String> {
    read_chunked(rel, first_block, npages)
}

/// Read ONE list's `(tid, vector)` entries — reads only that list's pages (the partial-read win, M31). `dim` and
/// `count`/`npages` come from the directory. Typed `Err` on corruption. (VACUUM path — allocates; the scan hot
/// path uses `read_ivf_list_bytes` + a scratch buffer instead.)
pub(crate) unsafe fn read_ivf_list(
    rel: pg_sys::Relation,
    first_block: u32,
    npages: u32,
    count: u32,
    dim: u32,
) -> Result<Vec<(i64, Vec<f32>)>, String> {
    let bytes = read_chunked(rel, first_block, npages)?;
    let d = dim as usize;
    let entry = 8 + d * 4;
    let count = count as usize;
    if bytes.len() < count * entry {
        return Err("theodb ivf: truncated list page".into());
    }
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let o = i * entry;
        let tid = i64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
        let mut v = Vec::with_capacity(d);
        for j in 0..d {
            let p = o + 8 + j * 4;
            v.push(f32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()));
        }
        out.push((tid, v));
    }
    Ok(out)
}

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
