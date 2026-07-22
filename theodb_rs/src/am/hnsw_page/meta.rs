//! meta — split from the M35 page-native `hnsw_page.rs` god-file (M126, behavior-preserving;
//! byte-identical same-index A/B). Sibling items resolve via `use super::*` (re-exported in `mod.rs`).
#![allow(unused_imports)]
use super::*;
use crate::am::page;
use crate::ann::{HnswIndex, Metric};
use pgrx::pg_sys;

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

pub(crate) const META_LEN: usize = 4 + 4 + 1 + 4 + 2 + 2 + 4 + 2 + 2 + 4 + 4 + 4 + 4 + 4; // = 45 bytes (v1 core)
pub(crate) const HNSW_STRUCT_VERSION_SBQ: u32 = 2; // M51 layout v2: same core header + trailing [sbq_bits:u8][cb_len:u32][codebook]
pub(crate) const HNSW_STRUCT_VERSION_AQ: u32 = 3; // M59 layout v3: same core header + trailing AQ descriptor (codebook on pages)
pub(crate) const HNSW_STRUCT_VERSION_V4: u32 = 4; // M59 layout v4: v3 AQ descriptor + raw-f32 region descriptor (code/vec split)
/// M59 fix: the v3 meta trailer is a FIXED 13-byte descriptor `[aq_m:u8][cb_len:u32][cb_first:u32][cb_npages:u32]`.
/// Unlike v2 (SBQ codebook inline — always ≤ a few hundred bytes, fits one page), the AQ codebook is ~48 KB at
/// dim=768 and lives on dedicated pages; the meta only points at them. So the whole v3 meta ITEM is always tiny.
pub(crate) const AQ_DESC_LEN: usize = 1 + 4 + 4 + 4;
/// M59 v4: the v4 meta trailer extends the v3 AQ descriptor with the raw-f32 region pointer
/// `[…v3 13 B…][raw_first:u32][raw_npages:u32]` = 21 bytes. Still tiny (dim-independent) → never overflows a page.
pub(crate) const V4_DESC_LEN: usize = AQ_DESC_LEN + 4 + 4;
/// Bytes of AQ codebook per dedicated page (one item per page). Same budget as the IVF blob chunk (`page::CHUNK`):
/// BLCKSZ − header − item-id − alignment slack; 8000 is safe. At dim=768 the ~48 KB codebook needs ⌈48 KB/8000⌉=7
/// pages. Kept local (parsimony) — `page.rs`'s `CHUNK` is private and this is a different layer's constant.
pub(crate) const CB_CHUNK: usize = 8000;

/// Encode the meta item. The version slot (bytes 4..8) is the discriminator: **v1** (f32-only, `sbq_bits==0` and
/// `aq_m==0`) is the byte-identical 45-byte core (legacy indexes + f32 builds unchanged, existing tests stay
/// green); **v2** (`sbq_bits>0`, M51) is the core + `[sbq_bits:u8][cb_len:u32][codebook]` (SBQ codebook inline —
/// it is small); **v3** (`aq_m>0`, M59) is the core + the fixed 13-byte AQ DESCRIPTOR
/// `[aq_m:u8][cb_len:u32][cb_first:u32][cb_npages:u32]` — the codebook itself is NOT inline (it lives on the
/// dedicated codebook pages `[aq_cb_first, aq_cb_first+aq_cb_npages)`; ~48 KB at dim=768 overflows one page). AQ
/// and SBQ are mutually exclusive (an AQ index has `sbq_bits==0`), so exactly one trailer is emitted; the version
/// tells the reader which one. The v3 meta item is always tiny — no dependence on `dim`, so it can never overflow.
pub(crate) fn encode_meta(m: &HnswMeta) -> Vec<u8> {
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
pub(crate) fn decode_trailer(b: &[u8], label: &str) -> Result<(u8, Vec<u8>), String> {
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
pub(crate) struct AqDescriptor {
    pub(crate) aq_m: u8,
    pub(crate) cb_len: usize,
    pub(crate) cb_first: u32,
    pub(crate) cb_npages: u32,
    pub(crate) raw_first: u32,
    pub(crate) raw_npages: u32,
}

/// Parse the v3 AQ DESCRIPTOR `[aq_m:u8][cb_len:u32][cb_first:u32][cb_npages:u32]` (M59 fix), OR the v4 descriptor
/// which appends `[raw_first:u32][raw_npages:u32]` (the cold raw-f32 region). `is_v4` selects the expected length.
/// The codebook bytes are NOT here — they live on `[cb_first, cb_first+cb_npages)`; the FFI `read_meta` reassembles
/// them. Typed `Err` on truncation (Rule 8) — never a slice panic across the C boundary.
pub(crate) fn decode_aq_descriptor(b: &[u8], is_v4: bool) -> Result<AqDescriptor, String> {
    let want = META_LEN + if is_v4 { V4_DESC_LEN } else { AQ_DESC_LEN };
    if b.len() != want {
        return Err(format!(
            "theodb hnsw: {} descriptor length mismatch (expected {want}, have {})",
            if is_v4 { "v4 AQ" } else { "v3 AQ" },
            b.len()
        ));
    }
    let u32a = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let (raw_first, raw_npages) =
        if is_v4 { (u32a(META_LEN + 13), u32a(META_LEN + 17)) } else { (0, 0) };
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
        return Err(
            "theodb hnsw: bad structured meta magic (REINDEX to upgrade the M26 blob to the M35 \
                    page-native format)"
                .into(),
        );
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
