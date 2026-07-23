//! IVF/AQ on-disk format encode/decode (M31 v3 → M60/M69 v5/v6/v7). Split out of `page.rs` (M104,
//! Boundaries): the structured IVF-list + AQ/SQ8 codebook + per-list record layout is a cohesive format
//! concern, distinct from the generic page/buffer/WAL primitives in the parent module. As a descendant it
//! reaches the parent's private primitives (`write_chunks`, `read_chunked`, `read_page_item*`, `peek_magic`,
//! `main_index_pages`, …) via `use super::*` — no visibility widening needed.
use super::*;
use pgrx::pg_sys;

pub(crate) const IVF_STRUCT_MAGIC: u32 = 0x5449_5653; // "TIVS" — structured IVFFlat (M31)
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
/// Build the ordered page-item sequence for the structured layout (meta · centroid chunks · per-list chunks).
/// Each item is one page. The meta's directory references the sequential block numbers (identical whether the
/// items are then extended into a fresh relation or reinit-ed in place).
fn structured_page_items(
    base: u32,
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

    // The per-list directory lives on its OWN chunked page range (M34) — NOT inline on the meta page — so `lists`
    // is no longer bounded by a single page (CHUNK=8000 → ~665 lists at 12 B/entry). M48 (v3): the generation body
    // starts at `base` (block 0 is the fixed meta/pivot page; `base==1` for the initial contiguous build, `base`
    // == tail/reclaimed-region for a crash-safe fold). Layout: [block 0 meta] · gen_base: dir pages · centroid
    // pages · per-list pages. The dir's per-list first_block cursors are ABSOLUTE, resolved from `base` here.
    let dir_npages = npages_for(nlists as usize * 12);
    let mut cursor = base + dir_npages + centroid_npages;
    let mut dir: Vec<(u32, u32, u32)> = Vec::with_capacity(lists.len());
    for (i, enc) in encoded.iter().enumerate() {
        let np = npages_for(enc.len());
        dir.push((cursor, np, lists[i].len() as u32));
        cursor += np;
    }
    let mut dirbytes = Vec::with_capacity(dir.len() * 12);
    for (fb, np, cnt) in &dir {
        dirbytes.extend_from_slice(&fb.to_le_bytes());
        dirbytes.extend_from_slice(&np.to_le_bytes());
        dirbytes.extend_from_slice(&cnt.to_le_bytes());
    }

    // Meta header (block 0), fixed 29 bytes: magic · ver=3 · metric · dim · nlists · dir_npages · centroid_npages
    // · gen_base (M48). v3 adds gen_base so the generation body is relocatable for the crash-safe fold; v2 (M34)
    // is still readable with an implicit gen_base of 1 (auto-migrated to v3 on the first VACUUM fold).
    let mut meta = Vec::with_capacity(29);
    meta.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    meta.extend_from_slice(&3u32.to_le_bytes()); // format v3 — relocatable generation (M48 issue #47)
    meta.push(metric_tag);
    meta.extend_from_slice(&dim.to_le_bytes());
    meta.extend_from_slice(&nlists.to_le_bytes());
    meta.extend_from_slice(&dir_npages.to_le_bytes());
    meta.extend_from_slice(&centroid_npages.to_le_bytes());
    meta.extend_from_slice(&base.to_le_bytes());

    // One page-item per page: meta · dir chunks · centroid chunks · each list's chunks.
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
    push_chunks(&mut items, &dirbytes);
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
    // Initial build: contiguous generation right after the meta page (base = block 1).
    for item in structured_page_items(1, dim, metric_tag, centroids, lists) {
        extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, &item);
    }
}
/// Build the IVFFlat structured page items for a generation based at `base` (M48 crash-safe fold). The caller
/// (`fold::fold`) writes item 0 (meta, carrying gen_base) to block 0 LAST and items 1.. to `base..`.
pub(crate) fn ivf_structured_items(
    base: u32,
    dim: u32,
    metric_tag: u8,
    centroids: &[Vec<f32>],
    lists: &[Vec<(i64, Vec<f32>)>],
) -> Vec<Vec<u8>> {
    structured_page_items(base, dim, metric_tag, centroids, lists)
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
    if m.len() < 25 {
        return Err("theodb ivf: truncated structured meta".into());
    }
    if u32::from_le_bytes(m[0..4].try_into().unwrap()) != IVF_STRUCT_MAGIC {
        return Err("theodb ivf: bad structured meta magic".into());
    }
    // v2 (M34) is read with an implicit gen_base of 1 (contiguous from block 1); v3 (M48) carries an explicit
    // gen_base so the generation can live at a relocated offset after a crash-safe fold. Anything else → REINDEX.
    let ver = u32::from_le_bytes(m[4..8].try_into().unwrap());
    if ver != 2 && ver != 3 {
        return Err(format!(
            "theodb ivf: unsupported structured format v{ver} — REINDEX to upgrade to the M48 relocatable generation (v3)"
        ));
    }
    let metric_tag = m[8];
    let dim = u32::from_le_bytes(m[9..13].try_into().unwrap());
    let nlists = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
    let dir_npages = u32::from_le_bytes(m[17..21].try_into().unwrap());
    let centroid_npages = u32::from_le_bytes(m[21..25].try_into().unwrap());
    let gen_base = if ver == 3 {
        if m.len() < 29 {
            return Err("theodb ivf: truncated v3 meta (missing gen_base)".into());
        }
        u32::from_le_bytes(m[25..29].try_into().unwrap())
    } else {
        1 // v2: directory implicitly at block 1
    };
    // Directory region: blocks gen_base..=+dir_npages, chunked (no longer inline on the meta page).
    let dbytes = read_chunked(rel, gen_base, dir_npages)?;
    if dbytes.len() < nlists * 12 {
        return Err("theodb ivf: truncated list directory".into());
    }
    let mut dir = Vec::with_capacity(nlists);
    for i in 0..nlists {
        let o = i * 12;
        dir.push((
            u32::from_le_bytes(dbytes[o..o + 4].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 4..o + 8].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 8..o + 12].try_into().unwrap()),
        ));
    }
    // Centroid region: blocks gen_base+dir_npages ..= +centroid_npages.
    let cbytes = read_chunked(rel, gen_base + dir_npages, centroid_npages)?;
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
/// The first block of the current IVF structured generation (v3 `gen_base`; v2 legacy = 1). The single-source
/// pointer the crash-safe fold's `free_region` reclaims BEFORE — same value the scan resolves the directory from.
pub(crate) unsafe fn ivf_gen_base(rel: pg_sys::Relation) -> Result<u32, String> {
    let m = read_page_item(rel, 0)?;
    if m.len() < 8 || u32::from_le_bytes(m[0..4].try_into().unwrap()) != IVF_STRUCT_MAGIC {
        return Err("theodb ivf: bad structured meta magic".into());
    }
    let ver = u32::from_le_bytes(m[4..8].try_into().unwrap());
    if ver == 3 {
        if m.len() < 29 {
            return Err("theodb ivf: truncated v3 meta (missing gen_base)".into());
        }
        Ok(u32::from_le_bytes(m[25..29].try_into().unwrap()))
    } else {
        Ok(1) // v2 (M34): directory implicitly at block 1
    }
}
pub(crate) struct IvfAqMeta {
    pub dim: u32,
    pub metric_tag: u8,
    pub m: u32,
    pub codebook: Vec<u8>,
    pub centroids: Vec<Vec<f32>>,
    pub dir: Vec<(u32, u32, u32)>,
}
/// True iff the index's structured meta is v4 (AQ) — cheap 8-byte read of block 0.
pub(crate) unsafe fn ivf_is_v4(rel: pg_sys::Relation) -> bool {
    match read_page_item(rel, 0) {
        Ok(m) if m.len() >= 8 => {
            u32::from_le_bytes(m[0..4].try_into().unwrap()) == IVF_STRUCT_MAGIC
                && u32::from_le_bytes(m[4..8].try_into().unwrap()) == 4
        }
        _ => false,
    }
}
/// Persist an IVF-AQ index in the v4 structured layout. `codes[i]` is list `i`'s block32-transposed AQ code bytes.
/// `codebook` is `AqQuantizer::to_meta_bytes()`.
pub(crate) unsafe fn write_ivf_aq(
    rel: pg_sys::Relation,
    dim: u32,
    metric_tag: u8,
    m: u32,
    codebook: &[u8],
    centroids: &[Vec<f32>],
    lists: &[Vec<(i64, Vec<f32>)>],
    codes: &[Vec<u8>],
) {
    let base: u32 = 1;
    let nlists = centroids.len() as u32;
    let mut cbytes = Vec::with_capacity(centroids.len() * dim as usize * 4);
    for c in centroids {
        for x in c {
            cbytes.extend_from_slice(&x.to_le_bytes());
        }
    }
    let encoded: Vec<Vec<u8>> = lists
        .iter()
        .zip(codes.iter())
        .map(|(l, cd)| {
            let mut b = Vec::with_capacity(l.len() * (8 + dim as usize * 4) + cd.len());
            for (tid, _) in l {
                b.extend_from_slice(&tid.to_le_bytes());
            }
            for (_, v) in l {
                for x in v {
                    b.extend_from_slice(&x.to_le_bytes());
                }
            }
            b.extend_from_slice(cd);
            b
        })
        .collect();

    let dir_npages = npages_for(nlists as usize * 12);
    let codebook_npages = npages_for(codebook.len());
    let centroid_npages = npages_for(cbytes.len());
    let mut cursor = base + dir_npages + codebook_npages + centroid_npages;
    let mut dir: Vec<(u32, u32, u32)> = Vec::with_capacity(lists.len());
    for (i, enc) in encoded.iter().enumerate() {
        let np = npages_for(enc.len());
        dir.push((cursor, np, lists[i].len() as u32));
        cursor += np;
    }
    let mut dirbytes = Vec::with_capacity(dir.len() * 12);
    for (fb, np, cnt) in &dir {
        dirbytes.extend_from_slice(&fb.to_le_bytes());
        dirbytes.extend_from_slice(&np.to_le_bytes());
        dirbytes.extend_from_slice(&cnt.to_le_bytes());
    }

    let mut meta = Vec::with_capacity(37);
    meta.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    meta.extend_from_slice(&4u32.to_le_bytes());
    meta.push(metric_tag);
    meta.extend_from_slice(&dim.to_le_bytes());
    meta.extend_from_slice(&nlists.to_le_bytes());
    meta.extend_from_slice(&m.to_le_bytes());
    meta.extend_from_slice(&codebook_npages.to_le_bytes());
    meta.extend_from_slice(&dir_npages.to_le_bytes());
    meta.extend_from_slice(&centroid_npages.to_le_bytes());
    meta.extend_from_slice(&base.to_le_bytes());

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
    push_chunks(&mut items, &dirbytes);
    push_chunks(&mut items, codebook);
    push_chunks(&mut items, &cbytes);
    for enc in &encoded {
        push_chunks(&mut items, enc);
    }
    for item in items {
        extend_page_with_item(rel, pg_sys::ForkNumber::MAIN_FORKNUM, &item);
    }
}
/// Read the v4 meta + codebook + centroid + dir regions (∝ nlists/codebook, NOT ∝ N). Typed `Err` on corruption.
pub(crate) unsafe fn read_ivf_aq_meta(rel: pg_sys::Relation) -> Result<IvfAqMeta, String> {
    let m = read_page_item(rel, 0)?;
    if m.len() < 37 {
        return Err("theodb ivf-aq: truncated v4 meta".into());
    }
    if u32::from_le_bytes(m[0..4].try_into().unwrap()) != IVF_STRUCT_MAGIC
        || u32::from_le_bytes(m[4..8].try_into().unwrap()) != 4
    {
        return Err("theodb ivf-aq: not a v4 structured index".into());
    }
    let metric_tag = m[8];
    let dim = u32::from_le_bytes(m[9..13].try_into().unwrap());
    let nlists = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
    let mval = u32::from_le_bytes(m[17..21].try_into().unwrap());
    let codebook_npages = u32::from_le_bytes(m[21..25].try_into().unwrap());
    let dir_npages = u32::from_le_bytes(m[25..29].try_into().unwrap());
    let centroid_npages = u32::from_le_bytes(m[29..33].try_into().unwrap());
    let gen_base = u32::from_le_bytes(m[33..37].try_into().unwrap());

    let dbytes = read_chunked(rel, gen_base, dir_npages)?;
    if dbytes.len() < nlists * 12 {
        return Err("theodb ivf-aq: truncated directory".into());
    }
    let mut dir = Vec::with_capacity(nlists);
    for i in 0..nlists {
        let o = i * 12;
        dir.push((
            u32::from_le_bytes(dbytes[o..o + 4].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 4..o + 8].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 8..o + 12].try_into().unwrap()),
        ));
    }
    let codebook = read_chunked(rel, gen_base + dir_npages, codebook_npages)?;
    let cbytes = read_chunked(rel, gen_base + dir_npages + codebook_npages, centroid_npages)?;
    let d = dim as usize;
    let mut centroids = Vec::with_capacity(nlists);
    if d > 0 && cbytes.len() >= nlists * d * 4 {
        for i in 0..nlists {
            let mut c = Vec::with_capacity(d);
            for j in 0..d {
                let o = (i * d + j) * 4;
                c.push(f32::from_le_bytes(cbytes[o..o + 4].try_into().unwrap()));
            }
            centroids.push(c);
        }
    } else if nlists != 0 {
        return Err("theodb ivf-aq: truncated centroid region".into());
    }
    Ok(IvfAqMeta { dim, metric_tag, m: mval, codebook, centroids, dir })
}
pub(crate) struct IvfAqMetaV5 {
    pub dim: u32,
    pub metric_tag: u8,
    pub m: u32,
    pub codebook: Vec<u8>,
    pub centroids: Vec<Vec<f32>>,
    pub dir: Vec<(u32, u32, u32, u32, u32)>, // code_fb, code_np, vec_fb, vec_np, cnt
}
/// True iff the index's structured meta is v5 (AQ storage-separated) — cheap 8-byte read of block 0.
pub(crate) unsafe fn ivf_is_v5(rel: pg_sys::Relation) -> bool {
    match read_page_item(rel, 0) {
        Ok(m) if m.len() >= 8 => {
            u32::from_le_bytes(m[0..4].try_into().unwrap()) == IVF_STRUCT_MAGIC
                && u32::from_le_bytes(m[4..8].try_into().unwrap()) == 5
        }
        _ => false,
    }
}
/// M90 — the v7 (label-aware) layout: same meta shape as v5 (magic `7`), code blob widened to `[ids][labels][codes]`.
pub(crate) unsafe fn ivf_is_v7(rel: pg_sys::Relation) -> bool {
    match read_page_item(rel, 0) {
        Ok(m) if m.len() >= 8 => {
            u32::from_le_bytes(m[0..4].try_into().unwrap()) == IVF_STRUCT_MAGIC
                && u32::from_le_bytes(m[4..8].try_into().unwrap()) == 7
        }
        _ => false,
    }
}
/// Persist an IVF-AQ index in the v5 storage-separated layout: each list's `[ids][codes]` and `[f32]` go on
/// distinct page ranges. `codes[i]` is list `i`'s block32-transposed AQ code bytes; `codebook` is
/// `AqQuantizer::to_meta_bytes()`.
/// M89 (ambuild streaming Increment 2) — writes the v5 layout by STREAMING: the vectors are read from `vectors`
/// via each list's `positions` (no `list_entries()` clone) and each list's CODE + VECTOR blob is materialized,
/// written to pages, and FREED one list at a time (no `enc_vec` / `items` full-buffering). Byte-identical page
/// image to the pre-M89 buffering writer (same order: meta, dir, codebook, centroids, then per-list [code][vec]);
/// the change is only WHEN the bytes are built. Peak memory drops from ~4× base to ~1× base + one list's blob.
pub(crate) unsafe fn write_ivf_aq_split(
    rel: pg_sys::Relation,
    dim: u32,
    metric_tag: u8,
    m: u32,
    codebook: &[u8],
    centroids: &[Vec<f32>],
    positions: &[Vec<usize>],
    ids: &[i64],
    vectors: &[Vec<f32>],
    codes: &[Vec<u8>],
) {
    let base: u32 = 1;
    let nlists = centroids.len() as u32;
    let mut cbytes = Vec::with_capacity(centroids.len() * dim as usize * 4);
    for c in centroids {
        for x in c {
            cbytes.extend_from_slice(&x.to_le_bytes());
        }
    }
    // Directory computed from COUNTS (+ codes lengths) — no per-list blob is materialized to size the regions.
    let dim_bytes = dim as usize * 4;
    let dir_npages = npages_for(nlists as usize * 20);
    let codebook_npages = npages_for(codebook.len());
    let centroid_npages = npages_for(cbytes.len());
    let mut cursor = base + dir_npages + codebook_npages + centroid_npages;
    let mut dir: Vec<(u32, u32, u32, u32, u32)> = Vec::with_capacity(positions.len());
    for i in 0..positions.len() {
        let code_len = positions[i].len() * 8 + codes[i].len();
        let cnp = npages_for(code_len);
        let code_fb = cursor;
        cursor += cnp;
        let vec_len = positions[i].len() * dim_bytes;
        let vnp = npages_for(vec_len);
        let vec_fb = cursor;
        cursor += vnp;
        dir.push((code_fb, cnp, vec_fb, vnp, positions[i].len() as u32));
    }
    let mut dirbytes = Vec::with_capacity(dir.len() * 20);
    for (cfb, cnp, vfb, vnp, cnt) in &dir {
        dirbytes.extend_from_slice(&cfb.to_le_bytes());
        dirbytes.extend_from_slice(&cnp.to_le_bytes());
        dirbytes.extend_from_slice(&vfb.to_le_bytes());
        dirbytes.extend_from_slice(&vnp.to_le_bytes());
        dirbytes.extend_from_slice(&cnt.to_le_bytes());
    }

    let mut meta = Vec::with_capacity(37);
    meta.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    meta.extend_from_slice(&5u32.to_le_bytes());
    meta.push(metric_tag);
    meta.extend_from_slice(&dim.to_le_bytes());
    meta.extend_from_slice(&nlists.to_le_bytes());
    meta.extend_from_slice(&m.to_le_bytes());
    meta.extend_from_slice(&codebook_npages.to_le_bytes());
    meta.extend_from_slice(&dir_npages.to_le_bytes());
    meta.extend_from_slice(&centroid_npages.to_le_bytes());
    meta.extend_from_slice(&base.to_le_bytes());

    // Stream each region straight to pages — no `items` accumulation.
    write_item(rel, &meta);
    write_chunks(rel, &dirbytes);
    write_chunks(rel, codebook);
    write_chunks(rel, &cbytes);
    for i in 0..positions.len() {
        // CODE blob [ids][codes] for list i.
        let mut ecode = Vec::with_capacity(positions[i].len() * 8 + codes[i].len());
        for &pos in &positions[i] {
            ecode.extend_from_slice(&ids[pos].to_le_bytes());
        }
        ecode.extend_from_slice(&codes[i]);
        write_chunks(rel, &ecode);
        drop(ecode);
        // VECTOR blob [f32] for list i — read from `vectors` by position, then freed.
        let mut evec = Vec::with_capacity(positions[i].len() * dim_bytes);
        for &pos in &positions[i] {
            for x in &vectors[pos] {
                evec.extend_from_slice(&x.to_le_bytes());
            }
        }
        write_chunks(rel, &evec);
        drop(evec);
    }
}
/// M90 (inline label filter) — fixed number of `smallint` labels stored per vector in the v7 code blob. Vectors
/// with fewer labels are padded with `LABEL_PAD` (a sentinel outside valid `i16` label use is impossible since any
/// i16 is a valid label, so we store the COUNT per vector and pad the rest — see `encode_labels_fixed`). 8 covers
/// the overwhelming majority of tag/category filters; variable-length labels are a documented follow-up.
// M146 — `LABEL_K` / `encode_labels_fixed` / `record_span` moved to the PURE sibling `ivf_codec`, so an
// `examples/` binary can link and exercise them without a backend (`cargo test` does not link in this crate).
// Re-exported here so every existing `super::ivf::LABEL_K` caller keeps working — one definition, no twin.
pub(crate) use super::ivf_codec::{LABEL_K, encode_labels_fixed, record_span};
/// M90 — the v7 layout: identical to v5 (`write_ivf_aq_split`) except the per-list CODE blob is
/// `[ids][labels_fixed][codes]` (each vector carries `2 + LABEL_K*2` label bytes right after its id), so the Stage-1
/// scan can skip non-overlapping candidates before the Stage-2 f32 rerank. Magic `7`; the label bytes per vector
/// are constant, so all offsets are computable from counts (streaming, byte-deterministic). `labels[pos]` is the
/// sorted-deduped label set for the vector at global position `pos` (parallel to `ids`/`vectors`).
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn write_ivf_aq_split_v7(
    rel: pg_sys::Relation,
    dim: u32,
    metric_tag: u8,
    m: u32,
    codebook: &[u8],
    centroids: &[Vec<f32>],
    positions: &[Vec<usize>],
    ids: &[i64],
    vectors: &[Vec<f32>],
    codes: &[Vec<u8>],
    labels: &[Vec<i16>],
) {
    let base: u32 = 1;
    let nlists = centroids.len() as u32;
    let label_bytes = 2 + LABEL_K * 2; // per vector
    let mut cbytes = Vec::with_capacity(centroids.len() * dim as usize * 4);
    for c in centroids {
        for x in c {
            cbytes.extend_from_slice(&x.to_le_bytes());
        }
    }
    let dim_bytes = dim as usize * 4;
    let dir_npages = npages_for(nlists as usize * 20);
    let codebook_npages = npages_for(codebook.len());
    let centroid_npages = npages_for(cbytes.len());
    let mut cursor = base + dir_npages + codebook_npages + centroid_npages;
    let mut dir: Vec<(u32, u32, u32, u32, u32)> = Vec::with_capacity(positions.len());
    for i in 0..positions.len() {
        // CODE blob now = [ids (8B)][labels (label_bytes)][codes].
        let code_len = positions[i].len() * (8 + label_bytes) + codes[i].len();
        let cnp = npages_for(code_len);
        let code_fb = cursor;
        cursor += cnp;
        let vec_len = positions[i].len() * dim_bytes;
        let vnp = npages_for(vec_len);
        let vec_fb = cursor;
        cursor += vnp;
        dir.push((code_fb, cnp, vec_fb, vnp, positions[i].len() as u32));
    }
    let mut dirbytes = Vec::with_capacity(dir.len() * 20);
    for (cfb, cnp, vfb, vnp, cnt) in &dir {
        dirbytes.extend_from_slice(&cfb.to_le_bytes());
        dirbytes.extend_from_slice(&cnp.to_le_bytes());
        dirbytes.extend_from_slice(&vfb.to_le_bytes());
        dirbytes.extend_from_slice(&vnp.to_le_bytes());
        dirbytes.extend_from_slice(&cnt.to_le_bytes());
    }
    let mut meta = Vec::with_capacity(37);
    meta.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    meta.extend_from_slice(&7u32.to_le_bytes()); // v7
    meta.push(metric_tag);
    meta.extend_from_slice(&dim.to_le_bytes());
    meta.extend_from_slice(&nlists.to_le_bytes());
    meta.extend_from_slice(&m.to_le_bytes());
    meta.extend_from_slice(&codebook_npages.to_le_bytes());
    meta.extend_from_slice(&dir_npages.to_le_bytes());
    meta.extend_from_slice(&centroid_npages.to_le_bytes());
    meta.extend_from_slice(&base.to_le_bytes());

    write_item(rel, &meta);
    write_chunks(rel, &dirbytes);
    write_chunks(rel, codebook);
    write_chunks(rel, &cbytes);
    for i in 0..positions.len() {
        // CODE blob [ids][labels_fixed][codes] for list i.
        let mut ecode = Vec::with_capacity(positions[i].len() * (8 + label_bytes) + codes[i].len());
        for &pos in &positions[i] {
            ecode.extend_from_slice(&ids[pos].to_le_bytes());
        }
        for &pos in &positions[i] {
            encode_labels_fixed(&labels[pos], &mut ecode);
        }
        ecode.extend_from_slice(&codes[i]);
        write_chunks(rel, &ecode);
        drop(ecode);
        // VECTOR blob [f32] for list i.
        let mut evec = Vec::with_capacity(positions[i].len() * dim_bytes);
        for &pos in &positions[i] {
            for x in &vectors[pos] {
                evec.extend_from_slice(&x.to_le_bytes());
            }
        }
        write_chunks(rel, &evec);
        drop(evec);
    }
}
/// Read the v5 meta + codebook + centroid + dir regions (∝ nlists/codebook, NOT ∝ N). Typed `Err` on corruption.
/// M90: also accepts the v7 (label-aware) meta — identical shape (magic 7); the label region lives inside the per-
/// list CODE blob, so the meta/dir are byte-identical to v5. The scan branches on `ivf_is_v7` for the code offsets.
pub(crate) unsafe fn read_ivf_aq_meta_split(rel: pg_sys::Relation) -> Result<IvfAqMetaV5, String> {
    let m = read_page_item(rel, 0)?;
    if m.len() < 37 {
        return Err("theodb ivf-aq: truncated v5 meta".into());
    }
    let ver = u32::from_le_bytes(m[4..8].try_into().unwrap());
    if u32::from_le_bytes(m[0..4].try_into().unwrap()) != IVF_STRUCT_MAGIC || (ver != 5 && ver != 7)
    {
        return Err("theodb ivf-aq: not a v5/v7 structured index".into());
    }
    let metric_tag = m[8];
    let dim = u32::from_le_bytes(m[9..13].try_into().unwrap());
    let nlists = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
    let mval = u32::from_le_bytes(m[17..21].try_into().unwrap());
    let codebook_npages = u32::from_le_bytes(m[21..25].try_into().unwrap());
    let dir_npages = u32::from_le_bytes(m[25..29].try_into().unwrap());
    let centroid_npages = u32::from_le_bytes(m[29..33].try_into().unwrap());
    let gen_base = u32::from_le_bytes(m[33..37].try_into().unwrap());

    let dbytes = read_chunked(rel, gen_base, dir_npages)?;
    if dbytes.len() < nlists * 20 {
        return Err("theodb ivf-aq: truncated v5 directory".into());
    }
    let mut dir = Vec::with_capacity(nlists);
    for i in 0..nlists {
        let o = i * 20;
        dir.push((
            u32::from_le_bytes(dbytes[o..o + 4].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 4..o + 8].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 8..o + 12].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 12..o + 16].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 16..o + 20].try_into().unwrap()),
        ));
    }
    let codebook = read_chunked(rel, gen_base + dir_npages, codebook_npages)?;
    let cbytes = read_chunked(rel, gen_base + dir_npages + codebook_npages, centroid_npages)?;
    let d = dim as usize;
    let mut centroids = Vec::with_capacity(nlists);
    if d > 0 && cbytes.len() >= nlists * d * 4 {
        for i in 0..nlists {
            let mut c = Vec::with_capacity(d);
            for j in 0..d {
                let o = (i * d + j) * 4;
                c.push(f32::from_le_bytes(cbytes[o..o + 4].try_into().unwrap()));
            }
            centroids.push(c);
        }
    } else if nlists != 0 {
        return Err("theodb ivf-aq: truncated v5 centroid region".into());
    }
    Ok(IvfAqMetaV5 { dim, metric_tag, m: mval, codebook, centroids, dir })
}
/// M83/M85 — random-read ONE fixed-size record (`ordinal`) from a chunked page range, touching only the 1-2 pages
/// that hold it — the Stage-2 rerank I/O the storage separation exists to minimize. Bytes are chunked CHUNK-per-item,
/// so record `i` at global offset `i·reclen` lives in item `off/CHUNK` at local `off%CHUNK`, straddling into the
/// next item when it crosses a chunk boundary. `read_vec_at` (f32, reclen=dim·4) and `read_sq8_at` (SQ8, reclen=dim)
/// are thin wrappers.
unsafe fn read_record_at(
    rel: pg_sys::Relation,
    first_block: u32,
    npages: u32,
    ordinal: usize,
    reclen: usize,
) -> Result<Vec<u8>, String> {
    // M146 — the span arithmetic (and its typed failures) lives in the pure `ivf_codec`, where it is actually
    // exercised (`examples/ivf_codec_check.rs`); this function keeps only the I/O.
    let span = record_span(ordinal, reclen, CHUNK, npages)?;
    let (p0, lo) = (span.chunk, span.lo);
    let mut buf = Vec::new();
    read_page_item_into(rel, first_block + p0 as u32, &mut buf)?;
    if lo + reclen <= buf.len() {
        return Ok(buf[lo..lo + reclen].to_vec());
    }
    // Straddles into the next chunk item — read it and stitch tail+head.
    if (p0 + 1) as u32 >= npages {
        return Err("theodb ivf-aq: truncated straddled record".into());
    }
    let mut out = buf[lo..].to_vec();
    let need = reclen - out.len();
    let mut buf2 = Vec::new();
    read_page_item_into(rel, first_block + (p0 + 1) as u32, &mut buf2)?;
    if buf2.len() < need {
        return Err("theodb ivf-aq: truncated straddled record".into());
    }
    out.extend_from_slice(&buf2[..need]);
    Ok(out)
}
/// M83 — random-read ONE f32 vector (reclen = `dim·4`) from a v5 VECTOR range.
pub(crate) unsafe fn read_vec_at(
    rel: pg_sys::Relation,
    vec_first_block: u32,
    vec_npages: u32,
    ordinal: usize,
    dim: usize,
) -> Result<Vec<u8>, String> {
    read_record_at(rel, vec_first_block, vec_npages, ordinal, dim * 4)
}
/// M85 — random-read ONE SQ8 code (reclen = `dim`) from a v6 SQ8 range — ¼ the bytes of the f32 read.
pub(crate) unsafe fn read_sq8_at(
    rel: pg_sys::Relation,
    sq8_first_block: u32,
    sq8_npages: u32,
    ordinal: usize,
    dim: usize,
) -> Result<Vec<u8>, String> {
    read_record_at(rel, sq8_first_block, sq8_npages, ordinal, dim)
}
pub(crate) struct IvfAqMetaV6 {
    pub dim: u32,
    pub metric_tag: u8,
    pub m: u32,
    pub aq_codebook: Vec<u8>,
    pub sq8_codebook: Vec<u8>,
    pub centroids: Vec<Vec<f32>>,
    pub dir: Vec<(u32, u32, u32, u32, u32)>, // code_fb, code_np, sq8_fb, sq8_np, cnt
}
/// E1 — v8 (refine=rabitq) meta. Same shape as v6 with the SQ8 codebook replaced by the RaBitQ codebook (rotation
/// + bits). `centroids` are REQUIRED at scan (the residual query `q_r = P(q−c)` needs the per-list centroid).
pub(crate) struct IvfAqMetaV8 {
    pub dim: u32,
    pub metric_tag: u8,
    pub m: u32,
    pub aq_codebook: Vec<u8>,
    pub rabitq_codebook: Vec<u8>,
    pub centroids: Vec<Vec<f32>>,
    pub dir: Vec<(u32, u32, u32, u32, u32)>, // code_fb, code_np, rq_fb, rq_np, cnt
}
/// E1 — random-read ONE RaBitQ residual record (reclen = `dim + 8` = [i8×dim][nr f32][w f32]) for a v8 survivor.
pub(crate) unsafe fn read_rabitq_at(
    rel: pg_sys::Relation,
    rq_first_block: u32,
    rq_npages: u32,
    ordinal: usize,
    dim: usize,
) -> Result<Vec<u8>, String> {
    read_record_at(rel, rq_first_block, rq_npages, ordinal, dim + 8)
}
/// M87 — the IVF list count (v3/v4/v5/v6), via the same fallback chain as the cost model. 0 on any unreadable
/// meta (fail-safe — the iterative scan then bounds growth by `max_scan_tuples` alone). Used by `amrescan` to bound
/// the iterative re-search (grow `probes` until all lists are probed, then stop).
pub(crate) unsafe fn ivf_list_count(rel: pg_sys::Relation) -> usize {
    read_ivf_meta(rel)
        .map(|m| m.dir.len())
        .or_else(|_| read_ivf_aq_meta(rel).map(|m| m.dir.len()))
        .or_else(|_| read_ivf_aq_meta_split(rel).map(|m| m.dir.len()))
        .or_else(|_| read_ivf_aq_meta_split_sq8(rel).map(|m| m.dir.len()))
        .or_else(|_| read_ivf_aq_meta_split_rabitq(rel).map(|m| m.dir.len()))
        .unwrap_or(0)
}
/// True iff the index's structured meta is v8 (AQ + RaBitQ residual refine, storage-separated).
pub(crate) unsafe fn ivf_is_v8(rel: pg_sys::Relation) -> bool {
    match read_page_item(rel, 0) {
        Ok(m) if m.len() >= 8 => {
            u32::from_le_bytes(m[0..4].try_into().unwrap()) == IVF_STRUCT_MAGIC
                && u32::from_le_bytes(m[4..8].try_into().unwrap()) == 8
        }
        _ => false,
    }
}
/// True iff the index's structured meta is v6 (AQ + SQ8-refine, storage-separated) — cheap 8-byte read of block 0.
pub(crate) unsafe fn ivf_is_v6(rel: pg_sys::Relation) -> bool {
    match read_page_item(rel, 0) {
        Ok(m) if m.len() >= 8 => {
            u32::from_le_bytes(m[0..4].try_into().unwrap()) == IVF_STRUCT_MAGIC
                && u32::from_le_bytes(m[4..8].try_into().unwrap()) == 6
        }
        _ => false,
    }
}
/// Persist an IVF-AQ index in the v6 SQ8-refine layout. `codes[i]` is list `i`'s block32 AH bytes; `sq8_codes[i]`
/// is list `i`'s SQ8 code bytes (`dim`×n, ordinal order matching `lists[i]`). `aq_codebook`/`sq8_codebook` are the
/// two quantizers' `to_meta_bytes()`.
#[allow(clippy::too_many_arguments)]
/// M96 — the STREAMING v5 writer: byte-identical on-disk output to [`write_ivf_aq_split`] but the per-list `(id,
/// vector)` members are pulled from a callback (the sorted tuplesort read-back) ONE LIST AT A TIME instead of from
/// full `positions`/`ids`/`vectors` arrays held in RAM. `counts[i]` is list i's member count (known from the
/// assignment histogram before the read-back); `next_list_member()` yields the next `(id, vector)` in sorted-by-list
/// order — the writer consumes exactly `counts[i]` of them per list. Peak extra RAM is O(one list) = O(N/lists), not
/// O(N). Same directory math, same magic (v5), same blob order → no REINDEX. The AQ codes are packed per list here
/// (block32) from the list buffer, matching the in-RAM `pack_block32_codes` layout (self-consistent order).
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn write_ivf_aq_split_streaming(
    rel: pg_sys::Relation,
    dim: u32,
    metric_tag: u8,
    m: u32,
    codebook: &[u8],
    centroids: &[Vec<f32>],
    counts: &[u32],
    quant: &crate::vec::aq::AqQuantizer,
    mut next_list_member: impl FnMut() -> Option<(i64, Vec<f32>)>,
) {
    let base: u32 = 1;
    let nlists = centroids.len() as u32;
    let pairs = (m as usize).div_ceil(2);
    let dim_bytes = dim as usize * 4;
    let mut cbytes = Vec::with_capacity(centroids.len() * dim_bytes);
    for c in centroids {
        for x in c {
            cbytes.extend_from_slice(&x.to_le_bytes());
        }
    }
    // Directory from COUNTS (code_len = count*8 ids + count*pairs codes; vec_len = count*dim_bytes) — no blob held.
    let dir_npages = npages_for(nlists as usize * 20);
    let codebook_npages = npages_for(codebook.len());
    let centroid_npages = npages_for(cbytes.len());
    let mut cursor = base + dir_npages + codebook_npages + centroid_npages;
    let mut dir: Vec<(u32, u32, u32, u32, u32)> = Vec::with_capacity(counts.len());
    for &cnt in counts {
        let cnt = cnt as usize;
        let nblocks = cnt.div_ceil(32);
        let code_len = cnt * 8 + nblocks * pairs * 32; // ids + block32-padded codes (matches the streamed pack below)
        let cnp = npages_for(code_len);
        let code_fb = cursor;
        cursor += cnp;
        let vec_len = cnt * dim_bytes;
        let vnp = npages_for(vec_len);
        let vec_fb = cursor;
        cursor += vnp;
        dir.push((code_fb, cnp, vec_fb, vnp, cnt as u32));
    }
    let mut dirbytes = Vec::with_capacity(dir.len() * 20);
    for (cfb, cnp, vfb, vnp, cnt) in &dir {
        dirbytes.extend_from_slice(&cfb.to_le_bytes());
        dirbytes.extend_from_slice(&cnp.to_le_bytes());
        dirbytes.extend_from_slice(&vfb.to_le_bytes());
        dirbytes.extend_from_slice(&vnp.to_le_bytes());
        dirbytes.extend_from_slice(&cnt.to_le_bytes());
    }

    let mut meta = Vec::with_capacity(37);
    meta.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    meta.extend_from_slice(&5u32.to_le_bytes());
    meta.push(metric_tag);
    meta.extend_from_slice(&dim.to_le_bytes());
    meta.extend_from_slice(&nlists.to_le_bytes());
    meta.extend_from_slice(&m.to_le_bytes());
    meta.extend_from_slice(&codebook_npages.to_le_bytes());
    meta.extend_from_slice(&dir_npages.to_le_bytes());
    meta.extend_from_slice(&centroid_npages.to_le_bytes());
    meta.extend_from_slice(&base.to_le_bytes());

    write_item(rel, &meta);
    write_chunks(rel, &dirbytes);
    write_chunks(rel, codebook);
    write_chunks(rel, &cbytes);
    // Per list: pull its `count` members from the sorted stream into a small buffer, pack codes (block32), write.
    for &cnt in counts {
        let cnt = cnt as usize;
        let mut list_ids: Vec<i64> = Vec::with_capacity(cnt);
        let mut list_vecs: Vec<Vec<f32>> = Vec::with_capacity(cnt);
        for _ in 0..cnt {
            let (id, v) = match next_list_member() {
                Some(m) => m,
                // Fail-loud (review LOW): the histogram and the stream must agree by construction; a shortfall is a
                // build bug — a typed error over a bare panic across the build (Rule 8). Runs after the C scan
                // returned, so this unwinds only to the guarded `ambuild`.
                None => pg_sys::error!(
                    "theodb streaming writer: stream shorter than the count histogram"
                ),
            };
            list_ids.push(id);
            list_vecs.push(v);
        }
        // CODE blob [ids][block32 codes] — pack in buffer order (self-consistent with the [vectors] blob below).
        let nblocks = cnt.div_ceil(32);
        let mut ecode = Vec::with_capacity(cnt * 8 + nblocks * pairs * 32);
        for &id in &list_ids {
            ecode.extend_from_slice(&id.to_le_bytes());
        }
        let mut blocks = vec![0u8; nblocks * pairs * 32];
        for (i, v) in list_vecs.iter().enumerate() {
            let code = quant.encode(v);
            let bbase = (i / 32) * pairs * 32;
            let vb = i % 32;
            for (p, &cb) in code.iter().enumerate().take(pairs) {
                blocks[bbase + p * 32 + vb] = cb;
            }
        }
        ecode.extend_from_slice(&blocks);
        write_chunks(rel, &ecode);
        drop(ecode);
        drop(blocks);
        // VECTOR blob [f32] in the same buffer order.
        let mut evec = Vec::with_capacity(cnt * dim_bytes);
        for v in &list_vecs {
            for x in v {
                evec.extend_from_slice(&x.to_le_bytes());
            }
        }
        write_chunks(rel, &evec);
        // list_ids / list_vecs freed here — only one list's worth held at a time (O(N/lists)).
    }
}
/// M89 (ambuild streaming Increment 2) — v6 (SQ8) streaming writer. Same contract as `write_ivf_aq_split` but the
/// per-list VECTOR region is the SQ8 code (`sq8_codes[i]`, already ¼ the f32 bytes) instead of f32. Reads TIDs from
/// `ids` by position (no `list_entries()` clone) and streams each list's [ids][codes] + sq8 blob to pages without
/// the `items` full-buffer. Byte-identical page image to the pre-M89 buffering writer.
///
/// (M146 T2.4: este bloco estava pendurado por engano na `write_ivf_aq_split_streaming`, que é o writer **v5/f32**
/// — descrever um writer v6/SQ8 sobre a função v5 induz a erro quem for mexer no codec. Movido para cá.)
pub(crate) unsafe fn write_ivf_aq_split_sq8(
    rel: pg_sys::Relation,
    dim: u32,
    metric_tag: u8,
    m: u32,
    aq_codebook: &[u8],
    sq8_codebook: &[u8],
    centroids: &[Vec<f32>],
    positions: &[Vec<usize>],
    ids: &[i64],
    codes: &[Vec<u8>],
    sq8_codes: &[Vec<u8>],
) {
    let base: u32 = 1;
    let nlists = centroids.len() as u32;
    let mut cbytes = Vec::with_capacity(centroids.len() * dim as usize * 4);
    for c in centroids {
        for x in c {
            cbytes.extend_from_slice(&x.to_le_bytes());
        }
    }
    let dir_npages = npages_for(nlists as usize * 20);
    let aq_codebook_npages = npages_for(aq_codebook.len());
    let sq8_codebook_npages = npages_for(sq8_codebook.len());
    let centroid_npages = npages_for(cbytes.len());
    let mut cursor = base + dir_npages + aq_codebook_npages + sq8_codebook_npages + centroid_npages;
    let mut dir: Vec<(u32, u32, u32, u32, u32)> = Vec::with_capacity(positions.len());
    for i in 0..positions.len() {
        let code_len = positions[i].len() * 8 + codes[i].len();
        let cnp = npages_for(code_len);
        let code_fb = cursor;
        cursor += cnp;
        let snp = npages_for(sq8_codes[i].len());
        let sq8_fb = cursor;
        cursor += snp;
        dir.push((code_fb, cnp, sq8_fb, snp, positions[i].len() as u32));
    }
    let mut dirbytes = Vec::with_capacity(dir.len() * 20);
    for (cfb, cnp, sfb, snp, cnt) in &dir {
        dirbytes.extend_from_slice(&cfb.to_le_bytes());
        dirbytes.extend_from_slice(&cnp.to_le_bytes());
        dirbytes.extend_from_slice(&sfb.to_le_bytes());
        dirbytes.extend_from_slice(&snp.to_le_bytes());
        dirbytes.extend_from_slice(&cnt.to_le_bytes());
    }

    let mut meta = Vec::with_capacity(41);
    meta.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    meta.extend_from_slice(&6u32.to_le_bytes());
    meta.push(metric_tag);
    meta.extend_from_slice(&dim.to_le_bytes());
    meta.extend_from_slice(&nlists.to_le_bytes());
    meta.extend_from_slice(&m.to_le_bytes());
    meta.extend_from_slice(&aq_codebook_npages.to_le_bytes());
    meta.extend_from_slice(&dir_npages.to_le_bytes());
    meta.extend_from_slice(&centroid_npages.to_le_bytes());
    meta.extend_from_slice(&base.to_le_bytes());
    meta.extend_from_slice(&sq8_codebook_npages.to_le_bytes());

    write_item(rel, &meta);
    write_chunks(rel, &dirbytes);
    write_chunks(rel, aq_codebook);
    write_chunks(rel, sq8_codebook);
    write_chunks(rel, &cbytes);
    for i in 0..positions.len() {
        let mut ecode = Vec::with_capacity(positions[i].len() * 8 + codes[i].len());
        for &pos in &positions[i] {
            ecode.extend_from_slice(&ids[pos].to_le_bytes());
        }
        ecode.extend_from_slice(&codes[i]);
        write_chunks(rel, &ecode);
        drop(ecode);
        write_chunks(rel, &sq8_codes[i]);
    }
}
/// Read the v6 meta + both codebooks + centroid + dir regions. Typed `Err` on corruption.
pub(crate) unsafe fn read_ivf_aq_meta_split_sq8(
    rel: pg_sys::Relation,
) -> Result<IvfAqMetaV6, String> {
    let m = read_page_item(rel, 0)?;
    if m.len() < 41 {
        return Err("theodb ivf-aq: truncated v6 meta".into());
    }
    if u32::from_le_bytes(m[0..4].try_into().unwrap()) != IVF_STRUCT_MAGIC
        || u32::from_le_bytes(m[4..8].try_into().unwrap()) != 6
    {
        return Err("theodb ivf-aq: not a v6 structured index".into());
    }
    let metric_tag = m[8];
    let dim = u32::from_le_bytes(m[9..13].try_into().unwrap());
    let nlists = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
    let mval = u32::from_le_bytes(m[17..21].try_into().unwrap());
    let aq_codebook_npages = u32::from_le_bytes(m[21..25].try_into().unwrap());
    let dir_npages = u32::from_le_bytes(m[25..29].try_into().unwrap());
    let centroid_npages = u32::from_le_bytes(m[29..33].try_into().unwrap());
    let gen_base = u32::from_le_bytes(m[33..37].try_into().unwrap());
    let sq8_codebook_npages = u32::from_le_bytes(m[37..41].try_into().unwrap());

    let dbytes = read_chunked(rel, gen_base, dir_npages)?;
    if dbytes.len() < nlists * 20 {
        return Err("theodb ivf-aq: truncated v6 directory".into());
    }
    let mut dir = Vec::with_capacity(nlists);
    for i in 0..nlists {
        let o = i * 20;
        dir.push((
            u32::from_le_bytes(dbytes[o..o + 4].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 4..o + 8].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 8..o + 12].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 12..o + 16].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 16..o + 20].try_into().unwrap()),
        ));
    }
    let aq_codebook = read_chunked(rel, gen_base + dir_npages, aq_codebook_npages)?;
    let sq8_codebook =
        read_chunked(rel, gen_base + dir_npages + aq_codebook_npages, sq8_codebook_npages)?;
    let cbytes = read_chunked(
        rel,
        gen_base + dir_npages + aq_codebook_npages + sq8_codebook_npages,
        centroid_npages,
    )?;
    let d = dim as usize;
    let mut centroids = Vec::with_capacity(nlists);
    if d > 0 && cbytes.len() >= nlists * d * 4 {
        for i in 0..nlists {
            let mut c = Vec::with_capacity(d);
            for j in 0..d {
                let o = (i * d + j) * 4;
                c.push(f32::from_le_bytes(cbytes[o..o + 4].try_into().unwrap()));
            }
            centroids.push(c);
        }
    } else if nlists != 0 {
        return Err("theodb ivf-aq: truncated v6 centroid region".into());
    }
    Ok(IvfAqMetaV6 { dim, metric_tag, m: mval, aq_codebook, sq8_codebook, centroids, dir })
}
/// E1 — persist an IVF-AQ index in the v8 RaBitQ-refine layout. Byte-for-byte the v6 writer with version 8 and the
/// refine blob = per-list RaBitQ residual records (`rabitq_codes[i]`, `(dim+8)`×n). `rabitq_codebook` = rotation+bits.
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn write_ivf_aq_split_rabitq(
    rel: pg_sys::Relation,
    dim: u32,
    metric_tag: u8,
    m: u32,
    aq_codebook: &[u8],
    rabitq_codebook: &[u8],
    centroids: &[Vec<f32>],
    positions: &[Vec<usize>],
    ids: &[i64],
    codes: &[Vec<u8>],
    rabitq_codes: &[Vec<u8>],
) {
    let base: u32 = 1;
    let nlists = centroids.len() as u32;
    let mut cbytes = Vec::with_capacity(centroids.len() * dim as usize * 4);
    for c in centroids {
        for x in c {
            cbytes.extend_from_slice(&x.to_le_bytes());
        }
    }
    let dir_npages = npages_for(nlists as usize * 20);
    let aq_codebook_npages = npages_for(aq_codebook.len());
    let rabitq_codebook_npages = npages_for(rabitq_codebook.len());
    let centroid_npages = npages_for(cbytes.len());
    let mut cursor =
        base + dir_npages + aq_codebook_npages + rabitq_codebook_npages + centroid_npages;
    let mut dir: Vec<(u32, u32, u32, u32, u32)> = Vec::with_capacity(positions.len());
    for i in 0..positions.len() {
        let code_len = positions[i].len() * 8 + codes[i].len();
        let cnp = npages_for(code_len);
        let code_fb = cursor;
        cursor += cnp;
        let rnp = npages_for(rabitq_codes[i].len());
        let rq_fb = cursor;
        cursor += rnp;
        dir.push((code_fb, cnp, rq_fb, rnp, positions[i].len() as u32));
    }
    let mut dirbytes = Vec::with_capacity(dir.len() * 20);
    for (cfb, cnp, rfb, rnp, cnt) in &dir {
        dirbytes.extend_from_slice(&cfb.to_le_bytes());
        dirbytes.extend_from_slice(&cnp.to_le_bytes());
        dirbytes.extend_from_slice(&rfb.to_le_bytes());
        dirbytes.extend_from_slice(&rnp.to_le_bytes());
        dirbytes.extend_from_slice(&cnt.to_le_bytes());
    }

    let mut meta = Vec::with_capacity(41);
    meta.extend_from_slice(&IVF_STRUCT_MAGIC.to_le_bytes());
    meta.extend_from_slice(&8u32.to_le_bytes());
    meta.push(metric_tag);
    meta.extend_from_slice(&dim.to_le_bytes());
    meta.extend_from_slice(&nlists.to_le_bytes());
    meta.extend_from_slice(&m.to_le_bytes());
    meta.extend_from_slice(&aq_codebook_npages.to_le_bytes());
    meta.extend_from_slice(&dir_npages.to_le_bytes());
    meta.extend_from_slice(&centroid_npages.to_le_bytes());
    meta.extend_from_slice(&base.to_le_bytes());
    meta.extend_from_slice(&rabitq_codebook_npages.to_le_bytes());

    write_item(rel, &meta);
    write_chunks(rel, &dirbytes);
    write_chunks(rel, aq_codebook);
    write_chunks(rel, rabitq_codebook);
    write_chunks(rel, &cbytes);
    for i in 0..positions.len() {
        let mut ecode = Vec::with_capacity(positions[i].len() * 8 + codes[i].len());
        for &pos in &positions[i] {
            ecode.extend_from_slice(&ids[pos].to_le_bytes());
        }
        ecode.extend_from_slice(&codes[i]);
        write_chunks(rel, &ecode);
        drop(ecode);
        write_chunks(rel, &rabitq_codes[i]);
    }
}
/// E1 — read the v8 meta + AQ codebook + RaBitQ codebook + centroid + dir regions. Typed `Err` on corruption.
pub(crate) unsafe fn read_ivf_aq_meta_split_rabitq(
    rel: pg_sys::Relation,
) -> Result<IvfAqMetaV8, String> {
    let m = read_page_item(rel, 0)?;
    if m.len() < 41 {
        return Err("theodb ivf-aq: truncated v8 meta".into());
    }
    if u32::from_le_bytes(m[0..4].try_into().unwrap()) != IVF_STRUCT_MAGIC
        || u32::from_le_bytes(m[4..8].try_into().unwrap()) != 8
    {
        return Err("theodb ivf-aq: not a v8 structured index".into());
    }
    let metric_tag = m[8];
    let dim = u32::from_le_bytes(m[9..13].try_into().unwrap());
    let nlists = u32::from_le_bytes(m[13..17].try_into().unwrap()) as usize;
    let mval = u32::from_le_bytes(m[17..21].try_into().unwrap());
    let aq_codebook_npages = u32::from_le_bytes(m[21..25].try_into().unwrap());
    let dir_npages = u32::from_le_bytes(m[25..29].try_into().unwrap());
    let centroid_npages = u32::from_le_bytes(m[29..33].try_into().unwrap());
    let gen_base = u32::from_le_bytes(m[33..37].try_into().unwrap());
    let rabitq_codebook_npages = u32::from_le_bytes(m[37..41].try_into().unwrap());

    let dbytes = read_chunked(rel, gen_base, dir_npages)?;
    if dbytes.len() < nlists * 20 {
        return Err("theodb ivf-aq: truncated v8 directory".into());
    }
    let mut dir = Vec::with_capacity(nlists);
    for i in 0..nlists {
        let o = i * 20;
        dir.push((
            u32::from_le_bytes(dbytes[o..o + 4].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 4..o + 8].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 8..o + 12].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 12..o + 16].try_into().unwrap()),
            u32::from_le_bytes(dbytes[o + 16..o + 20].try_into().unwrap()),
        ));
    }
    let aq_codebook = read_chunked(rel, gen_base + dir_npages, aq_codebook_npages)?;
    let rabitq_codebook =
        read_chunked(rel, gen_base + dir_npages + aq_codebook_npages, rabitq_codebook_npages)?;
    let cbytes = read_chunked(
        rel,
        gen_base + dir_npages + aq_codebook_npages + rabitq_codebook_npages,
        centroid_npages,
    )?;
    let d = dim as usize;
    let mut centroids = Vec::with_capacity(nlists);
    if d > 0 && cbytes.len() >= nlists * d * 4 {
        for i in 0..nlists {
            let mut c = Vec::with_capacity(d);
            for j in 0..d {
                let o = (i * d + j) * 4;
                c.push(f32::from_le_bytes(cbytes[o..o + 4].try_into().unwrap()));
            }
            centroids.push(c);
        }
    } else if nlists != 0 {
        return Err("theodb ivf-aq: truncated v8 centroid region".into());
    }
    Ok(IvfAqMetaV8 { dim, metric_tag, m: mval, aq_codebook, rabitq_codebook, centroids, dir })
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
