//! E2 T1.1 — `theodb_symqg` persisted co-located page layout (own-code, clean-room from arXiv:2411.12229).
//! Lives under `am/page/` (like `ivf.rs`) so it reaches the private page-write helpers (`write_item`,
//! `write_chunks`, `read_chunked`, `read_page_item_at`, `npages_for`) via `use super::*`.
//!
//! Design (SymphonyQG's per-vertex row co-locates RawData + neighbour codes so one page read serves both the
//! popped vertex's EXACT distance AND all its neighbour estimates). Layout:
//! ```text
//! block 0: SymqgMeta (write_item)
//! rotation codebook (RabitqQuantizer::to_meta_bytes; write_chunks)
//! directory: n × (row_first_block:u32, row_npages:u32)          — locates each vertex row by ORDINAL
//! tids:      n × i64                                            — vertex ordinal → heap tid (result mapping)
//! rows:      n × pack_row bytes (chunked; a high-dim×degree row MAY span pages — EC-2)
//! ```
//! Per-vertex row (`degree_bound R` a multiple of 32; padded slots carry `SENTINEL_ORD`, skipped at scan):
//! ```text
//! [dim × f32 rot vector] [R × u32 neighbour ORDINALS] [R × ⌈dim/8⌉ sign-bit bytes] [R × (nr:f32, w:f32) factors]
//! ```
//! Traversal uses ORDINALS (to read a neighbour's row via the directory); the final top-k ordinals map to tids
//! via the `tids` region. The rot vector is stored per vertex so the popped centre's exact distance is a local
//! read (no heap fetch); exact dist = ‖rot_q − rot‖² (rotation preserves norm), and q_r reuses the same subtraction.
use super::*;
use crate::ann::symqg_spike::SignCode;

pub(crate) const SYMQG_MAGIC: u32 = 0x5351_4753; // "SQGS" — Theodb SymphonyQG Structured
pub(crate) const SYMQG_VERSION: u32 = 1;
/// Padding ordinal for a vertex with < R real neighbours; skipped at scan.
pub(crate) const SENTINEL_ORD: u32 = u32::MAX;

/// Parsed meta header. `entry` is the graph entry-point vertex ordinal; `gen_base` is the first block of the
/// generation (relocatable-fold ready, mirrors the IVF v3 `gen_base` convention).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SymqgMeta {
    pub metric_tag: u8,
    pub dim: u32,
    pub degree_bound: u32,
    pub n: u32,
    pub entry: u32,
    pub gen_base: u32,
    pub rot_codebook_npages: u32,
    pub dir_npages: u32,
    pub tids_npages: u32,
}

impl SymqgMeta {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(41);
        b.extend_from_slice(&SYMQG_MAGIC.to_le_bytes());
        b.extend_from_slice(&SYMQG_VERSION.to_le_bytes());
        b.push(self.metric_tag);
        b.extend_from_slice(&self.dim.to_le_bytes());
        b.extend_from_slice(&self.degree_bound.to_le_bytes());
        b.extend_from_slice(&self.n.to_le_bytes());
        b.extend_from_slice(&self.entry.to_le_bytes());
        b.extend_from_slice(&self.gen_base.to_le_bytes());
        b.extend_from_slice(&self.rot_codebook_npages.to_le_bytes());
        b.extend_from_slice(&self.dir_npages.to_le_bytes());
        b.extend_from_slice(&self.tids_npages.to_le_bytes());
        b
    }
    pub(crate) fn decode(b: &[u8]) -> Result<SymqgMeta, String> {
        if b.len() < 41 {
            return Err(format!("theodb_symqg: truncated meta ({} < 41 bytes)", b.len()));
        }
        let magic = u32::from_le_bytes(b[0..4].try_into().unwrap());
        if magic != SYMQG_MAGIC {
            return Err("theodb_symqg: bad meta magic (not a symqg index — REINDEX)".into());
        }
        let version = u32::from_le_bytes(b[4..8].try_into().unwrap());
        if version != SYMQG_VERSION {
            return Err(format!("theodb_symqg: unsupported version v{version} — REINDEX to v{SYMQG_VERSION}"));
        }
        Ok(SymqgMeta {
            metric_tag: b[8],
            dim: u32::from_le_bytes(b[9..13].try_into().unwrap()),
            degree_bound: u32::from_le_bytes(b[13..17].try_into().unwrap()),
            n: u32::from_le_bytes(b[17..21].try_into().unwrap()),
            entry: u32::from_le_bytes(b[21..25].try_into().unwrap()),
            gen_base: u32::from_le_bytes(b[25..29].try_into().unwrap()),
            rot_codebook_npages: u32::from_le_bytes(b[29..33].try_into().unwrap()),
            dir_npages: u32::from_le_bytes(b[33..37].try_into().unwrap()),
            tids_npages: u32::from_le_bytes(b[37..41].try_into().unwrap()),
        })
    }
}

/// A decoded vertex row: its ROTATED vector P·x (‖q−x‖²=‖rot_q−rot‖², so exact-dist + q_r are one subtraction) + its real (sentinel-skipped) neighbours as `(ordinal, sign_code)`.
pub(crate) struct SymqgRow {
    pub rot: Vec<f32>,
    pub neighbours: Vec<(u32, SignCode)>,
}

/// Bytes per row: `dim·4 (rot P·x) + R·4 (ordinals) + R·⌈dim/8⌉ (signs) + R·8 (factors)`.
#[inline]
pub(crate) fn row_bytes(dim: usize, degree: usize) -> usize {
    let sign_bytes = dim.div_ceil(8);
    dim * 4 + degree * 4 + degree * sign_bytes + degree * 8
}

/// Pack one vertex's rot vector + neighbours (already padded to exactly `degree`; sentinel slots use
/// `(SENTINEL_ORD, zero-code)`) into the row byte layout.
pub(crate) fn pack_row(rot: &[f32], neighbours: &[(u32, SignCode)], dim: usize, degree: usize) -> Vec<u8> {
    debug_assert_eq!(rot.len(), dim);
    debug_assert_eq!(neighbours.len(), degree);
    let sign_bytes = dim.div_ceil(8);
    let mut b = Vec::with_capacity(row_bytes(dim, degree));
    for &x in rot { // the stored rotated vector P·x
        b.extend_from_slice(&x.to_le_bytes());
    }
    for (ord, _) in neighbours {
        b.extend_from_slice(&ord.to_le_bytes());
    }
    for (_, code) in neighbours {
        let mut bits = vec![0u8; sign_bytes];
        for (d, &u) in code.u.iter().enumerate().take(dim) {
            if u > 0 {
                bits[d / 8] |= 1u8 << (d % 8); // +1 → bit set, −1 → clear
            }
        }
        b.extend_from_slice(&bits);
    }
    for (_, code) in neighbours {
        b.extend_from_slice(&code.nr.to_le_bytes());
        b.extend_from_slice(&code.w.to_le_bytes());
    }
    b
}

/// Decode a row into its rot vector + real neighbours (sentinel slots SKIPPED). Typed `Err` on a short/corrupt
/// slice (EC-7 — never a panic/OOB).
pub(crate) fn decode_row(b: &[u8], dim: usize, degree: usize) -> Result<SymqgRow, String> {
    let need = row_bytes(dim, degree);
    if b.len() < need {
        return Err(format!("theodb_symqg: truncated row ({} < {} bytes)", b.len(), need));
    }
    let sign_bytes = dim.div_ceil(8);
    let rot: Vec<f32> = (0..dim).map(|d| f32::from_le_bytes(b[d * 4..d * 4 + 4].try_into().unwrap())).collect();
    let ord_base = dim * 4;
    let signs_base = ord_base + degree * 4;
    let factors_base = signs_base + degree * sign_bytes;
    let mut neighbours = Vec::with_capacity(degree);
    for j in 0..degree {
        let ord = u32::from_le_bytes(b[ord_base + j * 4..ord_base + j * 4 + 4].try_into().unwrap());
        if ord == SENTINEL_ORD {
            continue; // padding slot
        }
        let sbase = signs_base + j * sign_bytes;
        let mut u = Vec::with_capacity(dim);
        for d in 0..dim {
            let bit = (b[sbase + d / 8] >> (d % 8)) & 1;
            u.push(if bit == 1 { 1i8 } else { -1i8 });
        }
        let fbase = factors_base + j * 8;
        let nr = f32::from_le_bytes(b[fbase..fbase + 4].try_into().unwrap());
        let w = f32::from_le_bytes(b[fbase + 4..fbase + 8].try_into().unwrap());
        neighbours.push((ord, SignCode { u, nr, w }));
    }
    Ok(SymqgRow { rot, neighbours })
}

/// Write the whole index: meta + rotation codebook + directory + tids + rows. Mirrors `write_ivf_aq_split_sq8`'s
/// cursor/dir discipline. `rows[i]` is the packed row bytes for vertex ordinal `i` (from `pack_row`); a row may
/// exceed one page (EC-2) — `write_chunks` + the per-vertex directory handle multi-page rows. `tids[i]` is the
/// heap tid of vertex ordinal `i`.
pub(crate) unsafe fn pack_symqg(
    rel: pg_sys::Relation,
    metric_tag: u8,
    dim: u32,
    degree_bound: u32,
    entry: u32,
    rot_codebook: &[u8],
    tids: &[i64],
    rows: &[Vec<u8>],
) { unsafe {
    let base: u32 = 1;
    let n = rows.len() as u32;
    let rot_npages = npages_for(rot_codebook.len());
    let dir_npages = npages_for(n as usize * 8); // n × (first_block:u32, npages:u32)
    let tids_npages = npages_for(n as usize * 8); // n × i64
    let mut cursor = base + rot_npages + dir_npages + tids_npages;
    let mut dir = Vec::with_capacity(n as usize * 8);
    for row in rows {
        let fb = cursor;
        let np = npages_for(row.len());
        cursor += np;
        dir.extend_from_slice(&fb.to_le_bytes());
        dir.extend_from_slice(&np.to_le_bytes());
    }
    let mut tid_bytes = Vec::with_capacity(n as usize * 8);
    for &t in tids {
        tid_bytes.extend_from_slice(&t.to_le_bytes());
    }
    let meta = SymqgMeta {
        metric_tag,
        dim,
        degree_bound,
        n,
        entry,
        gen_base: base,
        rot_codebook_npages: rot_npages,
        dir_npages,
        tids_npages,
    };
    write_item(rel, &meta.encode());
    write_chunks(rel, rot_codebook);
    write_chunks(rel, &dir);
    write_chunks(rel, &tid_bytes);
    for row in rows {
        write_chunks(rel, row);
    }
}}

/// Read + parse the meta page (block 0).
pub(crate) unsafe fn read_symqg_meta(rel: pg_sys::Relation) -> Result<SymqgMeta, String> {
    let m = unsafe { read_page_item(rel, 0) }?; // block 0's single item (1-based offset handled by the helper)
    SymqgMeta::decode(&m)
}

/// The per-vertex directory `(first_block, npages)` for all `n` vertices.
pub(crate) unsafe fn read_symqg_dir(rel: pg_sys::Relation, meta: &SymqgMeta) -> Result<Vec<(u32, u32)>, String> {
    let dir_base = meta.gen_base + meta.rot_codebook_npages;
    let bytes = unsafe { read_chunked(rel, dir_base, meta.dir_npages) }?;
    if bytes.len() < meta.n as usize * 8 {
        return Err("theodb_symqg: truncated directory".into());
    }
    let mut out = Vec::with_capacity(meta.n as usize);
    for i in 0..meta.n as usize {
        let o = i * 8;
        let fb = u32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let np = u32::from_le_bytes(bytes[o + 4..o + 8].try_into().unwrap());
        out.push((fb, np));
    }
    Ok(out)
}

/// The ordinal → heap-tid table.
pub(crate) unsafe fn read_symqg_tids(rel: pg_sys::Relation, meta: &SymqgMeta) -> Result<Vec<i64>, String> {
    let tids_base = meta.gen_base + meta.rot_codebook_npages + meta.dir_npages;
    let bytes = unsafe { read_chunked(rel, tids_base, meta.tids_npages) }?;
    if bytes.len() < meta.n as usize * 8 {
        return Err("theodb_symqg: truncated tids region".into());
    }
    Ok((0..meta.n as usize)
        .map(|i| i64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap()))
        .collect())
}

/// Read + decode one vertex's row. `(first_block, npages)` come from the directory.
pub(crate) unsafe fn read_symqg_row(
    rel: pg_sys::Relation,
    first_block: u32,
    npages: u32,
    dim: usize,
    degree: usize,
) -> Result<SymqgRow, String> {
    let bytes = unsafe { read_chunked(rel, first_block, npages) }?;
    decode_row(&bytes, dim, degree)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(u: Vec<i8>, nr: f32, w: f32) -> SignCode {
        SignCode { u, nr, w }
    }

    #[test]
    fn symqg_page_meta_round_trips() {
        let m = SymqgMeta {
            metric_tag: 0,
            dim: 128,
            degree_bound: 32,
            n: 1000,
            entry: 42,
            gen_base: 1,
            rot_codebook_npages: 3,
            dir_npages: 2,
            tids_npages: 2,
        };
        assert_eq!(SymqgMeta::decode(&m.encode()).unwrap(), m);
    }

    #[test]
    fn symqg_page_meta_rejects_bad_magic_and_short() {
        assert!(SymqgMeta::decode(&[0u8; 10]).is_err());
        let mut b = SymqgMeta { metric_tag: 0, dim: 4, degree_bound: 32, n: 1, entry: 0, gen_base: 1, rot_codebook_npages: 0, dir_npages: 0, tids_npages: 0 }.encode();
        b[0] ^= 0xFF;
        assert!(SymqgMeta::decode(&b).is_err());
    }

    #[test]
    fn symqg_page_row_round_trips() {
        let (dim, degree) = (12, 32);
        let rot: Vec<f32> = (0..dim).map(|d| d as f32 * 0.5).collect();
        let mut nbrs: Vec<(u32, SignCode)> = vec![
            (7, code((0..dim).map(|d| if d % 2 == 0 { 1 } else { -1 }).collect(), 3.5, 2.1)),
            (99, code((0..dim).map(|d| if d % 3 == 0 { 1 } else { -1 }).collect(), 1.0, 0.8)),
        ];
        while nbrs.len() < degree {
            nbrs.push((SENTINEL_ORD, code(vec![0i8; dim], 0.0, 0.0)));
        }
        let packed = pack_row(&rot, &nbrs, dim, degree);
        assert_eq!(packed.len(), row_bytes(dim, degree));
        let got = decode_row(&packed, dim, degree).unwrap();
        assert_eq!(got.rot, rot, "rot vector survives");
        assert_eq!(got.neighbours.len(), 2, "sentinel slots skipped");
        assert_eq!(got.neighbours[0].0, 7);
        assert_eq!(got.neighbours[1].0, 99);
        assert_eq!(got.neighbours[0].1.u, nbrs[0].1.u);
        assert!((got.neighbours[0].1.nr - 3.5).abs() < 1e-6 && (got.neighbours[0].1.w - 2.1).abs() < 1e-6);
    }

    #[test]
    fn symqg_page_pads_partial_degree() {
        let (dim, degree) = (8, 32);
        let rot = vec![0.0f32; dim];
        let nbrs: Vec<(u32, SignCode)> = (0..degree).map(|_| (SENTINEL_ORD, code(vec![0i8; dim], 0.0, 0.0))).collect();
        let got = decode_row(&pack_row(&rot, &nbrs, dim, degree), dim, degree).unwrap();
        assert!(got.neighbours.is_empty());
    }

    #[test]
    fn symqg_page_row_spans_multiple_pages_bytes() {
        // EC-2: a dim=768/R=128 row is > 8KB — the byte codec must round-trip regardless of page size.
        let (dim, degree) = (768, 128);
        assert!(row_bytes(dim, degree) > 8192, "row must exceed one page for this EC");
        let rot: Vec<f32> = (0..dim).map(|d| d as f32).collect();
        let nbrs: Vec<(u32, SignCode)> = (0..degree)
            .map(|j| (j as u32, code((0..dim).map(|d| if (d + j) % 2 == 0 { 1 } else { -1 }).collect(), j as f32, 1.0)))
            .collect();
        let got = decode_row(&pack_row(&rot, &nbrs, dim, degree), dim, degree).unwrap();
        assert_eq!(got.rot.len(), dim);
        assert_eq!(got.neighbours.len(), degree);
        assert_eq!(got.neighbours[5].1.u, nbrs[5].1.u);
    }

    #[test]
    fn symqg_decode_truncated_row_errs() {
        // EC-7: a slice shorter than the declared row → typed Err, never a panic/OOB.
        let (dim, degree) = (16, 32);
        let short = vec![0u8; row_bytes(dim, degree) - 1];
        assert!(decode_row(&short, dim, degree).is_err());
    }
}
