//! store — split from the M35 page-native `hnsw_page.rs` god-file (M126, behavior-preserving;
//! byte-identical same-index A/B). Sibling items resolve via `use super::*` (re-exported in `mod.rs`).
#![allow(unused_imports)]
use super::*;
use crate::ann::{HnswIndex, Metric};
use crate::am::page;
use pgrx::pg_sys;


/// Write meta (block 0) + every page image to the (empty) fork, WAL-logged. Block ordering matches [`pack`]
/// (element pages first at block 1, then neighbor pages) so the analytic/packed addresses resolve on read.
pub(crate) unsafe fn write_structured(
    rel: pg_sys::Relation,
    fork: pg_sys::ForkNumber::Type,
    packed: &Packed,
) {
    page::extend_page_with_items(rel, fork, std::slice::from_ref(&packed.meta)); // block 0 = meta
    for pg in &packed.pages {
        page::extend_page_with_items(rel, fork, pg);
    }
}

/// Read + parse the meta page (block 0). Fail-fast typed `Err` on truncation / bad magic. For a v3 (AQ) index the
/// codebook is NOT in the meta item (it is ~48 KB at dim=768 — one page cannot hold it): after decoding the
/// descriptor, reassemble `aq_codebook` from its dedicated pages `[aq_cb_first, aq_cb_first+aq_cb_npages)` and
/// validate the length against the descriptor's `cb_len` (Rule 8 — a torn/short codebook is a typed REINDEX Err,
/// never a silently-wrong quantizer). v1/v2 read exactly as before (no codebook pages to touch).
pub(crate) unsafe fn read_meta(rel: pg_sys::Relation) -> Result<HnswMeta, String> {
    let b = page::read_page_item_at(rel, 0, 1)?;
    let mut meta = decode_meta(&b)?;
    if meta.aq_m != 0 {
        // Re-decode the descriptor for the declared codebook length (decode_meta drops it, keeping HnswMeta lean).
        // `raw_npages != 0` ⇒ this is a v4 (code/vec split) descriptor, which is longer than a v3 one.
        let d = decode_aq_descriptor(&b, meta.raw_npages != 0)?;
        meta.aq_codebook = read_codebook_pages(rel, meta.aq_cb_first, meta.aq_cb_npages, d.cb_len)?;
    }
    Ok(meta)
}

/// Reassemble the AQ codebook from its `npages` dedicated pages starting at `first` (one item per page). Validates
/// the total length equals the descriptor's `cb_len` — a mismatch (torn page, orphan, corrupt descriptor) is a
/// typed Err → REINDEX, never a silently-wrong codebook. `npages == 0` ⇒ empty (defensive; a v3 index always has
/// ≥ 1 codebook page).
pub(crate) unsafe fn read_codebook_pages(
    rel: pg_sys::Relation,
    first: u32,
    npages: u32,
    cb_len: usize,
) -> Result<Vec<u8>, String> {
    // Bounds-check the descriptor's page range against the relation BEFORE reading (the descriptor is on-disk and
    // corruptible). u64 arithmetic avoids the u32 `first + npages` wrap. A range past the fork end is a typed
    // REINDEX Err — consistent with the codebook-length guard below — not a generic C `smgrread` ERROR.
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM) as u64;
    if first as u64 + npages as u64 > nblocks {
        return Err(format!(
            "theodb hnsw: AQ codebook pages [{first}, {}) out of range (nblocks {nblocks}) — REINDEX (v3 corruption)",
            first as u64 + npages as u64
        ));
    }
    let mut cb = Vec::with_capacity(cb_len);
    for blk in first..first + npages {
        for item in page::read_all_page_items(rel, blk)? {
            cb.extend_from_slice(&item);
        }
    }
    if cb.len() != cb_len {
        return Err(format!(
            "theodb hnsw: AQ codebook length mismatch (declared {cb_len}, read {}) — REINDEX (v3 corruption)",
            cb.len()
        ));
    }
    Ok(cb)
}

/// Enumerate every stored `(tid, vector)` from the element tuples (VACUUM fold rebuilds over the live TIDs).
pub(crate) unsafe fn enumerate_entries(
    rel: pg_sys::Relation,
    meta: &HnswMeta,
) -> Result<Vec<(i64, Vec<f32>)>, String> {
    let mut out = Vec::with_capacity(meta.node_count as usize);
    // v4 (AQ code/vec split): the element pages hold HOT tuples (no f32) — the f32 lives in the cold raw region.
    // A fold enumerating a v4 index reads each live node's f32 by following its `raw_addr` into that region. v1/v2
    // keep the f32 inline in the element tuple (read it directly) — byte-identical to before.
    let is_v4 = meta.raw_npages != 0;
    let nblocks = page::main_fork_nblocks(rel);
    let bytes_to_vec = |vb: &[u8]| -> Vec<f32> {
        let dim = vb.len() / 4;
        let mut v = vec![0f32; dim];
        for (i, s) in v.iter_mut().enumerate() {
            *s = f32::from_le_bytes(vb[i * 4..i * 4 + 4].try_into().unwrap());
        }
        v
    };
    for blk in meta.elem_first..(meta.elem_first + meta.elem_npages) {
        for item in page::read_all_page_items(rel, blk)? {
            // M56: compaction reuses this enumerate → the rebuild drops tombstoned nodes here (they are gone
            // from the fresh graph, reclaiming their space). This is the ONLY reclaim path in phase 1 (slot
            // reuse on INSERT is phase 2 — it would mutate the immutable M35 graph).
            if is_v4 {
                let ev = decode_element_v4(&item)?;
                if ev.deleted {
                    continue;
                }
                let vb = page::with_page_item(rel, ev.raw_addr.0, ev.raw_addr.1, nblocks, |b| {
                    Ok(decode_raw_vec(b)?.to_vec())
                })?;
                // Rule 8 defense: the raw tuple's f32 count MUST match the hot tuple's recorded dim — a mismatch is
                // a torn/orphan raw page (corruption), a typed Err over a silently-wrong reconstructed vector.
                if vb.len() != ev.dim as usize * 4 {
                    return Err(format!(
                        "theodb hnsw: v4 raw tuple has {} f32 bytes, hot tuple dim says {} — REINDEX (v4 corruption)",
                        vb.len(), ev.dim as usize * 4
                    ));
                }
                out.push((ev.tid, bytes_to_vec(&vb)));
            } else {
                let ev = decode_element(&item)?;
                if ev.deleted {
                    continue;
                }
                out.push((ev.tid, bytes_to_vec(ev.vec_bytes)));
            }
        }
    }
    Ok(out)
}

/// M56: mark every DEAD element tuple as a tombstone IN PLACE, per page under WAL (no O(N) rebuild, no advisory
/// EXCLUSIVE). `is_dead(tid)` is the executor's visibility callback. Returns the count newly tombstoned. A
/// tombstone is navigated-through-but-not-emitted by the scan; its space is reclaimed by the next compaction.
pub(crate) unsafe fn tombstone_sweep(
    rel: pg_sys::Relation,
    meta: &HnswMeta,
    is_dead: &mut impl FnMut(i64) -> bool,
) -> u32 {
    let mut total = 0u32;
    for blk in meta.elem_first..(meta.elem_first + meta.elem_npages) {
        total += page::modify_items_under_wal(rel, blk, |item| {
            if item.len() < ELEM_HEADER || item[E_TAG] != ELEM_TAG || item[E_DELETED] != 0 {
                return false; // not an element tuple, or already a tombstone
            }
            let tid = i64::from_le_bytes(item[E_TID..E_TID + 8].try_into().unwrap());
            if is_dead(tid) {
                mark_tombstone_in_place(item)
            } else {
                false
            }
        });
    }
    total
}

/// M56: count the tombstoned element tuples (SHARE-locked read) — the numerator of the compaction ratio.
pub(crate) unsafe fn count_tombstones(rel: pg_sys::Relation, meta: &HnswMeta) -> u32 {
    let mut n = 0u32;
    for blk in meta.elem_first..(meta.elem_first + meta.elem_npages) {
        for item in page::read_all_page_items(rel, blk).unwrap_or_default() {
            if item.len() >= ELEM_HEADER && item[E_TAG] == ELEM_TAG && item[E_DELETED] != 0 {
                n += 1;
            }
        }
    }
    n
}

/// M56 fase 2: count CHURNED element slots — those whose `version > 0`, i.e. tombstoned OR revived-by-reuse since
/// the last fold (which resets `version` to 0 on rebuild). This is the trigger for the compaction that REPAIRS the
/// graph: with slot-reuse ON, tombstones are consumed by inserts before they reach the tombstone-ratio threshold,
/// so a tombstone-only trigger never fires and the incremental-insert degradation is never repaired (the churn
/// benchmark measured recall collapsing). Triggering on `version > 0` counts BOTH tombstones and reused slots, so
/// the fold fires under reuse churn too, keeping recall bounded. SHARE-locked read.
pub(crate) unsafe fn count_churned(rel: pg_sys::Relation, meta: &HnswMeta) -> u32 {
    let mut n = 0u32;
    for blk in meta.elem_first..(meta.elem_first + meta.elem_npages) {
        for item in page::read_all_page_items(rel, blk).unwrap_or_default() {
            if item.len() >= ELEM_HEADER && item[E_TAG] == ELEM_TAG && item[E_VERSION] != 0 {
                n += 1;
            }
        }
    }
    n
}

/// M56 fase 2 (T1.1 — RepairGraph/in-place insert slot-reuse): find a tombstoned element slot reusable for a
/// NEW node of level `need_level` — a slot whose element is `deleted` AND whose stored level ≥ `need_level`, so
/// the new node's fixed-per-level neighbor tuple (`level*m + m0` slots) fits where the old node's did (ADR-R1).
/// Bounded scan of the element pages; returns the first match as its `(block, off)` address. `None` ⇒ no reusable
/// slot (the caller falls back to `append_pending`). Read-only (SHARE-locked); never mutates.
pub(crate) unsafe fn find_reusable_slot(rel: pg_sys::Relation, meta: &HnswMeta, need_level: usize) -> Option<Addr> {
    for blk in meta.elem_first..(meta.elem_first + meta.elem_npages) {
        let items = match page::read_all_page_items_with_off(rel, blk) {
            Ok(v) => v,
            Err(_) => continue,
        };
        for (off, bytes) in items {
            // Never reuse the entry node's slot — it must keep its high level + entry role, else the descent from
            // meta.entry would read garbage upper-layer links. Match the level EXACTLY so the revived node Z is a
            // CLEAN node of that level (no stale upper-layer links inherited from a higher-level tombstone), which
            // the churn benchmark showed is required to keep recall from collapsing.
            if blk == meta.entry_blkno && off == meta.entry_offno {
                continue;
            }
            if let Ok(ev) = decode_element(&bytes) {
                if ev.deleted && ev.level as usize == need_level {
                    return Some((blk, off));
                }
            }
        }
    }
    None
}

/// M56 fase 2 (T1.2): revive a tombstoned element slot as a NEW node — overwrite its `tid` + f32 vector in place,
/// clear `deleted`, bump `version`; KEEP its `level`, neighbor-tuple address and `dim` (Z reuses X's graph slot so
/// its inbound arcs Y→slot now serve Z, correctly scored by Z's stored vector). v1 (f32-only) slots ONLY: a v2
/// (inline-SBQ) slot also carries a code region that would still hold X's code — reusing it would misroute the
/// Hamming walk, so we refuse (the item is longer than `E_VEC + dim*4`) and the caller falls back to `append_pending`
/// (recomputing Z's SBQ code in place is a tracked follow-up). Crash-safe via `page::modify_item_at` (GenericXLog).
/// Returns `true` iff the slot was revived.
pub(crate) unsafe fn write_reused_element(rel: pg_sys::Relation, elem_addr: Addr, tid: i64, vec: &[f32]) -> bool {
    let (blk, off) = elem_addr;
    page::modify_item_at(rel, blk, off, |item| {
        // Atomic claim: revive ONLY a still-tombstoned slot. `modify_item_at` holds the buffer EXCLUSIVE, so two
        // concurrent inserts racing for the same slot serialize here — the first flips `deleted=0` and wins; the
        // second sees `deleted==0` and returns false (the caller then falls back to `append_pending`). No lost write.
        if item.len() < ELEM_HEADER || item[E_TAG] != ELEM_TAG || item[E_DELETED] == 0 {
            return false;
        }
        let dim = u16::from_le_bytes([item[E_DIM], item[E_DIM + 1]]) as usize;
        // v1 only (exact size = header + vec, no trailing SBQ code) AND matching dim.
        if item.len() != E_VEC + dim * 4 || vec.len() != dim {
            return false;
        }
        item[E_DELETED] = 0;
        item[E_VERSION] = item[E_VERSION].wrapping_add(1);
        item[E_TID..E_TID + 8].copy_from_slice(&tid.to_le_bytes());
        for (i, &x) in vec.iter().enumerate() {
            item[E_VEC + i * 4..E_VEC + i * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
        true
    })
}

/// M56 fase 2 (T1.3): set the GROUND-layer neighbor slots of an existing neighbor tuple IN PLACE — write up to
/// `m0` addrs into the ground region (`[level*m .. level*m + m0)`), zero-padding the remainder with `(0,0)` =
/// empty. The tuple size is fixed by `level`, so this is a same-size byte edit under GenericXLog (crash-safe).
/// Used to set the reused node Z's ground neighbors after its insert search (its upper-layer slots are left as
/// the reused tuple had them — a level-0 Z has none). Returns `true` iff the slots were written.
pub(crate) unsafe fn set_ground_neighbors_inplace(
    rel: pg_sys::Relation,
    nbr_addr: Addr,
    level: usize,
    m: usize,
    m0: usize,
    neighbors: &[Addr],
) -> bool {
    let (blk, off) = nbr_addr;
    page::modify_item_at(rel, blk, off, |b| {
        if b.len() < NBR_HEADER || b[N_TAG] != NBR_TAG {
            return false;
        }
        let start = level * m;
        if b.len() < NBR_HEADER + (start + m0) * SLOT {
            return false;
        }
        for i in 0..m0 {
            let (nb_blk, nb_off) = neighbors.get(i).copied().unwrap_or((0, 0));
            let o = NBR_HEADER + (start + i) * SLOT;
            b[o..o + 4].copy_from_slice(&nb_blk.to_le_bytes());
            b[o + 4..o + 6].copy_from_slice(&nb_off.to_le_bytes());
        }
        true
    })
}

/// M56 fase 2 (T2.1): the insert-time neighbor search — greedy-descend the upper layers to a ground entry (exactly
/// as the scan's [`traverse`] does), then ground-search with `ef_construction` and return the `m0` nearest LIVE
/// nodes' ELEMENT addresses (the candidates the reused node Z will link to). v1 (exact f32) path — the reused-slot
/// insert is v1-only, so the walk scores by f32 and the candidates are already f32-ranked. Read-only.
pub(crate) unsafe fn insert_search_ground(
    rel: pg_sys::Relation,
    meta: &HnswMeta,
    q: &[f32],
    ef_construction: usize,
) -> Result<Vec<Addr>, String> {
    if meta.entry_level < 0 || meta.node_count == 0 {
        return Ok(Vec::new());
    }
    let metric = Metric::from_tag(meta.metric_tag).ok_or("theodb hnsw: unknown metric tag")?;
    let is_l2 = matches!(metric, Metric::L2);
    let (m, m0) = (meta.m as usize, meta.m0 as usize);
    let ef = ef_construction.max(m0);
    let mut reads = 0usize;
    let nblocks = page::main_fork_nblocks(rel);
    let qcode: Option<&[u8]> = None; // v1 only (the reused-slot insert gates to v1)
    let lut: Option<&crate::vec::ah::Lut16> = None; // build-time insert search is v1 f32 (no AH walk)

    let mut ep = load(rel, meta.entry_blkno, meta.entry_offno, q, metric, is_l2, qcode, lut, nblocks, &mut reads)?;
    let mut lc = meta.entry_level as usize;
    while lc >= 1 {
        loop {
            let nbrs = neighbors_of(rel, &ep, lc, m, m0, nblocks, &mut reads)?;
            let mut improved = false;
            for (nb, no) in nbrs {
                let cand = load(rel, nb, no, q, metric, is_l2, qcode, lut, nblocks, &mut reads)?;
                if cand.d < ep.d {
                    ep = cand;
                    improved = true;
                }
            }
            if !improved {
                break;
            }
        }
        lc -= 1;
    }

    let pg_src = PageNeighborSource {
        rel,
        nblocks,
        q,
        metric,
        is_l2,
        qcode,
        lut,
        m,
        m0,
        reads: std::cell::Cell::new(reads),
    };
    let (nodes, _candidates) = crate::ann::scan_core::ground_search_nodes(&pg_src, ep, ef, m0, true)?;
    let mut cands: Vec<(f64, Addr)> = nodes.iter().map(|(c, _)| (c.d, (c.blk, c.off))).collect();
    cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(cands.into_iter().take(m0).map(|(_, a)| a).collect())
}

/// M56 fase 2 (T3.1): the in-place insert with SLOT-REUSE — revive a tombstoned slot as the new node Z, properly
/// linked into the graph (`pgvector/hnswinsert.c` pattern adapted to the M35 layout). Returns `Ok(true)` iff a slot
/// was reused (the caller is done), `Ok(false)` iff there is no reusable v1 slot (the caller falls back to
/// `append_pending`). Steps, all crash-safe per page (GenericXLog), ordered so a crash never corrupts the graph:
///   1. find a reusable tombstoned v1 slot (`None` ⇒ fallback);
///   2. search the CURRENT graph for Z's `m0` nearest LIVE neighbors (the tombstone is navigated-through, not Z);
///   3. revive the slot as Z (tid + vec, `deleted=0`) — v1-gated, `false` ⇒ fallback;
///   4. set Z's ground neighbors (forward links);
///   5. add Z to each neighbor's ground list IF it has room (backward links; skip-if-full is a valid asymmetric
///      HNSW edge — the fold re-balances). Z's inbound arcs Y→slot (inherited from the reused tombstone) already
///      make Z reachable, correctly scored by its own vector, so recall is preserved (measured in T4).
pub(crate) unsafe fn insert_inplace(
    rel: pg_sys::Relation,
    meta: &HnswMeta,
    tid: i64,
    vec: &[f32],
) -> Result<bool, String> {
    if meta.sbq_bits > 0 || meta.aq_m > 0 {
        // v2 (SBQ) slot revive needs Z's recomputed inline code; v4 (AQ) needs Z's recomputed 4-bit code AND a raw
        // tuple write — both a tracked follow-up. Refuse ⇒ the caller falls back to `append_pending`. (The v4 hot
        // tuple offsets also differ from v1, so the v1-shaped revive path below must never touch a v4 index.)
        return Ok(false);
    }
    let (m, m0) = (meta.m as usize, meta.m0 as usize);
    let slot = match find_reusable_slot(rel, meta, 0) {
        Some(s) => s,
        None => return Ok(false),
    };
    // (2) find Z's neighbors in the CURRENT graph, before the slot becomes Z.
    let neighbors = insert_search_ground(rel, meta, vec, crate::am::build::HNSW_EF_CONSTRUCTION)?;
    // (3) revive the slot as Z (v1 only).
    if !write_reused_element(rel, slot, tid, vec) {
        return Ok(false);
    }
    // (4) set Z's ground neighbors. Read Z's kept level + neighbor-tuple address.
    let (zlvl, znbr) = {
        let zb = page::read_page_item_at(rel, slot.0, slot.1)?;
        let z = decode_element(&zb)?;
        (z.level as usize, z.nbr_addr)
    };
    set_ground_neighbors_inplace(rel, znbr, zlvl, m, m0, &neighbors);
    // (5) backward links: add Z (element addr == `slot`) to each neighbor's ground list if it has a free slot.
    for &n in &neighbors {
        if n == slot {
            continue;
        }
        let (nlvl, nnbr) = {
            let nb = page::read_page_item_at(rel, n.0, n.1)?;
            let ne = decode_element(&nb)?;
            (ne.level as usize, ne.nbr_addr)
        };
        let mut ground = {
            let nnb = page::read_page_item_at(rel, nnbr.0, nnbr.1)?;
            decode_neighbors(&nnb, nlvl, 0, m, m0)?
        };
        if ground.len() < m0 && !ground.contains(&slot) {
            ground.push(slot);
            set_ground_neighbors_inplace(rel, nnbr, nlvl, m, m0, &ground);
        }
    }
    Ok(true)
}

// (M48) The old in-place `rewrite_structured` was replaced by the crash-safe `fold::fold` (meta-pivot) — it
// rewrote block 0 first, so a crash mid-vacuum left the meta pointing at pages that still held old bytes (#47).
