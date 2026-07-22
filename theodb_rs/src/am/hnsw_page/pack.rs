//! pack — split from the M35 page-native `hnsw_page.rs` god-file (M126, behavior-preserving;
//! byte-identical same-index A/B). Sibling items resolve via `use super::*` (re-exported in `mod.rs`).
#![allow(unused_imports)]
use super::*;
use crate::am::page;
use crate::ann::{HnswIndex, Metric};
use pgrx::pg_sys;

/// Resolve the whole in-memory graph into meta + page images (ADR-2 — no I/O, unit-testable). Returns `Err` if a
/// neighbor tuple would exceed one page (impossible under the build's level cap, asserted here defensively).
pub(crate) fn pack(idx: &HnswIndex) -> Result<Packed, String> {
    // The initial build / buildempty writes a contiguous generation starting right after the meta (block 1).
    pack_at(idx, 1, 0)
}

/// Which trailing per-node code (if any) a `pack` writes inline + which meta trailer it emits. `None` ⇒ v1
/// (f32-only), `Sbq` ⇒ v2 (M51), `Aq` ⇒ v3 (M59). AQ and SBQ are mutually exclusive per index (D1).
pub(crate) enum CodeKind {
    None,
    Sbq { bits: u8 },
    Aq { m: usize, bits: u8, aq_threshold: f32 },
}

/// The trained per-node codes + the two meta-trailer slots for a given `CodeKind`. `code_len == 0` ⇒ v1.
/// Exactly one of (`sbq_bits`,`codebook`) / (`aq_m`,`aq_codebook`) is non-default — never both (D1).
pub(crate) struct CodeSpec {
    code_len: usize,
    codes: Vec<Vec<u8>>,
    sbq_bits: u8,
    codebook: Vec<u8>,
    aq_m: u8,
    aq_codebook: Vec<u8>,
}

/// Train the quantizer for `kind` over the graph's live vectors and emit one inline code per node + the meta
/// trailer bytes. Called once per pack; `CodeKind::None` yields the zero spec (v1 f32-only, byte-identical).
pub(crate) fn train_codes(idx: &HnswIndex, kind: &CodeKind) -> Result<CodeSpec, String> {
    let n = idx.node_count();
    let dim = idx.dim();
    match kind {
        CodeKind::None => Ok(CodeSpec {
            code_len: 0,
            codes: Vec::new(),
            sbq_bits: 0,
            codebook: Vec::new(),
            aq_m: 0,
            aq_codebook: Vec::new(),
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
                codes,
                sbq_bits: *bits,
                codebook: q.to_meta_bytes(),
                aq_m: 0,
                aq_codebook: Vec::new(),
            })
        }
        // M59 T3.1: train the anisotropic PQ, emit each node's ⌈m/2⌉-byte 4-bit code + the codebook. The seed is
        // fixed (deterministic build, mirrors SBQ's parameter-free train — the fold re-trains identically).
        CodeKind::Aq { m, bits, aq_threshold } => {
            let vecs: Vec<Vec<f32>> = (0..n).map(|i| idx.node_vector(i).to_vec()).collect();
            let q =
                crate::vec::aq::AqQuantizer::train(&vecs, *m, *bits, *aq_threshold, AQ_BUILD_SEED)?;
            let codes: Vec<Vec<u8>> = vecs.iter().map(|v| q.encode(v)).collect();
            Ok(CodeSpec {
                code_len: crate::vec::aq::AqQuantizer::bytes_per_vector(dim, *m),
                codes,
                sbq_bits: 0,
                codebook: Vec::new(),
                aq_m: *m as u8,
                aq_codebook: q.to_meta_bytes(),
            })
        }
    }
}

/// Fixed training seed so a v3 build (and every VACUUM re-fold of it) produces a byte-identical AQ codebook from
/// the same live corpus — the deterministic-build / relocatable-fold invariant (D1, mirrors SBQ's parameterless
/// deterministic train). Chosen arbitrarily; only its stability across folds matters.
pub(crate) const AQ_BUILD_SEED: u64 = 0x5943_4E41; // "ANCY" — anisotropic build.

/// Like [`pack`] but emits **layout v2**: trains an SBQ quantizer from the graph's vectors, persists the codebook
/// in the meta, and writes each node's compact SBQ code inline after its f32 vector (M51 T1.1/T2.1). `sbq_bits==0`
/// is identical to [`pack`].
pub(crate) fn pack_sbq(idx: &HnswIndex, sbq_bits: u8) -> Result<Packed, String> {
    pack_at(idx, 1, sbq_bits)
}

/// Like [`pack`] but emits **layout v4** (M59 — the code/vector separation of ADR-0019): trains an
/// [`crate::vec::aq::AqQuantizer`], persists the codebook on dedicated pages, and writes each node's HOT element
/// tuple (`⌈m/2⌉`-byte 4-bit code + `raw_addr`, **no f32**) plus a SEPARATE cold raw-f32 tuple linked by `raw_addr`.
/// This is the fix that shrinks the walk's hot working set (30 B/node vs ~3 KB): the f32 leaves the hot path and is
/// read only at rerank. `m == 0` falls back to the v1 f32-only pack. Position-independent (`base`) so the fold
/// relocates it for free.
///
/// M59 T3.3: wired into production — `ambuild_hnsw` (`pack_hnsw_for_build`, initial build reads the reloption)
/// and the VACUUM compaction fold (`pack_fold_layout`, reads the AQ params off the persisted meta so a fold
/// re-quantizes identically).
pub(crate) fn pack_aq(
    idx: &HnswIndex,
    base: usize,
    m: usize,
    bits: u8,
    aq_threshold: f32,
) -> Result<Packed, String> {
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
pub(crate) fn pack_v4(idx: &HnswIndex, base: usize, kind: &CodeKind) -> Result<Packed, String> {
    let (metric, m, m0, _ef) = idx.params();
    let n = idx.node_count();
    let dim = idx.dim();

    let CodeSpec { code_len, codes, aq_m, aq_codebook, .. } = train_codes(idx, kind)?;
    debug_assert!(aq_m != 0 && code_len > 0, "pack_v4 is the AQ path — code must be present");

    // 1. Analytic HOT element addresses (fixed size = header + code, dim-independent ⇒ hundreds per page).
    let ipp = elems_per_page_v4(code_len);
    let elem_npages = n.div_ceil(ipp);
    let elem_addr: Vec<Addr> =
        (0..n).map(|i| ((base + i / ipp) as u32, (1 + i % ipp) as u16)).collect();
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
            return Err(format!(
                "theodb hnsw: neighbor tuple for a level-{level} node exceeds one page \
                                ({size} B) — build must cap max level"
            ));
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
    let raw_addr: Vec<Addr> =
        (0..n).map(|i| ((raw_first + i / rpp) as u32, (1 + i % rpp) as u16)).collect();
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
                .map(|&node| {
                    encode_element_v4(idx, node, nbr_addr[node], raw_addr[node], dim, &codes[node])
                })
                .collect(),
        );
    }

    // 6. Meta: entry point → its hot element addr, plus the AQ codebook descriptor AND the raw-f32 region pointer.
    let entry_node = idx.entry().ok_or("theodb hnsw: non-empty graph without an entry point")?;
    let (eb, eo) = elem_addr[entry_node];
    let meta = encode_meta(&HnswMeta {
        metric_tag: metric.tag(),
        dim: dim as u32,
        m: m as u16,
        m0: m0 as u16,
        entry_blkno: eb,
        entry_offno: eo,
        entry_level: idx.node_level(entry_node) as i16,
        node_count: n as u32,
        elem_first: base as u32,
        elem_npages: elem_npages as u32,
        nbr_first: nbr_first as u32,
        nbr_npages: nbr_npages as u32,
        sbq_bits: 0,
        codebook: Vec::new(),
        aq_m,
        aq_codebook,
        aq_cb_first: cb_first as u32,
        aq_cb_npages: aq_cb_npages as u32,
        raw_first: raw_first as u32,
        raw_npages: raw_npages as u32,
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
pub(crate) fn pack_kind(idx: &HnswIndex, base: usize, kind: &CodeKind) -> Result<Packed, String> {
    let (metric, m, m0, _ef) = idx.params();
    let n = idx.node_count();
    let dim = idx.dim();

    // Empty graph: meta only, entry_level = -1. `base` is irrelevant (no body pages) — record it anyway so
    // pending_start (= nbr_first + nbr_npages = base) is consistent with a non-empty generation at `base`.
    // An empty index has no vectors to train the quantizer on, so it stays v1 (a code arrives on the first fold
    // after data lands — REINDEX/VACUUM).
    if n == 0 {
        let meta = encode_meta(&HnswMeta {
            metric_tag: metric.tag(),
            dim: dim as u32,
            m: m as u16,
            m0: m0 as u16,
            entry_blkno: 0,
            entry_offno: 0,
            entry_level: -1,
            node_count: 0,
            elem_first: base as u32,
            elem_npages: 0,
            nbr_first: base as u32,
            nbr_npages: 0,
            sbq_bits: 0,
            codebook: Vec::new(),
            aq_m: 0,
            aq_codebook: Vec::new(),
            aq_cb_first: 0,
            aq_cb_npages: 0,
            raw_first: 0,
            raw_npages: 0,
        });
        return Ok(Packed { meta, pages: Vec::new() });
    }

    // pack_kind now serves ONLY v1 (None) and v2 (SBQ). The AQ path is v4 (code/vec split) — routed through
    // `pack_v4` by `pack_aq`. A stray `Aq` kind reaching here is a wiring bug, not a runtime input, so it is a
    // typed Err (Rule 8) rather than silently emitting a v3-shaped (co-located) tuple.
    if matches!(kind, CodeKind::Aq { .. }) {
        return Err("theodb hnsw: internal — AQ must be packed via pack_v4 (v4 code/vector split), not pack_kind".into());
    }
    let CodeSpec { code_len, codes, sbq_bits, codebook, aq_m, aq_codebook } =
        train_codes(idx, kind)?;

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
            return Err(format!(
                "theodb hnsw: neighbor tuple for a level-{level} node exceeds one page \
                                ({size} B) — build must cap max level"
            ));
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
        metric_tag: metric.tag(),
        dim: dim as u32,
        m: m as u16,
        m0: m0 as u16,
        entry_blkno: eb,
        entry_offno: eo,
        entry_level: idx.node_level(entry_node) as i16,
        node_count: n as u32,
        elem_first: base as u32,
        elem_npages: elem_npages as u32,
        nbr_first: nbr_first as u32,
        nbr_npages: nbr_npages as u32,
        // `train_codes` already zeroes the code kind for v1; pass the spec through unchanged (D1: at most one
        // of sbq_bits / aq_m is non-zero, so `encode_meta` emits exactly one trailer).
        sbq_bits,
        codebook,
        aq_m,
        aq_codebook,
        aq_cb_first: if aq_m != 0 { cb_first as u32 } else { 0 },
        aq_cb_npages: if aq_m != 0 { aq_cb_npages as u32 } else { 0 },
        // v1/v2 have no separate raw-f32 region (the f32 lives inline in the element tuple); v4 (AQ) is packed by
        // `pack_v4`, never here.
        raw_first: 0,
        raw_npages: 0,
    });

    let mut pages = elem_pages;
    pages.extend(nbr_pages);
    pages.extend(cb_pages);
    Ok(Packed { meta, pages })
}

/// Split the AQ codebook into one-item-per-page images (`≤ CB_CHUNK` bytes each). Empty codebook ⇒ no pages (v1/v2
/// carry no AQ codebook pages). Each returned `Vec<Vec<u8>>` is a page holding exactly one codebook chunk item,
/// matching the `Packed.pages` shape the WAL writer consumes. Read back by [`read_codebook_pages`], concatenated.
pub(crate) fn codebook_pages(codebook: &[u8]) -> Vec<Vec<Vec<u8>>> {
    if codebook.is_empty() {
        return Vec::new();
    }
    codebook.chunks(CB_CHUNK).map(|chunk| vec![chunk.to_vec()]).collect()
}

// ---------------------------------------------------------------------------------------------------------------
// FFI: write the packed images to WAL-logged pages, read the meta, and traverse the graph on demand.
// ---------------------------------------------------------------------------------------------------------------
