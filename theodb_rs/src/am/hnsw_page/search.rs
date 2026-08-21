//! search — split from the M35 page-native `hnsw_page.rs` god-file (M126, behavior-preserving;
//! byte-identical same-index A/B). Sibling items resolve via `use super::*` (re-exported in `mod.rs`).
#![allow(unused_imports)]
use super::*;
use crate::am::page;
use crate::ann::{HnswIndex, Metric};
use pgrx::pg_sys;

/// A traversal candidate: its element address, neighbor-tuple address, level, heap tid, and distance to the query.
/// `pub(crate)` only so the `HnswResume` type alias (M118) can name it across the `am` module — its FIELDS stay
/// private (`am/scan.rs` holds a `ResumableGround<Cand>` opaquely and never touches a field).
#[derive(Clone, Copy)]
pub(crate) struct Cand {
    pub(crate) d: f64,
    pub(crate) blk: u32,
    pub(crate) off: u16,
    pub(crate) nbr_blk: u32,
    pub(crate) nbr_off: u16,
    /// M59 v4: the cold raw-f32 tuple address for this candidate. `(0,0)` for v1/v2 (their f32 is inline in the
    /// element tuple). For a v4 (AQ) index the walk carries it WITHOUT reading it; rerank follows it once per
    /// survivor to fetch the exact f32. This is the pointer that keeps the f32 out of the hot walk path.
    pub(crate) raw_blk: u32,
    pub(crate) raw_off: u16,
    pub(crate) level: u8,
    pub(crate) tid: i64,
    /// M56: a tombstoned node is navigated THROUGH (its arcs preserve connectivity — it enters the candidate
    /// heap and is expanded) but is NEVER pushed to the result set. Set from `ElementView.deleted`.
    pub(crate) deleted: bool,
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
        self.d.total_cmp(&o.d)
    }
}

// M49: 3-way fused dispatch — L2/IP/cosine all score from raw page bytes with ZERO per-node `Vec<f32>` alloc
// (was: only L2 fused; cosine/ip decoded a Vec per visited node — the ROADMAP-flagged mine). `_is_l2` is kept
// for call-site signature stability (the metric already carries the same information).
pub(crate) fn score(metric: Metric, q: &[f32], vec_bytes: &[u8], _is_l2: bool) -> f64 {
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
pub(crate) unsafe fn load(
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
    unsafe {
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
}

/// Read a candidate's neighbor addresses on `layer` (increments pages-read for the neighbor tuple).
/// M41: decodes the addrs INSIDE the pinned page scope — no `to_vec` of the neighbor tuple. `nblocks` cached.
pub(crate) unsafe fn neighbors_of(
    rel: pg_sys::Relation,
    c: &Cand,
    layer: usize,
    m: usize,
    m0: usize,
    nblocks: u32,
    reads: &mut usize,
) -> Result<Vec<Addr>, String> {
    *reads += 1;
    unsafe {
        page::with_page_item(rel, c.nbr_blk, c.nbr_off, nblocks, |b| {
            decode_neighbors(b, c.level as usize, layer, m, m0)
        })
    }
}

/// M46 L1-B: like `neighbors_of` but decodes into a caller-owned scratch `Vec` (cleared first) instead of
/// allocating a fresh one. The ground-layer loop reuses ONE scratch across every expanded node.
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe fn neighbors_into(
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
    unsafe {
        page::with_page_item(rel, c.nbr_blk, c.nbr_off, nblocks, |b| {
            decode_neighbors_into(b, c.level as usize, layer, m, m0, out)
        })
    }
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
    let nblocks = unsafe { page::main_fork_nblocks(rel) };

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
        let quant = crate::vec::aq::AqQuantizer::from_meta_bytes(&meta.aq_codebook)?;
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
    let mut ep = unsafe {
        load(
            rel,
            meta.entry_blkno,
            meta.entry_offno,
            q,
            metric,
            is_l2,
            qcode,
            lut,
            nblocks,
            &mut reads,
        )?
    };
    let mut lc = meta.entry_level as usize;
    while lc >= 1 {
        loop {
            let nbrs = unsafe { neighbors_of(rel, &ep, lc, m, m0, nblocks, &mut reads)? };
            let mut improved = false;
            for (nb, no) in nbrs {
                let cand = unsafe {
                    load(rel, nb, no, q, metric, is_l2, qcode, lut, nblocks, &mut reads)?
                };
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
    let candidates_seen: usize; // M68: the nodes navigated in the beam (observability); both arms assign it
    let out = if approximate {
        // SBQ (M51) / AQ (M59): the ground walk ranked candidates by the cheap surrogate. Widen the candidate
        // pool by `over_fetch` (scan GUC, reused for AQ per parsimony rung-4) so the true NN survives the
        // approximate ranking, then rerank the survivors by EXACT f32 — this is where recall is recovered
        // (carrier-limited, M40; ADR-0018). Only the surviving `walk_ef` pages are re-read for their f32 vectors;
        // the walk itself paid only the cheap surrogate cost.
        let over_fetch = crate::am::guc::over_fetch().max(1);
        let walk_ef = ef.saturating_mul(over_fetch);
        let (nodes, cand) =
            crate::ann::scan_core::ground_search_nodes(&pg_src, ep, walk_ef, m0, true)?;
        candidates_seen = cand; // the walk pool is `walk_ef = ef·over_fetch` here (honest — what it navigated)
        reads = pg_src.reads.get();
        let mut reranked: Vec<(i64, f64)> = Vec::with_capacity(nodes.len());
        for (cand, _ham) in &nodes {
            // v4 (AQ): the survivor's f32 is in the COLD raw tuple at `raw_addr` (the walk never read it) — follow
            // the pointer once here. v2 (SBQ): `raw_addr == (0,0)` ⇒ the f32 is inline in the element tuple, re-read
            // it as before. This one cold read per survivor (~ef·over_fetch of them) is the ONLY f32 I/O of a v4
            // scan — the whole point of the code/vector split (ADR-0019).
            let d = if cand.raw_blk != 0 {
                unsafe {
                    page::with_page_item(rel, cand.raw_blk, cand.raw_off, nblocks, |b| {
                        Ok(score(metric, q, decode_raw_vec(b)?, is_l2))
                    })?
                }
            } else {
                unsafe {
                    page::with_page_item(rel, cand.blk, cand.off, nblocks, |b| {
                        Ok(score(metric, q, decode_element(b)?.vec_bytes, is_l2))
                    })?
                }
            };
            reads += 1;
            reranked.push((cand.tid, d));
        }
        reranked.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        reranked.truncate(ef); // return the ef best by exact f32; the scan takes top-k
        reranked
    } else {
        // v1 exact f32 ground search. Use `ground_search_nodes` (not the `ground_search` wrapper) so the
        // candidates_seen count is captured for observability; map to (tid, dist) as the wrapper did.
        let (nodes, cand) = crate::ann::scan_core::ground_search_nodes(&pg_src, ep, ef, m0, true)?;
        candidates_seen = cand;
        reads = pg_src.reads.get();
        // `Cand` carries its own `tid` field (same the `ground_search` wrapper mapped via `NeighborSource::tid`);
        // use it directly so the trait need not be imported at this call site.
        nodes.into_iter().map(|(node, d)| (node.tid, d)).collect()
    };

    // M67/M68: feed the backend-local scan-stats collectors (cheap in-memory adds; no page write, no
    // crash-safety impact) so `theodb.scan_stats`/`explain_scan`/`index_scan_stats` report real per-scan cost.
    crate::am::autotune::record_scan_observation(reads as i64, candidates_seen as i64);

    if std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1") {
        // The wiring-triad runtime metric: pages read must be O(ef·M), flat in N (server LOG, not client WARNING).
        pgrx::log!(
            "theodb hnsw scan profile: pages_read={reads} ef={ef} m={m} m0={m0} results={}",
            out.len()
        );
    }
    Ok(out)
}

/// M118: the resume-from-discarded state for an HNSW iterative scan, keyed on the on-disk `Cand` node. Held by
/// `am/scan.rs::ScanState` across `amgettuple` calls so the search RESUMES from the retained frontier instead of
/// re-searching with a doubled `ef` (the M52 cost). Opaque to `scan.rs` (the private `Cand` is hidden behind this
/// alias); `scan.rs` only calls [`resumable_init`] / [`resumable_next`].
pub(crate) type HnswResume = crate::ann::scan_core::ResumableGround<Cand>;

/// M118: seed a resume-from-discarded ground search for the **V1 (exact-f32)** HNSW path. Does the upper-layer
/// greedy descent (identical to [`traverse`]) then inits a [`ResumableGround`] at the ground entry point.
/// Returns `Ok(None)` for SBQ (v2) / AQ (v3/v4) indexes — their per-batch exact-f32 rerank is a tracked
/// follow-up, so the caller keeps the M52 re-search there. Also `None` on an empty/unbuilt index.
///
/// [`ResumableGround`]: crate::ann::scan_core::ResumableGround
pub(crate) unsafe fn resumable_init(
    rel: pg_sys::Relation,
    meta: &HnswMeta,
    q: &[f32],
    ef_search: usize,
) -> Result<Option<HnswResume>, String> {
    unsafe {
        if !crate::am::guc::hnsw_resume() {
            return Ok(None); // M118 kill-switch OFF — caller uses the M52 re-search (own-path A/B baseline)
        }
        if meta.entry_level < 0 || meta.node_count == 0 {
            return Ok(None); // empty/unbuilt — caller falls back (traverse also short-circuits to [])
        }
        if meta.sbq_bits > 0 || meta.aq_m > 0 {
            return Ok(None); // SBQ/AQ rerank-per-batch is a follow-up — caller keeps the M52 re-search
        }
        let metric = Metric::from_tag(meta.metric_tag).ok_or("theodb hnsw: unknown metric tag")?;
        let is_l2 = matches!(metric, Metric::L2);
        let (m, m0) = (meta.m as usize, meta.m0 as usize);
        let ef = ef_search.max(1);
        let nblocks = page::main_fork_nblocks(rel);
        let mut reads = 0usize;
        // Greedy upper-layer descent — byte-identical to `traverse` (v1: qcode=None, lut=None).
        let mut ep = load(
            rel,
            meta.entry_blkno,
            meta.entry_offno,
            q,
            metric,
            is_l2,
            None,
            None,
            nblocks,
            &mut reads,
        )?;
        let mut lc = meta.entry_level as usize;
        while lc >= 1 {
            loop {
                let nbrs = neighbors_of(rel, &ep, lc, m, m0, nblocks, &mut reads)?;
                let mut improved = false;
                for (nb, no) in nbrs {
                    let cand =
                        load(rel, nb, no, q, metric, is_l2, None, None, nblocks, &mut reads)?;
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
            qcode: None,
            lut: None,
            m,
            m0,
            reads: std::cell::Cell::new(reads),
        };
        let rg = crate::ann::scan_core::ResumableGround::init(&pg_src, ep, ef, m0, true);
        // B-015: report the FIRST segment of this scan (upper-layer descent + the seeded ground frontier).
        // `traverse` has always reported here; this path did not, and it is the DEFAULT for every V1 index —
        // so `explain_scan`/`scan_stats`/`_index_scan_stats` read zero for the product's most common scan.
        // `pg_src.reads` already carries the descent (it was seeded with `reads` above) plus whatever `init`
        // read; `candidates_seen()` is the `visited` set the accessor was built to expose and nobody called.
        crate::am::autotune::record_scan_observation(
            pg_src.reads.get() as i64,
            rg.candidates_seen() as i64,
        );
        Ok(Some(rg))
    }
}

/// M118: pull the next resumed batch from the retained frontier (V1 path). Reconstructs the page source (cheap —
/// only field refs) and maps the batch to `(tid, dist)`. An empty result ⇒ the reachable graph is exhausted
/// (the caller marks the scan exhausted — EC-1). Pending rows are folded only on the FIRST gather, not here (the
/// scan dedups tids, so a resumed batch never needs to re-fold pending).
pub(crate) unsafe fn resumable_next(
    rel: pg_sys::Relation,
    meta: &HnswMeta,
    q: &[f32],
    rg: &mut HnswResume,
) -> Result<Vec<(i64, f64)>, String> {
    unsafe {
        let metric = Metric::from_tag(meta.metric_tag).ok_or("theodb hnsw: unknown metric tag")?;
        let is_l2 = matches!(metric, Metric::L2);
        let (m, m0) = (meta.m as usize, meta.m0 as usize);
        let nblocks = page::main_fork_nblocks(rel);
        let pg_src = PageNeighborSource {
            rel,
            nblocks,
            q,
            metric,
            is_l2,
            qcode: None,
            lut: None,
            m,
            m0,
            reads: std::cell::Cell::new(0),
        };
        // B-015: `candidates_seen()` is CUMULATIVE on `rg`, and `record_scan_observation` ADDS — so reporting
        // the running total here would count every earlier batch again on each resumed pull. The delta around
        // `next_batch` is this segment's own contribution, and it needs no new state on `HnswResume`. Pages do
        // not need the same care: `pg_src` is rebuilt per call with `reads: Cell::new(0)`, so its counter is
        // already per-segment.
        let candidates_before = rg.candidates_seen();
        let batch = rg.next_batch(&pg_src)?;
        crate::am::autotune::record_scan_observation(
            pg_src.reads.get() as i64,
            rg.candidates_seen().saturating_sub(candidates_before) as i64,
        );
        Ok(batch.into_iter().map(|(cand, d)| (cand.tid, d)).collect())
    }
}

/// The production [`scan_core::NeighborSource`]: drives the ground search over PostgreSQL pages by reusing the
/// existing `load` + `neighbors_into` page readers (FU-1). `Node` is the on-disk `Cand` (distance + tid + the
/// neighbor-tuple address for expansion); `Ref` is a neighbor element address `(blk,off)`. The page-read counter
/// is threaded through a `Cell` (the trait methods take `&self`); it mirrors the pre-FU-1 `&mut reads` exactly.
pub(crate) struct PageNeighborSource<'a> {
    pub(crate) rel: pg_sys::Relation,
    pub(crate) nblocks: u32,
    pub(crate) q: &'a [f32],
    pub(crate) metric: Metric,
    pub(crate) is_l2: bool,
    /// M51: the quantized query code (SBQ index) — `Some` ⇒ the walk scores by Hamming; `None` ⇒ f32.
    pub(crate) qcode: Option<&'a [u8]>,
    /// M59: the per-query AH LUT (AQ v3 index) — `Some` ⇒ the walk scores by asymmetric hashing; `None` ⇒
    /// falls through to `qcode`/f32. AQ ⊥ SBQ per index (D1), so at most one of `lut`/`qcode` is `Some`.
    pub(crate) lut: Option<&'a crate::vec::ah::Lut16>,
    pub(crate) m: usize,
    pub(crate) m0: usize,
    pub(crate) reads: std::cell::Cell<usize>,
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
        let r = unsafe {
            neighbors_into(self.rel, node, 0, self.m, self.m0, self.nblocks, &mut reads, out)
        };
        self.reads.set(self.reads.get() + reads);
        r
    }
    fn load(&self, r: &Addr) -> Result<Cand, String> {
        let mut reads = 0usize;
        let cand = unsafe {
            load(
                self.rel,
                r.0,
                r.1,
                self.q,
                self.metric,
                self.is_l2,
                self.qcode,
                self.lut,
                self.nblocks,
                &mut reads,
            )
        };
        self.reads.set(self.reads.get() + reads);
        cand
    }
}
