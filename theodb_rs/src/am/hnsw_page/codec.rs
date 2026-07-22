//! codec — split from the M35 page-native `hnsw_page.rs` god-file (M126, behavior-preserving;
//! byte-identical same-index A/B). Sibling items resolve via `use super::*` (re-exported in `mod.rs`).
#![allow(unused_imports)]
use super::*;
use crate::am::page;
use crate::ann::{HnswIndex, Metric};
use pgrx::pg_sys;

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
pub(crate) fn encode_element_v4(
    idx: &HnswIndex,
    node: usize,
    nbr_addr: Addr,
    raw_addr: Addr,
    dim: usize,
    code: &[u8],
) -> Vec<u8> {
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
pub(crate) fn encode_raw_vec(vec: &[f32]) -> Vec<u8> {
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
pub(crate) fn encode_element(
    idx: &HnswIndex,
    node: usize,
    nbr_addr: Addr,
    dim: usize,
    code: &[u8],
) -> Vec<u8> {
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

pub(crate) fn encode_neighbors(
    idx: &HnswIndex,
    node: usize,
    elem_addr: &[Addr],
    m: usize,
    m0: usize,
) -> Vec<u8> {
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
