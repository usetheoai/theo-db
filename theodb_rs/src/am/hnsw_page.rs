//! M35 — page-native structured persistence for `theodb_hnsw`.
//!
//! Replaces the M26 single-blob layout (scan deserializes the whole graph → O(N), `ann/hnsw.rs:243`) with the
//! pgvector-style split-tuple layout so the scan traverses ON DEMAND, reading only visited nodes → O(ef·M) pages.
//!
//! Layout (mirrors pgvector `HnswMetaPageData` / `HnswElementTupleData` / `HnswNeighborTupleData`):
//! - block 0 = **meta**: params + entry point `(blkno,offno,level)` + the four range bounds.
//! - blocks `[1 .. 1+elem_npages)` = **element tuples** (fixed size): `tag,level,tid,nbr_addr,dim,vector`.
//! - blocks `[nbr_first .. nbr_first+nbr_npages)` = **neighbor tuples** (variable, one per node): all layers'
//!   neighbor addresses, laid out top→ground (`start=(level-lc)*m`, ground `m0` slots at the end).
//! - the pending region (INSERTed-after-build rows) follows, located via the meta (unchanged from M26/M31).
//!
//! ADR-1 (blueprint): the graph is IMMUTABLE between VACUUM rebuilds (INSERT→pending, DELETE→rebuild), so there
//! is no on-disk incremental-insert / tombstone / stale-ref machinery — just a codec + an on-demand read path.
//! ADR-2: because the whole graph is in memory at build time, [`pack`] resolves every `(blkno,offno)` up front
//! (element size is fixed → analytic addrs; neighbor tuples packed by a deterministic in-memory packer) and
//! returns fully-formed page images — no placeholder tuple, no `PageIndexTupleOverwrite`.
use crate::ann::{HnswIndex, Metric};

pub(crate) const HNSW_STRUCT_MAGIC: u32 = 0x5448_5353; // "THSS" (Theodb Hnsw StructuredScan)
const HNSW_STRUCT_VERSION: u32 = 1;
const ELEM_TAG: u8 = 1;
const NBR_TAG: u8 = 2;

// PostgreSQL page arithmetic (BLCKSZ 8192, standard build). `usable` excludes the page header; each item costs
// its 4-byte line pointer + the MAXALIGN'd item length. Matches how `PageAddItem` accounts free space.
const BLCKSZ: usize = 8192;
const PAGE_HEADER: usize = 24; // SizeOfPageHeaderData
const ITEMID: usize = 4; // sizeof(ItemIdData)
const USABLE: usize = BLCKSZ - PAGE_HEADER;

// Element tuple: fixed header + the raw f32 vector. `nbr_blkno/nbr_offno` point to this node's neighbor tuple.
const E_TAG: usize = 0;
const E_LEVEL: usize = 1;
// bytes 2..4 = pad (keeps the i64 tid 4-aligned)
const E_TID: usize = 4;
const E_NBR_BLK: usize = 12;
const E_NBR_OFF: usize = 16;
const E_DIM: usize = 18;
const E_VEC: usize = 20; // ELEM_HEADER
const ELEM_HEADER: usize = E_VEC;

// Neighbor tuple: header + `count` slots, each a 6-byte index pointer (blkno u32, offno u16). (0,0) = empty slot.
const N_TAG: usize = 0;
// byte 1 = version (0)
const N_COUNT: usize = 2;
const N_SLOTS: usize = 4; // NBR_HEADER
const NBR_HEADER: usize = N_SLOTS;
const SLOT: usize = 6; // (u32 blkno, u16 offno)

const fn maxalign(n: usize) -> usize {
    (n + 7) & !7
}
fn elem_size(dim: usize) -> usize {
    ELEM_HEADER + dim * 4
}
fn elems_per_page(dim: usize) -> usize {
    (USABLE / (ITEMID + maxalign(elem_size(dim)))).max(1)
}
/// Total neighbor slots for a node at `level`: `m` per upper layer (`level` of them) + `m0` on the ground.
fn nbr_slots(level: usize, m: usize, m0: usize) -> usize {
    level * m + m0
}
fn nbr_size(level: usize, m: usize, m0: usize) -> usize {
    NBR_HEADER + nbr_slots(level, m, m0) * SLOT
}

/// An index-internal pointer to a tuple (its Postgres `(BlockNumber, OffsetNumber)`; offsets are 1-based).
pub(crate) type Addr = (u32, u16);

/// Fully-resolved page images ready for a dumb WAL writer to flush. `pages[0]` is block 1, `pages[1]` block 2, …
/// (block 0 is `meta`). Element pages come first, then neighbor pages.
pub(crate) struct Packed {
    pub(crate) meta: Vec<u8>,
    pub(crate) pages: Vec<Vec<Vec<u8>>>, // pages[p] = the item blobs for block (p+1), in offset order
}

/// Parsed meta (block 0). `entry_level < 0` ⇒ empty graph.
#[derive(Debug, Clone)]
pub(crate) struct HnswMeta {
    pub(crate) metric_tag: u8,
    pub(crate) dim: u32,
    pub(crate) m: u16,
    pub(crate) m0: u16,
    pub(crate) entry_blkno: u32,
    pub(crate) entry_offno: u16,
    pub(crate) entry_level: i16,
    pub(crate) node_count: u32,
    pub(crate) elem_first: u32,
    pub(crate) elem_npages: u32,
    pub(crate) nbr_first: u32,
    pub(crate) nbr_npages: u32,
}

impl HnswMeta {
    /// First block of the pending region (the structured index occupies blocks `0 ..= nbr_first+nbr_npages-1`).
    pub(crate) fn pending_start(&self) -> u32 {
        self.nbr_first + self.nbr_npages
    }
}

const META_LEN: usize = 4 + 4 + 1 + 4 + 2 + 2 + 4 + 2 + 2 + 4 + 4 + 4 + 4 + 4; // = 45 bytes

fn encode_meta(m: &HnswMeta) -> Vec<u8> {
    let mut b = Vec::with_capacity(META_LEN);
    b.extend_from_slice(&HNSW_STRUCT_MAGIC.to_le_bytes());
    b.extend_from_slice(&HNSW_STRUCT_VERSION.to_le_bytes());
    b.push(m.metric_tag);
    b.extend_from_slice(&m.dim.to_le_bytes());
    b.extend_from_slice(&m.m.to_le_bytes());
    b.extend_from_slice(&m.m0.to_le_bytes());
    b.extend_from_slice(&m.entry_blkno.to_le_bytes());
    b.extend_from_slice(&m.entry_offno.to_le_bytes());
    b.extend_from_slice(&m.entry_level.to_le_bytes());
    b.extend_from_slice(&m.node_count.to_le_bytes());
    b.extend_from_slice(&m.elem_first.to_le_bytes());
    b.extend_from_slice(&m.elem_npages.to_le_bytes());
    b.extend_from_slice(&m.nbr_first.to_le_bytes());
    b.extend_from_slice(&m.nbr_npages.to_le_bytes());
    b
}

/// Parse the meta item. Fail-fast typed `Err` on truncation / bad magic / unknown version — never panic.
pub(crate) fn decode_meta(b: &[u8]) -> Result<HnswMeta, String> {
    if b.len() < META_LEN {
        return Err("theodb hnsw: truncated meta page".into());
    }
    let magic = u32::from_le_bytes(b[0..4].try_into().unwrap());
    if magic != HNSW_STRUCT_MAGIC {
        return Err("theodb hnsw: bad structured meta magic (REINDEX to upgrade the M26 blob to the M35 \
                    page-native format)".into());
    }
    if u32::from_le_bytes(b[4..8].try_into().unwrap()) != HNSW_STRUCT_VERSION {
        return Err("theodb hnsw: unsupported structured meta version".into());
    }
    let u16a = |o: usize| u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
    let u32a = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    Ok(HnswMeta {
        metric_tag: b[8],
        dim: u32a(9),
        m: u16a(13),
        m0: u16a(15),
        entry_blkno: u32a(17),
        entry_offno: u16a(21),
        entry_level: i16::from_le_bytes(b[23..25].try_into().unwrap()),
        node_count: u32a(25),
        elem_first: u32a(29),
        elem_npages: u32a(33),
        nbr_first: u32a(37),
        nbr_npages: u32a(41),
    })
}

/// A decoded element tuple: its level, heap tid, neighbor-tuple address, and the raw vector byte slice (scored
/// directly via the M31b SIMD `l2_dist_from_bytes` — no per-node `Vec<f32>` alloc).
pub(crate) struct ElementView<'a> {
    pub(crate) level: u8,
    pub(crate) tid: i64,
    pub(crate) nbr_addr: Addr,
    pub(crate) vec_bytes: &'a [u8],
}

pub(crate) fn decode_element(b: &[u8]) -> Result<ElementView<'_>, String> {
    if b.len() < ELEM_HEADER || b[E_TAG] != ELEM_TAG {
        return Err("theodb hnsw: bad element tuple".into());
    }
    let dim = u16::from_le_bytes(b[E_DIM..E_DIM + 2].try_into().unwrap()) as usize;
    let end = E_VEC + dim * 4;
    if b.len() < end {
        return Err("theodb hnsw: truncated element tuple".into());
    }
    Ok(ElementView {
        level: b[E_LEVEL],
        tid: i64::from_le_bytes(b[E_TID..E_TID + 8].try_into().unwrap()),
        nbr_addr: (
            u32::from_le_bytes(b[E_NBR_BLK..E_NBR_BLK + 4].try_into().unwrap()),
            u16::from_le_bytes(b[E_NBR_OFF..E_NBR_OFF + 2].try_into().unwrap()),
        ),
        vec_bytes: &b[E_VEC..end],
    })
}

/// Neighbor addresses of a node at `level` on `layer` `lc`, skipping empty `(0,0)` slots. Per-layer slice math
/// mirrors pgvector (`hnswutils.c:784`): ground (lc==0) is the last `m0` slots; upper layer `lc` starts at
/// `(level-lc)*m` and spans `m`.
pub(crate) fn decode_neighbors(
    b: &[u8],
    level: usize,
    lc: usize,
    m: usize,
    m0: usize,
) -> Result<Vec<Addr>, String> {
    let mut out = Vec::new();
    decode_neighbors_into(b, level, lc, m, m0, &mut out)?;
    Ok(out)
}

/// M46 L1-B: the allocation-free heart of `decode_neighbors`. Decodes the neighbor addrs of a node's layer
/// into a caller-owned scratch `Vec` (cleared first), so the ground-layer traversal can reuse ONE buffer
/// across every expanded node instead of allocating a fresh `Vec` per node (`hnsw_page.rs` ground loop).
/// Mirrors pgvector's reused `unvisited` scratch (`hnswutils.c:834`). Semantically identical to the original.
pub(crate) fn decode_neighbors_into(
    b: &[u8],
    level: usize,
    lc: usize,
    m: usize,
    m0: usize,
    out: &mut Vec<Addr>,
) -> Result<(), String> {
    out.clear();
    if b.len() < NBR_HEADER || b[N_TAG] != NBR_TAG {
        return Err("theodb hnsw: bad neighbor tuple".into());
    }
    if lc > level {
        return Ok(());
    }
    let (start, len) = if lc == 0 { (level * m, m0) } else { ((level - lc) * m, m) };
    out.reserve(len);
    for i in 0..len {
        let o = NBR_HEADER + (start + i) * SLOT;
        if b.len() < o + SLOT {
            return Err("theodb hnsw: truncated neighbor tuple".into());
        }
        let blk = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let off = u16::from_le_bytes(b[o + 4..o + 6].try_into().unwrap());
        if blk != 0 || off != 0 {
            out.push((blk, off));
        }
    }
    Ok(())
}

fn encode_element(idx: &HnswIndex, node: usize, nbr_addr: Addr, dim: usize) -> Vec<u8> {
    let mut b = vec![0u8; elem_size(dim)];
    b[E_TAG] = ELEM_TAG;
    b[E_LEVEL] = idx.node_level(node) as u8;
    b[E_TID..E_TID + 8].copy_from_slice(&idx.node_id(node).to_le_bytes());
    b[E_NBR_BLK..E_NBR_BLK + 4].copy_from_slice(&nbr_addr.0.to_le_bytes());
    b[E_NBR_OFF..E_NBR_OFF + 2].copy_from_slice(&nbr_addr.1.to_le_bytes());
    b[E_DIM..E_DIM + 2].copy_from_slice(&(dim as u16).to_le_bytes());
    for (j, &f) in idx.node_vector(node).iter().enumerate() {
        b[E_VEC + j * 4..E_VEC + j * 4 + 4].copy_from_slice(&f.to_le_bytes());
    }
    b
}

fn encode_neighbors(idx: &HnswIndex, node: usize, elem_addr: &[Addr], m: usize, m0: usize) -> Vec<u8> {
    let level = idx.node_level(node);
    let slots = nbr_slots(level, m, m0);
    let mut b = vec![0u8; nbr_size(level, m, m0)];
    b[N_TAG] = NBR_TAG;
    b[N_COUNT..N_COUNT + 2].copy_from_slice(&(slots as u16).to_le_bytes());
    // top→ground; upper layer lc at (level-lc)*m for `m` slots, ground at level*m for `m0` slots.
    let write_layer = |b: &mut [u8], lc: usize, start: usize, cap: usize| {
        for (i, &nb) in idx.node_neighbors(node, lc).iter().take(cap).enumerate() {
            let (blk, off) = elem_addr[nb];
            let o = NBR_HEADER + (start + i) * SLOT;
            b[o..o + 4].copy_from_slice(&blk.to_le_bytes());
            b[o + 4..o + 6].copy_from_slice(&off.to_le_bytes());
        }
    };
    for lc in (1..=level).rev() {
        write_layer(&mut b, lc, (level - lc) * m, m);
    }
    write_layer(&mut b, 0, level * m, m0);
    b
}

/// Resolve the whole in-memory graph into meta + page images (ADR-2 — no I/O, unit-testable). Returns `Err` if a
/// neighbor tuple would exceed one page (impossible under the build's level cap, asserted here defensively).
pub(crate) fn pack(idx: &HnswIndex) -> Result<Packed, String> {
    let (metric, m, m0, _ef) = idx.params();
    let n = idx.node_count();
    let dim = idx.dim();

    // Empty graph: meta only, entry_level = -1.
    if n == 0 {
        let meta = encode_meta(&HnswMeta {
            metric_tag: metric.tag(), dim: dim as u32, m: m as u16, m0: m0 as u16,
            entry_blkno: 0, entry_offno: 0, entry_level: -1, node_count: 0,
            elem_first: 1, elem_npages: 0, nbr_first: 1, nbr_npages: 0,
        });
        return Ok(Packed { meta, pages: Vec::new() });
    }

    // 1. Analytic element addresses (fixed size ⇒ node i is at block 1+i/ipp, offset 1+i%ipp).
    let ipp = elems_per_page(dim);
    let elem_npages = n.div_ceil(ipp);
    let elem_addr: Vec<Addr> =
        (0..n).map(|i| ((1 + i / ipp) as u32, (1 + i % ipp) as u16)).collect();
    let nbr_first = 1 + elem_npages;

    // 2. Pack neighbor tuples by free space, starting at nbr_first. Content uses the analytic elem addrs.
    let mut nbr_pages: Vec<Vec<Vec<u8>>> = vec![Vec::new()];
    let mut used = 0usize;
    let mut nbr_addr: Vec<Addr> = Vec::with_capacity(n);
    for node in 0..n {
        let level = idx.node_level(node);
        let size = nbr_size(level, m, m0);
        let cost = ITEMID + maxalign(size);
        if cost > USABLE {
            return Err(format!("theodb hnsw: neighbor tuple for a level-{level} node exceeds one page \
                                ({size} B) — build must cap max level"));
        }
        if used + cost > USABLE && !nbr_pages.last().unwrap().is_empty() {
            nbr_pages.push(Vec::new());
            used = 0;
        }
        let blkno = (nbr_first + nbr_pages.len() - 1) as u32;
        let page = nbr_pages.last_mut().unwrap();
        let offno = (page.len() + 1) as u16;
        nbr_addr.push((blkno, offno));
        page.push(encode_neighbors(idx, node, &elem_addr, m, m0));
        used += cost;
    }
    let nbr_npages = nbr_pages.len();

    // 3. Element pages: fixed ipp items per page (matches the analytic addrs), neighbortid = the packed nbr addr.
    let mut elem_pages: Vec<Vec<Vec<u8>>> = Vec::with_capacity(elem_npages);
    for chunk in (0..n).collect::<Vec<_>>().chunks(ipp) {
        elem_pages.push(chunk.iter().map(|&node| encode_element(idx, node, nbr_addr[node], dim)).collect());
    }

    // 4. Meta with the entry point resolved to its element addr.
    let entry_node = idx.entry().ok_or("theodb hnsw: non-empty graph without an entry point")?;
    let (eb, eo) = elem_addr[entry_node];
    let meta = encode_meta(&HnswMeta {
        metric_tag: metric.tag(), dim: dim as u32, m: m as u16, m0: m0 as u16,
        entry_blkno: eb, entry_offno: eo, entry_level: idx.node_level(entry_node) as i16,
        node_count: n as u32, elem_first: 1, elem_npages: elem_npages as u32,
        nbr_first: nbr_first as u32, nbr_npages: nbr_npages as u32,
    });

    let mut pages = elem_pages;
    pages.extend(nbr_pages);
    Ok(Packed { meta, pages })
}

// ---------------------------------------------------------------------------------------------------------------
// FFI: write the packed images to WAL-logged pages, read the meta, and traverse the graph on demand.
// ---------------------------------------------------------------------------------------------------------------
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

/// Read + parse the meta page (block 0). Fail-fast typed `Err` on truncation / bad magic.
pub(crate) unsafe fn read_meta(rel: pg_sys::Relation) -> Result<HnswMeta, String> {
    let b = page::read_page_item_at(rel, 0, 1)?;
    decode_meta(&b)
}

/// Enumerate every stored `(tid, vector)` from the element tuples (VACUUM fold rebuilds over the live TIDs).
pub(crate) unsafe fn enumerate_entries(
    rel: pg_sys::Relation,
    meta: &HnswMeta,
) -> Result<Vec<(i64, Vec<f32>)>, String> {
    let mut out = Vec::with_capacity(meta.node_count as usize);
    for blk in meta.elem_first..(meta.elem_first + meta.elem_npages) {
        for item in page::read_all_page_items(rel, blk)? {
            let ev = decode_element(&item)?;
            let dim = ev.vec_bytes.len() / 4;
            let mut v = vec![0f32; dim];
            for (i, s) in v.iter_mut().enumerate() {
                *s = f32::from_le_bytes(ev.vec_bytes[i * 4..i * 4 + 4].try_into().unwrap());
            }
            out.push((ev.tid, v));
        }
    }
    Ok(out)
}

/// Rewrite the structured graph in place (VACUUM fold): reinit block 0 (meta) + the page images, then empty every
/// leftover page (the old pending region) so no stale pending survives — mirrors `page::rewrite_blob`'s in-place
/// reinit strategy, avoiding unbounded relation growth across vacuums.
pub(crate) unsafe fn rewrite_structured(rel: pg_sys::Relation, packed: &Packed) {
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(rel, pg_sys::ForkNumber::MAIN_FORKNUM);
    // block 0 = meta, then the page images; reinit existing blocks in place, extend for any new ones.
    // NB: closures do not inherit the enclosing `unsafe fn` context, so the FFI calls are wrapped explicitly.
    let write_block = |block: u32, items: &[Vec<u8>]| unsafe {
        if block < nblocks {
            page::reinit_page_with_items(rel, block, items);
        } else {
            page::extend_page_with_items(rel, pg_sys::ForkNumber::MAIN_FORKNUM, items);
        }
    };
    write_block(0, std::slice::from_ref(&packed.meta));
    for (i, pg) in packed.pages.iter().enumerate() {
        write_block(1 + i as u32, pg);
    }
    let total = 1 + packed.pages.len() as u32;
    for b in total..nblocks {
        page::reinit_page_with_items(rel, b, &[]); // empty the old pending pages
    }
}

/// A traversal candidate: its element address, neighbor-tuple address, level, heap tid, and distance to the query.
#[derive(Clone, Copy)]
struct Cand {
    d: f64,
    blk: u32,
    off: u16,
    nbr_blk: u32,
    nbr_off: u16,
    level: u8,
    tid: i64,
}
impl PartialEq for Cand {
    fn eq(&self, o: &Self) -> bool {
        self.d == o.d
    }
}
impl Eq for Cand {}
impl PartialOrd for Cand {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Cand {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.d.partial_cmp(&o.d).unwrap_or(std::cmp::Ordering::Equal)
    }
}

fn score(metric: Metric, q: &[f32], vec_bytes: &[u8], is_l2: bool) -> f64 {
    if is_l2 {
        crate::vec::l2_dist_from_bytes(q, vec_bytes)
    } else {
        let dim = vec_bytes.len() / 4;
        let mut v = vec![0f32; dim];
        for (i, s) in v.iter_mut().enumerate() {
            *s = f32::from_le_bytes(vec_bytes[i * 4..i * 4 + 4].try_into().unwrap());
        }
        metric.dist(q, &v)
    }
}

/// Load an element at `(blk,off)`, score it, and return a candidate. Increments the pages-read counter.
/// M41: decodes + scores the vector INSIDE the pinned page scope (`with_page_item`) — no `to_vec` alloc/memcpy.
/// `nblocks` is cached by the caller (traverse) so this does not re-read `RelationGetNumberOfBlocksInFork`.
unsafe fn load(
    rel: pg_sys::Relation,
    blk: u32,
    off: u16,
    q: &[f32],
    metric: Metric,
    is_l2: bool,
    nblocks: u32,
    reads: &mut usize,
) -> Result<Cand, String> {
    *reads += 1;
    page::with_page_item(rel, blk, off, nblocks, |b| {
        let ev = decode_element(b)?;
        Ok(Cand {
            d: score(metric, q, ev.vec_bytes, is_l2),
            blk,
            off,
            nbr_blk: ev.nbr_addr.0,
            nbr_off: ev.nbr_addr.1,
            level: ev.level,
            tid: ev.tid,
        })
    })
}

/// Read a candidate's neighbor addresses on `layer` (increments pages-read for the neighbor tuple).
/// M41: decodes the addrs INSIDE the pinned page scope — no `to_vec` of the neighbor tuple. `nblocks` cached.
unsafe fn neighbors_of(
    rel: pg_sys::Relation,
    c: &Cand,
    layer: usize,
    m: usize,
    m0: usize,
    nblocks: u32,
    reads: &mut usize,
) -> Result<Vec<Addr>, String> {
    *reads += 1;
    page::with_page_item(rel, c.nbr_blk, c.nbr_off, nblocks, |b| {
        decode_neighbors(b, c.level as usize, layer, m, m0)
    })
}

/// M46 L1-B: like `neighbors_of` but decodes into a caller-owned scratch `Vec` (cleared first) instead of
/// allocating a fresh one. The ground-layer loop reuses ONE scratch across every expanded node.
#[allow(clippy::too_many_arguments)]
unsafe fn neighbors_into(
    rel: pg_sys::Relation,
    c: &Cand,
    layer: usize,
    m: usize,
    m0: usize,
    nblocks: u32,
    reads: &mut usize,
    out: &mut Vec<Addr>,
) -> Result<(), String> {
    *reads += 1;
    page::with_page_item(rel, c.nbr_blk, c.nbr_off, nblocks, |b| {
        decode_neighbors_into(b, c.level as usize, layer, m, m0, out)
    })
}

/// On-demand top-`ef` traversal (mirrors pgvector `HnswSearchLayer`): greedy-descend the upper layers with ef=1,
/// then the ground layer with `ef_search`, reading a node's element/neighbor tuple ONLY when it is visited.
/// Returns `(tid, dist)` ascending. Reads ≈ 1(meta) + O(entry_level) + O(ef·M) pages — flat in N.
pub(crate) unsafe fn traverse(
    rel: pg_sys::Relation,
    meta: &HnswMeta,
    q: &[f32],
    ef_search: usize,
) -> Result<Vec<(i64, f64)>, String> {
    if meta.entry_level < 0 || meta.node_count == 0 {
        return Ok(Vec::new());
    }
    let metric = Metric::from_tag(meta.metric_tag).ok_or("theodb hnsw: unknown metric tag")?;
    let is_l2 = matches!(metric, Metric::L2);
    let (m, m0) = (meta.m as usize, meta.m0 as usize);
    let ef = ef_search.max(1);
    let mut reads = 0usize;
    // M41: cache the block count once (was read per-item inside read_page_item_at — a syscall-ish call ×2/node).
    let nblocks = page::main_fork_nblocks(rel);

    // Entry point (from meta), then greedy-descend the upper layers keeping a single best candidate.
    let mut ep = load(rel, meta.entry_blkno, meta.entry_offno, q, metric, is_l2, nblocks, &mut reads)?;
    let mut lc = meta.entry_level as usize;
    while lc >= 1 {
        loop {
            let nbrs = neighbors_of(rel, &ep, lc, m, m0, nblocks, &mut reads)?;
            let mut improved = false;
            for (nb, no) in nbrs {
                let cand = load(rel, nb, no, q, metric, is_l2, nblocks, &mut reads)?;
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

    // Ground layer with ef_search: a min-heap of candidates to expand, a max-heap of the ef best found.
    // M46 L1-A: pre-size the three per-query structures so they never rehash/realloc mid-search — the
    // accidental overhead that scaled super-linearly with ef (default SipHash `HashSet` starts at capacity 0
    // and rehashes ~12× over an ef=200 search). Anchors: pgvector `tidhash_create(ef*m*2)` (`hnswutils.c:675`),
    // pgvectorscale `with_capacity(search_list_size*neigbors)` (`graph/mod.rs:109-111`). Recall-neutral.
    let cap = ef.saturating_mul(m0.max(1)).max(1);
    let mut visited: std::collections::HashSet<(u32, u16)> =
        std::collections::HashSet::with_capacity(cap.saturating_mul(2));
    let mut cands: std::collections::BinaryHeap<std::cmp::Reverse<Cand>> =
        std::collections::BinaryHeap::with_capacity(cap);
    let mut result: std::collections::BinaryHeap<Cand> =
        std::collections::BinaryHeap::with_capacity(ef + 1);
    // M46 L1-B: one neighbor scratch reused across every expanded node (was a fresh `Vec` per node).
    let mut scratch: Vec<Addr> = Vec::with_capacity(m0.max(1));
    visited.insert((ep.blk, ep.off));
    cands.push(std::cmp::Reverse(ep));
    result.push(ep);
    while let Some(std::cmp::Reverse(c)) = cands.pop() {
        let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
        if c.d > worst && result.len() >= ef {
            break;
        }
        neighbors_into(rel, &c, 0, m, m0, nblocks, &mut reads, &mut scratch)?;
        for i in 0..scratch.len() {
            let (nb, no) = scratch[i];
            if !visited.insert((nb, no)) {
                continue;
            }
            let cand = load(rel, nb, no, q, metric, is_l2, nblocks, &mut reads)?;
            let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
            if cand.d < worst || result.len() < ef {
                cands.push(std::cmp::Reverse(cand));
                result.push(cand);
                if result.len() > ef {
                    result.pop();
                }
            }
        }
    }

    if std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1") {
        // The wiring-triad runtime metric: pages read must be O(ef·M), flat in N (server LOG, not client WARNING).
        pgrx::log!("theodb hnsw scan profile: pages_read={reads} ef={ef} m={m} m0={m0} results={}", result.len());
    }
    let mut out: Vec<(i64, f64)> = result.into_iter().map(|c| (c.tid, c.d)).collect();
    out.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
    Ok(out)
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use crate::ann::HnswIndex;

    fn corpus() -> Vec<(i64, Vec<f32>)> {
        (0..40)
            .map(|i| (i as i64 + 100, vec![(i % 7) as f32, (i % 5) as f32, (i % 3) as f32, i as f32 / 40.0]))
            .collect()
    }

    /// Map an element `(blkno,offno)` back to its node index (inverse of the analytic addr) — test helper.
    fn node_of_elem_addr(addr: Addr, ipp: usize) -> usize {
        (addr.0 as usize - 1) * ipp + (addr.1 as usize - 1)
    }

    #[pgrx::pg_test]
    fn pack_element_addresses_are_analytic() {
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 42);
        let packed = pack(&idx).expect("pack");
        let dim = idx.dim();
        let ipp = elems_per_page(dim);
        // Element pages are the first `elem_npages`; each element decodes and its addr maps back to its node.
        let meta = decode_meta(&packed.meta).unwrap();
        for p in 0..meta.elem_npages as usize {
            for (off, blob) in packed.pages[p].iter().enumerate() {
                let addr = ((1 + p) as u32, (off + 1) as u16);
                let node = node_of_elem_addr(addr, ipp);
                let ev = decode_element(blob).unwrap();
                assert_eq!(ev.tid, idx.node_id(node), "element tid must match its node");
                assert_eq!(ev.level as usize, idx.node_level(node));
            }
        }
    }

    #[pgrx::pg_test]
    fn neighbor_slice_matches_in_memory_graph_every_layer() {
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 7);
        let (_metric, m, m0, _ef) = idx.params();
        let packed = pack(&idx).expect("pack");
        let meta = decode_meta(&packed.meta).unwrap();
        let dim = idx.dim();
        let ipp = elems_per_page(dim);
        let n = idx.node_count();
        // For every node, decode each layer's neighbor addrs, map back to node indices, and assert the SET equals
        // the in-memory `neighbors[node][lc]` — the load-bearing correctness check (blueprint R2).
        for node in 0..n {
            let level = idx.node_level(node);
            // find this node's neighbor tuple via its element's nbr_addr
            let ea = ((1 + node / ipp) as u32, (1 + node % ipp) as u16);
            let ep = packed.pages[ea.0 as usize - 1][ea.1 as usize - 1].as_slice();
            let ev = decode_element(ep).unwrap();
            let (nb_blk, nb_off) = ev.nbr_addr;
            // neighbor pages start at nbr_first
            assert!(nb_blk >= meta.nbr_first, "nbr addr must be in the neighbor range");
            let np = packed.pages[nb_blk as usize - 1][nb_off as usize - 1].as_slice();
            for lc in 0..=level {
                let got: std::collections::HashSet<usize> = decode_neighbors(np, level, lc, m, m0)
                    .unwrap()
                    .into_iter()
                    .map(|a| node_of_elem_addr(a, ipp))
                    .collect();
                let want: std::collections::HashSet<usize> =
                    idx.node_neighbors(node, lc).iter().copied().collect();
                assert_eq!(got, want, "node {node} layer {lc}: decoded neighbors must equal in-memory");
            }
        }
    }

    #[pgrx::pg_test]
    fn empty_graph_packs_to_meta_only_with_sentinel_entry() {
        let idx = HnswIndex::build(&[], 16, 64, Metric::Cosine, 1);
        let packed = pack(&idx).expect("pack empty");
        assert!(packed.pages.is_empty());
        let meta = decode_meta(&packed.meta).unwrap();
        assert_eq!(meta.entry_level, -1);
        assert_eq!(meta.node_count, 0);
    }

    /// M46 L1-B: the reused-scratch variant `decode_neighbors_into` must produce EXACTLY the same addrs as the
    /// allocating `decode_neighbors`, AND must clear any prior contents of the scratch (the scratch-not-cleared
    /// bug that would leak a previous node's neighbors into the next — EC-1 of the edge-case review).
    #[pgrx::pg_test]
    fn decode_neighbors_into_matches_original() {
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 11);
        let (_metric, m, m0, _ef) = idx.params();
        let packed = pack(&idx).expect("pack");
        let dim = idx.dim();
        let ipp = elems_per_page(dim);
        for node in 0..idx.node_count() {
            let level = idx.node_level(node);
            let ea = ((1 + node / ipp) as u32, (1 + node % ipp) as u16);
            let ep = packed.pages[ea.0 as usize - 1][ea.1 as usize - 1].as_slice();
            let ev = decode_element(ep).unwrap();
            let (nb_blk, nb_off) = ev.nbr_addr;
            let np = packed.pages[nb_blk as usize - 1][nb_off as usize - 1].as_slice();
            for lc in 0..=level {
                let orig = decode_neighbors(np, level, lc, m, m0).unwrap();
                // pre-dirty the scratch to prove `_into` clears it before writing.
                let mut scratch: Vec<Addr> = vec![(9999, 9999), (8888, 8888)];
                decode_neighbors_into(np, level, lc, m, m0, &mut scratch).unwrap();
                assert_eq!(scratch, orig, "node {node} layer {lc}: _into must equal original AND clear prior");
            }
        }
    }

    #[pgrx::pg_test]
    fn decode_meta_rejects_bad_magic_and_truncation() {
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 3);
        let good = pack(&idx).unwrap().meta;
        assert!(decode_meta(&good[..good.len() - 1]).is_err(), "truncated meta must Err");
        let mut bad = good.clone();
        bad[0] ^= 0xFF;
        assert!(decode_meta(&bad).is_err(), "bad magic must Err");
    }

    /// Collect the id column of `SELECT id FROM rn ORDER BY e <-> q LIMIT k` under the current planner GUCs,
    /// via a real Spi round-trip — the only way to exercise the on-disk `traverse` (it needs a live Relation).
    #[cfg(any(test, feature = "pg_test"))]
    fn topk_ids(query_lit: &str, k: i64) -> Vec<i32> {
        let sql = format!("SELECT id FROM rn ORDER BY e <-> '{query_lit}'::vector LIMIT {k}");
        pgrx::Spi::connect(|client| {
            client
                .select(&sql, None, None)
                .unwrap()
                .filter_map(|row| row.get::<i32>(1).unwrap())
                .collect::<Vec<i32>>()
        })
    }

    /// M46 L1-A recall-neutrality, proven END-TO-END through the real `traverse` (index scan) against an
    /// INDEPENDENT oracle (the exact seqscan) — NOT a golden vector snapshotted from the already-mutated tree
    /// (which would be circular; EC-2 + SEPA initial-brief). The pre-size (`with_capacity`) is a std-guaranteed
    /// capacity hint that cannot alter visit order; this test is the load-bearing regression guard proving it.
    /// On a tiny distinct corpus at ef_search=200, HNSW recall is 100%, so the index top-k set MUST equal the
    /// exact top-k set, and repeated index runs MUST be byte-identical (determinism).
    #[pgrx::pg_test]
    fn traverse_presize_is_recall_neutral_end_to_end() {
        pgrx::Spi::run("CREATE TEMP TABLE rn (id int PRIMARY KEY, e vector(4))").unwrap();
        // 30 deterministic, distinct points — no distance ties near the probe → unambiguous exact NN.
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO rn VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX rn_idx ON rn USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();

        let probe = "[3.3,1.1,2.2,0.4]";
        // Exact oracle: force a seqscan (bypass the AM entirely).
        pgrx::Spi::run("SET enable_indexscan = off; SET enable_bitmapscan = off; SET enable_seqscan = on")
            .unwrap();
        let exact = topk_ids(probe, 5);
        // Index path: force the theodb_hnsw index scan → exercises `traverse` with the pre-sized structures.
        pgrx::Spi::run("SET enable_seqscan = off; SET enable_bitmapscan = off; SET enable_indexscan = on")
            .unwrap();
        let via_index_1 = topk_ids(probe, 5);
        let via_index_2 = topk_ids(probe, 5);

        assert_eq!(via_index_1, via_index_2, "traverse must be deterministic (pre-size adds no nondeterminism)");
        let (mut si, mut se) = (via_index_1.clone(), exact.clone());
        si.sort_unstable();
        se.sort_unstable();
        assert_eq!(si, se, "recall-neutral: index top-5 set must equal exact top-5 set (100% recall at ef=200)");
    }

    /// Negative case (testing.md §4.1): `ef_search = 0` is rejected at the GUC boundary (MIN_EF_SEARCH=1) with a
    /// typed error — it can never reach `traverse`, so the internal `ef_search.max(1)` clamp is defense-in-depth.
    /// This fail-fast-at-the-boundary is the honest form of the plan's "ef=0 → clamp, no crash" acceptance.
    #[pgrx::pg_test(error = "outside the valid range")]
    fn ef_search_zero_rejected_at_guc_boundary() {
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 0").unwrap();
    }
}
