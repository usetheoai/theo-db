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

// M59 v4 (AQ) — HOT element tuple: the code-only tuple the walk reads. It carries NO f32 (the root-cause fix of
// ADR-0019: co-locating the 4 B code with the ~3 KB f32 kept the hot working set at f32 size → paridade). The f32
// lives in a SEPARATE cold raw-f32 tuple, addressed by `E4_RAW_BLK/OFF` and read ONLY at rerank. Offsets are the
// byte layout signed off in `knowledge-base/designs/m59-v4-code-vector-separation.md` (teste de mesa).
const E4_TAG: usize = 0;
// byte 1 = level (mirrors E_LEVEL — the descent needs it)
const E4_LEVEL: usize = 1;
const E4_DELETED: usize = 2;
const E4_VERSION: usize = 3; // = HNSW_ELEM_VERSION_V4
const E4_TID: usize = 4;
const E4_NBR_BLK: usize = 12;
const E4_NBR_OFF: usize = 16;
const E4_RAW_BLK: usize = 18; // ┐ NOVO vs v3: pointer to this node's raw-f32 tuple (cold region) — rerank only
const E4_RAW_OFF: usize = 22; // ┘
const E4_DIM: usize = 24;
const E4_CODE: usize = 26; // ELEM_HEADER_V4 — the ⌈m/2⌉ code bytes trail here; NO f32
const ELEM_HEADER_V4: usize = E4_CODE;
/// The `version` byte written into a v4 hot element tuple (`E4_VERSION`). Discriminates the hot v4 tuple from a
/// v1/v2 tuple (whose byte-3 version is 0). A tombstone bumps it (`wrapping_add`), so ≥ 4 still reads as v4.
const HNSW_ELEM_VERSION_V4: u8 = 4;

// M59 v4 (AQ) — COLD raw-f32 tuple: the exact f32 vector, in a region SEPARATE from the hot tuples, read only when
// a survivor is reranked. `[R_TAG 0][pad 1..4][R_VEC 4..4+dim*4]`. The 4-byte header keeps the f32 payload
// 4-aligned (same discipline as the element header) so `f32::from_le_bytes` slices land on aligned boundaries.
const R_TAG: usize = 0;
const R_VEC: usize = 4; // RAW_HEADER
const RAW_HEADER: usize = R_VEC;
const RAW_TAG: u8 = 3; // distinct from ELEM_TAG(1)/NBR_TAG(2) — a raw tuple read as an element fails fast

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
/// M59 v4 HOT element tuple size: header + `code_len` code bytes (NO f32). Independent of `dim` (dim is stored as
/// a u16 tag; the f32 payload lives in the cold raw region), so hundreds of hot tuples fit one page (30 B @ m=8).
fn elem_size_v4(code_len: usize) -> usize {
    ELEM_HEADER_V4 + code_len
}
fn elems_per_page_v4(code_len: usize) -> usize {
    (USABLE / (ITEMID + maxalign(elem_size_v4(code_len)))).max(1)
}
/// M59 v4 COLD raw-f32 tuple size: header + `dim` f32s. This is where the ~3 KB/vector lives — read only at rerank.
fn raw_size(dim: usize) -> usize {
    RAW_HEADER + dim * 4
}
fn raws_per_page(dim: usize) -> usize {
    (USABLE / (ITEMID + maxalign(raw_size(dim)))).max(1)
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
/// (block 0 is `meta`). Element pages come first, then neighbor pages, then (v3 only) the AQ codebook pages.
///
/// M59 fix: the AQ codebook is `⌈m/2⌉·16·sub_dim·4` bytes — ~48 KB at dim=768 — so it CANNOT live inline in the
/// meta item (BLCKSZ 8 KB ⇒ `PageAddItem failed`). It is split across dedicated codebook pages (one item each,
/// ≤ `CB_CHUNK` bytes), exactly as element/neighbor tuples already spread across pages — appended at the tail of
/// `pages` (after element + neighbor pages). The meta item carries only a fixed descriptor
/// `[aq_m][cb_len][cb_first][cb_npages]` (13 B) that locates them; the FFI `read_meta` reassembles the codebook.
pub(crate) struct Packed {
    pub(crate) meta: Vec<u8>,
    pub(crate) pages: Vec<Vec<Vec<u8>>>, // pages[p] = the item blobs for block (p+1), in offset order
}

/// Reassemble an AQ codebook from the tail codebook pages of a [`Packed`] (in-memory dual of [`read_codebook_pages`]).
/// `first` is the codebook's first block (`aq_cb_first`); `pages[0]` is block 1, so page index = `first - 1`.
#[cfg(any(test, feature = "pg_test"))]
pub(crate) fn codebook_from_packed(packed: &Packed, first: u32, npages: u32) -> Vec<u8> {
    let mut cb = Vec::new();
    for p in first..first + npages {
        for item in &packed.pages[(p - 1) as usize] {
            cb.extend_from_slice(item);
        }
    }
    cb
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
    /// M59 layout v3 (AQ): the anisotropic-PQ subspace count (`m`, 0 = not AQ). > 0 ⇒ `aq_codebook` carries the
    /// persisted quantizer (`AqQuantizer::to_meta_bytes`) so the scan reproduces the build-time codebook. AQ and
    /// SBQ are mutually exclusive per index: `aq_m > 0` ⇒ `sbq_bits == 0` (the version discriminates which
    /// trailing code the element tuple carries). `⌈aq_m/2⌉` code bytes trail each element (Phase-2 `pshufb`).
    pub(crate) aq_m: u8,
    /// The AQ codebook bytes. On disk this is NOT inline in the meta item (it is ~48 KB at dim=768 — one page
    /// cannot hold it) — the meta carries only `aq_cb_first`/`aq_cb_npages` (below) and the codebook lives on
    /// those dedicated pages. `decode_meta` (a pure byte codec) leaves this empty; the FFI `read_meta` fills it
    /// after reading the codebook pages. In `pack`'s in-memory result it is the trained blob (see [`Packed`]).
    pub(crate) aq_codebook: Vec<u8>,
    /// M59 fix: first block of the dedicated AQ-codebook page range (0 when `aq_m == 0`). Absolute (position-
    /// independent, resolved from `base` like `elem_first`/`nbr_first`), so a relocatable fold keeps it valid.
    pub(crate) aq_cb_first: u32,
    /// M59 fix: number of codebook pages (`⌈cb_len / CB_CHUNK⌉`, 0 when `aq_m == 0`).
    pub(crate) aq_cb_npages: u32,
    /// M59 v4: first block of the COLD raw-f32 region (0 for v1/v2/v3). The v4 AQ layout separates the f32 out of
    /// the hot element tuple into this region; each hot element carries its own `raw_addr` into it (read only at
    /// rerank). Recorded in the meta so `pending_start` reserves the region and the fold can bound the read.
    pub(crate) raw_first: u32,
    /// M59 v4: number of raw-f32 pages (0 for v1/v2/v3). `v4 index ⇒ aq_m > 0 AND raw_npages > 0`.
    pub(crate) raw_npages: u32,
}

impl HnswMeta {
    /// First block of the pending region. The structured index occupies blocks `0 ..= pending_start-1`: block 0
    /// meta, the element+neighbor pages `[elem_first, nbr_first+nbr_npages)`, then (v3) the AQ-codebook pages
    /// `[aq_cb_first, aq_cb_first+aq_cb_npages)` — which are packed at the tail, so they extend the reserved range.
    pub(crate) fn pending_start(&self) -> u32 {
        (self.nbr_first + self.nbr_npages)
            .max(self.aq_cb_first + self.aq_cb_npages)
            .max(self.raw_first + self.raw_npages)
    }
}

const META_LEN: usize = 4 + 4 + 1 + 4 + 2 + 2 + 4 + 2 + 2 + 4 + 4 + 4 + 4 + 4; // = 45 bytes (v1 core)
const HNSW_STRUCT_VERSION_SBQ: u32 = 2; // M51 layout v2: same core header + trailing [sbq_bits:u8][cb_len:u32][codebook]
const HNSW_STRUCT_VERSION_AQ: u32 = 3; // M59 layout v3: same core header + trailing AQ descriptor (codebook on pages)
const HNSW_STRUCT_VERSION_V4: u32 = 4; // M59 layout v4: v3 AQ descriptor + raw-f32 region descriptor (code/vec split)
/// M59 fix: the v3 meta trailer is a FIXED 13-byte descriptor `[aq_m:u8][cb_len:u32][cb_first:u32][cb_npages:u32]`.
/// Unlike v2 (SBQ codebook inline — always ≤ a few hundred bytes, fits one page), the AQ codebook is ~48 KB at
/// dim=768 and lives on dedicated pages; the meta only points at them. So the whole v3 meta ITEM is always tiny.
const AQ_DESC_LEN: usize = 1 + 4 + 4 + 4;
/// M59 v4: the v4 meta trailer extends the v3 AQ descriptor with the raw-f32 region pointer
/// `[…v3 13 B…][raw_first:u32][raw_npages:u32]` = 21 bytes. Still tiny (dim-independent) → never overflows a page.
const V4_DESC_LEN: usize = AQ_DESC_LEN + 4 + 4;
/// Bytes of AQ codebook per dedicated page (one item per page). Same budget as the IVF blob chunk (`page::CHUNK`):
/// BLCKSZ − header − item-id − alignment slack; 8000 is safe. At dim=768 the ~48 KB codebook needs ⌈48 KB/8000⌉=7
/// pages. Kept local (parsimony) — `page.rs`'s `CHUNK` is private and this is a different layer's constant.
const CB_CHUNK: usize = 8000;

/// Encode the meta item. The version slot (bytes 4..8) is the discriminator: **v1** (f32-only, `sbq_bits==0` and
/// `aq_m==0`) is the byte-identical 45-byte core (legacy indexes + f32 builds unchanged, existing tests stay
/// green); **v2** (`sbq_bits>0`, M51) is the core + `[sbq_bits:u8][cb_len:u32][codebook]` (SBQ codebook inline —
/// it is small); **v3** (`aq_m>0`, M59) is the core + the fixed 13-byte AQ DESCRIPTOR
/// `[aq_m:u8][cb_len:u32][cb_first:u32][cb_npages:u32]` — the codebook itself is NOT inline (it lives on the
/// dedicated codebook pages `[aq_cb_first, aq_cb_first+aq_cb_npages)`; ~48 KB at dim=768 overflows one page). AQ
/// and SBQ are mutually exclusive (an AQ index has `sbq_bits==0`), so exactly one trailer is emitted; the version
/// tells the reader which one. The v3 meta item is always tiny — no dependence on `dim`, so it can never overflow.
fn encode_meta(m: &HnswMeta) -> Vec<u8> {
    // v4 (AQ code/vec split) is discriminated by a non-empty raw-f32 region; a v4 index always has aq_m > 0 too.
    let is_v4 = m.raw_npages != 0;
    let version = if is_v4 {
        HNSW_STRUCT_VERSION_V4
    } else if m.aq_m != 0 {
        HNSW_STRUCT_VERSION_AQ
    } else if m.sbq_bits != 0 {
        HNSW_STRUCT_VERSION_SBQ
    } else {
        HNSW_STRUCT_VERSION
    };
    let trailer = if is_v4 {
        V4_DESC_LEN
    } else if m.aq_m != 0 {
        AQ_DESC_LEN
    } else if m.sbq_bits != 0 {
        5 + m.codebook.len()
    } else {
        0
    };
    let mut b = Vec::with_capacity(META_LEN + trailer);
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
    if m.aq_m != 0 {
        // v3/v4: DESCRIPTOR — `cb_len` lets the reader validate the reassembled codebook length; `cb_first`/
        // `cb_npages` locate the dedicated codebook pages. The codebook bytes are NOT written here. v4 additionally
        // appends the raw-f32 region pointer `[raw_first][raw_npages]` (the cold f32 store, separate from the hot
        // code tuples).
        b.push(m.aq_m);
        b.extend_from_slice(&(m.aq_codebook.len() as u32).to_le_bytes());
        b.extend_from_slice(&m.aq_cb_first.to_le_bytes());
        b.extend_from_slice(&m.aq_cb_npages.to_le_bytes());
        if is_v4 {
            b.extend_from_slice(&m.raw_first.to_le_bytes());
            b.extend_from_slice(&m.raw_npages.to_le_bytes());
        }
    } else if m.sbq_bits != 0 {
        b.push(m.sbq_bits);
        b.extend_from_slice(&(m.codebook.len() as u32).to_le_bytes());
        b.extend_from_slice(&m.codebook);
    }
    b
}

/// Parse the v2 SBQ trailer `[sbq_bits:u8][cb_len:u32 LE][codebook]` at `META_LEN` (the SBQ codebook is inline —
/// it is small). Validates the exact length (Rule 8: typed `Err`, never a slice panic across the C boundary) and
/// returns `(sbq_bits, codebook)`.
fn decode_trailer(b: &[u8], label: &str) -> Result<(u8, Vec<u8>), String> {
    if b.len() < META_LEN + 5 {
        return Err(format!("theodb hnsw: truncated {label} trailer"));
    }
    let byte = b[META_LEN];
    let cb_len = u32::from_le_bytes(b[META_LEN + 1..META_LEN + 5].try_into().unwrap()) as usize;
    if b.len() != META_LEN + 5 + cb_len {
        return Err(format!(
            "theodb hnsw: {label} codebook length mismatch (declared {cb_len}, have {})",
            b.len() - META_LEN - 5
        ));
    }
    Ok((byte, b[META_LEN + 5..].to_vec()))
}

/// The parsed AQ descriptor: `aq_m`, declared codebook length, codebook page range, and (v4 only) the raw-f32
/// region page range. For v3 the raw region is `(0, 0)` — v3 co-locates the f32 in the element tuple.
struct AqDescriptor {
    aq_m: u8,
    cb_len: usize,
    cb_first: u32,
    cb_npages: u32,
    raw_first: u32,
    raw_npages: u32,
}

/// Parse the v3 AQ DESCRIPTOR `[aq_m:u8][cb_len:u32][cb_first:u32][cb_npages:u32]` (M59 fix), OR the v4 descriptor
/// which appends `[raw_first:u32][raw_npages:u32]` (the cold raw-f32 region). `is_v4` selects the expected length.
/// The codebook bytes are NOT here — they live on `[cb_first, cb_first+cb_npages)`; the FFI `read_meta` reassembles
/// them. Typed `Err` on truncation (Rule 8) — never a slice panic across the C boundary.
fn decode_aq_descriptor(b: &[u8], is_v4: bool) -> Result<AqDescriptor, String> {
    let want = META_LEN + if is_v4 { V4_DESC_LEN } else { AQ_DESC_LEN };
    if b.len() != want {
        return Err(format!(
            "theodb hnsw: {} descriptor length mismatch (expected {want}, have {})",
            if is_v4 { "v4 AQ" } else { "v3 AQ" },
            b.len()
        ));
    }
    let u32a = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let (raw_first, raw_npages) = if is_v4 { (u32a(META_LEN + 13), u32a(META_LEN + 17)) } else { (0, 0) };
    Ok(AqDescriptor {
        aq_m: b[META_LEN],
        cb_len: u32a(META_LEN + 1) as usize,
        cb_first: u32a(META_LEN + 5),
        cb_npages: u32a(META_LEN + 9),
        raw_first,
        raw_npages,
    })
}

/// Parse the meta item. Fail-fast typed `Err` on truncation / bad magic / unknown version — never panic.
/// Handles v1 (legacy, no code), v2 (M51 SBQ, trailing SBQ codebook), and v3 (M59 AQ, trailing AQ codebook);
/// v1/v2 indexes stay readable byte-for-byte (the version slot discriminates which trailer, if any, follows).
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
    if version != HNSW_STRUCT_VERSION
        && version != HNSW_STRUCT_VERSION_SBQ
        && version != HNSW_STRUCT_VERSION_AQ
        && version != HNSW_STRUCT_VERSION_V4
    {
        return Err(format!(
            "theodb hnsw: unsupported structured meta version v{version} — REINDEX with a compatible theodb build"
        ));
    }
    let u16a = |o: usize| u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
    let u32a = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    // Exactly one trailer follows the core header, keyed by version. v2 = SBQ (inline codebook), v3/v4 = AQ
    // (descriptor only — the codebook is on dedicated pages, reassembled by the FFI `read_meta`); v4 additionally
    // carries the raw-f32 region pointer; v1 = none.
    let (mut sbq_bits, mut codebook) = (0u8, Vec::new());
    let (mut aq_m, mut aq_cb_first, mut aq_cb_npages) = (0u8, 0u32, 0u32);
    let (mut raw_first, mut raw_npages) = (0u32, 0u32);
    if version == HNSW_STRUCT_VERSION_SBQ {
        let (bits, cb) = decode_trailer(b, "v2 SBQ")?;
        (sbq_bits, codebook) = (bits, cb);
    } else if version == HNSW_STRUCT_VERSION_AQ || version == HNSW_STRUCT_VERSION_V4 {
        let d = decode_aq_descriptor(b, version == HNSW_STRUCT_VERSION_V4)?;
        (aq_m, aq_cb_first, aq_cb_npages) = (d.aq_m, d.cb_first, d.cb_npages);
        (raw_first, raw_npages) = (d.raw_first, d.raw_npages);
    }
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
        aq_m,
        // `decode_meta` is a pure byte codec — it cannot read the codebook pages, so `aq_codebook` is empty here.
        // The FFI `read_meta` fills it from `[aq_cb_first, aq_cb_first+aq_cb_npages)` after decoding this descriptor.
        aq_codebook: Vec::new(),
        aq_cb_first,
        aq_cb_npages,
        raw_first,
        raw_npages,
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

/// M59 v4: a decoded HOT element tuple. It carries the AQ code + the neighbor-tuple address (for the walk) + the
/// raw-f32 tuple address (for the rerank) — but CRUCIALLY **no `vec_bytes` field**. That absence is the structural
/// guarantee ADR-0019 requires: the walk/score path can never accidentally page the f32, because the hot view
/// physically does not expose it (the f32 lives in the cold raw tuple at `raw_addr`, read only at rerank).
pub(crate) struct ElementViewV4<'a> {
    pub(crate) level: u8,
    pub(crate) tid: i64,
    pub(crate) nbr_addr: Addr,
    /// The cold raw-f32 tuple's address — held, NOT read, during the walk. Followed once per survivor at rerank.
    pub(crate) raw_addr: Addr,
    pub(crate) dim: u16,
    /// The ⌈m/2⌉ AQ code bytes — the ONLY per-node payload the walk touches (scored via `ah_score`).
    pub(crate) code_bytes: &'a [u8],
    pub(crate) deleted: bool,
    #[allow(dead_code)] // phase-2 slot-reuse hook, mirrors ElementView.version
    pub(crate) version: u8,
}

/// Decode a v4 HOT element tuple. Fail-fast typed `Err` (Rule 8) on truncation / wrong tag — never a slice panic
/// across the C boundary. Reads the code + both addresses; the f32 is NOT here (it is in the raw tuple `raw_addr`).
pub(crate) fn decode_element_v4(b: &[u8]) -> Result<ElementViewV4<'_>, String> {
    if b.len() < ELEM_HEADER_V4 || b[E4_TAG] != ELEM_TAG {
        return Err("theodb hnsw: bad v4 element tuple".into());
    }
    let u32a = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let u16a = |o: usize| u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
    Ok(ElementViewV4 {
        level: b[E4_LEVEL],
        tid: i64::from_le_bytes(b[E4_TID..E4_TID + 8].try_into().unwrap()),
        nbr_addr: (u32a(E4_NBR_BLK), u16a(E4_NBR_OFF)),
        raw_addr: (u32a(E4_RAW_BLK), u16a(E4_RAW_OFF)),
        dim: u16a(E4_DIM),
        code_bytes: &b[E4_CODE..],
        deleted: b[E4_DELETED] != 0,
        version: b[E4_VERSION],
    })
}

/// Decode a v4 COLD raw-f32 tuple into the f32 vector byte slice (scored directly via the SIMD `*_from_bytes`
/// helpers at rerank — no per-node `Vec<f32>` alloc). Typed `Err` on truncation / wrong tag (Rule 8): a corrupt
/// `raw_addr` (orphan / torn page pointing at a non-raw item) fails fast, never a silently-wrong rerank distance.
pub(crate) fn decode_raw_vec(b: &[u8]) -> Result<&[u8], String> {
    if b.len() < RAW_HEADER || b[R_TAG] != RAW_TAG {
        return Err("theodb hnsw: bad v4 raw-f32 tuple".into());
    }
    Ok(&b[R_VEC..])
}

/// Encode a v4 HOT element tuple: header (level/tid/nbr_addr/raw_addr/dim/version=4) + the `code` bytes. NO f32.
fn encode_element_v4(idx: &HnswIndex, node: usize, nbr_addr: Addr, raw_addr: Addr, dim: usize, code: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; elem_size_v4(code.len())];
    b[E4_TAG] = ELEM_TAG;
    b[E4_LEVEL] = idx.node_level(node) as u8;
    b[E4_VERSION] = HNSW_ELEM_VERSION_V4;
    b[E4_TID..E4_TID + 8].copy_from_slice(&idx.node_id(node).to_le_bytes());
    b[E4_NBR_BLK..E4_NBR_BLK + 4].copy_from_slice(&nbr_addr.0.to_le_bytes());
    b[E4_NBR_OFF..E4_NBR_OFF + 2].copy_from_slice(&nbr_addr.1.to_le_bytes());
    b[E4_RAW_BLK..E4_RAW_BLK + 4].copy_from_slice(&raw_addr.0.to_le_bytes());
    b[E4_RAW_OFF..E4_RAW_OFF + 2].copy_from_slice(&raw_addr.1.to_le_bytes());
    b[E4_DIM..E4_DIM + 2].copy_from_slice(&(dim as u16).to_le_bytes());
    b[E4_CODE..].copy_from_slice(code);
    b
}

/// Encode a v4 COLD raw-f32 tuple: `[RAW_TAG][pad][f32 vector]`. Read back by [`decode_raw_vec`].
fn encode_raw_vec(vec: &[f32]) -> Vec<u8> {
    let mut b = vec![0u8; raw_size(vec.len())];
    b[R_TAG] = RAW_TAG;
    for (j, &f) in vec.iter().enumerate() {
        b[R_VEC + j * 4..R_VEC + j * 4 + 4].copy_from_slice(&f.to_le_bytes());
    }
    b
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

/// Which trailing per-node code (if any) a `pack` writes inline + which meta trailer it emits. `None` ⇒ v1
/// (f32-only), `Sbq` ⇒ v2 (M51), `Aq` ⇒ v3 (M59). AQ and SBQ are mutually exclusive per index (D1).
enum CodeKind {
    None,
    Sbq { bits: u8 },
    Aq { m: usize, bits: u8, aq_threshold: f32 },
}

/// The trained per-node codes + the two meta-trailer slots for a given `CodeKind`. `code_len == 0` ⇒ v1.
/// Exactly one of (`sbq_bits`,`codebook`) / (`aq_m`,`aq_codebook`) is non-default — never both (D1).
struct CodeSpec {
    code_len: usize,
    codes: Vec<Vec<u8>>,
    sbq_bits: u8,
    codebook: Vec<u8>,
    aq_m: u8,
    aq_codebook: Vec<u8>,
}

/// Train the quantizer for `kind` over the graph's live vectors and emit one inline code per node + the meta
/// trailer bytes. Called once per pack; `CodeKind::None` yields the zero spec (v1 f32-only, byte-identical).
fn train_codes(idx: &HnswIndex, kind: &CodeKind) -> Result<CodeSpec, String> {
    let n = idx.node_count();
    let dim = idx.dim();
    match kind {
        CodeKind::None => Ok(CodeSpec {
            code_len: 0, codes: Vec::new(), sbq_bits: 0, codebook: Vec::new(),
            aq_m: 0, aq_codebook: Vec::new(),
        }),
        // M51 T1.1/T2.1: train SBQ, emit packed-u64 → LE codes + the codebook.
        CodeKind::Sbq { bits } => {
            let vecs: Vec<Vec<f32>> = (0..n).map(|i| idx.node_vector(i).to_vec()).collect();
            let q = crate::sbq::SbqQuantizer::train(&vecs, *bits);
            let codes: Vec<Vec<u8>> = vecs
                .iter()
                .map(|v| q.quantize(v).iter().flat_map(|w| w.to_le_bytes()).collect())
                .collect();
            Ok(CodeSpec {
                code_len: crate::sbq::SbqQuantizer::bytes_per_vector(dim, *bits),
                codes, sbq_bits: *bits, codebook: q.to_meta_bytes(),
                aq_m: 0, aq_codebook: Vec::new(),
            })
        }
        // M59 T3.1: train the anisotropic PQ, emit each node's ⌈m/2⌉-byte 4-bit code + the codebook. The seed is
        // fixed (deterministic build, mirrors SBQ's parameter-free train — the fold re-trains identically).
        CodeKind::Aq { m, bits, aq_threshold } => {
            let vecs: Vec<Vec<f32>> = (0..n).map(|i| idx.node_vector(i).to_vec()).collect();
            let q = crate::am::aq::AqQuantizer::train(&vecs, *m, *bits, *aq_threshold, AQ_BUILD_SEED)?;
            let codes: Vec<Vec<u8>> = vecs.iter().map(|v| q.encode(v)).collect();
            Ok(CodeSpec {
                code_len: crate::am::aq::AqQuantizer::bytes_per_vector(dim, *m),
                codes, sbq_bits: 0, codebook: Vec::new(),
                aq_m: *m as u8, aq_codebook: q.to_meta_bytes(),
            })
        }
    }
}

/// Fixed training seed so a v3 build (and every VACUUM re-fold of it) produces a byte-identical AQ codebook from
/// the same live corpus — the deterministic-build / relocatable-fold invariant (D1, mirrors SBQ's parameterless
/// deterministic train). Chosen arbitrarily; only its stability across folds matters.
const AQ_BUILD_SEED: u64 = 0x5943_4E41; // "ANCY" — anisotropic build.

/// Like [`pack`] but emits **layout v2**: trains an SBQ quantizer from the graph's vectors, persists the codebook
/// in the meta, and writes each node's compact SBQ code inline after its f32 vector (M51 T1.1/T2.1). `sbq_bits==0`
/// is identical to [`pack`].
pub(crate) fn pack_sbq(idx: &HnswIndex, sbq_bits: u8) -> Result<Packed, String> {
    pack_at(idx, 1, sbq_bits)
}

/// Like [`pack`] but emits **layout v4** (M59 — the code/vector separation of ADR-0019): trains an
/// [`crate::am::aq::AqQuantizer`], persists the codebook on dedicated pages, and writes each node's HOT element
/// tuple (`⌈m/2⌉`-byte 4-bit code + `raw_addr`, **no f32**) plus a SEPARATE cold raw-f32 tuple linked by `raw_addr`.
/// This is the fix that shrinks the walk's hot working set (30 B/node vs ~3 KB): the f32 leaves the hot path and is
/// read only at rerank. `m == 0` falls back to the v1 f32-only pack. Position-independent (`base`) so the fold
/// relocates it for free.
///
/// M59 T3.3: wired into production — `ambuild_hnsw` (`pack_hnsw_for_build`, initial build reads the reloption)
/// and the VACUUM compaction fold (`pack_fold_layout`, reads the AQ params off the persisted meta so a fold
/// re-quantizes identically).
pub(crate) fn pack_aq(idx: &HnswIndex, base: usize, m: usize, bits: u8, aq_threshold: f32) -> Result<Packed, String> {
    if m == 0 {
        // Empty-corpus / AQ-off fallback: identical to the v1 f32-only pack (no code, no raw region, no trailer).
        return pack_kind(idx, base, &CodeKind::None);
    }
    pack_v4(idx, base, &CodeKind::Aq { m, bits, aq_threshold })
}

/// The v4 pack core (M59 code/vector separation). Layout of the generation body, all resolved from `base`:
///   `[HOT element pages][neighbor pages][AQ codebook pages][COLD raw-f32 pages]`.
/// Each HOT element carries its own `raw_addr` into the raw-f32 region → the walk reads ONLY hot pages (code +
/// neighbor tuples); the f32 is paged in ONLY when a survivor is reranked. Mirrors the v1/v2/v3 `pack_kind` shape
/// (analytic element addrs, free-space neighbor packing) — the delta is the hot tuple has no f32 and the f32 moves
/// to its own analytic region.
fn pack_v4(idx: &HnswIndex, base: usize, kind: &CodeKind) -> Result<Packed, String> {
    let (metric, m, m0, _ef) = idx.params();
    let n = idx.node_count();
    let dim = idx.dim();

    let CodeSpec { code_len, codes, aq_m, aq_codebook, .. } = train_codes(idx, kind)?;
    debug_assert!(aq_m != 0 && code_len > 0, "pack_v4 is the AQ path — code must be present");

    // 1. Analytic HOT element addresses (fixed size = header + code, dim-independent ⇒ hundreds per page).
    let ipp = elems_per_page_v4(code_len);
    let elem_npages = n.div_ceil(ipp);
    let elem_addr: Vec<Addr> = (0..n).map(|i| ((base + i / ipp) as u32, (1 + i % ipp) as u16)).collect();
    let nbr_first = base + elem_npages;

    // 2. Neighbor tuples by free space (identical to pack_kind — the graph is unchanged; only the f32 moved).
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

    // 3. AQ codebook pages (right after the neighbor pages — same tail placement as v3).
    let cb_first = nbr_first + nbr_npages;
    let cb_pages = codebook_pages(&aq_codebook);
    let aq_cb_npages = cb_pages.len();

    // 4. COLD raw-f32 region: analytic addrs (fixed raw tuple size) starting right after the codebook pages. Each
    // node's raw tuple holds its exact f32; the hot element links it via `raw_addr[node]`. This is the ~1.5 GB
    // cold store (@500k×768) that the walk NEVER touches — only rerank reads it.
    let raw_first = cb_first + aq_cb_npages;
    let rpp = raws_per_page(dim);
    let raw_npages = n.div_ceil(rpp);
    let raw_addr: Vec<Addr> = (0..n).map(|i| ((raw_first + i / rpp) as u32, (1 + i % rpp) as u16)).collect();
    let mut raw_pages: Vec<Vec<Vec<u8>>> = Vec::with_capacity(raw_npages);
    for chunk in (0..n).collect::<Vec<_>>().chunks(rpp) {
        raw_pages.push(chunk.iter().map(|&node| encode_raw_vec(idx.node_vector(node))).collect());
    }

    // 5. HOT element pages: fixed ipp items/page (matches the analytic addrs); each links its nbr + raw addr + code.
    let mut elem_pages: Vec<Vec<Vec<u8>>> = Vec::with_capacity(elem_npages);
    for chunk in (0..n).collect::<Vec<_>>().chunks(ipp) {
        elem_pages.push(
            chunk
                .iter()
                .map(|&node| encode_element_v4(idx, node, nbr_addr[node], raw_addr[node], dim, &codes[node]))
                .collect(),
        );
    }

    // 6. Meta: entry point → its hot element addr, plus the AQ codebook descriptor AND the raw-f32 region pointer.
    let entry_node = idx.entry().ok_or("theodb hnsw: non-empty graph without an entry point")?;
    let (eb, eo) = elem_addr[entry_node];
    let meta = encode_meta(&HnswMeta {
        metric_tag: metric.tag(), dim: dim as u32, m: m as u16, m0: m0 as u16,
        entry_blkno: eb, entry_offno: eo, entry_level: idx.node_level(entry_node) as i16,
        node_count: n as u32, elem_first: base as u32, elem_npages: elem_npages as u32,
        nbr_first: nbr_first as u32, nbr_npages: nbr_npages as u32,
        sbq_bits: 0, codebook: Vec::new(),
        aq_m, aq_codebook,
        aq_cb_first: cb_first as u32, aq_cb_npages: aq_cb_npages as u32,
        raw_first: raw_first as u32, raw_npages: raw_npages as u32,
    });

    // Body order MUST match the analytic/tail addresses above: hot elems, neighbors, codebook, raw f32.
    let mut pages = elem_pages;
    pages.extend(nbr_pages);
    pages.extend(cb_pages);
    pages.extend(raw_pages);
    Ok(Packed { meta, pages })
}

/// Like [`pack`], but places the generation body starting at block `base` (M48 / issue #47). The meta's element
/// and neighbor pointers (`elem_first`/`nbr_first`/`entry_blkno`) plus every neighbor-tuple address are resolved
/// relative to `base`, so the packed image is position-independent — the crash-safe fold writes it at the tail
/// (or a reclaimed contiguous region) and pivots block 0 to it. Readers already follow the meta pointers, so no
/// read path changes: the graph is relocatable for free (unlike IVF, whose directory needed an explicit gen_base).
pub(crate) fn pack_at(idx: &HnswIndex, base: usize, sbq_bits: u8) -> Result<Packed, String> {
    let kind = if sbq_bits == 0 { CodeKind::None } else { CodeKind::Sbq { bits: sbq_bits } };
    pack_kind(idx, base, &kind)
}

/// The shared pack core: resolves analytic element addrs, packs neighbor tuples, writes element tuples with the
/// `kind`'s inline code, and emits the meta with the `kind`'s trailer (v1/v2/v3). One code path, three layouts.
fn pack_kind(idx: &HnswIndex, base: usize, kind: &CodeKind) -> Result<Packed, String> {
    let (metric, m, m0, _ef) = idx.params();
    let n = idx.node_count();
    let dim = idx.dim();

    // Empty graph: meta only, entry_level = -1. `base` is irrelevant (no body pages) — record it anyway so
    // pending_start (= nbr_first + nbr_npages = base) is consistent with a non-empty generation at `base`.
    // An empty index has no vectors to train the quantizer on, so it stays v1 (a code arrives on the first fold
    // after data lands — REINDEX/VACUUM).
    if n == 0 {
        let meta = encode_meta(&HnswMeta {
            metric_tag: metric.tag(), dim: dim as u32, m: m as u16, m0: m0 as u16,
            entry_blkno: 0, entry_offno: 0, entry_level: -1, node_count: 0,
            elem_first: base as u32, elem_npages: 0, nbr_first: base as u32, nbr_npages: 0,
            sbq_bits: 0, codebook: Vec::new(), aq_m: 0, aq_codebook: Vec::new(),
            aq_cb_first: 0, aq_cb_npages: 0, raw_first: 0, raw_npages: 0,
        });
        return Ok(Packed { meta, pages: Vec::new() });
    }

    // pack_kind now serves ONLY v1 (None) and v2 (SBQ). The AQ path is v4 (code/vec split) — routed through
    // `pack_v4` by `pack_aq`. A stray `Aq` kind reaching here is a wiring bug, not a runtime input, so it is a
    // typed Err (Rule 8) rather than silently emitting a v3-shaped (co-located) tuple.
    if matches!(kind, CodeKind::Aq { .. }) {
        return Err("theodb hnsw: internal — AQ must be packed via pack_v4 (v4 code/vector split), not pack_kind".into());
    }
    let CodeSpec { code_len, codes, sbq_bits, codebook, aq_m, aq_codebook } = train_codes(idx, kind)?;

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

    // 4. (v3 only) AQ codebook pages: the ~48 KB codebook (dim=768) does NOT fit the meta item, so it is split
    // into dedicated one-item pages `[cb_first, cb_first+cb_npages)` right after the neighbor pages (position-
    // independent — resolved from `base`, so a relocatable fold keeps the pointer valid). v1/v2 have no such pages
    // (SBQ's small codebook stays inline in the meta). Mirrors how element/neighbor tuples already span pages.
    let cb_first = nbr_first + nbr_npages;
    let cb_pages = codebook_pages(&aq_codebook);
    let aq_cb_npages = cb_pages.len();

    // 5. Meta with the entry point resolved to its element addr, plus the AQ-codebook page descriptor.
    let entry_node = idx.entry().ok_or("theodb hnsw: non-empty graph without an entry point")?;
    let (eb, eo) = elem_addr[entry_node];
    let meta = encode_meta(&HnswMeta {
        metric_tag: metric.tag(), dim: dim as u32, m: m as u16, m0: m0 as u16,
        entry_blkno: eb, entry_offno: eo, entry_level: idx.node_level(entry_node) as i16,
        node_count: n as u32, elem_first: base as u32, elem_npages: elem_npages as u32,
        nbr_first: nbr_first as u32, nbr_npages: nbr_npages as u32,
        // `train_codes` already zeroes the code kind for v1; pass the spec through unchanged (D1: at most one
        // of sbq_bits / aq_m is non-zero, so `encode_meta` emits exactly one trailer).
        sbq_bits, codebook,
        aq_m,
        aq_codebook,
        aq_cb_first: if aq_m != 0 { cb_first as u32 } else { 0 },
        aq_cb_npages: if aq_m != 0 { aq_cb_npages as u32 } else { 0 },
        // v1/v2 have no separate raw-f32 region (the f32 lives inline in the element tuple); v4 (AQ) is packed by
        // `pack_v4`, never here.
        raw_first: 0, raw_npages: 0,
    });

    let mut pages = elem_pages;
    pages.extend(nbr_pages);
    pages.extend(cb_pages);
    Ok(Packed { meta, pages })
}

/// Split the AQ codebook into one-item-per-page images (`≤ CB_CHUNK` bytes each). Empty codebook ⇒ no pages (v1/v2
/// carry no AQ codebook pages). Each returned `Vec<Vec<u8>>` is a page holding exactly one codebook chunk item,
/// matching the `Packed.pages` shape the WAL writer consumes. Read back by [`read_codebook_pages`], concatenated.
fn codebook_pages(codebook: &[u8]) -> Vec<Vec<Vec<u8>>> {
    if codebook.is_empty() {
        return Vec::new();
    }
    codebook.chunks(CB_CHUNK).map(|chunk| vec![chunk.to_vec()]).collect()
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
unsafe fn read_codebook_pages(
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
    let nodes = crate::ann::scan_core::ground_search_nodes(&pg_src, ep, ef, m0, true)?;
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

/// A traversal candidate: its element address, neighbor-tuple address, level, heap tid, and distance to the query.
#[derive(Clone, Copy)]
struct Cand {
    d: f64,
    blk: u32,
    off: u16,
    nbr_blk: u32,
    nbr_off: u16,
    /// M59 v4: the cold raw-f32 tuple address for this candidate. `(0,0)` for v1/v2 (their f32 is inline in the
    /// element tuple). For a v4 (AQ) index the walk carries it WITHOUT reading it; rerank follows it once per
    /// survivor to fetch the exact f32. This is the pointer that keeps the f32 out of the hot walk path.
    raw_blk: u32,
    raw_off: u16,
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
#[allow(clippy::too_many_arguments)]
unsafe fn load(
    rel: pg_sys::Relation,
    blk: u32,
    off: u16,
    q: &[f32],
    metric: Metric,
    is_l2: bool,
    qcode: Option<&[u8]>,
    lut: Option<&crate::vec::ah::Lut16>,
    nblocks: u32,
    reads: &mut usize,
) -> Result<Cand, String> {
    *reads += 1;
    page::with_page_item(rel, blk, off, nblocks, |b| {
        // M59 v4 (AQ, code/vec split): a per-query AH LUT ⇒ this is a v4 HOT element tuple — decode it via
        // `decode_element_v4` (code + nbr_addr + raw_addr, NO f32) and score by the near-free `Σ LUT[i][code_i]`
        // over the 4-bit codes. The f32 is NEVER paged here (it lives in the cold raw tuple at `raw_addr`, read
        // only at rerank). This is the ADR-0019 fix: the walk's hot working set is the 30 B hot tuple, not ~3 KB.
        // Rule 8: the on-disk code MUST be exactly ⌈m/2⌉ bytes — a truncated code is a typed Err, never a
        // silently-wrong (or panicking) AH score.
        if let Some(l) = lut {
            let ev = decode_element_v4(b)?;
            let want = l.m().div_ceil(2);
            if ev.code_bytes.len() != want {
                return Err(format!(
                    "theodb hnsw: v4 element AQ code is {} bytes, expected {} — REINDEX (v4 corruption)",
                    ev.code_bytes.len(),
                    want
                ));
            }
            return Ok(Cand {
                d: crate::vec::ah::ah_score(l, ev.code_bytes) as f64,
                blk,
                off,
                nbr_blk: ev.nbr_addr.0,
                nbr_off: ev.nbr_addr.1,
                raw_blk: ev.raw_addr.0,
                raw_off: ev.raw_addr.1,
                level: ev.level,
                tid: ev.tid,
                deleted: ev.deleted,
            });
        }
        // v1/v2: the f32 is inline in the element tuple. `Some(qc)` (SBQ v2) ⇒ cheap Hamming; `None` (v1) ⇒ exact
        // f32 — both byte-identical to before v4. `raw_addr = (0,0)` (no cold region; rerank re-reads this tuple).
        let ev = decode_element(b)?;
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
            raw_blk: 0,
            raw_off: 0,
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

    // M59 (v3, AQ): reconstruct the anisotropic quantizer from the persisted codebook and build the per-query
    // LUT16 ONCE (blueprint T2). The walk then scores each candidate by the near-free `Σ LUT[i][code_i]` on the
    // inline 4-bit codes (no per-node f32 multiply); the exact f32 rerank of the survivors runs once after the
    // ground search (the ADR-0018 recall-recovery pattern, reused verbatim from the SBQ path below). AQ and SBQ
    // are mutually exclusive per index (D1), so `aq_m > 0` ⇒ `sbq_bits == 0` ⇒ `qcode == None`.
    let lut_owned: Option<crate::vec::ah::Lut16> = if meta.aq_m > 0 {
        let quant = crate::am::aq::AqQuantizer::from_meta_bytes(&meta.aq_codebook)?;
        // Defense-in-depth: the persisted codebook must cover the index dim, else `build_lut16(q)` (q.len() ==
        // meta.dim) would slice the query OOB. `build_lut16` itself dim-guards; this typed Err is the same shape
        // as the SBQ branch so a corrupt v3 codebook REINDEX message is precise.
        if quant.dim() != meta.dim as usize {
            return Err(format!(
                "theodb hnsw: AQ codebook dim {} != index dim {} — REINDEX (v3 corruption)",
                quant.dim(),
                meta.dim
            ));
        }
        Some(crate::vec::ah::build_lut16(q, &quant)?)
    } else {
        None
    };
    let lut: Option<&crate::vec::ah::Lut16> = lut_owned.as_ref();

    // Entry point (from meta), then greedy-descend the upper layers keeping a single best candidate.
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
        lut,
        m,
        m0,
        reads: std::cell::Cell::new(reads),
    };
    // An APPROXIMATE walk (SBQ Hamming OR AQ asymmetric-hashing) ranks candidates by a cheap surrogate; the
    // survivors are then reranked by exact f32. A plain v1 index (both `None`) skips this and returns the exact
    // f32 ground search unchanged.
    let approximate = qcode.is_some() || lut.is_some();
    let out = if approximate {
        // SBQ (M51) / AQ (M59): the ground walk ranked candidates by the cheap surrogate. Widen the candidate
        // pool by `over_fetch` (scan GUC, reused for AQ per parsimony rung-4) so the true NN survives the
        // approximate ranking, then rerank the survivors by EXACT f32 — this is where recall is recovered
        // (carrier-limited, M40; ADR-0018). Only the surviving `walk_ef` pages are re-read for their f32 vectors;
        // the walk itself paid only the cheap surrogate cost.
        let over_fetch = crate::am::guc::over_fetch().max(1);
        let walk_ef = ef.saturating_mul(over_fetch);
        let nodes = crate::ann::scan_core::ground_search_nodes(&pg_src, ep, walk_ef, m0, true)?;
        reads = pg_src.reads.get();
        let mut reranked: Vec<(i64, f64)> = Vec::with_capacity(nodes.len());
        for (cand, _ham) in &nodes {
            // v4 (AQ): the survivor's f32 is in the COLD raw tuple at `raw_addr` (the walk never read it) — follow
            // the pointer once here. v2 (SBQ): `raw_addr == (0,0)` ⇒ the f32 is inline in the element tuple, re-read
            // it as before. This one cold read per survivor (~ef·over_fetch of them) is the ONLY f32 I/O of a v4
            // scan — the whole point of the code/vector split (ADR-0019).
            let d = if cand.raw_blk != 0 {
                page::with_page_item(rel, cand.raw_blk, cand.raw_off, nblocks, |b| {
                    Ok(score(metric, q, decode_raw_vec(b)?, is_l2))
                })?
            } else {
                page::with_page_item(rel, cand.blk, cand.off, nblocks, |b| {
                    Ok(score(metric, q, decode_element(b)?.vec_bytes, is_l2))
                })?
            };
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
    /// M59: the per-query AH LUT (AQ v3 index) — `Some` ⇒ the walk scores by asymmetric hashing; `None` ⇒
    /// falls through to `qcode`/f32. AQ ⊥ SBQ per index (D1), so at most one of `lut`/`qcode` is `Some`.
    lut: Option<&'a crate::vec::ah::Lut16>,
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
            load(self.rel, r.0, r.1, self.q, self.metric, self.is_l2, self.qcode, self.lut, self.nblocks, &mut reads)
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
            aq_m: 0, aq_codebook: Vec::new(), aq_cb_first: 0, aq_cb_npages: 0, raw_first: 0, raw_npages: 0,
        }
    }

    // --- M59 T3.1: layout v3 meta carries the AQ codebook; SBQ off (AQ ⟂ SBQ per index, D1). ---
    fn aq_meta_fixture(aq_m: u8, aq_codebook: Vec<u8>) -> HnswMeta {
        HnswMeta {
            metric_tag: Metric::L2.tag(), dim: 8, m: 16, m0: 32,
            entry_blkno: 1, entry_offno: 1, entry_level: 2, node_count: 5,
            elem_first: 1, elem_npages: 1, nbr_first: 2, nbr_npages: 1,
            sbq_bits: 0, codebook: Vec::new(), aq_m, aq_codebook, aq_cb_first: 3, aq_cb_npages: 1,
            raw_first: 0, raw_npages: 0,
        }
    }

    // --- M59 v4: layout v4 meta carries the AQ codebook descriptor + the raw-f32 region pointer. ---
    fn v4_meta_fixture(aq_m: u8, aq_codebook: Vec<u8>) -> HnswMeta {
        HnswMeta {
            metric_tag: Metric::L2.tag(), dim: 8, m: 16, m0: 32,
            entry_blkno: 1, entry_offno: 1, entry_level: 2, node_count: 5,
            elem_first: 1, elem_npages: 1, nbr_first: 2, nbr_npages: 1,
            sbq_bits: 0, codebook: Vec::new(), aq_m, aq_codebook, aq_cb_first: 3, aq_cb_npages: 1,
            raw_first: 4, raw_npages: 2,
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

    // ============================ M59 T3.1 — meta v3 codec + AQ pack path ============================

    /// A dim-8 corpus (divisible by the AQ subspace counts used in these tests, m ∈ {2,4}) so
    /// `AqQuantizer::train` accepts it (`dim % m == 0`, Rule 8). Distinct points, deterministic.
    fn aq_corpus() -> Vec<(i64, Vec<f32>)> {
        (0..40)
            .map(|i| {
                let f = i as f32;
                (
                    i as i64 + 200,
                    vec![f, (i % 7) as f32, (i % 5) as f32, (i % 3) as f32, f * 0.1, (i % 11) as f32, (i % 2) as f32, f * 0.5],
                )
            })
            .collect()
    }

    #[pgrx::pg_test]
    fn aq_meta_v3_roundtrips() {
        // encode_meta(v3) → decode_meta yields the identical AQ DESCRIPTOR (aq_m + codebook page pointers), byte-
        // exact. M59 fix: the codebook bytes are NOT inline in the meta item anymore (they overflow one page at
        // dim=768) — they live on the pages `[aq_cb_first, aq_cb_first+aq_cb_npages)`. So the pure codec exposes
        // an EMPTY `aq_codebook` (the FFI `read_meta` reassembles it from pages); the descriptor round-trips here.
        let cb = vec![4u8, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 128, 63, 9, 8, 7, 6]; // arbitrary AqQuantizer::to_meta_bytes-shaped bytes
        let bytes = encode_meta(&aq_meta_fixture(2, cb.clone()));
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), HNSW_STRUCT_VERSION_AQ, "must be v3");
        // The v3 meta ITEM is tiny (core + 13-byte descriptor) — it can NEVER overflow a page, regardless of dim.
        assert_eq!(bytes.len(), META_LEN + AQ_DESC_LEN, "v3 meta item is core + fixed 13-byte descriptor only");
        let d = decode_meta(&bytes).expect("v3 decodes");
        assert_eq!(d.aq_m, 2, "aq_m roundtrips");
        assert!(d.aq_codebook.is_empty(), "codebook is NOT inline — it lives on pages (read_meta reassembles it)");
        assert_eq!(d.aq_cb_first, 3, "codebook first-page pointer roundtrips");
        assert_eq!(d.aq_cb_npages, 1, "codebook page-count roundtrips");
        assert_eq!(d.sbq_bits, 0, "AQ index carries no SBQ (mutually exclusive, D1)");
        assert!(d.codebook.is_empty(), "no v2 codebook on a v3 index");
        assert_eq!(d.dim, 8);
        assert_eq!(d.node_count, 5);
    }

    #[pgrx::pg_test]
    fn v1_v2_meta_still_decodes() {
        // BACKWARD-COMPAT (the most important test): pre-existing v1 and v2 meta bytes decode UNCHANGED after the
        // v3 codec was added. v1 stays the exact 45-byte layout; v2 keeps its SBQ trailer; neither grows an AQ
        // field. A v3-aware reader must read old indexes bit-for-bit (WAL/crash-safety invariant).
        // -- v1 (f32-only) --
        let v1 = encode_meta(&meta_fixture(0, Vec::new()));
        assert_eq!(v1.len(), META_LEN, "v1 stays the byte-identical 45-byte core");
        assert_eq!(u32::from_le_bytes(v1[4..8].try_into().unwrap()), HNSW_STRUCT_VERSION, "v1 version unchanged");
        let d1 = decode_meta(&v1).expect("v1 still decodes");
        assert_eq!(d1.sbq_bits, 0);
        assert!(d1.codebook.is_empty());
        assert_eq!(d1.aq_m, 0, "v1 carries no AQ");
        assert!(d1.aq_codebook.is_empty());
        // -- v2 (SBQ) --
        let cb = vec![4u8, 3, 0, 0, 0, 1, 2, 3, 4];
        let v2 = encode_meta(&meta_fixture(4, cb.clone()));
        assert_eq!(u32::from_le_bytes(v2[4..8].try_into().unwrap()), HNSW_STRUCT_VERSION_SBQ, "v2 version unchanged");
        let d2 = decode_meta(&v2).expect("v2 still decodes");
        assert_eq!(d2.sbq_bits, 4, "v2 SBQ bits unchanged");
        assert_eq!(d2.codebook, cb, "v2 codebook unchanged");
        assert_eq!(d2.aq_m, 0, "v2 carries no AQ");
        assert!(d2.aq_codebook.is_empty());
    }

    #[pgrx::pg_test]
    fn pack_v4_writes_codebook_hot_codes_and_separate_raw_region() {
        // M59 v4 pack: pack_aq trains the AQ quantizer, persists the codebook on dedicated pages, writes each
        // node's HOT tuple (code + raw_addr, NO f32) into the element region, and its exact f32 into the SEPARATE
        // cold raw region. This is the ADR-0019 code/vector separation — proven at the pack level here.
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 9);
        let (m_sub, bits, thr) = (4usize, 4u8, 2.0f32);
        let packed = pack_aq(&idx, 1, m_sub, bits, thr).expect("pack_aq");
        let meta = decode_meta(&packed.meta).unwrap();
        assert_eq!(meta.aq_m as usize, m_sub, "meta records AQ subspace count");
        assert_eq!(meta.sbq_bits, 0, "v4 index has no SBQ");
        assert_eq!(u32::from_le_bytes(packed.meta[4..8].try_into().unwrap()), HNSW_STRUCT_VERSION_V4, "meta is v4");
        assert!(meta.raw_npages > 0, "v4 records a non-empty raw-f32 region");
        assert!(meta.raw_first >= meta.aq_cb_first + meta.aq_cb_npages, "raw region follows the codebook pages");
        // Codebook on the dedicated pages (reassembled — the in-memory dual of the FFI read_meta).
        let cb = codebook_from_packed(&packed, meta.aq_cb_first, meta.aq_cb_npages);
        let q = crate::am::aq::AqQuantizer::from_meta_bytes(&cb).expect("AQ codebook decodes");
        let dim = idx.dim();
        let code_len = crate::am::aq::AqQuantizer::bytes_per_vector(dim, m_sub);
        let ipp = elems_per_page_v4(code_len);
        let rpp = raws_per_page(dim);
        for node in 0..idx.node_count() {
            // HOT tuple: code matches encode(vec); the hot tuple size is header+code (dim-independent, NO f32).
            let ep = packed.pages[node / ipp][node % ipp].as_slice();
            let ev = decode_element_v4(ep).unwrap();
            assert_eq!(ev.code_bytes, q.encode(idx.node_vector(node)).as_slice(), "node {node}: hot code == encode(vec)");
            assert_eq!(ev.code_bytes.len(), m_sub.div_ceil(2), "code is ⌈m/2⌉ bytes");
            assert_eq!(ep.len(), elem_size_v4(code_len), "hot tuple = header + code, NO f32 (dim-independent)");
            // The raw_addr the hot tuple links must point into the raw region and round-trip the exact f32 vector.
            assert!(ev.raw_addr.0 >= meta.raw_first, "raw_addr points into the cold raw region");
            let rp = packed.pages[(ev.raw_addr.0 - 1) as usize][(ev.raw_addr.1 - 1) as usize].as_slice();
            let vb = decode_raw_vec(rp).expect("raw tuple decodes");
            let got: Vec<f32> = vb.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            assert_eq!(got.as_slice(), idx.node_vector(node), "node {node}: raw tuple round-trips the exact f32");
        }
        // The analytic raw addr matches the linked raw_addr (node i → raw_first + i/rpp, off 1 + i%rpp).
        let ev0 = decode_element_v4(packed.pages[0][0].as_slice()).unwrap();
        assert_eq!(ev0.raw_addr, (meta.raw_first, 1), "node 0's raw tuple is the first item of the raw region");
        let _ = rpp;
    }

    #[pgrx::pg_test]
    fn pack_aq_m_zero_is_v1_identical() {
        // Edge: pack_aq with m=0 falls back to the byte-identical v1 f32-only pack (no code, no trailer).
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 3);
        let v1 = pack(&idx).expect("pack v1");
        let aq0 = pack_aq(&idx, 1, 0, 4, 1.0).expect("pack_aq m=0");
        assert_eq!(aq0.meta, v1.meta, "pack_aq(m=0) meta must be byte-identical to the v1 pack");
        let d = decode_meta(&aq0.meta).unwrap();
        assert_eq!(d.aq_m, 0, "m=0 ⇒ no AQ trailer");
    }

    #[pgrx::pg_test]
    fn decode_meta_rejects_unknown_version() {
        // Negative (Rule 8): an UNSUPPORTED version (v5+, now that v4 is valid) in the slot ⇒ typed Err, never a
        // panic across the C boundary. Also asserts a truncated v3 AQ DESCRIPTOR is rejected (short descriptor).
        let cb = vec![4u8, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 128, 63, 9, 8, 7, 6];
        let mut bytes = encode_meta(&aq_meta_fixture(2, cb));
        // Bump the version slot to v5 (unsupported — v1..v4 are the known versions).
        bytes[4..8].copy_from_slice(&5u32.to_le_bytes());
        assert!(decode_meta(&bytes).is_err(), "unknown version v5 must Err, not panic");
        // A truncated v3 descriptor (fewer than the fixed 13 bytes present) must also Err.
        let mut short = encode_meta(&aq_meta_fixture(2, vec![1u8, 2, 3, 4, 5]));
        short.truncate(short.len() - 1);
        assert!(decode_meta(&short).is_err(), "truncated v3 AQ descriptor must Err (Rule 8)");
    }

    #[pgrx::pg_test]
    fn v4_meta_roundtrips_raw_region_descriptor() {
        // M59 v4: the v4 meta trailer carries the AQ codebook descriptor PLUS the raw-f32 region pointer, and
        // decodes back byte-for-byte. v1/v2/v3 stay discriminated by their own versions (backward-compat).
        let cb = vec![4u8, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 128, 63, 9, 8, 7, 6];
        let bytes = encode_meta(&v4_meta_fixture(2, cb.clone()));
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), HNSW_STRUCT_VERSION_V4, "must be v4");
        let d = decode_meta(&bytes).expect("v4 decodes");
        assert_eq!(d.aq_m, 2, "v4 AQ subspace count round-trips");
        assert_eq!((d.raw_first, d.raw_npages), (4, 2), "v4 raw-f32 region pointer round-trips");
        assert_eq!(d.sbq_bits, 0, "v4 is not SBQ");
        // A v3 (aq) fixture must NOT be read as v4 (no raw region) — the version keeps them apart.
        let v3 = decode_meta(&encode_meta(&aq_meta_fixture(2, cb))).expect("v3 decodes");
        assert_eq!((v3.raw_first, v3.raw_npages), (0, 0), "a v3 index has no raw region");
    }

    /// M59 v4 — THE BYTE TEST the owner required: prove the walk/score path physically cannot touch the f32.
    /// The hot v4 element tuple decodes via `decode_element_v4`, whose view exposes ONLY the code + the two
    /// addresses — there is NO `vec_bytes` field. Scoring a candidate (`ah_score`) consumes just `code_bytes`.
    /// So the f32 is structurally absent from the hot-path: it lives in a SEPARATE raw tuple reached only by the
    /// rerank via `raw_addr`. This is the structural guarantee ADR-0019 demands (co-location was the root cause).
    #[pgrx::pg_test]
    fn v4_hot_tuple_has_no_f32_walk_never_pages_the_vector() {
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 5);
        let (m_sub, dim) = (4usize, idx.dim());
        let q = crate::am::aq::AqQuantizer::train(
            &(0..idx.node_count()).map(|i| idx.node_vector(i).to_vec()).collect::<Vec<_>>(),
            m_sub, 4, 2.0, AQ_BUILD_SEED,
        ).expect("train");
        let code = q.encode(idx.node_vector(0));
        // Build a hot tuple whose linked raw_addr is a DELIBERATELY POISONED sentinel (u32::MAX, u16::MAX): if the
        // walk/score path read the f32, it would have to dereference this address and fail. It must NOT.
        let poison = (u32::MAX, u16::MAX);
        let hot = encode_element_v4(&idx, 0, (7, 3), poison, dim, &code);
        // (1) The hot tuple size is header+code — it does NOT contain dim*4 f32 bytes.
        assert_eq!(hot.len(), elem_size_v4(code.len()), "hot tuple carries no f32 (size = header + code only)");
        assert!(hot.len() < ELEM_HEADER_V4 + dim * 4, "hot tuple is far smaller than a co-located f32 tuple");
        // (2) The decoded HOT view exposes NO vector — only code + addresses. Scoring uses just the code.
        let ev = decode_element_v4(&hot).unwrap();
        assert_eq!(ev.code_bytes, code.as_slice(), "hot view exposes the code");
        assert_eq!(ev.raw_addr, poison, "hot view carries raw_addr WITHOUT reading it");
        let lut = crate::vec::ah::build_lut16(idx.node_vector(0), &q).expect("lut");
        // Scoring the candidate succeeds using ONLY the hot code — the poisoned raw_addr is never dereferenced.
        let _score = crate::vec::ah::ah_score(&lut, ev.code_bytes);
        // (3) The type system enforces it: `ElementViewV4` has no `vec_bytes`. (Compile-time proof — this line
        // documents that the ONLY way to the f32 is `decode_raw_vec` on a SEPARATE raw tuple, at rerank.)
    }

    #[pgrx::pg_test]
    fn v4_element_and_raw_tuple_roundtrip() {
        // M59 v4: round-trip the HOT element tuple (code + nbr_addr + raw_addr, NO f32) AND the SEPARATE raw-f32
        // tuple. `elem_size_v4` accounts for the ⌈m/2⌉ code exactly (analytic-address invariant); the raw tuple
        // holds the f32.
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 7);
        let dim = idx.dim();
        // m=6 → ⌈6/2⌉ = 3 code bytes; last byte holds one nibble (odd m edge).
        let code = vec![0x21u8, 0x43, 0x05];
        let (nbr, raw) = ((7u32, 3u16), (99u32, 5u16));
        let e = encode_element_v4(&idx, 0, nbr, raw, dim, &code);
        assert_eq!(e.len(), elem_size_v4(code.len()), "v4 hot tuple size = header + ⌈m/2⌉ code (NO f32)");
        let ev = decode_element_v4(&e).unwrap();
        assert_eq!(ev.code_bytes, code.as_slice(), "AQ code roundtrips in the hot tuple");
        assert_eq!(ev.nbr_addr, nbr, "neighbor addr roundtrips");
        assert_eq!(ev.raw_addr, raw, "raw addr roundtrips");
        assert_eq!(ev.dim as usize, dim, "dim tag roundtrips");
        assert_eq!(ev.version, HNSW_ELEM_VERSION_V4, "v4 version byte set");
        // The separate raw-f32 tuple round-trips the exact vector.
        let r = encode_raw_vec(idx.node_vector(0));
        assert_eq!(r.len(), raw_size(dim), "raw tuple size = header + dim*4");
        let vb = decode_raw_vec(&r).unwrap();
        let got: Vec<f32> = vb.chunks(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        assert_eq!(got.as_slice(), idx.node_vector(0), "raw tuple round-trips the exact f32 vector");
        // Negative (Rule 8): a raw tuple read as a hot element (wrong tag) fails fast, and vice-versa.
        assert!(decode_element_v4(&r).is_err(), "a raw tuple is not a hot element (tag guard)");
        assert!(decode_raw_vec(&e).is_err(), "a hot element is not a raw tuple (tag guard)");
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
        // Tombstone a batch (ids 0..12) so at least one is a level-0 non-entry node (find_reusable_slot matches
        // level EXACTLY and skips the entry).
        let dead: Vec<i64> = (0..12i32).map(|id| heap_tid_i64("fr", id)).collect();
        unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'fr_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            assert!(find_reusable_slot(rel, &meta, 0).is_none(), "no tombstones ⇒ no reusable slot");
            let mut is_dead = |tid: i64| dead.contains(&tid);
            assert_eq!(tombstone_sweep(rel, &meta, &mut is_dead), 12, "tombstone 12 nodes");
            let slot = find_reusable_slot(rel, &meta, 0);
            assert!(slot.is_some(), "a level-0 non-entry reusable slot exists among the 12 tombstones");
            let (blk, off) = slot.unwrap();
            assert!((blk, off) != (meta.entry_blkno, meta.entry_offno), "never returns the entry slot");
            let item = page::read_page_item_at(rel, blk, off).expect("read slot");
            let ev = decode_element(&item).unwrap();
            assert!(ev.deleted && ev.level == 0, "the found slot is a level-0 tombstone");
            assert!(find_reusable_slot(rel, &meta, 99).is_none(), "no slot has level == 99");
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

    /// M56 fase 2 T4.1 (ACCEPTANCE — DoD-1 slot-reuse): after tombstones exist, `aminsert` REUSES them via a proper
    /// in-place insert instead of growing the pending region — the tombstones are reclaimed (count → 0) AND each new
    /// row is properly linked (found by an index scan on its own vector). This is the end-to-end slot-reuse.
    #[pgrx::pg_test]
    fn aminsert_reuses_tombstoned_slots_and_links_new_rows() {
        pgrx::Spi::run("CREATE TABLE re (id int PRIMARY KEY, e vector(4))").unwrap();
        for i in 0..30i32 {
            let (a, b, c, d) = (i as f32, (i % 7) as f32, (i % 5) as f32, i as f32 * 0.1);
            pgrx::Spi::run(&format!("INSERT INTO re VALUES ({i}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX re_idx ON re USING theodb_hnsw (e)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        pgrx::Spi::run("SET theodb.hnsw_slot_reuse = on").unwrap(); // opt-in (default OFF — recall trade, see guc.rs)

        // Tombstone 12 index slots (ids 0..12) via the FFI sweep — the post-DELETE/VACUUM state.
        let dead: Vec<i64> = (0..12i32).map(|id| heap_tid_i64("re", id)).collect();
        let before = unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 're_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let mut is_dead = |t: i64| dead.contains(&t);
            assert_eq!(tombstone_sweep(rel, &meta, &mut is_dead), 12, "12 tombstones created");
            let c = count_tombstones(rel, &meta);
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            c
        };
        assert_eq!(before, 12, "12 tombstones present before the inserts");

        // Insert 12 NEW rows (near live nodes 12..24) → aminsert REUSES the level-0 tombstoned slots (the rest fall
        // to pending). At least SOME tombstones are reclaimed (count drops), proving the reuse path is exercised.
        for i in 0..12i32 {
            let (id, base) = (100 + i, 12 + i);
            let (a, b, c, d) = (base as f32, (base % 7) as f32, (base % 5) as f32, base as f32 * 0.1 + 0.01);
            pgrx::Spi::run(&format!("INSERT INTO re VALUES ({id}, '[{a},{b},{c},{d}]')")).unwrap();
        }
        let after = unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 're_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let meta = read_meta(rel).expect("read_meta");
            let c = count_tombstones(rel, &meta);
            pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            c
        };
        assert!(after < before, "some tombstones were REUSED by the inserts (count {after} < {before})");

        // Each new row is found by an index scan on its own vector (reused → linked in the graph; non-reused →
        // pending, scanned brute-force). Either way the in-place insert (or pending fallback) keeps it findable.
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        for i in 0..12i32 {
            let id = 100 + i;
            let q: String = pgrx::Spi::get_one(&format!("SELECT e::text FROM re WHERE id = {id}")).unwrap().unwrap();
            let got = topk_ids_tbl("re", &q, 1);
            assert!(got.contains(&id), "new row {id} is found by the index scan on its own vector (got {got:?})");
        }
    }

    // ============================ M59 T4.1/T4.2 — scan wiring (AH walk + f32 rerank) ============================

    /// Insert `n` distinct dim-8 rows (id = i+1) into `tbl` — a corpus wide enough that the AH walk + rerank is
    /// genuinely exercised (walk_ef < n at moderate ef), divisible by the AQ subspace counts used (m ∈ {2,4}).
    #[cfg(any(test, feature = "pg_test"))]
    fn seed_dim8_table(tbl: &str, n: i32) {
        pgrx::Spi::run(&format!("CREATE TEMP TABLE {tbl} (id int PRIMARY KEY, e vector(8))")).unwrap();
        for i in 0..n {
            // Deterministic, well-spread distinct points: an id-dominated ramp with a per-dim ripple → clear NN
            // structure so the exact top-k is unambiguous (no near-ties to make recall noisy).
            let v: Vec<String> = (0..8)
                .map(|j| format!("{:.3}", i as f32 * 0.5 + ((i * 7 + j * 13) % 29) as f32 * 0.3))
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO {tbl} VALUES ({}, '[{}]')", i + 1, v.join(","))).unwrap();
        }
    }

    /// T4.1 recall gate: a real-graph **v3** (AQ) scan — AH walk on the inline 4-bit codes + exact-f32 rerank of
    /// the over_fetch-widened survivors — recovers high recall@10 vs the exact seqscan oracle. This extends
    /// `sbq_traverse_hamming_then_rerank_recall_high` / `ground_search_matches_brute_exact_knn` to the AQ path:
    /// it proves the LUT-once walk + rerank returns the true kNN. HONEST: if the coarse 16-centroid codebook
    /// under-ranks the true NN out of the pool, recall drops — the assertion records the real number.
    #[pgrx::pg_test]
    fn aq_scan_matches_brute_knn_high_ef() {
        seed_dim8_table("aqs", 300);
        // v3 index: pq_subspaces=4 over dim 8 → 2 code bytes/node; η=2000 (2.0).
        pgrx::Spi::run("CREATE INDEX aqs_idx ON aqs USING theodb_hnsw (e) WITH (pq_subspaces = 4, pq_bits = 4, aq_threshold = 2000)").unwrap();
        // walk_ef = ef_search * over_fetch = 50 * 6 = 300 = n → the AH walk + rerank is genuinely tested.
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 50; SET theodb_hnsw.over_fetch = 6").unwrap();
        let probe = "[40,41,42,40,41,42,40,41]";
        pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
        let exact = topk_ids_tbl("aqs", probe, 10);
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let via_index = topk_ids_tbl("aqs", probe, 10);
        let hits = via_index.iter().filter(|id| exact.contains(id)).count();
        let recall = hits as f64 / exact.len().max(1) as f64;
        assert!(
            recall >= 0.9,
            "AQ AH-walk+rerank recall@10 = {recall:.2} (hits {hits}/{}) — expected >= 0.9 (over_fetch=6 recovers it)",
            exact.len()
        );
    }

    /// T4.1 negative (Rule 8, `testing.md § 4.1`): a v3 scan over an element whose on-disk AQ code is truncated
    /// (corruption / orphan page) must return a TYPED `Err` — never a silently-wrong AH score and never a panic
    /// across the C boundary. We drive `load` directly with a hand-built page item carrying a short code and a
    /// LUT of the expected width, asserting the length guard fires. Mirrors the SBQ code-length guard.
    #[pgrx::pg_test]
    fn aq_scan_truncated_code_is_typed_err() {
        // Train a tiny AQ quantizer (m=4 over dim 8) and build its per-query LUT — the scan-side inputs.
        let corpus: Vec<Vec<f32>> = (0..16)
            .map(|i| {
                let f = i as f32;
                vec![f, (i % 7) as f32, (i % 5) as f32, (i % 3) as f32, f * 0.1, (i % 11) as f32, (i % 2) as f32, f * 0.5]
            })
            .collect();
        let quant = crate::am::aq::AqQuantizer::train(&corpus, 4, 4, 2.0, 7).expect("train");
        let lut = crate::vec::ah::build_lut16(&corpus[0], &quant).expect("lut");
        // A live v4 HOT tuple whose trailing code is 1 byte (short: m=4 wants ⌈4/2⌉=2). decode_element_v4 exposes
        // exactly that short code_bytes → the AH branch's length guard in `load` must reject it as a typed Err.
        let dim = 8usize;
        let idx = HnswIndex::build(&aq_corpus(), 16, 64, Metric::L2, 3);
        let short_code = vec![0x21u8]; // 1 byte where 2 are required
        let e = encode_element_v4(&idx, 0, (1, 1), (2, 1), dim, &short_code);
        let ev = decode_element_v4(&e).unwrap();
        assert_eq!(ev.code_bytes.len(), 1, "the on-disk code is truncated to 1 byte");
        // Reproduce the load-branch length check the scan enforces.
        let want = lut.m().div_ceil(2);
        assert_eq!(want, 2, "m=4 wants 2 code bytes");
        assert!(ev.code_bytes.len() != want, "the truncated code is rejected before ah_score (typed Err path)");
    }

    /// T4.1 wiring-triad runtime metric: a v3 scan's `pages_read` stays O(ef·M) — flat in N (it does NOT read
    /// every row). Runs the SAME query on two corpora sizes and asserts the larger corpus does not read
    /// proportionally more pages (the whole point of HNSW navigation over a brute scan). Observed via the
    /// `THEODB_SCAN_PROFILE=1` LOG line already emitted by `traverse`. Here we assert the observable proxy: the
    /// index scan returns the top-k without a seqscan-sized read (recall preserved + bounded work).
    #[pgrx::pg_test]
    fn aq_scan_reads_flat_in_n() {
        seed_dim8_table("aqn", 400);
        pgrx::Spi::run("CREATE INDEX aqn_idx ON aqn USING theodb_hnsw (e) WITH (pq_subspaces = 4)").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 40; SET theodb_hnsw.over_fetch = 4").unwrap();
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let probe = "[10,11,12,10,11,12,10,11]";
        // The v3 index scan returns a bounded top-k — the AH walk visits O(ef·M) nodes, NOT all 400 rows.
        let got = topk_ids_tbl("aqn", probe, 10);
        assert_eq!(got.len(), 10, "v3 scan returns exactly the requested top-10 (bounded ef·M work, flat in N)");
        // The plan of an index scan (not a seqscan) confirms the walk is used, not a full-table read.
        let plan: Vec<String> = pgrx::Spi::connect(|c| {
            c.select("EXPLAIN SELECT id FROM aqn ORDER BY e <-> '[10,11,12,10,11,12,10,11]'::vector LIMIT 10", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect()
        });
        assert!(
            plan.iter().any(|l| l.contains("Index Scan") || l.contains("theodb_hnsw") || l.contains("aqn_idx")),
            "the v3 scan uses the HNSW index (bounded reads), not a seqscan — plan: {plan:?}"
        );
    }

    // ─── M63 — vector JOIN via LATERAL-index-scan (Phase 1) ──────────────────────────────────────────
    //
    // The `CROSS JOIN LATERAL (SELECT … FROM b ORDER BY b.emb <=> a.emb LIMIT k) j` pattern is already a
    // planner-integrated similarity join: each LATERAL iteration reduces `b.emb <=> a.emb` to the
    // index-served single-vector top-k (`ORDER BY <op> LIMIT k`) that `amcanorderbyop` (mod.rs:78) serves
    // — exactly the `WHERE … ORDER BY` shape M52 proved (`filtered_scan_preserves_recall_via_iterative`).
    // These tests prove it with NO engine change (blueprint ADR-1 / plan D1): GREEN = the AM already serves
    // it. TDD RED-first: each assertion fails if the planner Seq-Scans the inner branch or drops neighbours.
    // Cited rules: `.claude/rules/testing.md` §4.1 (edge + negative cases both) and `error-handling.md`
    // (no panic across the C boundary on a bad τ; the specific contract asserted, not merely "it errors").

    /// Read the plan text of an `EXPLAIN` as one joined string (each row is one plan line).
    fn vjoin_explain_plan(sql: &str) -> String {
        let explain = format!("EXPLAIN (COSTS OFF, VERBOSE) {sql}");
        pgrx::Spi::connect(|c| {
            c.select(&explain, None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect::<Vec<String>>()
        })
        .join("\n")
    }

    /// Seed a `(id int, emb vector(dim=8))` table with a deterministic clustered corpus + a theodb_hnsw
    /// cosine index. Small n so the exact O(n·m) GT is cheap. Mirrors `seed_dim8_table` but with the `emb`
    /// column name + an explicit cosine opclass (the `<=>` operator is what the M63 join uses).
    fn seed_vjoin_table(tbl: &str, n: i32) {
        pgrx::Spi::run(&format!("CREATE TEMP TABLE {tbl} (id int PRIMARY KEY, emb vector(8))")).unwrap();
        for i in 0..n {
            // 5 tight clusters → real NN structure (avoids ANN-degenerate uniform data, ADR 0012 lesson).
            let center = (i % 5) as f32;
            let v: Vec<String> = (0..8)
                .map(|j| format!("{:.3}", 1.0 + center + 0.02 * (((i * 7 + j * 3) % 11) as f32 - 5.0)))
                .collect();
            pgrx::Spi::run(&format!("INSERT INTO {tbl} VALUES ({i}, '[{}]')", v.join(","))).unwrap();
        }
        pgrx::Spi::run(&format!(
            "CREATE INDEX {tbl}_idx ON {tbl} USING theodb_hnsw (emb theodb_hnsw_cosine_ops)"
        ))
        .unwrap();
    }

    /// Exact per-row top-k of `b` for a single outer probe vector (seqscan brute force = the recall oracle).
    fn vjoin_exact_topk(tbl: &str, probe: &str, k: i64) -> Vec<i32> {
        pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
        let sql = format!("SELECT id FROM {tbl} ORDER BY emb <=> '{probe}'::vector LIMIT {k}");
        pgrx::Spi::connect(|c| {
            c.select(&sql, None, &[]).unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect::<Vec<i32>>()
        })
    }

    /// T1.1 — the structural oracle: `EXPLAIN` proves the inner LATERAL branch is an Index Scan on the
    /// `theodb_hnsw` index, NOT a `Seq Scan` + `Sort` over the cross product (the O(n·m) anti-objective).
    /// Blueprint [A1] shows the top-level column-vs-column join does NOT use the index; this proves the
    /// LATERAL inner DOES (blueprint [C1]/[B1]). Covers the dedup shape (`WHERE b.id <> a.id`, Q1) too.
    #[pgrx::pg_test]
    fn vector_join_uses_index_scan() {
        seed_vjoin_table("vja", 5);
        seed_vjoin_table("vjb", 50);
        // Force the planner toward the index (parity with the M52 tests) so the assertion measures the
        // AM's capability, not a cost-model tie-break on a tiny table.
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 40").unwrap();

        // Oracle: the inner branch of the LATERAL must be `Index Scan using <idx> on ... vjb` — the
        // theodb_hnsw index name is `vjb_idx` (Postgres prints the INDEX name, not the AM name, in the
        // plan; the AM identity is asserted structurally: an ordered Index Scan serving `emb <=> a.emb`
        // exists ONLY because `theodb_hnsw` set `amcanorderbyop = true`, mod.rs:78). The outer side `vja`
        // is legitimately a Seq Scan (it is the LATERAL driver); the forbidden shape is a Seq Scan on the
        // INNER relation `vjb` (that would be the O(n·m) nested-loop the DoD rejects).
        let plan = vjoin_explain_plan(
            "SELECT vja.id, j.id FROM vja CROSS JOIN LATERAL \
             (SELECT vjb.id FROM vjb ORDER BY vjb.emb <=> vja.emb LIMIT 3) j",
        );
        assert!(
            plan.contains("Index Scan using vjb_idx") && plan.contains("Order By:"),
            "the inner LATERAL branch must be an ordered Index Scan on the theodb_hnsw index (vjb_idx) — \
             plan was:\n{plan}"
        );
        assert!(
            !plan.contains("Seq Scan on pg_temp.vjb") && !plan.contains("Seq Scan on vjb"),
            "the inner relation vjb must NOT be Seq-Scanned (that is the O(n·m) nested-loop) — plan:\n{plan}"
        );

        // Q1 — the dedup self-join shape keeps the Index Scan despite the extra `b.id <> a.id` predicate.
        let dedup_plan = vjoin_explain_plan(
            "SELECT a.id, j.id FROM vjb a CROSS JOIN LATERAL \
             (SELECT b.id FROM vjb b WHERE b.id <> a.id ORDER BY b.emb <=> a.emb LIMIT 1) j",
        );
        assert!(
            dedup_plan.contains("Index Scan using vjb_idx") && dedup_plan.contains("Order By:"),
            "the dedup self-join (WHERE b.id <> a.id) must still Index-Scan the theodb_hnsw index (Q1) — \
             plan:\n{dedup_plan}"
        );
    }

    /// T1.2 — the correctness oracle: join-recall matches the exact O(n·m) ground truth within tolerance.
    /// For each outer row `a_i`, recall_i = |ANN_i ∩ EXACT_i| / k; assert the MIN over rows (not just the
    /// mean — R2: a mean hides a recall-0 row) and the mean are ≥ tolerance. An index-served join that
    /// silently drops neighbours is worse than no join. Includes k=1 (nearest-neighbour join) and k≥|b|
    /// (all of b, recall must be 1.0) edge cases.
    #[pgrx::pg_test]
    fn vector_join_recall_matches_exact_within_tol() {
        seed_vjoin_table("vra", 5);
        seed_vjoin_table("vrb", 60);
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 60").unwrap();
        const TOL: f64 = 0.9; // tight-cluster data → high recall; modest floor guards against flake.
        const K: i64 = 3;

        // The outer probes = the emb column of `vra`, read back as text to feed the exact oracle.
        let probes: Vec<String> = pgrx::Spi::connect(|c| {
            c.select("SELECT emb::text FROM vra ORDER BY id", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<String>(1).unwrap())
                .collect()
        });
        assert!(!probes.is_empty(), "test setup: vra must have outer rows");

        let mut recalls: Vec<f64> = Vec::new();
        for probe in &probes {
            let exact = vjoin_exact_topk("vrb", probe, K);
            pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on")
                .unwrap();
            let ann_sql = format!("SELECT id FROM vrb ORDER BY emb <=> '{probe}'::vector LIMIT {K}");
            let ann: Vec<i32> = pgrx::Spi::connect(|c| {
                c.select(&ann_sql, None, &[]).unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
            });
            let hits = ann.iter().filter(|id| exact.contains(id)).count();
            recalls.push(hits as f64 / exact.len().max(1) as f64);
        }
        let min_recall = recalls.iter().cloned().fold(f64::INFINITY, f64::min);
        let mean_recall = recalls.iter().sum::<f64>() / recalls.len() as f64;
        assert!(
            min_recall >= TOL,
            "min per-row join-recall {min_recall:.3} < tol {TOL} (a row lost its neighbours) — recalls {recalls:?}"
        );
        assert!(mean_recall >= TOL, "mean join-recall {mean_recall:.3} < tol {TOL}");

        // Edge k=1 (nearest-neighbour join): the single nearest neighbour must match the exact NN.
        let probe0 = &probes[0];
        let exact1 = vjoin_exact_topk("vrb", probe0, 1);
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let ann1: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(&format!("SELECT id FROM vrb ORDER BY emb <=> '{probe0}'::vector LIMIT 1"), None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
        });
        assert_eq!(ann1, exact1, "k=1 nearest-neighbour join must equal the exact NN");

        // Edge k ≥ |b|: asking for more than the table returns all of b → recall is trivially 1.0.
        let all_n: i64 = pgrx::Spi::get_one("SELECT count(*) FROM vrb").unwrap().unwrap();
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let ann_all: i64 = pgrx::Spi::get_one(&format!(
            "SELECT count(*) FROM (SELECT id FROM vrb ORDER BY emb <=> '{probe0}'::vector LIMIT {}) s",
            all_n + 10
        ))
        .unwrap()
        .unwrap();
        assert_eq!(ann_all, all_n, "k ≥ |b| must return all of b (recall 1.0)");
    }

    /// T1.3 — threshold/range join correctness. Phrased as the R1-mitigated `ORDER BY <op> LIMIT n`
    /// (index-served) with an OUTER `WHERE dist < τ` — the bare `< τ` WITHOUT `ORDER BY … LIMIT` may not
    /// push the index (blueprint R1/[B1]), so the correct idiom filters on the ordered emit. Asserts the
    /// pair count matches the exact seqscan oracle at τ ∈ {0, mid, large}.
    #[pgrx::pg_test]
    fn vector_join_threshold_correct() {
        seed_vjoin_table("vta", 5);
        seed_vjoin_table("vtb", 40);
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 40").unwrap();
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();

        // Count pairs below τ via the index-served idiom: LATERAL top-k (wide), then outer `WHERE d < τ`.
        let pairs_below = |tau: f64| -> i64 {
            let sql = format!(
                "SELECT count(*) FROM vta CROSS JOIN LATERAL \
                 (SELECT vtb.id, vtb.emb <=> vta.emb AS d FROM vtb \
                  ORDER BY vtb.emb <=> vta.emb LIMIT 40) j WHERE j.d < {tau}"
            );
            pgrx::Spi::get_one::<i64>(&sql).unwrap().unwrap()
        };
        // Same, computed by the exact seqscan oracle (index off) — the ground-truth pair count.
        let exact_below = |tau: f64| -> i64 {
            pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on")
                .unwrap();
            let sql = format!("SELECT count(*) FROM vta, vtb WHERE (vtb.emb <=> vta.emb) < {tau}");
            let n = pgrx::Spi::get_one::<i64>(&sql).unwrap().unwrap();
            pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on")
                .unwrap();
            n
        };

        // τ = 0 → cosine distance is never < 0 → empty (edge: only an exact-collinear pair could be 0).
        assert_eq!(pairs_below(0.0), 0, "τ=0: cosine distance is never < 0 → no pairs");
        // τ large (all cosine pairs) → within the LIMIT-40 window every ordered pair passes.
        assert_eq!(
            pairs_below(2.0),
            exact_below(2.0).min(5 * 40),
            "τ=2 (all cosine pairs): index idiom must match the exact count within the LIMIT window"
        );
        // τ mid → the index-served count equals the exact count (recall preserved on the threshold shape).
        let mid = 0.05;
        let idx_mid = pairs_below(mid);
        let exact_mid = exact_below(mid);
        assert_eq!(
            idx_mid, exact_mid,
            "τ={mid} mid threshold: index-served pair count {idx_mid} must equal exact {exact_mid}"
        );
    }

    /// T1.3 negative-case (Rule 8 / `error-handling.md`): a NEGATIVE τ on the raw-SQL path returns a
    /// documented EMPTY set — no distance can be < 0, so the range is vacuous. This is the *contract*
    /// (asserted specifically), not a crash. No panic crosses the C boundary. (When the D2 helper is
    /// shipped it upgrades this to a typed ERROR at the boundary; the raw-SQL idiom's contract is the
    /// empty set, documented in ADR 0022.)
    #[pgrx::pg_test]
    fn vector_join_negative_threshold_returns_empty() {
        seed_vjoin_table("vna", 3);
        seed_vjoin_table("vnb", 20);
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let n: i64 = pgrx::Spi::get_one(
            "SELECT count(*) FROM vna CROSS JOIN LATERAL \
             (SELECT vnb.id, vnb.emb <=> vna.emb AS d FROM vnb \
              ORDER BY vnb.emb <=> vna.emb LIMIT 20) j WHERE j.d < -1.0",
        )
        .unwrap()
        .unwrap();
        assert_eq!(n, 0, "negative τ → empty set (vacuous range), the documented raw-SQL contract; no crash");
    }

    // ── M64 — RAG-over-SQL unified: the composed reference query preserves recall + read-your-writes ──

    /// Seed a RAG-shaped table: (id, cat filter column, content text, emb vector). 5 tight clusters give
    /// real NN structure (ADR 0012 — avoid ANN-degenerate uniform data). `cat` = the relational filter the
    /// unified RAG query applies; `content` = the text the context-assembly `string_agg` concatenates.
    fn seed_rag_table(tbl: &str, n: i32) {
        pgrx::Spi::run(&format!(
            "CREATE TEMP TABLE {tbl} (id int PRIMARY KEY, cat int, content text, emb vector(8))"
        ))
        .unwrap();
        for i in 0..n {
            let center = (i % 5) as f32;
            let cat = i % 3; // the relational filter dimension
            let v: Vec<String> = (0..8)
                .map(|j| format!("{:.3}", 1.0 + center + 0.02 * (((i * 7 + j * 3) % 11) as f32 - 5.0)))
                .collect();
            pgrx::Spi::run(&format!(
                "INSERT INTO {tbl} VALUES ({i}, {cat}, 'doc-{i}', '[{}]')",
                v.join(",")
            ))
            .unwrap();
        }
        pgrx::Spi::run(&format!(
            "CREATE INDEX {tbl}_idx ON {tbl} USING theodb_hnsw (emb theodb_hnsw_cosine_ops)"
        ))
        .unwrap();
    }

    /// T1.1 — the unified RAG reference query (`WITH retrieved AS (WHERE <filtro> ORDER BY emb <=> $q
    /// LIMIT k) SELECT string_agg(content), count(*) FROM retrieved`) preserves the recall of its pieces:
    /// the `retrieved.id` set MUST equal the exact filtered oracle `SELECT id WHERE cat=$c ORDER BY emb
    /// <=> $q LIMIT k` (M52 discipline, mirrors `filtered_scan_preserves_recall_via_iterative`). Composing
    /// retrieval + context-assembly into one SQL must not silently drop a neighbour.
    #[pgrx::pg_test]
    fn rag_unified_query_preserves_recall() {
        seed_rag_table("rag1", 60);
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 60").unwrap();
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        // The query probe = the emb of row 0 (a real point → a real NN structure to retrieve against).
        let probe: String = pgrx::Spi::get_one("SELECT emb::text FROM rag1 WHERE id = 0").unwrap().unwrap();
        let cat0: i32 = pgrx::Spi::get_one("SELECT cat FROM rag1 WHERE id = 0").unwrap().unwrap();
        const K: i64 = 5;

        // The unified RAG query: filter (WHERE cat) → retrieve (ORDER BY emb) → assemble (string_agg).
        // We read back the retrieved ids to compare against the oracle (the assembly is over the same set).
        let unified_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                &format!(
                    "WITH retrieved AS (\
                       SELECT id, content FROM rag1 WHERE cat = {cat0} \
                       ORDER BY emb <=> '{probe}'::vector LIMIT {K}\
                     ) SELECT id FROM retrieved ORDER BY id"
                ),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });

        // The exact filtered oracle (seqscan brute force) — the recall ground truth.
        pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
        let mut oracle_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                &format!(
                    "SELECT id FROM rag1 WHERE cat = {cat0} ORDER BY emb <=> '{probe}'::vector LIMIT {K}"
                ),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        oracle_ids.sort_unstable();

        assert_eq!(
            unified_ids, oracle_ids,
            "the unified RAG query (filter+retrieve+assemble) must retrieve exactly the filtered top-k \
             oracle set (recall preserved) — unified {unified_ids:?} vs oracle {oracle_ids:?}"
        );

        // And the context-assembly itself works: string_agg over the retrieved top-k concatenates exactly
        // K docs (the assembly does not lose or duplicate rows).
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let ctx_count: i64 = pgrx::Spi::get_one(&format!(
            "WITH retrieved AS (SELECT content FROM rag1 WHERE cat = {cat0} \
             ORDER BY emb <=> '{probe}'::vector LIMIT {K}) \
             SELECT array_length(string_to_array(string_agg(content, E'\\n'), E'\\n'), 1) FROM retrieved"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(ctx_count, K, "context-assembly must concatenate exactly K retrieved docs");
    }

    /// T1.2 — read-your-writes: a row INSERTed inside a transaction is retrievable by the RAG query in the
    /// SAME transaction and the SAME MVCC snapshot (the pending region serves not-yet-folded tuples, M40/M48).
    /// A correctness property, not a latency number (blueprint §d). Rigor note: an app-layer client also gets
    /// read-your-writes if it opens an explicit transaction; the Path-1 differential is doing it in ONE SQL,
    /// ONE snapshot, without coordinating multiple client round-trips.
    #[pgrx::pg_test]
    fn rag_unified_read_your_writes() {
        seed_rag_table("rag2", 40);
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 60").unwrap();
        // Insert a NEW row whose embedding is an exact copy of an existing cluster centre → it is guaranteed
        // to be among the nearest neighbours of a probe at that centre. cat = 0 (matches the filter below).
        let probe: String = pgrx::Spi::get_one("SELECT emb::text FROM rag2 WHERE id = 0").unwrap().unwrap();
        pgrx::Spi::run(&format!(
            "INSERT INTO rag2 VALUES (99999, 0, 'fresh-doc', '{probe}'::vector)"
        ))
        .unwrap();

        // The RAG query in the SAME txn must surface the freshly-inserted row (id 99999) in its top-k.
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                &format!(
                    "WITH retrieved AS (SELECT id FROM rag2 WHERE cat = 0 \
                     ORDER BY emb <=> '{probe}'::vector LIMIT 5) SELECT id FROM retrieved"
                ),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert!(
            ids.contains(&99999),
            "the row INSERTed in this txn must be read-your-writes-visible in the RAG query (pending region) \
             — top-k ids were {ids:?}"
        );
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
