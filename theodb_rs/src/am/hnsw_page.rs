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
// M56: bytes 2..4 were pad (kept the i64 tid 4-aligned) — always zero in v1/v2. Byte 2 = tombstone flag,
// byte 3 = version. Both fit WITHOUT changing the tuple size or any analytic address (elem_size/pack_at
// unchanged). A v1/v2 index reads pad=0 → deleted=0/version=0 (live) — backward-compatible, REINDEX optional.
// Mirrors pgvector's HnswElementTupleData `uint8 deleted; uint8 version` (hnsw.h:361-362).
const E_DELETED: usize = 2;
const E_VERSION: usize = 3;
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
/// Element tuple size: header + f32 vector + optional trailing SBQ code (`code_len` bytes; 0 ⇒ v1 f32-only).
fn elem_size(dim: usize, code_len: usize) -> usize {
    ELEM_HEADER + dim * 4 + code_len
}
fn elems_per_page(dim: usize, code_len: usize) -> usize {
    (USABLE / (ITEMID + maxalign(elem_size(dim, code_len)))).max(1)
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
    /// M51 layout v2: SBQ bits-per-dim (0 = f32-only / legacy v1). > 0 ⇒ `codebook` carries the persisted
    /// quantizer (`SbqQuantizer::to_meta_bytes`) so the scan reproduces the build-time quantization.
    pub(crate) sbq_bits: u8,
    pub(crate) codebook: Vec<u8>,
}

impl HnswMeta {
    /// First block of the pending region (the structured index occupies blocks `0 ..= nbr_first+nbr_npages-1`).
    pub(crate) fn pending_start(&self) -> u32 {
        self.nbr_first + self.nbr_npages
    }
}

const META_LEN: usize = 4 + 4 + 1 + 4 + 2 + 2 + 4 + 2 + 2 + 4 + 4 + 4 + 4 + 4; // = 45 bytes (v1 core)
const HNSW_STRUCT_VERSION_SBQ: u32 = 2; // M51 layout v2: same core header + trailing [sbq_bits:u8][cb_len:u32][codebook]

/// Encode the meta item. `sbq_bits == 0` ⇒ emit the byte-identical **v1** layout (legacy indexes + f32-only
/// builds are unchanged; existing tests stay green). `sbq_bits > 0` ⇒ emit **v2** = the same 45-byte core with
/// `HNSW_STRUCT_VERSION_SBQ` in the version slot, then `[sbq_bits:u8][codebook_len:u32 LE][codebook bytes]`.
fn encode_meta(m: &HnswMeta) -> Vec<u8> {
    let version = if m.sbq_bits == 0 { HNSW_STRUCT_VERSION } else { HNSW_STRUCT_VERSION_SBQ };
    let mut b = Vec::with_capacity(META_LEN + if m.sbq_bits == 0 { 0 } else { 5 + m.codebook.len() });
    b.extend_from_slice(&HNSW_STRUCT_MAGIC.to_le_bytes());
    b.extend_from_slice(&version.to_le_bytes());
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
    if m.sbq_bits != 0 {
        b.push(m.sbq_bits);
        b.extend_from_slice(&(m.codebook.len() as u32).to_le_bytes());
        b.extend_from_slice(&m.codebook);
    }
    b
}

/// Parse the meta item. Fail-fast typed `Err` on truncation / bad magic / unknown version — never panic.
/// Handles both v1 (legacy, no SBQ) and v2 (M51, trailing codebook); v1 indexes stay readable.
pub(crate) fn decode_meta(b: &[u8]) -> Result<HnswMeta, String> {
    if b.len() < META_LEN {
        return Err("theodb hnsw: truncated meta page".into());
    }
    let magic = u32::from_le_bytes(b[0..4].try_into().unwrap());
    if magic != HNSW_STRUCT_MAGIC {
        return Err("theodb hnsw: bad structured meta magic (REINDEX to upgrade the M26 blob to the M35 \
                    page-native format)".into());
    }
    let version = u32::from_le_bytes(b[4..8].try_into().unwrap());
    if version != HNSW_STRUCT_VERSION && version != HNSW_STRUCT_VERSION_SBQ {
        return Err(format!(
            "theodb hnsw: unsupported structured meta version v{version} — REINDEX with a compatible theodb build"
        ));
    }
    let u16a = |o: usize| u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
    let u32a = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    // v2 trailer: [sbq_bits:u8][codebook_len:u32][codebook]. Validate exact length (Rule 8) before slicing.
    let (sbq_bits, codebook) = if version == HNSW_STRUCT_VERSION_SBQ {
        if b.len() < META_LEN + 5 {
            return Err("theodb hnsw: truncated v2 SBQ trailer".into());
        }
        let bits = b[META_LEN];
        let cb_len = u32::from_le_bytes(b[META_LEN + 1..META_LEN + 5].try_into().unwrap()) as usize;
        if b.len() != META_LEN + 5 + cb_len {
            return Err(format!(
                "theodb hnsw: v2 codebook length mismatch (declared {cb_len}, have {})",
                b.len() - META_LEN - 5
            ));
        }
        (bits, b[META_LEN + 5..].to_vec())
    } else {
        (0u8, Vec::new())
    };
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
        sbq_bits,
        codebook,
    })
}

/// A decoded element tuple: its level, heap tid, neighbor-tuple address, and the raw vector byte slice (scored
/// directly via the M31b SIMD `l2_dist_from_bytes` — no per-node `Vec<f32>` alloc).
pub(crate) struct ElementView<'a> {
    pub(crate) level: u8,
    pub(crate) tid: i64,
    pub(crate) nbr_addr: Addr,
    pub(crate) vec_bytes: &'a [u8],
    /// M51 layout v2: the trailing SBQ code bytes (empty for v1 f32-only tuples). Scored by `sbq::hamming`.
    pub(crate) code_bytes: &'a [u8],
    /// M56: tombstone flag. A `deleted` node is still navigated THROUGH by the scan (its arcs preserve graph
    /// connectivity — live nodes still point at it) but is NEVER emitted to the result set. Set in-place by
    /// `ambulkdelete`; dropped by the next compaction (`fold`). `version` bumps on delete (forward-compat hook
    /// for a phase-2 slot-reuse insert; NOT load-bearing in phase 1 — the M52 iterative scan dedups by tid).
    pub(crate) deleted: bool,
    #[allow(dead_code)] // phase-2 slot-reuse hook: written on delete, not yet consumed in phase 1
    pub(crate) version: u8,
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
        // v1: nothing after the vec (empty). v2: the SBQ code occupies `[end..]` (its length = the item's
        // trailing bytes; the caller knows `bytes_per_vector(dim, meta.sbq_bits)` to validate if needed).
        code_bytes: &b[end..],
        deleted: b[E_DELETED] != 0,
        version: b[E_VERSION],
    })
}

/// M56: mark an element tuple's bytes as a tombstone IN PLACE (byte 2 = deleted flag, byte 3 = version bump).
/// The tuple size is unchanged, so this is a pure byte-write on the fixed-offset item — safe to do on a pinned
/// buffer under a single `GenericXLog` (no `PageIndexTupleOverwrite`). Idempotent: re-marking a tombstone is a
/// no-op on the flag and does not double-bump. Returns true iff this call flipped a live tuple to deleted.
pub(crate) fn mark_tombstone_in_place(b: &mut [u8]) -> bool {
    if b.len() < ELEM_HEADER || b[E_TAG] != ELEM_TAG || b[E_DELETED] != 0 {
        return false;
    }
    b[E_DELETED] = 1;
    b[E_VERSION] = b[E_VERSION].wrapping_add(1);
    true
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

/// Encode an element tuple. `code` is the node's SBQ code (empty ⇒ v1 f32-only, byte-identical to before);
/// when non-empty it is written immediately after the f32 vector (layout v2).
fn encode_element(idx: &HnswIndex, node: usize, nbr_addr: Addr, dim: usize, code: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; elem_size(dim, code.len())];
    b[E_TAG] = ELEM_TAG;
    b[E_LEVEL] = idx.node_level(node) as u8;
    b[E_TID..E_TID + 8].copy_from_slice(&idx.node_id(node).to_le_bytes());
    b[E_NBR_BLK..E_NBR_BLK + 4].copy_from_slice(&nbr_addr.0.to_le_bytes());
    b[E_NBR_OFF..E_NBR_OFF + 2].copy_from_slice(&nbr_addr.1.to_le_bytes());
    b[E_DIM..E_DIM + 2].copy_from_slice(&(dim as u16).to_le_bytes());
    for (j, &f) in idx.node_vector(node).iter().enumerate() {
        b[E_VEC + j * 4..E_VEC + j * 4 + 4].copy_from_slice(&f.to_le_bytes());
    }
    if !code.is_empty() {
        b[E_VEC + dim * 4..].copy_from_slice(code);
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
    // The initial build / buildempty writes a contiguous generation starting right after the meta (block 1).
    pack_at(idx, 1, 0)
}

/// Like [`pack`] but emits **layout v2**: trains an SBQ quantizer from the graph's vectors, persists the codebook
/// in the meta, and writes each node's compact SBQ code inline after its f32 vector (M51 T1.1/T2.1). `sbq_bits==0`
/// is identical to [`pack`].
pub(crate) fn pack_sbq(idx: &HnswIndex, sbq_bits: u8) -> Result<Packed, String> {
    pack_at(idx, 1, sbq_bits)
}

/// Like [`pack`], but places the generation body starting at block `base` (M48 / issue #47). The meta's element
/// and neighbor pointers (`elem_first`/`nbr_first`/`entry_blkno`) plus every neighbor-tuple address are resolved
/// relative to `base`, so the packed image is position-independent — the crash-safe fold writes it at the tail
/// (or a reclaimed contiguous region) and pivots block 0 to it. Readers already follow the meta pointers, so no
/// read path changes: the graph is relocatable for free (unlike IVF, whose directory needed an explicit gen_base).
pub(crate) fn pack_at(idx: &HnswIndex, base: usize, sbq_bits: u8) -> Result<Packed, String> {
    let (metric, m, m0, _ef) = idx.params();
    let n = idx.node_count();
    let dim = idx.dim();

    // Empty graph: meta only, entry_level = -1. `base` is irrelevant (no body pages) — record it anyway so
    // pending_start (= nbr_first + nbr_npages = base) is consistent with a non-empty generation at `base`.
    // An empty index has no vectors to train the quantizer on, so it stays v1 (SBQ arrives on the first fold
    // after data lands — REINDEX/VACUUM).
    if n == 0 {
        let meta = encode_meta(&HnswMeta {
            metric_tag: metric.tag(), dim: dim as u32, m: m as u16, m0: m0 as u16,
            entry_blkno: 0, entry_offno: 0, entry_level: -1, node_count: 0,
            elem_first: base as u32, elem_npages: 0, nbr_first: base as u32, nbr_npages: 0,
            sbq_bits: 0, codebook: Vec::new(),
        });
        return Ok(Packed { meta, pages: Vec::new() });
    }

    // M51 T1.1/T2.1: when SBQ is enabled, train the quantizer from the graph's vectors, emit one compact code per
    // node (packed u64 words → LE bytes) and the codebook for the meta. `code_len == 0` ⇒ the v1 f32-only path.
    let (code_len, codes, codebook) = if sbq_bits > 0 {
        let vecs: Vec<Vec<f32>> = (0..n).map(|i| idx.node_vector(i).to_vec()).collect();
        let q = crate::sbq::SbqQuantizer::train(&vecs, sbq_bits);
        let codes: Vec<Vec<u8>> = vecs
            .iter()
            .map(|v| q.quantize(v).iter().flat_map(|w| w.to_le_bytes()).collect())
            .collect();
        (crate::sbq::SbqQuantizer::bytes_per_vector(dim, sbq_bits), codes, q.to_meta_bytes())
    } else {
        (0usize, Vec::new(), Vec::new())
    };

    // 1. Analytic element addresses (fixed size ⇒ node i is at block base+i/ipp, offset 1+i%ipp).
    let ipp = elems_per_page(dim, code_len);
    let elem_npages = n.div_ceil(ipp);
    let elem_addr: Vec<Addr> =
        (0..n).map(|i| ((base + i / ipp) as u32, (1 + i % ipp) as u16)).collect();
    let nbr_first = base + elem_npages;

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
        elem_pages.push(
            chunk
                .iter()
                .map(|&node| {
                    let code: &[u8] = if code_len > 0 { &codes[node] } else { &[] };
                    encode_element(idx, node, nbr_addr[node], dim, code)
                })
                .collect(),
        );
    }

    // 4. Meta with the entry point resolved to its element addr.
    let entry_node = idx.entry().ok_or("theodb hnsw: non-empty graph without an entry point")?;
    let (eb, eo) = elem_addr[entry_node];
    let meta = encode_meta(&HnswMeta {
        metric_tag: metric.tag(), dim: dim as u32, m: m as u16, m0: m0 as u16,
        entry_blkno: eb, entry_offno: eo, entry_level: idx.node_level(entry_node) as i16,
        node_count: n as u32, elem_first: base as u32, elem_npages: elem_npages as u32,
        nbr_first: nbr_first as u32, nbr_npages: nbr_npages as u32,
        sbq_bits: if code_len > 0 { sbq_bits } else { 0 }, codebook,
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
            // M56: compaction reuses this enumerate → the rebuild drops tombstoned nodes here (they are gone
            // from the fresh graph, reclaiming their space). This is the ONLY reclaim path in phase 1 (slot
            // reuse on INSERT is phase 2 — it would mutate the immutable M35 graph).
            if ev.deleted {
                continue;
            }
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
            if let Ok(ev) = decode_element(&bytes) {
                if ev.deleted && ev.level as usize >= need_level {
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
        if item.len() < ELEM_HEADER || item[E_TAG] != ELEM_TAG {
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

    let mut ep = load(rel, meta.entry_blkno, meta.entry_offno, q, metric, is_l2, qcode, nblocks, &mut reads)?;
    let mut lc = meta.entry_level as usize;
    while lc >= 1 {
        loop {
            let nbrs = neighbors_of(rel, &ep, lc, m, m0, nblocks, &mut reads)?;
            let mut improved = false;
            for (nb, no) in nbrs {
                let cand = load(rel, nb, no, q, metric, is_l2, qcode, nblocks, &mut reads)?;
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
        m,
        m0,
        reads: std::cell::Cell::new(reads),
    };
    let nodes = crate::ann::scan_core::ground_search_nodes(&pg_src, ep, ef, m0, true)?;
    let mut cands: Vec<(f64, Addr)> = nodes.iter().map(|(c, _)| (c.d, (c.blk, c.off))).collect();
    cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(cands.into_iter().take(m0).map(|(_, a)| a).collect())
}

// (M48) The old in-place `rewrite_structured` was replaced by the crash-safe `fold::fold` (meta-pivot) — it
// rewrote block 0 first, so a crash mid-vacuum left the meta pointing at pages that still held old bytes (#47).

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
    /// M56: a tombstoned node is navigated THROUGH (its arcs preserve connectivity — it enters the candidate
    /// heap and is expanded) but is NEVER pushed to the result set. Set from `ElementView.deleted`.
    deleted: bool,
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

// M49: 3-way fused dispatch — L2/IP/cosine all score from raw page bytes with ZERO per-node `Vec<f32>` alloc
// (was: only L2 fused; cosine/ip decoded a Vec per visited node — the ROADMAP-flagged mine). `_is_l2` is kept
// for call-site signature stability (the metric already carries the same information).
fn score(metric: Metric, q: &[f32], vec_bytes: &[u8], _is_l2: bool) -> f64 {
    match metric {
        Metric::L2 => crate::vec::l2_dist_from_bytes(q, vec_bytes),
        Metric::Ip => crate::vec::ip_dist_from_bytes(q, vec_bytes),
        Metric::Cosine => crate::vec::cosine_dist_from_bytes(q, vec_bytes),
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
    qcode: Option<&[u8]>,
    nblocks: u32,
    reads: &mut usize,
) -> Result<Cand, String> {
    *reads += 1;
    page::with_page_item(rel, blk, off, nblocks, |b| {
        let ev = decode_element(b)?;
        // M51: when a quantized query code is present (SBQ index), guide the walk by the cheap Hamming distance
        // on the inline codes; the f32 vector is NOT touched here (that is the SBQ scan-cost saving). The exact
        // f32 rerank of the survivors happens once, in `traverse`, after the walk. Rule 8: the element code MUST
        // be exactly the query code's length (both = bytes_per_vector(dim, sbq_bits)); a truncated on-disk code
        // (corruption / orphan page) is a typed Err, never a silently-wrong Hamming distance.
        let d = match qcode {
            Some(qc) => {
                if ev.code_bytes.len() != qc.len() {
                    return Err(format!(
                        "theodb hnsw: element SBQ code is {} bytes, expected {} — REINDEX (v2 corruption)",
                        ev.code_bytes.len(),
                        qc.len()
                    ));
                }
                crate::sbq::hamming_bytes(qc, ev.code_bytes) as f64
            }
            None => score(metric, q, ev.vec_bytes, is_l2),
        };
        Ok(Cand {
            d,
            blk,
            off,
            nbr_blk: ev.nbr_addr.0,
            nbr_off: ev.nbr_addr.1,
            level: ev.level,
            tid: ev.tid,
            deleted: ev.deleted,
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

    // M51: for an SBQ index (v2), reconstruct the quantizer from the persisted codebook and quantize the query
    // once. The walk then scores by cheap Hamming on the inline codes; the exact f32 rerank of the survivors runs
    // once after the ground search. `None` (v1) ⇒ the walk scores by exact f32 — byte-identical to before.
    let qcode_owned: Option<Vec<u8>> = if meta.sbq_bits > 0 {
        let quant = crate::sbq::SbqQuantizer::from_meta_bytes(&meta.codebook)?;
        // Defense-in-depth (F1): the persisted codebook must cover the index dim, else `quantize(q)` (q.len() ==
        // meta.dim, enforced by the scan dim-guard) would index the codebook OOB. A typed Err, never a panic.
        if quant.dim() != meta.dim as usize {
            return Err(format!(
                "theodb hnsw: SBQ codebook dim {} != index dim {} — REINDEX (v2 corruption)",
                quant.dim(),
                meta.dim
            ));
        }
        Some(quant.quantize(q).iter().flat_map(|w| w.to_le_bytes()).collect())
    } else {
        None
    };
    let qcode: Option<&[u8]> = qcode_owned.as_deref();

    // Entry point (from meta), then greedy-descend the upper layers keeping a single best candidate.
    let mut ep = load(rel, meta.entry_blkno, meta.entry_offno, q, metric, is_l2, qcode, nblocks, &mut reads)?;
    let mut lc = meta.entry_level as usize;
    while lc >= 1 {
        loop {
            let nbrs = neighbors_of(rel, &ep, lc, m, m0, nblocks, &mut reads)?;
            let mut improved = false;
            for (nb, no) in nbrs {
                let cand = load(rel, nb, no, q, metric, is_l2, qcode, nblocks, &mut reads)?;
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

    // Ground layer with ef_search — extracted to `ann/scan_core::ground_search` behind a `NeighborSource` seam
    // (FU-1). The M46 pre-size + reused scratch live there now (`presize = true` keeps the M46 behavior);
    // production drives it via `PageNeighborSource`, the criterion bench via `MemNeighborSource`. Recall-neutral:
    // the ground loop reads the same pages in the same order (dedup-before-load preserved). `reads` is threaded
    // through the source's `Cell` so `pages_read` stays exact.
    let pg_src = PageNeighborSource {
        rel,
        nblocks,
        q,
        metric,
        is_l2,
        qcode,
        m,
        m0,
        reads: std::cell::Cell::new(reads),
    };
    let out = if qcode.is_some() {
        // SBQ (M51): the ground walk ranked candidates by Hamming. Widen the candidate pool by `over_fetch`
        // (scan GUC) so the true NN survives the approximate ranking, then rerank the survivors by EXACT f32 —
        // this is where recall is recovered (carrier-limited, M40). Only the surviving `walk_ef` pages are
        // re-read for their f32 vectors; the walk itself paid only the cheap Hamming cost.
        let over_fetch = crate::am::guc::over_fetch().max(1);
        let walk_ef = ef.saturating_mul(over_fetch);
        let nodes = crate::ann::scan_core::ground_search_nodes(&pg_src, ep, walk_ef, m0, true)?;
        reads = pg_src.reads.get();
        let mut reranked: Vec<(i64, f64)> = Vec::with_capacity(nodes.len());
        for (cand, _ham) in &nodes {
            let d = page::with_page_item(rel, cand.blk, cand.off, nblocks, |b| {
                Ok(score(metric, q, decode_element(b)?.vec_bytes, is_l2))
            })?;
            reads += 1;
            reranked.push((cand.tid, d));
        }
        reranked.sort_by(|a, b| {
            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
        });
        reranked.truncate(ef); // return the ef best by exact f32; the scan takes top-k
        reranked
    } else {
        let o = crate::ann::scan_core::ground_search(&pg_src, ep, ef, m0, true)?;
        reads = pg_src.reads.get();
        o
    };

    if std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1") {
        // The wiring-triad runtime metric: pages read must be O(ef·M), flat in N (server LOG, not client WARNING).
        pgrx::log!("theodb hnsw scan profile: pages_read={reads} ef={ef} m={m} m0={m0} results={}", out.len());
    }
    Ok(out)
}

/// The production [`scan_core::NeighborSource`]: drives the ground search over PostgreSQL pages by reusing the
/// existing `load` + `neighbors_into` page readers (FU-1). `Node` is the on-disk `Cand` (distance + tid + the
/// neighbor-tuple address for expansion); `Ref` is a neighbor element address `(blk,off)`. The page-read counter
/// is threaded through a `Cell` (the trait methods take `&self`); it mirrors the pre-FU-1 `&mut reads` exactly.
struct PageNeighborSource<'a> {
    rel: pg_sys::Relation,
    nblocks: u32,
    q: &'a [f32],
    metric: Metric,
    is_l2: bool,
    /// M51: the quantized query code (SBQ index) — `Some` ⇒ the walk scores by Hamming; `None` ⇒ f32.
    qcode: Option<&'a [u8]>,
    m: usize,
    m0: usize,
    reads: std::cell::Cell<usize>,
}

impl<'a> crate::ann::scan_core::NeighborSource for PageNeighborSource<'a> {
    type Node = Cand;
    type Ref = Addr;

    fn dist(&self, node: &Cand) -> f64 {
        node.d
    }
    fn tid(&self, node: &Cand) -> i64 {
        node.tid
    }
    /// M56: a tombstoned node is navigated through (its neighbors still expand) but never emitted.
    fn emittable(&self, node: &Cand) -> bool {
        !node.deleted
    }
    fn node_key(&self, node: &Cand) -> u64 {
        ((node.blk as u64) << 16) | node.off as u64
    }
    fn ref_key(&self, r: &Addr) -> u64 {
        ((r.0 as u64) << 16) | r.1 as u64
    }
    fn neighbors_into(&self, node: &Cand, out: &mut Vec<Addr>) -> Result<(), String> {
        let mut reads = 0usize;
        // bare `neighbors_into(..)` = the free page reader below; `self`-less → no recursion into this method.
        let r = unsafe { neighbors_into(self.rel, node, 0, self.m, self.m0, self.nblocks, &mut reads, out) };
        self.reads.set(self.reads.get() + reads);
        r
    }
    fn load(&self, r: &Addr) -> Result<Cand, String> {
        let mut reads = 0usize;
        let cand = unsafe {
            load(self.rel, r.0, r.1, self.q, self.metric, self.is_l2, self.qcode, self.nblocks, &mut reads)
        };
        self.reads.set(self.reads.get() + reads);
        cand
    }
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
        let ipp = elems_per_page(dim, 0);
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
        let ipp = elems_per_page(dim, 0);
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
        let ipp = elems_per_page(dim, 0);
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

    // --- M51 T1.1: layout v2 meta carries the SBQ codebook; v1 (f32-only) stays byte-identical. ---
    fn meta_fixture(sbq_bits: u8, codebook: Vec<u8>) -> HnswMeta {
        HnswMeta {
            metric_tag: Metric::L2.tag(), dim: 3, m: 16, m0: 32,
            entry_blkno: 1, entry_offno: 1, entry_level: 2, node_count: 5,
            elem_first: 1, elem_npages: 1, nbr_first: 2, nbr_npages: 1, sbq_bits, codebook,
        }
    }

    #[pgrx::pg_test]
    fn hnsw_meta_v2_roundtrips_codebook() {
        let cb = vec![4u8, 3, 0, 0, 0, 1, 2, 3, 4]; // arbitrary codebook bytes (e.g. to_meta_bytes output)
        let bytes = encode_meta(&meta_fixture(4, cb.clone()));
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), HNSW_STRUCT_VERSION_SBQ, "must be v2");
        let d = decode_meta(&bytes).expect("v2 decodes");
        assert_eq!(d.sbq_bits, 4);
        assert_eq!(d.codebook, cb, "codebook roundtrips byte-exact");
        assert_eq!(d.dim, 3);
        assert_eq!(d.node_count, 5);
    }

    #[pgrx::pg_test]
    fn hnsw_meta_v1_byte_identical_when_no_sbq() {
        let bytes = encode_meta(&meta_fixture(0, Vec::new()));
        assert_eq!(bytes.len(), META_LEN, "f32-only meta must be exactly the 45-byte v1 layout");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), HNSW_STRUCT_VERSION, "must stay v1");
        let d = decode_meta(&bytes).unwrap();
        assert_eq!(d.sbq_bits, 0);
        assert!(d.codebook.is_empty());
    }

    #[pgrx::pg_test]
    fn hnsw_meta_v2_rejects_truncated_codebook() {
        let mut bytes = encode_meta(&meta_fixture(1, vec![1, 2, 3, 4]));
        bytes.truncate(bytes.len() - 1); // drop one codebook byte → declared cb_len mismatch
        assert!(decode_meta(&bytes).is_err(), "truncated v2 codebook must Err (Rule 8)");
    }

    #[pgrx::pg_test]
    fn pack_sbq_writes_codebook_and_matching_codes() {
        // T1.1-build + T2.1-write (M51): pack_sbq trains the quantizer, persists the codebook in the v2 meta, and
        // writes each node's inline code == the quantizer's code for that node's vector.
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 9);
        let bits = 2u8;
        let packed = pack_sbq(&idx, bits).expect("pack_sbq");
        let meta = decode_meta(&packed.meta).unwrap();
        assert_eq!(meta.sbq_bits, bits, "meta records SBQ bits");
        assert!(!meta.codebook.is_empty(), "codebook persisted in meta");
        // reconstruct the quantizer from the persisted codebook and verify each element's code matches.
        let q = crate::sbq::SbqQuantizer::from_meta_bytes(&meta.codebook).expect("codebook decodes");
        let dim = idx.dim();
        let ipp = elems_per_page(dim, crate::sbq::SbqQuantizer::bytes_per_vector(dim, bits));
        for node in 0..idx.node_count() {
            let ep = packed.pages[node / ipp][node % ipp].as_slice();
            let ev = decode_element(ep).unwrap();
            let expect: Vec<u8> =
                q.quantize(idx.node_vector(node)).iter().flat_map(|w| w.to_le_bytes()).collect();
            assert_eq!(ev.code_bytes, expect.as_slice(), "node {node}: inline code == quantize(vec)");
        }
    }

    #[pgrx::pg_test]
    fn element_code_length_is_exact_so_load_guard_detects_truncation() {
        // H2 (M51 review, Rule 8): a truncated on-disk SBQ code must be caught, not silently Hamming'd. The guard
        // in `load` compares `ev.code_bytes.len()` to the query code length (both = bytes_per_vector). This proves
        // `decode_element` exposes the EXACT trailing length, so any truncation is observable by that guard.
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 5);
        let dim = idx.dim();
        let full = crate::sbq::SbqQuantizer::bytes_per_vector(dim, 2);
        let code = vec![0xAAu8; full];
        let e = encode_element(&idx, 0, (1, 1), dim, &code);
        assert_eq!(decode_element(&e).unwrap().code_bytes.len(), full, "full code exposes its exact length");
        // a shorter (truncated) code decodes to a SHORTER code_bytes → the load guard (len != qcode.len()) fires.
        let e_short = encode_element(&idx, 0, (1, 1), dim, &code[..full - 1]);
        assert_eq!(decode_element(&e_short).unwrap().code_bytes.len(), full - 1, "truncation is observable");
    }

    #[pgrx::pg_test]
    fn element_tuple_carries_optional_sbq_code() {
        // T2.1 (M51): an element tuple optionally carries the SBQ code after the f32 vec; v1 (no code) stays
        // byte-identical and the f32 vec bytes are untouched when a code is appended.
        let idx = HnswIndex::build(&corpus(), 16, 64, Metric::L2, 7);
        let dim = idx.dim();
        let e1 = encode_element(&idx, 0, (1, 1), dim, &[]);
        assert_eq!(e1.len(), elem_size(dim, 0), "v1 tuple size unchanged");
        let v1 = decode_element(&e1).unwrap();
        assert!(v1.code_bytes.is_empty(), "v1 tuple has no SBQ code");

        let code = vec![0xABu8, 0xCD, 0x01, 0x02];
        let e2 = encode_element(&idx, 0, (1, 1), dim, &code);
        assert_eq!(e2.len(), elem_size(dim, code.len()), "v2 tuple grows by the code length");
        let v2 = decode_element(&e2).unwrap();
        assert_eq!(v2.code_bytes, code.as_slice(), "SBQ code roundtrips inline after the vec");
        assert_eq!(v2.vec_bytes, v1.vec_bytes, "appending a code must not change the f32 vec bytes");
    }

    /// Collect the id column of `SELECT id FROM rn ORDER BY e <-> q LIMIT k` under the current planner GUCs,
    /// via a real Spi round-trip — the only way to exercise the on-disk `traverse` (it needs a live Relation).
    #[cfg(any(test, feature = "pg_test"))]
    fn topk_ids(query_lit: &str, k: i64) -> Vec<i32> {
        topk_ids_tbl("rn", query_lit, k)
    }
    #[cfg(any(test, feature = "pg_test"))]
    fn topk_ids_tbl(tbl: &str, query_lit: &str, k: i64) -> Vec<i32> {
        let sql = format!("SELECT id FROM {tbl} ORDER BY e <-> '{query_lit}'::vector LIMIT {k}");
        pgrx::Spi::connect(|client| {
            // pgrx 0.16.1: `select`'s 3rd arg is `args: &[DatumWithOid]` (a slice) — `&[]`, never `None`
            // (matches hybrid.rs / ann_query.rs). Column ordinals are 1-based → `row.get::<i32>(1)`.
            client
                .select(&sql, None, &[])
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
    #[pgrx::pg_test(error = "0 is outside the valid range for parameter \"theodb_hnsw.ef_search\" (1 .. 1000)")]
    fn ef_search_zero_rejected_at_guc_boundary() {
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 0").unwrap();
    }

    /// M51 reloption connect: `CREATE INDEX ... WITH (sbq_bits=4)` builds a v2 index (SBQ codes inline). Until the
    /// traverse uses the Hamming path (T3.1), the f32 rerank scan MUST still return the exact top-k — proving the
    /// inline codes do not corrupt the vector scoring and the reloption is wired end-to-end.
    #[pgrx::pg_test]
    fn create_index_with_sbq_bits_scans_correctly() {
        pgrx::Spi::run("CREATE TEMP TABLE rs (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO rs VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX rs_idx ON rs USING theodb_hnsw (e) WITH (sbq_bits = 4)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        let probe = "[3.3,1.1,2.2,0.4]";
        pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
        let exact = topk_ids_tbl("rs", probe, 5);
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let mut via_index = topk_ids_tbl("rs", probe, 5);
        assert!(!via_index.is_empty(), "the SBQ-built index must be scannable (reloption wired)");
        let (mut si, mut se) = (via_index.clone(), exact.clone());
        si.sort_unstable();
        se.sort_unstable();
        via_index.sort_unstable();
        assert_eq!(si, se, "SBQ v2 index top-5 must equal exact top-5 (codes present don't corrupt f32 scoring)");
    }

    /// M51 T3.1 recall gate: on a corpus where the Hamming walk does NOT cover everything (walk_ef < node_count),
    /// the cheap-Hamming navigation + exact-f32 rerank still recovers high recall@10 vs the exact oracle. This is
    /// the property M40 predicts (carrier-limited: over_fetch widens the pool so the true NN survives the rerank).
    /// NOTE: `sbq_bits=2` recovers here because the corpus is low-dim (16-d, structured). At high dim (128-d) 2-bit
    /// navigation is too lossy (`docs/benchmarks/m51-sbq-inline.md § 3` measured recall 0.52); the benchmark uses
    /// 8-bit for the ≥0.99 gate. This unit test proves the read-path MECHANISM, not that 2-bit is safe in general.
    #[pgrx::pg_test]
    fn sbq_traverse_hamming_then_rerank_recall_high() {
        pgrx::Spi::run("CREATE TEMP TABLE rq (id int PRIMARY KEY, e vector(16))").unwrap();
        for i in 0..400i32 {
            // deterministic, well-spread distinct points (id-dominated with a per-dim ripple → clear NN structure)
            let v: Vec<String> = (0..16)
                .map(|j| format!("{:.3}", i as f32 * 0.5 + ((i * 7 + j * 13) % 29) as f32 * 0.3))
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO rq VALUES ({i}, '[{}]')", v.join(","))).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX rq_idx ON rq USING theodb_hnsw (e) WITH (sbq_bits = 2)").unwrap();
        // walk_ef = ef_search * over_fetch = 50 * 6 = 300 < 400 → navigation + rerank genuinely tested.
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 50; SET theodb_hnsw.over_fetch = 6").unwrap();
        let probe = "[40,41,42,40,41,42,40,41,42,40,41,42,40,41,42,40]";
        pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
        let exact = topk_ids_tbl("rq", probe, 10);
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let via_index = topk_ids_tbl("rq", probe, 10);
        let hits = via_index.iter().filter(|id| exact.contains(id)).count();
        let recall = hits as f64 / exact.len().max(1) as f64;
        assert!(
            recall >= 0.9,
            "SBQ Hamming+rerank recall@10 = {recall:.2} (hits {hits}/{}) — expected >= 0.9 (over_fetch=6 recovers it)",
            exact.len()
        );
    }

    #[cfg(any(test, feature = "pg_test"))]
    fn filtered_topk(tbl: &str, filter: &str, q: &str, k: i64) -> Vec<i32> {
        let sql = format!("SELECT id FROM {tbl} WHERE {filter} ORDER BY e <-> '{q}'::vector LIMIT {k}");
        pgrx::Spi::connect(|c| {
            c.select(&sql, None, &[]).unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect::<Vec<i32>>()
        })
    }

    /// M52 recall gate: under a SELECTIVE `WHERE` a naive HNSW (≤ ef_search tuples) would miss the top-k that pass
    /// the filter; the iterative scan grows ef until `max_scan_tuples`, so the index-scan top-k EQUALS the exact
    /// seqscan top-k (recall preserved). The executor applies `cat = 7` as a recheck over the ordered index emit.
    #[pgrx::pg_test]
    fn filtered_scan_preserves_recall_via_iterative() {
        pgrx::Spi::run("CREATE TEMP TABLE ft (id int PRIMARY KEY, cat int, e vector(8))").unwrap();
        for i in 0..500i32 {
            let cat = i % 100; // `WHERE cat = 7` selects ~5 rows (~1% selectivity)
            let v: Vec<String> = (0..8)
                .map(|j| format!("{:.3}", i as f32 * 0.1 + j as f32 + ((i * 7 + j) % 13) as f32 * 0.2))
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO ft VALUES ({i}, {cat}, '[{}]')", v.join(","))).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX ft_idx ON ft USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 40; SET theodb_hnsw.max_scan_tuples = 20000").unwrap();
        let probe = "[20,21,22,23,24,25,26,27]";
        pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
        let exact = filtered_topk("ft", "cat = 7", probe, 3);
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let via_index = filtered_topk("ft", "cat = 7", probe, 3);
        assert!(!exact.is_empty(), "the filtered oracle must return rows (test setup)");
        assert!(!via_index.is_empty(), "the filtered index scan must return results (iterative scan)");
        let hits = via_index.iter().filter(|id| exact.contains(id)).count();
        assert_eq!(
            hits,
            exact.len(),
            "iterative-scan recall under a selective filter must equal exact seqscan ({hits}/{})",
            exact.len()
        );
    }

    /// M52 OFF switch: `max_scan_tuples = 0` disables the iterative scan (pre-M52 behavior) — at most the
    /// ef_search window is emitted; no panic / infinite loop under a selective filter.
    #[pgrx::pg_test]
    fn iterative_scan_off_when_max_scan_tuples_zero() {
        pgrx::Spi::run("CREATE TEMP TABLE fo (id int PRIMARY KEY, cat int, e vector(4))").unwrap();
        for i in 0..300i32 {
            pgrx::Spi::run(&format!(
                "INSERT INTO fo VALUES ({i}, {}, '[{},{},{},{}]')",
                i % 100,
                i as f32 * 0.1,
                (i % 7) as f32,
                (i % 5) as f32,
                i as f32 / 30.0
            ))
            .unwrap();
        }
        pgrx::Spi::run("CREATE INDEX fo_idx ON fo USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 10; SET theodb_hnsw.max_scan_tuples = 0").unwrap();
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let got = filtered_topk("fo", "cat = 3", "[5,1,2,0.5]", 3);
        assert!(got.len() <= 3, "OFF path returns at most k, no infinite loop (got {})", got.len());
    }

    /// M53 item 1 (filter_sql) + item 4 (language): `ai.hybrid_search_rrf` accepts a relational filter (confined
    /// to the CTE WHERE) and a parametrizable FTS language. This SQL-level test proves the filter is APPLIED
    /// (every returned id satisfies `cat = 1`) and the language param is honored (no error with 'simple').
    #[pgrx::pg_test]
    fn hybrid_search_accepts_filter_and_language() {
        pgrx::Spi::run(
            "CREATE TEMP TABLE hy (id int, cat int, tsv tsvector, emb vector(3))",
        )
        .unwrap();
        for i in 0..20i32 {
            let cat = i % 2; // half cat=0, half cat=1
            pgrx::Spi::run(&format!(
                "INSERT INTO hy VALUES ({i}, {cat}, to_tsvector('english', 'alpha beta doc{i}'), '[{},{},{}]')",
                i as f32 * 0.1, (i % 3) as f32, (i % 5) as f32
            ))
            .unwrap();
        }
        // filter_sql => 'cat = 1' : every fused id must be a cat=1 row.
        let ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id::int FROM ai.hybrid_search_rrf(tbl => 'hy', id_col => 'id', \
                 content_tsv_col => 'tsv', vector_col => 'emb', query_text => 'alpha', \
                 query_vector => '[0.5,1,2]'::vector, result_limit => 20, filter_sql => 'cat = 1')",
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert!(!ids.is_empty(), "filtered hybrid search must return the cat=1 rows");
        assert!(ids.iter().all(|id| id % 2 == 1), "every fused id must satisfy filter_sql cat=1, got {ids:?}");
        // language => 'simple' : must not error (item 4 — parametrizable regconfig).
        let n: Option<i64> = pgrx::Spi::get_one(
            "SELECT count(*) FROM ai.hybrid_search_rrf(tbl => 'hy', id_col => 'id', \
             content_tsv_col => 'tsv', vector_col => 'emb', query_text => 'alpha', \
             query_vector => '[0.5,1,2]'::vector, language => 'simple')",
        )
        .unwrap();
        assert!(n.is_some(), "language => 'simple' must run without error");
    }

    /// M53 item 1 negative case (Rule 8): a filter_sql containing a statement terminator is rejected. The
    /// guard is syntactic confinement (blacklist), NOT injection-proofing — a read-only subquery still
    /// composes with the caller's own privileges by design (see the hybrid.rs module docstring).
    #[pgrx::pg_test(error = "ai.hybrid_search_rrf: filter_sql must be a single boolean predicate (no ';', comment, or chaining) — it is raw caller-privilege SQL, never build it from untrusted input")]
    fn hybrid_filter_rejects_statement_terminator() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, tsv tsvector, emb vector(2))").unwrap();
        pgrx::Spi::run("INSERT INTO hz VALUES (1, to_tsvector('a'), '[1,2]')").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'tsv', \
             vector_col => 'emb', query_vector => '[1,2]'::vector, filter_sql => 'true; DROP TABLE hz')",
        )
        .unwrap();
    }

    /// M53 security hardening (council-security F1): the confinement guard also rejects SQL comment
    /// sequences (`--`), so the predicate cannot comment out the closing paren / trailing clauses to break
    /// out of `( ... )`. Defense-in-depth on the SECURITY INVOKER path (does not claim full parse safety).
    #[pgrx::pg_test(error = "ai.hybrid_search_rrf: filter_sql must be a single boolean predicate (no ';', comment, or chaining) — it is raw caller-privilege SQL, never build it from untrusted input")]
    fn hybrid_filter_rejects_sql_comment() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, tsv tsvector, emb vector(2))").unwrap();
        pgrx::Spi::run("INSERT INTO hz VALUES (1, to_tsvector('a'), '[1,2]')").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'tsv', \
             vector_col => 'emb', query_vector => '[1,2]'::vector, filter_sql => 'true) -- ')",
        )
        .unwrap();
    }

    /// M53 item 2 negative case (Rule 8): lexical_engine='bm25' without content_text_col fails fast (typed
    /// 22023) — the BM25 leg operates on a raw TEXT column, not the tsvector, so the column is required.
    #[pgrx::pg_test(error = "ai.hybrid_search_rrf: lexical_engine='bm25' requires content_text_col (the TEXT column indexed USING bm25)")]
    fn hybrid_bm25_without_text_col_errors() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, tsv tsvector, emb vector(2))").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'tsv', \
             vector_col => 'emb', query_vector => '[1,2]'::vector, lexical_engine => 'bm25')",
        )
        .unwrap();
    }

    /// M53 item 2 negative case (Rule 8): an invalid lexical_engine value fails fast (typed 22023) naming the
    /// valid values — NO silent fallback to ts_rank_cd (that would let a caller measure the wrong engine).
    #[pgrx::pg_test(error = "ai.hybrid_search_rrf: lexical_engine must be 'ts_rank_cd' or 'bm25' (got 'okapi')")]
    fn hybrid_invalid_lexical_engine_errors() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, tsv tsvector, emb vector(2))").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'tsv', \
             vector_col => 'emb', query_vector => '[1,2]'::vector, lexical_engine => 'okapi')",
        )
        .unwrap();
    }

    /// M53 item 2 packaging gate: on the shipped image (no pg_textsearch), lexical_engine='bm25' surfaces a
    /// clear 0A000 (feature_not_supported) rather than a cryptic 42883 mid-query. The pgrx test instance has
    /// no pg_textsearch, so this exercises the gate directly (mirrors the embed-seam guard).
    #[pgrx::pg_test(error = "ai.hybrid_search_rrf: lexical_engine='bm25' requires the pg_textsearch extension (CREATE EXTENSION pg_textsearch, shared_preload_libraries=pg_textsearch) — not present on the shipped image; use lexical_engine='ts_rank_cd' (default)")]
    fn hybrid_bm25_without_extension_raises_unsupported() {
        pgrx::Spi::run("CREATE TEMP TABLE hz (id int, body text, emb vector(2))").unwrap();
        pgrx::Spi::run(
            "SELECT * FROM ai.hybrid_search_rrf(tbl => 'hz', id_col => 'id', content_tsv_col => 'body', \
             vector_col => 'emb', query_text => 'x', query_vector => '[1,2]'::vector, \
             lexical_engine => 'bm25', content_text_col => 'body')",
        )
        .unwrap();
    }

    /// M56: decode the heap `ctid` of a row into the i64 the index packs (`(block << 16) | offset`, per
    /// `crate::am::tid::encode`), so a test can name specific on-disk nodes for the tombstone sweep.
    fn heap_tid_i64(tbl: &str, id: i32) -> i64 {
        let txt: String = pgrx::Spi::get_one(&format!("SELECT ctid::text FROM {tbl} WHERE id = {id}"))
            .unwrap()
            .expect("row exists");
        // ctid text form is "(block,offset)".
        let inner = txt.trim_start_matches('(').trim_end_matches(')');
        let (b, o) = inner.split_once(',').expect("ctid has block,offset");
        let block: i64 = b.trim().parse().expect("block int");
        let offset: i64 = o.trim().parse().expect("offset int");
        (block << 16) | offset
    }

    /// M56 DoD — the DELETE path tombstones dead nodes IN PLACE and the scan NAVIGATES THROUGH tombstones but
    /// NEVER emits them, proven END-TO-END against a REAL on-disk graph. VACUUM the *command* cannot run inside a
    /// pg_test transaction (M55/M48 precedent drives the command from an external harness), so this test invokes
    /// the FFI sweep DIRECTLY — the same `tombstone_sweep` `ambulkdelete` calls — against the built index.
    ///
    /// The load-bearing subtlety: the heap rows are NOT deleted. The executor's heap-recheck therefore CANNOT
    /// hide the swept nodes; the ONLY thing that can drop them from the result is the tombstone `emittable`
    /// filter. This isolates the filter under test from the heap-recheck backstop (which would mask a broken
    /// filter). A regular (WAL-logged) table exercises the real GenericXLog per-page path, not the temp/local-
    /// buffer path.
    #[pgrx::pg_test]
    fn tombstone_sweep_filters_dead_and_preserves_recall() {
        pgrx::Spi::run("CREATE TABLE tz (id int PRIMARY KEY, e vector(4))").unwrap();
        // 30 deterministic, distinct points — no distance ties near the probe → unambiguous exact NN (100%
        // recall at ef=200, so index order == exact order; same corpus as the recall-neutrality test above).
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO tz VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX tz_idx ON tz USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();

        let probe = "[3.3,1.1,2.2,0.4]";
        // Exact oracle over the full live set (seqscan): top-7 tells us the 2 victims AND the post-sweep truth.
        pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
        let exact_full = topk_ids_tbl("tz", probe, 7);
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let before = topk_ids_tbl("tz", probe, 5);
        assert_eq!(before.len(), 5, "index returns 5 results before any tombstone");

        // Tombstone the 2 nearest nodes IN the index (their heap rows stay ALIVE — see the doc comment).
        let victims = [before[0], before[1]];
        let victim_tids: Vec<i64> = victims.iter().map(|&id| heap_tid_i64("tz", id)).collect();
        unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'tz_idx'::regclass::oid").unwrap().expect("index oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let mut is_dead = |tid: i64| victim_tids.contains(&tid);
            let swept = tombstone_sweep(rel, &meta, &mut is_dead);
            let counted = count_tombstones(rel, &meta);
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            assert_eq!(swept, 2, "sweep tombstones exactly the 2 dead nodes in place (per-page WAL, no rebuild)");
            assert_eq!(counted, 2, "count_tombstones sees exactly the 2 on-page marks");
        }

        // After the sweep the scan must NOT emit the tombstoned nodes, yet still return 5 LIVE results — it
        // navigated THROUGH the 2 tombstones (their arcs preserved connectivity, so the graph is not severed).
        let after = topk_ids_tbl("tz", probe, 5);
        for v in &victims {
            assert!(!after.contains(v), "tombstoned node {v} is filtered (heap row still live → only the emittable filter can drop it)");
        }
        assert_eq!(after.len(), 5, "scan navigates through tombstones and still returns 5 live results (graph not disconnected)");

        // Recall preserved: post-sweep index top-5 == exact top-5 of (live set minus the 2 victims).
        let mut oracle: Vec<i32> = exact_full.into_iter().filter(|id| !victims.contains(id)).take(5).collect();
        let mut got = after.clone();
        oracle.sort_unstable();
        got.sort_unstable();
        assert_eq!(got, oracle, "navigate-through-don't-emit preserves recall: top-5 == exact top-5 of the survivors");
    }

    /// M56: the compaction ratio path — when tombstones exceed `theodb.hnsw_tombstone_compact_pct` of the graph,
    /// `vacuum_delete_inplace` folds (rebuild) to reclaim their space, dropping them from the physical layout.
    /// With the GUC set low and enough nodes swept, the fold runs and `count_tombstones` returns 0 afterwards
    /// (they are gone, not merely flagged), while the surviving live nodes still scan correctly.
    #[pgrx::pg_test]
    fn compaction_reclaims_tombstones_past_ratio_threshold() {
        pgrx::Spi::run("CREATE TABLE tc (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO tc VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX tc_idx ON tc USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        // Compact as soon as >10% of the graph is tombstoned; we will tombstone ~1/3 → well past the ratio.
        pgrx::Spi::run("SET theodb.hnsw_tombstone_compact_pct = 10").unwrap();

        // Delete 10 of 30 rows FROM THE HEAP so the executor callback agrees they are dead, then drive the
        // in-place delete path (which compacts because 10/30 > 10%).
        pgrx::Spi::run("DELETE FROM tc WHERE id < 10").unwrap();

        unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'tc_idx'::regclass::oid").unwrap().expect("index oid");
            // Predicate: a node is dead iff its heap row no longer exists (id < 10 were deleted).
            // Map each index tid back to its heap id via the surviving set: query which ids remain.
            let surviving: std::collections::HashSet<i64> = {
                let mut s = std::collections::HashSet::new();
                for id in 10..30i32 {
                    s.insert(heap_tid_i64("tc", id));
                }
                s
            };
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let mut dead = |tid: i64| !surviving.contains(&tid);
            let live = crate::am::build::vacuum_delete_inplace(rel, &mut dead);
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            assert_eq!(live, 20, "vacuum_delete_inplace reports 20 live nodes after compacting away 10");
            // After compaction the tombstones are physically GONE (reclaimed), not merely flagged.
            let rel2 = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let meta2 = read_meta(rel2).expect("read_meta after compaction");
            let remaining = count_tombstones(rel2, &meta2);
            assert_eq!(meta2.node_count, 20, "compacted graph has exactly the 20 surviving nodes");
            assert_eq!(remaining, 0, "compaction reclaimed the tombstones (0 left in the physical layout)");
            pg_sys::index_close(rel2, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        }

        // The compacted index still scans correctly against the surviving live set.
        let probe = "[13.3,1.1,2.2,1.4]"; // near ids in the surviving 10..30 range
        pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
        let exact = topk_ids_tbl("tc", probe, 5);
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let mut via_index = topk_ids_tbl("tc", probe, 5);
        assert!(via_index.iter().all(|id| *id >= 10), "no compacted-away node leaks into results");
        let (mut si, mut se) = (via_index.clone(), exact.clone());
        si.sort_unstable();
        se.sort_unstable();
        via_index.sort_unstable();
        assert_eq!(si, se, "compacted index top-5 == exact top-5 of the surviving set");
    }

    /// M56 DoD 2 — "recall BETWEEN compactions": as tombstones ACCUMULATE (up to the compaction threshold)
    /// WITHOUT a fold, does recall degrade? This is the blueprint's KEY uncertainty — it decides whether the
    /// phase-2 `RepairGraph`/slot-reuse (pgvector `hnswvacuum.c`) is needed NOW. We tombstone 20% of the graph
    /// in place (the `tombstone_sweep` is called DIRECTLY → NO fold, regardless of the GUC), delete those rows
    /// from the heap so the exact oracle agrees, and measure recall@10 (index vs exact seqscan of the 320
    /// survivors) averaged over 6 probes. High recall here ⇒ accumulated tombstones do NOT sever navigation ⇒
    /// phase 2 is deferred WITH EVIDENCE, not by omission.
    #[pgrx::pg_test]
    fn recall_holds_under_20pct_accumulated_tombstones() {
        pgrx::Spi::run("CREATE TABLE rc2 (id int PRIMARY KEY, e vector(16))").unwrap();
        for i in 0..400i32 {
            let v: Vec<String> = (0..16)
                .map(|j| format!("{:.3}", i as f32 * 0.5 + ((i * 7 + j * 13) % 29) as f32 * 0.3))
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO rc2 VALUES ({i}, '[{}]')", v.join(","))).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX rc2_idx ON rc2 USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 100").unwrap();

        // Tombstone 20% (ids 0..80): capture their tids BEFORE deleting, delete from heap, sweep DIRECTLY (no fold).
        let dead_tids: Vec<i64> = (0..80i32).map(|id| heap_tid_i64("rc2", id)).collect();
        pgrx::Spi::run("DELETE FROM rc2 WHERE id < 80").unwrap();
        unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'rc2_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let mut is_dead = |tid: i64| dead_tids.contains(&tid);
            let swept = tombstone_sweep(rel, &meta, &mut is_dead); // DIRECT sweep → NO compaction
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            assert_eq!(swept, 80, "20% of the 400-node graph tombstoned in place, no fold");
        }

        // recall@10 over 6 surviving probes (each probe = a survivor's own vector) vs the exact seqscan oracle.
        let probe_ids = [100, 150, 200, 250, 300, 350];
        let mut total_recall = 0.0f64;
        for &pid in &probe_ids {
            let probe: String = pgrx::Spi::get_one(&format!("SELECT e::text FROM rc2 WHERE id = {pid}"))
                .unwrap().expect("survivor vector");
            pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
            let exact = topk_ids_tbl("rc2", &probe, 10);
            pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
            let via_index = topk_ids_tbl("rc2", &probe, 10);
            let hits = via_index.iter().filter(|id| exact.contains(id)).count();
            total_recall += hits as f64 / exact.len().max(1) as f64;
        }
        let mean_recall = total_recall / probe_ids.len() as f64;
        assert!(
            mean_recall >= 0.9,
            "recall@10 under 20% accumulated tombstones (no compaction) = {mean_recall:.3} — expected >= 0.9 \
             (navigate-through preserves recall ⇒ phase-2 RepairGraph not urgent)"
        );
    }

    /// M56 fase 2 T1.1: `find_reusable_slot` returns None with no tombstones, and the tombstoned node's
    /// `(block, off)` after one is marked — the entry point of the in-place-insert slot-reuse (RepairGraph).
    #[pgrx::pg_test]
    fn find_reusable_slot_locates_a_tombstoned_slot() {
        pgrx::Spi::run("CREATE TABLE fr (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO fr VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX fr_idx ON fr USING theodb_hnsw (e)").unwrap();
        let dead_tid = heap_tid_i64("fr", 5);
        unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'fr_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            assert!(find_reusable_slot(rel, &meta, 0).is_none(), "no tombstones ⇒ no reusable slot");
            let mut is_dead = |tid: i64| tid == dead_tid;
            assert_eq!(tombstone_sweep(rel, &meta, &mut is_dead), 1, "tombstone node id=5");
            let slot = find_reusable_slot(rel, &meta, 0);
            assert!(slot.is_some(), "after tombstoning, a level-0 reusable slot exists");
            let (blk, off) = slot.unwrap();
            let item = page::read_page_item_at(rel, blk, off).expect("read slot");
            assert!(decode_element(&item).unwrap().deleted, "the found slot is the tombstone");
            assert!(find_reusable_slot(rel, &meta, 99).is_none(), "no slot has level ≥ 99 (nbr tuple wouldn't fit)");
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
        }
    }

    /// M56 fase 2 T1.2: `write_reused_element` revives a tombstoned slot with a new tid + vector, clears
    /// `deleted`, bumps `version`, and KEEPS the slot's level + neighbor-tuple address (Z takes X's graph position).
    #[pgrx::pg_test]
    fn write_reused_element_revives_slot_keeping_graph_position() {
        pgrx::Spi::run("CREATE TABLE wr (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO wr VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX wr_idx ON wr USING theodb_hnsw (e)").unwrap();
        let dead_tid = heap_tid_i64("wr", 7);
        unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'wr_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let mut is_dead = |t: i64| t == dead_tid;
            assert_eq!(tombstone_sweep(rel, &meta, &mut is_dead), 1, "tombstone node id=7");
            let slot = find_reusable_slot(rel, &meta, 0).expect("reusable slot");
            let bytes_before = page::read_page_item_at(rel, slot.0, slot.1).unwrap();
            let before = decode_element(&bytes_before).unwrap();
            let (lvl, nbr, ver) = (before.level, before.nbr_addr, before.version);
            assert!(before.deleted, "slot is a tombstone before revive");

            let newvec = [9.0f32, 8.0, 7.0, 6.0];
            assert!(write_reused_element(rel, slot, 424242, &newvec), "revive the v1 slot");
            let bytes_after = page::read_page_item_at(rel, slot.0, slot.1).unwrap();
            let after = decode_element(&bytes_after).unwrap();
            assert!(!after.deleted, "revived slot is live");
            assert_eq!(after.tid, 424242, "new tid written");
            assert_eq!(after.version, ver.wrapping_add(1), "version bumped");
            assert_eq!((after.level, after.nbr_addr), (lvl, nbr), "graph position (level + nbr slot) preserved");
            assert_eq!(f32::from_le_bytes(after.vec_bytes[0..4].try_into().unwrap()), 9.0, "new vector written");
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
        }
    }

    /// M56 fase 2 T1.3: `set_ground_neighbors_inplace` writes the given addrs into a node's ground slots (and the
    /// round-trip through `decode_neighbors` returns exactly them), the in-place neighbor-write half of the insert.
    #[pgrx::pg_test]
    fn set_ground_neighbors_inplace_round_trips() {
        pgrx::Spi::run("CREATE TABLE sg (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO sg VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX sg_idx ON sg USING theodb_hnsw (e)").unwrap();
        unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'sg_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let (m, m0) = (meta.m as usize, meta.m0 as usize);
            // node 0 = first element at (elem_first, 1); take its level + neighbor-tuple address.
            let ebytes = page::read_page_item_at(rel, meta.elem_first, 1).unwrap();
            let ev = decode_element(&ebytes).unwrap();
            let (lvl, nbr) = (ev.level as usize, ev.nbr_addr);
            let wanted: Vec<Addr> = vec![(meta.elem_first, 3), (meta.elem_first, 5)];
            assert!(set_ground_neighbors_inplace(rel, nbr, lvl, m, m0, &wanted), "write ground slots");
            let nbytes = page::read_page_item_at(rel, nbr.0, nbr.1).unwrap();
            let got = decode_neighbors(&nbytes, lvl, 0, m, m0).unwrap();
            assert_eq!(got, wanted, "ground slots round-trip through decode_neighbors (empties padded, dropped)");
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
        }
    }

    /// M56 fase 2 T2.1: `insert_search_ground` (descent + ground search) returns LIVE element addrs; querying with
    /// a node's OWN vector puts that node (distance ~0) as the nearest candidate — proves the on-disk insert search.
    #[pgrx::pg_test]
    fn insert_search_ground_finds_live_neighbors() {
        pgrx::Spi::run("CREATE TABLE ins (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO ins VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX ins_idx ON ins USING theodb_hnsw (e)").unwrap();
        let qtext: String = pgrx::Spi::get_one("SELECT e::text FROM ins WHERE id = 12").unwrap().expect("vec");
        let qv: Vec<f32> = qtext.trim_matches(|c| c == '[' || c == ']')
            .split(',').map(|s| s.trim().parse().unwrap()).collect();
        unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'ins_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let cands = insert_search_ground(rel, &meta, &qv, 100).expect("insert search");
            assert!(!cands.is_empty(), "search returns candidates");
            assert!(cands.len() <= meta.m0 as usize, "at most m0 ground neighbors");
            for (blk, off) in &cands {
                let b = page::read_page_item_at(rel, *blk, *off).unwrap();
                assert!(!decode_element(&b).unwrap().deleted, "each candidate is a LIVE element");
            }
            let (nblk, noff) = cands[0];
            let nb = page::read_page_item_at(rel, nblk, noff).unwrap();
            let nev = decode_element(&nb).unwrap();
            let d: f32 = (0..4)
                .map(|i| {
                    let x = f32::from_le_bytes(nev.vec_bytes[i * 4..i * 4 + 4].try_into().unwrap());
                    (qv[i] - x) * (qv[i] - x)
                })
                .sum();
            assert!(d < 0.01, "nearest candidate is node 12 itself (d={d})");
            pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        }
    }
}

// M56 — pure unit tests for the tombstone byte-layout (CI-runnable via `cargo test`, no DB/pgrx needed).
#[cfg(test)]
mod m56_tombstone_layout {
    use super::*;

    /// A minimal live element tuple: ELEM_TAG, pad(deleted/version)=0, dim=2, zero vector, no SBQ code.
    fn live_tuple() -> Vec<u8> {
        let dim = 2usize;
        let mut b = vec![0u8; ELEM_HEADER + dim * 4];
        b[E_TAG] = ELEM_TAG;
        b[E_DIM..E_DIM + 2].copy_from_slice(&(dim as u16).to_le_bytes());
        b
    }

    #[test]
    fn fresh_tuple_decodes_as_live() {
        let b = live_tuple();
        let ev = decode_element(&b).expect("decode");
        assert!(!ev.deleted, "a fresh tuple (pad=0) decodes as live");
        assert_eq!(ev.version, 0, "fresh version is 0");
    }

    #[test]
    fn mark_tombstone_flips_deleted_and_bumps_version_idempotently() {
        let mut b = live_tuple();
        assert!(mark_tombstone_in_place(&mut b), "marking a live tuple returns true");
        let ev = decode_element(&b).expect("decode");
        assert!(ev.deleted, "marked tuple decodes as deleted");
        assert_eq!(ev.version, 1, "version bumped to 1 on first delete");
        // Idempotent: re-marking a tombstone is a no-op (no double-bump, returns false).
        assert!(!mark_tombstone_in_place(&mut b), "re-marking a tombstone returns false");
        assert_eq!(decode_element(&b).unwrap().version, 1, "version stays 1 (no double-bump)");
    }

    #[test]
    fn tombstone_preserves_tid_nbr_and_vec_bytes() {
        // The tombstone flag must NOT corrupt tid/neighbor/vector (the node is still navigated THROUGH).
        let dim = 2usize;
        let mut b = vec![0u8; ELEM_HEADER + dim * 4];
        b[E_TAG] = ELEM_TAG;
        b[E_LEVEL] = 3;
        b[E_TID..E_TID + 8].copy_from_slice(&42i64.to_le_bytes());
        b[E_NBR_BLK..E_NBR_BLK + 4].copy_from_slice(&7u32.to_le_bytes());
        b[E_NBR_OFF..E_NBR_OFF + 2].copy_from_slice(&9u16.to_le_bytes());
        b[E_DIM..E_DIM + 2].copy_from_slice(&(dim as u16).to_le_bytes());
        b[E_VEC..E_VEC + 4].copy_from_slice(&1.5f32.to_le_bytes());
        mark_tombstone_in_place(&mut b);
        let ev = decode_element(&b).expect("decode");
        assert_eq!((ev.tid, ev.level, ev.nbr_addr), (42, 3, (7, 9)), "tid/level/neighbor intact");
        assert_eq!(f32::from_le_bytes(ev.vec_bytes[0..4].try_into().unwrap()), 1.5, "vector intact (navigation needs it)");
        assert!(ev.deleted);
    }
}
