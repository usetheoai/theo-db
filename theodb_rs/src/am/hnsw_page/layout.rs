//! layout — split from the M35 page-native `hnsw_page.rs` god-file (M126, behavior-preserving;
//! byte-identical same-index A/B). Sibling items resolve via `use super::*` (re-exported in `mod.rs`).
#![allow(unused_imports)]
use super::*;
use crate::am::page;
use crate::ann::{HnswIndex, Metric};
use pgrx::pg_sys;

pub(crate) const HNSW_STRUCT_MAGIC: u32 = 0x5448_5353; // "THSS" (Theodb Hnsw StructuredScan)
pub(crate) const HNSW_STRUCT_VERSION: u32 = 1;
pub(crate) const ELEM_TAG: u8 = 1;
pub(crate) const NBR_TAG: u8 = 2;

// PostgreSQL page arithmetic (BLCKSZ 8192, standard build). `usable` excludes the page header; each item costs
// its 4-byte line pointer + the MAXALIGN'd item length. Matches how `PageAddItem` accounts free space.
pub(crate) const BLCKSZ: usize = 8192;
pub(crate) const PAGE_HEADER: usize = 24; // SizeOfPageHeaderData
pub(crate) const ITEMID: usize = 4; // sizeof(ItemIdData)
pub(crate) const USABLE: usize = BLCKSZ - PAGE_HEADER;

// Element tuple: fixed header + the raw f32 vector. `nbr_blkno/nbr_offno` point to this node's neighbor tuple.
pub(crate) const E_TAG: usize = 0;
pub(crate) const E_LEVEL: usize = 1;
// M56: bytes 2..4 were pad (kept the i64 tid 4-aligned) — always zero in v1/v2. Byte 2 = tombstone flag,
// byte 3 = version. Both fit WITHOUT changing the tuple size or any analytic address (elem_size/pack_at
// unchanged). A v1/v2 index reads pad=0 → deleted=0/version=0 (live) — backward-compatible, REINDEX optional.
// Mirrors pgvector's HnswElementTupleData `uint8 deleted; uint8 version` (hnsw.h:361-362).
pub(crate) const E_DELETED: usize = 2;
pub(crate) const E_VERSION: usize = 3;
pub(crate) const E_TID: usize = 4;
pub(crate) const E_NBR_BLK: usize = 12;
pub(crate) const E_NBR_OFF: usize = 16;
pub(crate) const E_DIM: usize = 18;
pub(crate) const E_VEC: usize = 20; // ELEM_HEADER
pub(crate) const ELEM_HEADER: usize = E_VEC;

// M59 v4 (AQ) — HOT element tuple: the code-only tuple the walk reads. It carries NO f32 (the root-cause fix of
// ADR-0019: co-locating the 4 B code with the ~3 KB f32 kept the hot working set at f32 size → paridade). The f32
// lives in a SEPARATE cold raw-f32 tuple, addressed by `E4_RAW_BLK/OFF` and read ONLY at rerank. Offsets are the
// byte layout signed off in `knowledge-base/designs/m59-v4-code-vector-separation.md` (teste de mesa).
pub(crate) const E4_TAG: usize = 0;
// byte 1 = level (mirrors E_LEVEL — the descent needs it)
pub(crate) const E4_LEVEL: usize = 1;
pub(crate) const E4_DELETED: usize = 2;
pub(crate) const E4_VERSION: usize = 3; // = HNSW_ELEM_VERSION_V4
pub(crate) const E4_TID: usize = 4;
pub(crate) const E4_NBR_BLK: usize = 12;
pub(crate) const E4_NBR_OFF: usize = 16;
pub(crate) const E4_RAW_BLK: usize = 18; // ┐ NOVO vs v3: pointer to this node's raw-f32 tuple (cold region) — rerank only
pub(crate) const E4_RAW_OFF: usize = 22; // ┘
pub(crate) const E4_DIM: usize = 24;
pub(crate) const E4_CODE: usize = 26; // ELEM_HEADER_V4 — the ⌈m/2⌉ code bytes trail here; NO f32
pub(crate) const ELEM_HEADER_V4: usize = E4_CODE;
/// The `version` byte written into a v4 hot element tuple (`E4_VERSION`). Discriminates the hot v4 tuple from a
/// v1/v2 tuple (whose byte-3 version is 0). A tombstone bumps it (`wrapping_add`), so ≥ 4 still reads as v4.
pub(crate) const HNSW_ELEM_VERSION_V4: u8 = 4;

// M59 v4 (AQ) — COLD raw-f32 tuple: the exact f32 vector, in a region SEPARATE from the hot tuples, read only when
// a survivor is reranked. `[R_TAG 0][pad 1..4][R_VEC 4..4+dim*4]`. The 4-byte header keeps the f32 payload
// 4-aligned (same discipline as the element header) so `f32::from_le_bytes` slices land on aligned boundaries.
pub(crate) const R_TAG: usize = 0;
pub(crate) const R_VEC: usize = 4; // RAW_HEADER
pub(crate) const RAW_HEADER: usize = R_VEC;
pub(crate) const RAW_TAG: u8 = 3; // distinct from ELEM_TAG(1)/NBR_TAG(2) — a raw tuple read as an element fails fast

// Neighbor tuple: header + `count` slots, each a 6-byte index pointer (blkno u32, offno u16). (0,0) = empty slot.
pub(crate) const N_TAG: usize = 0;
// byte 1 = version (0)
pub(crate) const N_COUNT: usize = 2;
pub(crate) const N_SLOTS: usize = 4; // NBR_HEADER
pub(crate) const NBR_HEADER: usize = N_SLOTS;
pub(crate) const SLOT: usize = 6; // (u32 blkno, u16 offno)

pub(crate) const fn maxalign(n: usize) -> usize {
    (n + 7) & !7
}
/// Element tuple size: header + f32 vector + optional trailing SBQ code (`code_len` bytes; 0 ⇒ v1 f32-only).
pub(crate) fn elem_size(dim: usize, code_len: usize) -> usize {
    ELEM_HEADER + dim * 4 + code_len
}
pub(crate) fn elems_per_page(dim: usize, code_len: usize) -> usize {
    (USABLE / (ITEMID + maxalign(elem_size(dim, code_len)))).max(1)
}
/// M59 v4 HOT element tuple size: header + `code_len` code bytes (NO f32). Independent of `dim` (dim is stored as
/// a u16 tag; the f32 payload lives in the cold raw region), so hundreds of hot tuples fit one page (30 B @ m=8).
pub(crate) fn elem_size_v4(code_len: usize) -> usize {
    ELEM_HEADER_V4 + code_len
}
pub(crate) fn elems_per_page_v4(code_len: usize) -> usize {
    (USABLE / (ITEMID + maxalign(elem_size_v4(code_len)))).max(1)
}
/// M59 v4 COLD raw-f32 tuple size: header + `dim` f32s. This is where the ~3 KB/vector lives — read only at rerank.
pub(crate) fn raw_size(dim: usize) -> usize {
    RAW_HEADER + dim * 4
}
pub(crate) fn raws_per_page(dim: usize) -> usize {
    (USABLE / (ITEMID + maxalign(raw_size(dim)))).max(1)
}
/// Total neighbor slots for a node at `level`: `m` per upper layer (`level` of them) + `m0` on the ground.
pub(crate) fn nbr_slots(level: usize, m: usize, m0: usize) -> usize {
    level * m + m0
}
pub(crate) fn nbr_size(level: usize, m: usize, m0: usize) -> usize {
    NBR_HEADER + nbr_slots(level, m, m0) * SLOT
}

/// An index-internal pointer to a tuple (its Postgres `(BlockNumber, OffsetNumber)`; offsets are 1-based).
pub(crate) type Addr = (u32, u16);
