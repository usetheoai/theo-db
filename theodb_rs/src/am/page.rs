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
        blob.extend_from_slice(&read_page_item(rel, i as pg_sys::BlockNumber)?);
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
unsafe fn read_all_page_items(rel: pg_sys::Relation, block: pg_sys::BlockNumber) -> Result<Vec<Vec<u8>>, String> {
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
        if m.len() < 21 {
            return Err("theodb am: truncated structured meta".into());
        }
        let nlists = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
        let centroid_npages = u32::from_le_bytes(m[17..21].try_into().unwrap());
        let mut total = 1 + centroid_npages;
        for i in 0..nlists {
            let o = 21 + i * 12 + 4; // dir entry npages field
            if m.len() < o + 4 {
                return Err("theodb am: truncated directory".into());
            }
            total += u32::from_le_bytes(m[o..o + 4].try_into().unwrap());
        }
        Ok(total)
    } else {
        // blob (M26 / HNSW): 1 meta + nchunks data pages.
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
        out.extend_from_slice(&read_page_item(rel, b)?);
    }
    Ok(out)
}

/// Build the ordered page-item sequence for the structured layout (meta · centroid chunks · per-list chunks).
/// Each item is one page. The meta's directory references the sequential block numbers (identical whether the
/// items are then extended into a fresh relation or reinit-ed in place).
fn structured_page_items(
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

    let mut cursor = 1 + centroid_npages;
    let mut dir: Vec<(u32, u32, u32)> = Vec::with_capacity(lists.len());
    for (i, enc) in encoded.iter().enumerate() {
        let np = npages_for(enc.len());
        dir.push((cursor, np, lists[i].len() as u32));
        cursor += np;
    }

    let mut meta = Vec::with_capacity(24 + dir.len() * 12);
    meta.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    meta.extend_from_slice(&1u32.to_le_bytes());
    meta.push(metric_tag);
    meta.extend_from_slice(&dim.to_le_bytes());
    meta.extend_from_slice(&nlists.to_le_bytes());
    meta.extend_from_slice(&centroid_npages.to_le_bytes());
    for (fb, np, cnt) in &dir {
        meta.extend_from_slice(&fb.to_le_bytes());
        meta.extend_from_slice(&np.to_le_bytes());
        meta.extend_from_slice(&cnt.to_le_bytes());
    }
    assert!(meta.len() < CHUNK, "theodb ivf: too many lists for a one-page directory");

    // Flatten into one page-item per page: meta, then centroid chunks, then each list's chunks.
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
    for item in structured_page_items(dim, metric_tag, centroids, lists) {
        extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, &item);
    }
}

/// Rewrite the structured layout in place (M31 VACUUM fold) — reinit existing blocks, extend new, empty leftovers
/// (same discipline as `rewrite_blob`). Drops any old pending pages beyond the new item count.
pub(crate) unsafe fn rewrite_ivf_structured(
    rel: pg_sys::Relation,
    dim: u32,
    metric_tag: u8,
    centroids: &[Vec<f32>],
    lists: &[Vec<(i64, Vec<f32>)>],
) {
    let items = structured_page_items(dim, metric_tag, centroids, lists);
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    for (i, item) in items.iter().enumerate() {
        let b = i as u32;
        if b < nblocks {
            reinit_page_with_item(rel, b, item);
        } else {
            extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, item);
        }
    }
    for b in (items.len() as u32)..nblocks {
        reinit_page_with_item(rel, b, &[]);
    }
}

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
    if m.len() < 21 {
        return Err("theodb ivf: truncated structured meta".into());
    }
    if u32::from_le_bytes(m[0..4].try_into().unwrap()) != IVF_STRUCT_MAGIC {
        return Err("theodb ivf: bad structured meta magic".into());
    }
    let metric_tag = m[8];
    let dim = u32::from_le_bytes(m[9..13].try_into().unwrap());
    let nlists = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
    let centroid_npages = u32::from_le_bytes(m[17..21].try_into().unwrap());
    let dir_off = 21;
    if m.len() < dir_off + nlists * 12 {
        return Err("theodb ivf: truncated list directory".into());
    }
    let mut dir = Vec::with_capacity(nlists);
    for i in 0..nlists {
        let o = dir_off + i * 12;
        dir.push((
            u32::from_le_bytes(m[o..o + 4].try_into().unwrap()),
            u32::from_le_bytes(m[o + 4..o + 8].try_into().unwrap()),
            u32::from_le_bytes(m[o + 8..o + 12].try_into().unwrap()),
        ));
    }
    // Centroid region: blocks 1..=centroid_npages.
    let cbytes = read_chunked(rel, 1, centroid_npages)?;
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
        return Ok(Vec::new()); // empty data page
    }
    let item_id = page_get_item_id(page, 1);
    let len = (*item_id).lp_len() as usize;
    let ptr = page_get_item(page, item_id) as *const u8;
    let out = std::slice::from_raw_parts(ptr, len).to_vec();
    pg_sys::UnlockReleaseBuffer(buf);
    Ok(out)
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
