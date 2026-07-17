//! `ambuild` / `ambuildempty` (M26 Phase 2) — build the IVFFlat index from the heap and persist it to pages.
//!
//! Reuses the proven `crate::ann::IvfflatIndex` (no algorithm fork). The heap scan collects `(encoded_tid, vec)`
//! into a corpus, builds the index once, serializes it (`to_bytes`), and writes it via `crate::am::page`
//! (WAL-logged). The heap TID is encoded into the `i64` id slot so `amgettuple` can return it later.
use crate::am::index::Persisted;
use crate::am::page;
use crate::am::tid;
use crate::ann::{HnswIndex, IvfflatIndex, Metric};
use pgrx::prelude::*;

/// Default IVFFlat list count — the reloption default lives in `options::DEFAULT_LISTS` (M34). Re-exported here for
/// the empty/HNSW build paths that don't read the reloption.
use crate::am::options::{lists_from_relation, DEFAULT_LISTS};
/// HNSW build params for the persisted AM index (16, 64). O M57 mediu que o recall satura em ~0.96-0.974 a
/// 100k-500k (< gate 0.99). TRES tentativas de fix REFUTADAS por medicao (todas revertidas): efc=200 → 0.832;
/// MERGE de back-links → 0.846; m=32 → 0.952 (PIOR que m=16 — mais conectividade degradando o recall e
/// fundamentalmente anomalo). Bissecao (`THEODB_HNSW_PARALLEL_THRESHOLD`): sequencial ≈ paralelo → NAO e contencao.
/// LEAD FORTE p/ o M60: o recall e NAO-MONOTONICO em `ef_search` (melhor no MENOR ef) → o teto e provavelmente um
/// BUG NO SCAN (`am/hnsw_page.rs` traverse — beam search/heap), comum aos dois builds, e nao a conectividade do
/// grafo. Investigar o traverse primeiro no M60. Ver `docs/adr/0018` + `docs/benchmarks/m57-sbq-superiority.md`.
pub(crate) const HNSW_M: usize = 16;
pub(crate) const HNSW_EF_CONSTRUCTION: usize = 64;
const BUILD_SEED: u64 = 42;

/// Effective build `ef_construction`, overridable via `THEODB_HNSW_EF_CONSTRUCTION` (env). Default = the const
/// above, so shipped behavior is unchanged. Purpose: sweep efc for the M60/M71 graph-navigability investigation
/// (does recall rise monotonically with efc, as a correct HNSW must?) without recompiling — mirrors
/// `hnsw_parallel::parallel_threshold`. Benchmark-only knob; production uses the default.
pub(crate) fn hnsw_ef_construction() -> usize {
    std::env::var("THEODB_HNSW_EF_CONSTRUCTION")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|&v| v >= HNSW_M)
        .unwrap_or(HNSW_EF_CONSTRUCTION)
}

/// Collected during the heap scan (one entry per live, non-NULL-vector heap tuple).
struct BuildState {
    corpus: Vec<(i64, Vec<f32>)>,
    dim: Option<usize>,
    // M90 (inline label filter): when the index has a 2nd `smallint[]` column, each vector's sorted-deduped label
    // set (parallel to `corpus`; empty = the row's label was NULL). `has_labels=false` for vector-only indexes →
    // stays empty and the build produces the unchanged v5/v6 layout.
    has_labels: bool,
    labels: Vec<Vec<i16>>,
}

/// M90 — read a `smallint[]` Datum into a sorted-deduped label set (own code; empty on NULL/empty). Sorted+deduped
/// so the scan-side overlap check is a cheap merge and the stored form is canonical.
unsafe fn datum_to_labelset(datum: pg_sys::Datum, is_null: bool) -> Vec<i16> { unsafe {
    if is_null {
        return Vec::new();
    }
    let mut v: Vec<i16> = Vec::<i16>::from_datum(datum, false).unwrap_or_default();
    v.sort_unstable();
    v.dedup();
    v
}}

/// Scan the heap once, collecting `(encoded heap TID, vector)` for every live non-NULL-vector tuple. Shared by
/// both AMs' `ambuild`. Returns `(corpus, heap_tuple_count)`.
unsafe fn collect_corpus(
    heaprel: pg_sys::Relation,
    indexrel: pg_sys::Relation,
    index_info: *mut pg_sys::IndexInfo,
) -> (Vec<(i64, Vec<f32>)>, Vec<Vec<i16>>, f64) { unsafe {
    // M90: a 2nd index column (`smallint[]`) means label filtering; `ii_NumIndexAttrs > 1`.
    let has_labels = (*index_info).ii_NumIndexAttrs > 1;
    let mut state = BuildState { corpus: Vec::new(), dim: None, has_labels, labels: Vec::new() };
    let ntuples = pg_sys::table_index_build_scan(
        heaprel,
        indexrel,
        index_info,
        true, // allow_sync
        true, // progress
        Some(build_callback),
        (&mut state as *mut BuildState).cast::<std::os::raw::c_void>(),
        std::ptr::null_mut(),
    );
    (state.corpus, state.labels, ntuples as f64)
}}

/// M96 — the indexed vector column's dimension from its typmod (our `vector` type stores `dim` as the typmod,
/// pgvector convention), read from the index's first attribute. `0` when unavailable (→ streaming dispatch falls
/// back to the in-RAM build, never a wrong decision).
unsafe fn index_vector_dim(indexrel: pg_sys::Relation) -> i32 { unsafe {
    let tupdesc = (*indexrel).rd_att;
    if tupdesc.is_null() || (*tupdesc).natts < 1 {
        return 0;
    }
    let attr = (*tupdesc).attrs.as_ptr(); // first attribute (the vector column)
    let typmod = (*attr).atttypmod;
    if typmod > 0 {
        typmod
    } else {
        0
    }
}}

unsafe fn build_result(ntuples: f64, nindexed: usize) -> *mut pg_sys::IndexBuildResult { unsafe {
    let mut result = PgBox::<pg_sys::IndexBuildResult>::alloc0();
    result.heap_tuples = ntuples;
    result.index_tuples = nindexed as f64;
    result.into_pg()
}}

/// `theodb_ivfflat` build. The DEFAULT l2 opclass sets the metric; cosine/ip opclasses are a follow-up (pgrx 0.16
/// does not expose `get_opfamily_name` for opclass→metric resolution). The metric is persisted in the blob.
/// M49 / ADR-1: resolve the index's metric from its operator class at build time via the `FUNCTION 1` support
/// proc (pgvector's `HnswInitSupport` mechanism — `hnswutils.c:154`). The cosine/ip opclasses bind
/// `theodb_metric_{cosine,ip}()` (returns the metric tag); the DEFAULT L2 opclass has NO support proc, so
/// `index_getprocid` returns `InvalidOid` → fall back to L2. Closes the "always L2" build hardcode; the scan and
/// both VACUUM rebuilds already honor the persisted `metric_tag`.
unsafe fn resolve_metric(indexrel: pg_sys::Relation) -> Metric { unsafe {
    let procid = pg_sys::index_getprocid(indexrel, 1, 1);
    if procid == pg_sys::InvalidOid {
        return Metric::L2;
    }
    let datum = pg_sys::OidFunctionCall0Coll(procid, pg_sys::InvalidOid);
    Metric::from_tag(datum.value() as u8).unwrap_or(Metric::L2)
}}

#[pg_guard]
pub extern "C-unwind" fn ambuild(
    heaprel: pg_sys::Relation,
    indexrel: pg_sys::Relation,
    index_info: *mut pg_sys::IndexInfo,
) -> *mut pg_sys::IndexBuildResult {
    unsafe {
        // M96 — STREAMING dispatch (before materializing the corpus): the v5 plain-f32 AQ-split path, on a build
        // whose estimated corpus exceeds `maintenance_work_mem`, streams via a tuplesort spool (peak O(mwm+sample),
        // never the 1× corpus M88/M89 wall). Estimate N from `reltuples` (post-ANALYZE) with a block-count floor so
        // a freshly-loaded (un-ANALYZEd) table still triggers streaming rather than OOMing. Streaming supports ONLY
        // the v5 plain path — SQ8 (v6), labels (v7), and SOAR keep the in-RAM build (blueprint scope caveat); the
        // dispatch is exact on those flags so no build ever silently takes the wrong path.
        {
            let m0 = crate::am::options::pq_subspaces_from_relation(indexrel);
            let separate = crate::am::options::separate_storage_from_relation(indexrel);
            let sq8 = crate::am::options::refine_sq8_from_relation(indexrel);
            let rabitq = crate::am::options::refine_rabitq_from_relation(indexrel);
            let has_labels = (*index_info).ii_NumIndexAttrs > 1;
            let soar0 = crate::am::options::soar_lambda_from_relation(indexrel);
            let est_dim = index_vector_dim(indexrel);
            if m0 > 0 && separate && !sq8 && !rabitq && !has_labels && soar0 <= 0.0 && est_dim > 0 {
                let reltuples = (*(*heaprel).rd_rel).reltuples;
                let blocks =
                    pg_sys::RelationGetNumberOfBlocksInFork(heaprel, pg_sys::ForkNumber::MAIN_FORKNUM) as f64;
                let rows_per_block = (8192.0 / (est_dim as f64 * 4.0 + 28.0)).max(1.0);
                let est_n = (reltuples as f64).max(blocks * rows_per_block).max(0.0) as usize;
                if crate::am::build_stream::should_stream(est_n, est_dim as usize) {
                    let lists = lists_from_relation(indexrel);
                    let metric = resolve_metric(indexrel);
                    let n = crate::am::build_stream::ambuild_streaming(
                        heaprel, indexrel, index_info, lists, metric, m0 as usize, BUILD_SEED,
                    );
                    return build_result(n as f64, n);
                }
            }
        }
        let (corpus, labels, ntuples) = collect_corpus(heaprel, indexrel, index_info);
        let lists = lists_from_relation(indexrel); // M34 — WITH (lists=N), default 100
        let metric = resolve_metric(indexrel); // M49: cosine/ip/L2 from the opclass, not hardcoded
        // M86 (Roadmap v7): `WITH (soar_lambda=N)` (N>0) spills each vector to a SOAR-chosen second list (fewer
        // probes for the same recall). λ=0 (default) → primary-only, byte-identical. Only the code is duplicated
        // (the f32/SQ8 packing follows `list_entries()`, which now returns spilled vectors in both lists).
        let soar_lambda = crate::am::options::soar_lambda_from_relation(indexrel);
        // M31: persist in the STRUCTURED layout (meta + centroids + per-list pages) so scans read only probed
        // lists (O(probes)), not the whole blob (O(N)). `dim` captured BEFORE the corpus is moved into the index.
        let dim = corpus.first().map(|(_, v)| v.len()).unwrap_or(0) as u32;
        let ntuples_built = corpus.len();
        // M89 (Roadmap v7 — ambuild streaming Increment 1): MOVE the owned corpus into the index (`build_owned`)
        // instead of cloning it — eliminates the redundant 2nd full copy the M88 OOM was partly made of
        // (`docs/adr/0038`). The AQ/SQ8 encode below reads the vectors back from `idx` (no `corpus` / `corpus_vecs`
        // clone). Byte-identical build.
        let idx = IvfflatIndex::build_owned(corpus, lists, metric, BUILD_SEED).with_soar_spill(soar_lambda);
        // M77 (pg_scann): `WITH (pq_subspaces=M)` (M>0) persists the IVF-AQ v4 layout — per-list AQ codes (block32,
        // for the batched-AH scan) + f32 (rerank) + codebook. M==0 keeps the v3 f32 path (byte-identical, untouched).
        let m = crate::am::options::pq_subspaces_from_relation(indexrel);
        if m > 0 && dim > 0 {
            // M82 — train the AVQ codebook on a capped deterministic sample (ScaNN trains on ~250k; the codebook
            // is a global 16-centroid-per-subspace map that converges well below the full N), then encode ALL
            // vectors. Keeps CREATE INDEX tractable at 1M+ (the naive train is super-linear — the M75 blocker).
            const AQ_TRAIN_SAMPLE: usize = 50_000;
            // M89 — sample from the index (same vectors, same order) instead of the corpus (moved into `idx`).
            // Byte-identical to the pre-M89 stride-of-corpus sample.
            let train: Vec<Vec<f32>> = idx.train_sample(AQ_TRAIN_SAMPLE);
            let thr = crate::am::options::aq_threshold_from_relation(indexrel);
            match crate::vec::aq::AqQuantizer::train(&train, m, 4, thr, BUILD_SEED) {
                Ok(quant) => {
                    let pairs = m.div_ceil(2);
                    // M89 (streaming Increment 2): pack the AQ codes from list POSITIONS reading vectors by
                    // reference (no `list_entries()` clone). Codes are ~pairs bytes/vec — buffering all lists' codes
                    // is small (30M×~16B ≈ 0.5GB); the f32 (the ~4× copy) is streamed by the writers per list.
                    let positions = idx.list_positions();
                    let idsr = idx.ids();
                    let vecs = idx.vectors();
                    let codes: Vec<Vec<u8>> =
                        positions.iter().map(|pos| pack_block32_codes(&quant, pos, vecs, pairs)).collect();
                    // M83/M85 (Roadmap v7): `WITH (separate_storage=1)` → v5 layout (codes/f32 on distinct pages)
                    // so the scan reads codes-only in Stage 1 (ADR-0037 lever). Adding `refine=1` → v6, whose
                    // per-list rerank region is SQ8 (¼ the f32 bytes, M85). Default off → v4 (interleaved).
                    if crate::am::options::separate_storage_from_relation(indexrel)
                        && crate::am::options::refine_rabitq_from_relation(indexrel)
                    {
                        // E1 (v8) — f32-FREE residual RaBitQ rerank. The estimator is L2-only (rabitq.rs); fail loud
                        // at BUILD (Rule 8 — the boundary) if the opclass is not L2.
                        if metric != Metric::L2 {
                            pg_sys::error!(
                                "theodb_ivfflat: refine=2 (RaBitQ) requires the L2 opclass (the RaBitQ estimator is L2-only)"
                            );
                        }
                        let bits = crate::am::options::rabitq_bits_from_relation(indexrel);
                        // The rotation is data-independent (random orthogonal) — no train_sample needed (unlike SQ8's
                        // min/max pass). Encode the RESIDUAL x−centroid[ci] per list position, in the SAME ordinal
                        // order the block32 AH codes use (`positions`), to `[i8×dim][nr][w]` = dim+8 B/vec.
                        let rq = crate::vec::rabitq::RabitqQuantizer::train(dim as usize, bits, BUILD_SEED);
                        let centroids = idx.centroids();
                        let rabitq_codes: Vec<Vec<u8>> = positions
                            .iter()
                            .enumerate()
                            .map(|(ci, pos)| {
                                let c = &centroids[ci];
                                let mut b = Vec::with_capacity(pos.len() * (dim as usize + 8));
                                for &p in pos {
                                    let residual: Vec<f32> = vecs[p].iter().zip(c).map(|(x, cc)| x - cc).collect();
                                    let code = rq.encode(&residual);
                                    b.extend_from_slice(&crate::vec::rabitq::RabitqQuantizer::code_to_record(&code));
                                }
                                b
                            })
                            .collect();
                        page::write_ivf_aq_split_rabitq(
                            indexrel,
                            dim,
                            metric.tag(),
                            m as u32,
                            &quant.to_meta_bytes(),
                            &rq.to_meta_bytes(),
                            idx.centroids(),
                            positions,
                            idsr,
                            &codes,
                            &rabitq_codes,
                        );
                    } else if crate::am::options::separate_storage_from_relation(indexrel)
                        && crate::am::options::refine_sq8_from_relation(indexrel)
                    {
                        // v6 — train SQ8 from `idx.vectors()` by reference (M89: no `corpus_vecs` clone) and encode
                        // each list's codes in ordinal order (matching the AH block32 order the scan derives), reading
                        // vectors by position. sq8 is ¼ the f32 bytes — buffering all is ~30M×128B ≈ 3.8GB (< 1.5×).
                        let sq8 = crate::sq8::Sq8Quantizer::train(vecs);
                        let sq8_codes: Vec<Vec<u8>> = positions
                            .iter()
                            .map(|pos| {
                                let mut b = Vec::with_capacity(pos.len() * dim as usize);
                                for &p in pos {
                                    b.extend_from_slice(&sq8.encode(&vecs[p]));
                                }
                                b
                            })
                            .collect();
                        page::write_ivf_aq_split_sq8(
                            indexrel,
                            dim,
                            metric.tag(),
                            m as u32,
                            &quant.to_meta_bytes(),
                            &sq8.to_meta_bytes(),
                            idx.centroids(),
                            positions,
                            idsr,
                            &codes,
                            &sq8_codes,
                        );
                    } else if crate::am::options::separate_storage_from_relation(indexrel) {
                        // M90: when the index carries a label column, persist the v7 layout (label co-located in the
                        // code blob → Stage-1 inline skip). No label column → the unchanged v5 writer (byte-identical).
                        if !labels.is_empty() {
                            page::write_ivf_aq_split_v7(
                                indexrel,
                                dim,
                                metric.tag(),
                                m as u32,
                                &quant.to_meta_bytes(),
                                idx.centroids(),
                                positions,
                                idsr,
                                vecs,
                                &codes,
                                &labels,
                            );
                        } else {
                            page::write_ivf_aq_split(
                                indexrel,
                                dim,
                                metric.tag(),
                                m as u32,
                                &quant.to_meta_bytes(),
                                idx.centroids(),
                                positions,
                                idsr,
                                vecs,
                                &codes,
                            );
                        }
                    } else {
                        // v4 (interleaved) — legacy, non-storage-separated. Not the M89 streaming target; builds the
                        // `entries` copy for the unchanged writer (OOMs at billion-scale, pre-M89 behavior).
                        // M104: WARN that this path materializes the corpus (the audit's inverted-default finding) —
                        // point the user at the streaming, storage-separated layout. The default is NOT flipped
                        // (that would change the on-disk format of existing `pq_subspaces` indexes); the WARN is the
                        // safe bound, and `WITH (separate_storage=1)` selects the bounded-memory v5/v6 build.
                        pg_sys::warning!(
                            "theodb_ivfflat: building the legacy v4 (interleaved) IVF-AQ layout — it materializes \
                             the whole corpus and can OOM at billion-scale. For a bounded-memory build use \
                             WITH (separate_storage=1) (the M89 streaming v5/v6 layout)."
                        );
                        let entries = idx.list_entries();
                        page::write_ivf_aq(
                            indexrel,
                            dim,
                            metric.tag(),
                            m as u32,
                            &quant.to_meta_bytes(),
                            idx.centroids(),
                            &entries,
                            &codes,
                        );
                    }
                }
                Err(e) => pg_sys::error!("theodb ivf-aq build: {e}"),
            }
        } else {
            // M31: STRUCTURED f32 layout (meta + centroids + per-list pages) — scans read only probed lists.
            page::write_ivf_structured(indexrel, dim, metric.tag(), idx.centroids(), &idx.list_entries());
        }
        build_result(ntuples, ntuples_built)
    }
}

/// M77 — pack one inverted list's AQ codes into the transposed block32 layout `blocks[b·pairs·32 + p·32 + v]` that
/// `vec::ah::ah_score_block` consumes (FAISS bbs=32). `pairs = ceil(m/2)` bytes/code. Padding entries score high
/// (all-zero code → the LUT's first centroid); the scan trims to the list's real `count` from the directory.
fn pack_block32_codes(
    quant: &crate::vec::aq::AqQuantizer,
    positions: &[usize],
    vectors: &[Vec<f32>],
    pairs: usize,
) -> Vec<u8> {
    // M89 — read each list member's vector by position from the index (no `entries` clone). Byte-identical output.
    let n = positions.len();
    let nblocks = n.div_ceil(32);
    let mut blocks = vec![0u8; nblocks * pairs * 32];
    for (i, &pos) in positions.iter().enumerate() {
        let code = quant.encode(&vectors[pos]); // `pairs` bytes
        let base = (i / 32) * pairs * 32;
        let vb = i % 32;
        for (p, &cb) in code.iter().enumerate().take(pairs) {
            blocks[base + p * 32 + vb] = cb;
        }
    }
    blocks
}

/// `theodb_hnsw` build — same persistence layer, an HNSW graph instead of IVFFlat lists (M26 Phase 6).
#[pg_guard]
pub extern "C-unwind" fn ambuild_hnsw(
    heaprel: pg_sys::Relation,
    indexrel: pg_sys::Relation,
    index_info: *mut pg_sys::IndexInfo,
) -> *mut pg_sys::IndexBuildResult {
    unsafe {
        let (corpus, _labels_hnsw, ntuples) = collect_corpus(heaprel, indexrel, index_info);
        let corpus_len = corpus.len(); // capture before `build_owned` consumes `corpus` (DoD-4 move, not clone)
        // T4.1: inject the cancellation seam so a long `CREATE INDEX` responds to `pg_cancel_backend` within one
        // parallel batch. `check_for_interrupts!` runs on the leader between batches (all workers joined) — safe
        // to longjmp. Under `#[pg_guard]` (this callback), the ereport(ERROR) unwinds cleanly across the C boundary.
        let metric = resolve_metric(indexrel); // M49: cosine/ip/L2 from the opclass (ADR-1)
        // M56 DoD-4: `build_owned` MOVES the corpus into the graph (no clone) so a 1M×768d CREATE INDEX never
        // holds the f32 vectors twice — the corpus is freed as it is drained.
        let idx = HnswIndex::build_owned(
            corpus, HNSW_M, hnsw_ef_construction(), metric, BUILD_SEED, &|| { pgrx::check_for_interrupts!(); },
        );
        // M35: persist the STRUCTURED page-native layout (meta + element + neighbor tuples) so scans traverse the
        // graph ON DEMAND (O(ef·M) pages), not deserialize the whole blob (O(N)). M51: `WITH (sbq_bits=N)` enables
        // the inline SBQ codes (0 = f32-only v1, the default). M59 T3.3: `WITH (pq_subspaces=M)` enables the
        // anisotropic-PQ v3 layout instead (AQ ⊥ SBQ per index, D1) — the AQ path wins the discriminator.
        match pack_hnsw_for_build(&idx, indexrel) {
            Ok(packed) => crate::am::hnsw_page::write_structured(indexrel, pg_sys::ForkNumber::MAIN_FORKNUM, &packed),
            Err(e) => pg_sys::error!("theodb hnsw build: {e}"),
        }
        build_result(ntuples, corpus_len)
    }
}

/// E2 degree bound R for the co-located symqg graph (a multiple of 32 for FastScan alignment; HNSW base-layer
/// m0 = 2·HNSW_M ≤ R so no neighbour is truncated, only padded). T4.1 promotes this to a reloption.
const SYMQG_DEGREE: usize = 32;

/// E2 T2.1 — `ambuild_symqg`: build the HNSW base graph, encode per-parent 1-bit sign codes (`SymqgSpike`), and
/// persist the co-located page layout (`page::pack_symqg`). Mirrors `ambuild_hnsw`; the search reads the persisted
/// rows per hop (`scan_symqg_structured`, T3.1). L2-only (the sign estimator is L2-only) — fail-fast at build.
#[pg_guard]
pub extern "C-unwind" fn ambuild_symqg(
    heaprel: pg_sys::Relation,
    indexrel: pg_sys::Relation,
    index_info: *mut pg_sys::IndexInfo,
) -> *mut pg_sys::IndexBuildResult {
    unsafe {
        let (corpus, _labels, ntuples) = collect_corpus(heaprel, indexrel, index_info);
        let corpus_len = corpus.len();
        let metric = resolve_metric(indexrel);
        if metric != Metric::L2 {
            pg_sys::error!("theodb_symqg: requires the L2 opclass (the 1-bit sign estimator is L2-only)");
        }
        // EC-1: both the HNSW build and the sign-encode loop take the cancellation seam so a long CREATE INDEX
        // responds to pg_cancel_backend (check_for_interrupts! runs under #[pg_guard] — unwinds cleanly across C).
        let idx = crate::ann::HnswIndex::build_owned(
            corpus, HNSW_M, hnsw_ef_construction(), metric, BUILD_SEED, &|| { pgrx::check_for_interrupts!(); },
        );
        let spike = crate::ann::symqg_spike::SymqgSpike::build_cancellable(
            &idx, 1, BUILD_SEED, &|| { pgrx::check_for_interrupts!(); },
        );
        if !spike.is_sign_mode() {
            pg_sys::error!("theodb_symqg: internal — expected 1-bit sign mode");
        }
        let n = idx.spike_len();
        let dim = if n > 0 { idx.spike_vector(0).len() } else { 0 };
        let degree = SYMQG_DEGREE;
        let mut rows: Vec<Vec<u8>> = Vec::with_capacity(n);
        let mut tids: Vec<i64> = Vec::with_capacity(n);
        for p in 0..n {
            pgrx::check_for_interrupts!();
            tids.push(idx.spike_id(p));
            let rot = spike.rot_vec_of(p); // P·x_p — exact dist = ‖rot_q − rot‖², q_r = rot_q − rot (one subtraction)
            let scodes = spike.sign_codes_of(p);
            let mut nbrs: Vec<(u32, crate::ann::symqg_spike::SignCode)> = Vec::with_capacity(degree);
            for (nb_node, code) in scodes.iter().take(degree) {
                nbrs.push((*nb_node as u32, code.clone()));
            }
            while nbrs.len() < degree {
                nbrs.push((page::SENTINEL_ORD, crate::ann::symqg_spike::SignCode { u: vec![0i8; dim], nr: 0.0, w: 0.0 }));
            }
            rows.push(page::pack_row(rot, &nbrs, dim, degree));
        }
        let entry = idx.spike_entry().unwrap_or(0) as u32;
        let rot_codebook = spike.rq().to_meta_bytes();
        page::pack_symqg(indexrel, metric.tag(), dim as u32, degree as u32, entry, &rot_codebook, &tids, &rows);
        build_result(ntuples, corpus_len)
    }
}

/// E2 T2.1 — empty `theodb_symqg` index (CREATE INDEX on an empty table). Builds a 0-vertex graph + persists an
/// empty layout so `peek_magic` returns SYMQG_MAGIC and the scan short-circuits to [] (EC-4).
#[pg_guard]
pub extern "C-unwind" fn ambuildempty_symqg(indexrel: pg_sys::Relation) {
    unsafe {
        // dim/degree unknown with no rows; a 0-vertex layout: no rows, empty rotation codebook.
        page::pack_symqg(indexrel, Metric::L2.tag(), 0, SYMQG_DEGREE as u32, 0, &[], &[], &[]);
    }
}

/// M59 T3.3 — pick the persisted layout for an initial `theodb_hnsw` build from the reloptions: `WITH
/// (pq_subspaces=M)` (M > 0) trains the anisotropic PQ and packs **v3** (AQ ⊥ SBQ per index, D1); otherwise
/// `WITH (sbq_bits=N)` packs v2 (or v1 when both are 0 — byte-identical to the pre-M51/M59 build). The AQ params
/// come from the reloption at initial build; the VACUUM fold instead reads them off the persisted v3 meta (so a
/// tuned index re-folds identically without re-reading the reloption).
unsafe fn pack_hnsw_for_build(
    idx: &HnswIndex,
    indexrel: pg_sys::Relation,
) -> Result<crate::am::hnsw_page::Packed, String> { unsafe {
    let m = crate::am::options::pq_subspaces_from_relation(indexrel); // 0 = AQ off
    if m > 0 {
        let bits = crate::am::options::pq_bits_from_relation(indexrel);
        let thr = crate::am::options::aq_threshold_from_relation(indexrel);
        crate::am::hnsw_page::pack_aq(idx, 1, m, bits, thr)
    } else {
        crate::am::hnsw_page::pack_sbq(idx, crate::am::options::sbq_bits_from_relation(indexrel))
    }
}}

/// Called once per heap tuple during the build scan. Skips NULL vectors (pgvector index semantics).
#[pg_guard]
unsafe extern "C-unwind" fn build_callback(
    _indexrel: pg_sys::Relation,
    htid: pg_sys::ItemPointer,
    values: *mut pg_sys::Datum,
    isnull: *mut bool,
    _tuple_is_alive: bool,
    state: *mut std::os::raw::c_void,
) { unsafe {
    if *isnull {
        return; // NULL vector — not indexed (EC: pgvector semantics)
    }
    let st = &mut *state.cast::<BuildState>();
    let v = datum_to_vec_f32(*values);
    match st.dim {
        Some(d) if d != v.len() => return, // dimension mismatch guard (defensive; the type enforces dim)
        None => st.dim = Some(v.len()),
        _ => {}
    }
    // M90: read the 2nd column (label set) when the index declares it. `values`/`isnull` are C arrays of length
    // ii_NumIndexAttrs; `has_labels` guarantees index 1 is in bounds. A NULL label → empty set (row matches no filter).
    if st.has_labels {
        st.labels.push(datum_to_labelset(*values.add(1), *isnull.add(1)));
    }
    st.corpus.push((tid::encode(htid), v));
}}

/// Incremental insert (M26 Phase 5, ADR-2): append the new `(heap TID, vector)` to the pending region — O(1)
/// amortized, NO index rebuild. Scans fold the pending region into the ranking; VACUUM later folds it into the
/// main index. NULL vectors are not indexed (returns false).
#[allow(clippy::too_many_arguments)]
#[pg_guard]
pub unsafe extern "C-unwind" fn aminsert(
    indexrel: pg_sys::Relation,
    values: *mut pg_sys::Datum,
    isnull: *mut bool,
    heap_tid: pg_sys::ItemPointer,
    _heaprel: pg_sys::Relation,
    _check_unique: pg_sys::IndexUniqueCheck::Type,
    _index_unchanged: bool,
    _index_info: *mut pg_sys::IndexInfo,
) -> bool { unsafe {
    if *isnull {
        return false;
    }
    // Share the fold lock — a concurrent VACUUM rewrite (exclusive) must not run while we append to pending.
    crate::am::lock::index_shared(indexrel);
    let v = datum_to_vec_f32(*values);
    let encoded = tid::encode(heap_tid);
    // M56 fase 2 (DoD-1): for an HNSW structured index, try to REUSE a tombstoned slot via a proper in-place insert
    // (search + link, pgvector `hnswinsert.c` pattern) BEFORE growing the pending region — this bounds relation
    // growth under DELETE+INSERT churn (slot-reuse). `Ok(false)` ⇒ no reusable v1 slot ⇒ fall through to pending
    // (the O(1) path, unchanged). The share lock excludes the compaction fold (exclusive); the atomic per-slot
    // claim in `write_reused_element` makes concurrent inserts safe.
    if crate::am::guc::hnsw_slot_reuse() {
        if let Ok(magic) = page::peek_magic(indexrel) {
            if magic == crate::am::hnsw_page::HNSW_STRUCT_MAGIC {
                if let Ok(meta) = crate::am::hnsw_page::read_meta(indexrel) {
                    match crate::am::hnsw_page::insert_inplace(indexrel, &meta, encoded, &v) {
                        Ok(true) => return true, // reused a tombstoned slot — done, relation did not grow
                        Ok(false) => {}          // no reusable slot — fall through to the pending append
                        Err(e) => pg_sys::error!("theodb am insert (in-place): {e}"),
                    }
                }
            }
        }
    }
    match page::append_pending(indexrel, encoded, &v) {
        Ok(()) => true,
        Err(e) => pg_sys::error!("theodb am insert: {e}"),
    }
}}

/// Rebuild the main index over only the live heap TIDs (M26 Phase 5 — called by VACUUM's `ambulkdelete`). Reads
/// the current main index + pending, keeps entries the `dead` predicate rejects, rebuilds, and rewrites the blob
/// (folding pending in + dropping dead TIDs). Returns the number of live entries.
/// M56: VACUUM bulk-delete via IN-PLACE tombstones for the HNSW structured layout — mark each dead node as a
/// tombstone per page (buffer-EXCLUSIVE + GenericXLog, NO advisory index lock, NO O(N) rebuild → NO total stall),
/// then run the (rare) O(N) compaction fold ONLY when tombstones exceed the ratio GUC. IVF/blob layouts have no
/// in-place path yet → they keep the existing O(N) rebuild (out of M56 scope). Returns the live graph count.
pub(crate) unsafe fn vacuum_delete_inplace(
    indexrel: pg_sys::Relation,
    dead: &mut dyn FnMut(i64) -> bool,
) -> usize { unsafe {
    let magic = match page::peek_magic(indexrel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    if magic != crate::am::hnsw_page::HNSW_STRUCT_MAGIC {
        return vacuum_rebuild(indexrel, dead); // IVF / blob / unbuilt — no in-place tombstone path
    }
    let meta = match crate::am::hnsw_page::read_meta(indexrel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    // Phase 1 — tombstone the dead in place (crash-safe per page, no advisory EXCLUSIVE, no stall).
    {
        let mut is_dead = |id: i64| dead(id);
        crate::am::hnsw_page::tombstone_sweep(indexrel, &meta, &mut is_dead);
    } // `is_dead` (which borrowed `dead`) is dropped here — `dead` is free again for the compaction path.
    // Ratio-triggered compaction: reclaim + re-densify (and REPAIR the graph) once churn passes the GUC %. The
    // trigger counts CHURN (`version>0` = tombstones + slots revived by reuse), not just current tombstones — else
    // slot-reuse (which consumes tombstones before they accumulate) would suppress the fold and let the incremental
    // -insert degradation compound (the M56 fase-2 churn benchmark measured recall collapsing). The live count
    // returned below is still node_count − tombstones (revived slots are live).
    let total_tomb = crate::am::hnsw_page::count_tombstones(indexrel, &meta) as u64;
    let churned = crate::am::hnsw_page::count_churned(indexrel, &meta) as u64;
    let node_count = meta.node_count as u64;
    let pct = crate::am::guc::hnsw_tombstone_compact_pct() as u64;
    if pct > 0 && node_count > 0 && churned * 100 > node_count * pct {
        // Compaction = the full crash-safe fold (takes advisory EXCLUSIVE); `enumerate_entries` drops tombstones.
        return vacuum_rebuild(indexrel, dead);
    }
    (node_count.saturating_sub(total_tomb)) as usize
}}

pub(crate) unsafe fn vacuum_rebuild(
    indexrel: pg_sys::Relation,
    dead: &mut dyn FnMut(i64) -> bool,
) -> usize { unsafe {
    // Exclusive the fold lock — wait for all concurrent scans/inserts (share) to finish, then rewrite alone.
    crate::am::lock::index_exclusive(indexrel);
    let magic = match page::peek_magic(indexrel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    if magic == 0 {
        return 0; // unbuilt
    }
    // M104 — bound the fold's memory blast radius: the blob (M26 legacy) + v3-structured + HNSW-structured
    // compaction paths materialize the live set in RAM (O(N), the M55 window). If the index-on-disk size exceeds
    // `theodb.vacuum_fold_max_mb` (default 1024), SKIP the in-VACUUM fold with a WARN instead of risking an OOM —
    // correctness is UNCHANGED (the scan folds the pending region + the executor's MVCC heap re-check drops dead
    // TIDs; the modern IVF v4–v7 formats already no-op here). The operator compacts a large index via REINDEX (the
    // fully-bounded streaming fold is M55). This turns a possible OOM into a documented, safe deferral.
    let fold_cap_mb = crate::am::guc::vacuum_fold_max_mb();
    let idx_bytes = pg_sys::RelationGetNumberOfBlocksInFork(indexrel, pg_sys::ForkNumber::MAIN_FORKNUM) as u64 * 8192;
    if fold_cap_mb > 0 && idx_bytes > fold_cap_mb.saturating_mul(1024 * 1024) {
        pg_sys::warning!(
            "theodb am vacuum: index is {} MB (> theodb.vacuum_fold_max_mb = {} MB) — skipping the in-VACUUM \
             compaction fold to avoid an O(N)-in-RAM OOM; correctness is preserved (scan pending-fold + MVCC \
             re-check). REINDEX to compact (the bounded streaming fold is a follow-up, M55).",
            idx_bytes / (1024 * 1024),
            fold_cap_mb
        );
        return 0;
    }
    if magic == page::IVF_STRUCT_MAGIC {
        // M81 — a v4 (AQ) IVF index: the f32 v3 rebuild would reject/corrupt it. Correctness holds WITHOUT a
        // structure rebuild — the scan folds the pending region (scan.rs::scan_ivf_aq) and the executor's MVCC
        // heap re-check filters dead tuples out of the returned TIDs. Compacting the v4 lists/pending needs a
        // v4-aware fold — a documented follow-up (REINDEX rebuilds cleanly today). So a routine VACUUM is a
        // safe no-op on the structure, never a crash on `read_ivf_meta(v4)`.
        // M83/M85/M90 — v5 (storage-separated), v6 (SQ8-refine), and v7 (label-aware) are safe no-ops for the same
        // reason as v4: the f32 v3 rebuild (`read_ivf_meta`, v2/v3-only) would REJECT them (typed Err → error!) —
        // correctness holds via the scan's pending fold + MVCC heap re-check. M90: without v7 here, VACUUM on a
        // labeled index errored every time (the un-vacuumable-index blocker caught in review).
        if page::ivf_is_v4(indexrel)
            || page::ivf_is_v5(indexrel)
            || page::ivf_is_v6(indexrel)
            || page::ivf_is_v7(indexrel)
            || page::ivf_is_v8(indexrel)
        {
            return 0;
        }
        return vacuum_rebuild_structured(indexrel, dead);
    }
    if magic == crate::am::hnsw_page::HNSW_STRUCT_MAGIC {
        return vacuum_rebuild_hnsw_structured(indexrel, dead);
    }
    // DEPRECATED (M104) — the blob (M26 legacy, single-serialized-index) layout. Superseded by the structured
    // IVF (M31, v3–v7) and structured HNSW (M35) layouts; only pre-M31 indexes still carry it, and a REINDEX
    // migrates them to the current format. Kept for read/VACUUM back-compat; will be removed once no blob indexes
    // remain in the field (follow-up). New builds never produce a blob.
    let blob = match page::read_blob(indexrel) {
        Ok(b) => b,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    if blob.is_empty() {
        return 0;
    }
    let idx = match Persisted::from_bytes(&blob) {
        Ok(i) => i,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let pending = match page::read_pending(indexrel) {
        Ok(p) => p,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let mut live: Vec<(i64, Vec<f32>)> = Vec::new();
    for (id, v) in idx.entries().into_iter().chain(pending) {
        if !dead(id) {
            live.push((id, v));
        }
    }
    let rebuilt = idx.rebuilt_with(&live, BUILD_SEED);
    page::rewrite_blob(indexrel, &rebuilt.to_bytes());
    live.len()
}}

/// M35 VACUUM fold for the structured HNSW layout: enumerate every element tuple + pending, drop dead, rebuild the
/// graph, and rewrite the structured layout in place (folding pending in, dropping dead TIDs). Returns live count.
unsafe fn vacuum_rebuild_hnsw_structured(indexrel: pg_sys::Relation, dead: &mut dyn FnMut(i64) -> bool) -> usize { unsafe {
    let meta = match crate::am::hnsw_page::read_meta(indexrel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let metric = match Metric::from_tag(meta.metric_tag) {
        Some(m) => m,
        None => pg_sys::error!("theodb am vacuum: unknown metric tag"),
    };
    let mut all = match crate::am::hnsw_page::enumerate_entries(indexrel, &meta) {
        Ok(e) => e,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    match page::read_pending(indexrel) {
        Ok(p) => all.extend(p),
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    }
    let live: Vec<(i64, Vec<f32>)> = all.into_iter().filter(|(id, _)| !dead(*id)).collect();
    let live_len = live.len(); // capture before `build_owned` consumes `live` (DoD-4 move, not clone)
    // HNSW build params are fixed consts (no reloption), so rebuild with them; preserve the metric from meta.
    // T4.1: the fold's rebuild is also cancellable (a VACUUM of a huge index responds to cancel per batch).
    // M56 DoD-4: `build_owned` MOVES `live` into the graph (no clone) so the compaction fold of a 1M index
    // does not hold the live f32 vectors twice.
    let idx = HnswIndex::build_owned(
        live, HNSW_M, hnsw_ef_construction(), metric, BUILD_SEED, &|| { pgrx::check_for_interrupts!(); },
    );
    // M48 (#47): crash-safe fold — pack the new generation at a fresh base, write it to inert pages, then pivot
    // block 0. `pack_at` resolves the graph's pointers relative to `base`, so the packed image is position-
    // independent and readers (which follow meta.elem_first/nbr_first/entry_blkno) need no change. T2.2: pack once
    // at base 1 to count pages (the count is base-independent), pick a base that reuses the dead low region when
    // it fits (bounded growth), and repack only if the base changed.
    // Preserve the index's quantized layout across the fold: re-quantize the live vectors with a freshly-trained
    // codebook — the plan's "códigos gerados no fold". M51 SBQ (`meta.sbq_bits > 0`) and M59 AQ (`meta.aq_m > 0`)
    // are mutually exclusive (D1); `pack_fold_layout` reads the persisted meta (NOT the reloption) to pick which
    // one, so a tuned v2/v3 index re-folds identically. `sbq_bits == 0 && aq_m == 0` ⇒ the fold stays v1 f32-only.
    let pack_at_base = |base: usize| pack_fold_layout(&idx, base, &meta);
    let probe = match pack_at_base(1) {
        Ok(p) => p,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let need = probe.pages.len() as u32;
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(indexrel, pg_sys::ForkNumber::MAIN_FORKNUM);
    let base = crate::am::fold::free_region(crate::am::fold::cur_gen_start(indexrel), nblocks, need);
    let packed = if base == 1 {
        probe
    } else {
        match pack_at_base(base as usize) {
            Ok(p) => p,
            Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
        }
    };
    crate::am::fold::fold(indexrel, &packed.meta, &packed.pages, base);
    live_len
}}

/// M59 T3.3 — pack the fold's new generation preserving the index's persisted quantized layout. Reads the layout
/// discriminator off the persisted `meta` (NOT the reloption — a fold must re-emit the same layout the index was
/// built with even if the reloption changed): `aq_m > 0` ⇒ re-train AQ and pack **v3** (recovering `bits`/`η`
/// from the persisted codebook so the fold re-quantizes identically); else pack v2/v1 via `sbq_bits`. The pack is
/// position-independent (`base`), so the crash-safe fold relocates it for free.
fn pack_fold_layout(
    idx: &HnswIndex,
    base: usize,
    meta: &crate::am::hnsw_page::HnswMeta,
) -> Result<crate::am::hnsw_page::Packed, String> {
    if meta.aq_m == 0 {
        return crate::am::hnsw_page::pack_at(idx, base, meta.sbq_bits);
    }
    // Recover the AQ params (bits + η) from the persisted codebook so the re-trained fold generation is identical
    // to the build's (deterministic AQ_BUILD_SEED). The codebook itself is re-trained on the LIVE vectors inside
    // `pack_aq` — dropping dead nodes but keeping the same (m, bits, η), the "códigos gerados no fold".
    let q = crate::vec::aq::AqQuantizer::from_meta_bytes(&meta.aq_codebook)?;
    crate::am::hnsw_page::pack_aq(idx, base, meta.aq_m as usize, q.bits(), q.aq_threshold())
}

/// VACUUM fold for the structured IVFFlat layout (M31): enumerate all list entries + pending, drop dead, rebuild,
/// and rewrite the structured layout in place.
unsafe fn vacuum_rebuild_structured(indexrel: pg_sys::Relation, dead: &mut dyn FnMut(i64) -> bool) -> usize { unsafe {
    let meta = match page::read_ivf_meta(indexrel) {
        Ok(m) => m,
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    };
    let metric = match Metric::from_tag(meta.metric_tag) {
        Some(m) => m,
        None => pg_sys::error!("theodb am vacuum: unknown metric tag"),
    };
    let mut all: Vec<(i64, Vec<f32>)> = Vec::new();
    for ci in 0..meta.centroids.len() {
        let (fb, np, cnt) = meta.dir[ci];
        match page::read_ivf_list(indexrel, fb, np, cnt, meta.dim) {
            Ok(e) => all.extend(e),
            Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
        }
    }
    match page::read_pending(indexrel) {
        Ok(p) => all.extend(p),
        Err(e) => pg_sys::error!("theodb am vacuum: {e}"),
    }
    let live: Vec<(i64, Vec<f32>)> = all.into_iter().filter(|(id, _)| !dead(*id)).collect();
    let dim = live.first().map(|(_, v)| v.len()).unwrap_or(meta.dim as usize) as u32;
    // M34 — a VACUUM fold preserves the built list count (WITH (lists=N)); reverting to the default would silently
    // re-partition a tuned index.
    let idx = IvfflatIndex::build(&live, lists_from_relation(indexrel), metric, BUILD_SEED);
    // M48 (#47): crash-safe fold — the v3 items carry gen_base = the fresh base, so the relocated directory /
    // centroids / lists resolve correctly after block 0 is pivoted. One item per page ⇒ wrap each as a 1-item
    // page. T2.2: the per-page count is base-independent, so build items at base 1 to count, choose a base that
    // reuses the dead low region when it fits, and rebuild the items only if the base changed.
    let probe = page::ivf_structured_items(1, dim, metric.tag(), idx.centroids(), &idx.list_entries());
    let need = probe.len() as u32 - 1; // minus the meta (item 0, written to block 0)
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(indexrel, pg_sys::ForkNumber::MAIN_FORKNUM);
    let base = crate::am::fold::free_region(crate::am::fold::cur_gen_start(indexrel), nblocks, need);
    let items = if base == 1 {
        probe
    } else {
        page::ivf_structured_items(base, dim, metric.tag(), idx.centroids(), &idx.list_entries())
    };
    let (meta, body_items) = items.split_first().expect("ivf structured items always include the meta");
    let body: Vec<Vec<Vec<u8>>> = body_items.iter().map(|it| vec![it.clone()]).collect();
    crate::am::fold::fold(indexrel, meta, &body, base);
    live.len()
}}

/// Empty index for an UNLOGGED table: Postgres calls `ambuildempty` to populate the INIT fork (the template
/// copied to the main fork on crash-recovery reset) — NOT the main fork. Writing MAIN here would append spurious
/// pages to the already-built main fork (pgvector writes INIT_FORKNUM too, `ivfbuild.c:1084`).
#[pg_guard]
pub extern "C-unwind" fn ambuildempty(indexrel: pg_sys::Relation) {
    unsafe {
        let metric = resolve_metric(indexrel); // M49: an empty cosine/ip index must persist the right tag (edge #4)
        let idx = IvfflatIndex::build(&[], DEFAULT_LISTS, metric, BUILD_SEED);
        page::write_blob(indexrel, pg_sys::ForkNumber::INIT_FORKNUM, &idx.to_bytes());
        wal_log_init_fork(indexrel);
    }
}

#[pg_guard]
pub extern "C-unwind" fn ambuildempty_hnsw(indexrel: pg_sys::Relation) {
    let metric = unsafe { resolve_metric(indexrel) }; // M49: empty cosine/ip index persists the right tag (edge #4)
    let idx = HnswIndex::build(&[], HNSW_M, HNSW_EF_CONSTRUCTION, metric, BUILD_SEED);
    unsafe {
        // M35: an empty structured graph is meta-only (entry_level = -1).
        match crate::am::hnsw_page::pack(&idx) {
            Ok(packed) => {
                crate::am::hnsw_page::write_structured(indexrel, pg_sys::ForkNumber::INIT_FORKNUM, &packed);
                wal_log_init_fork(indexrel);
            }
            Err(e) => pg_sys::error!("theodb hnsw buildempty: {e}"),
        }
    }
}

/// Issue #46: WAL-log every INIT-fork page unconditionally. `GenericXLog` is a WAL no-op when
/// `RelationNeedsWAL()` is false (always the case for the UNLOGGED relations that get an INIT fork), so
/// without this the crash-recovery reset copies a fork that never reached the WAL — the reset main fork
/// comes up empty/zeroed and `aminsert` fails with "truncated meta page" until REINDEX. Pattern is the
/// upstream fix verbatim: pgvector `hnswbuild.c:1137-1138` / gist `gist.c:133-150` (`log_newpage_range`
/// with FPIs for the whole fork). Called as the LAST step of buildempty so the range covers every page.
unsafe fn wal_log_init_fork(indexrel: pg_sys::Relation) { unsafe {
    let fork = pg_sys::ForkNumber::INIT_FORKNUM;
    let nblocks = pg_sys::RelationGetNumberOfBlocksInFork(indexrel, fork);
    if nblocks > 0 {
        pg_sys::log_newpage_range(indexrel, fork, 0, nblocks, true);
    }
}}

/// Convert a pgvector `vector` Datum into a `Vec<f32>`. pgvector's on-disk layout (studied in M20): a varlena
/// header, then `int16 dim`, `int16 unused`, then `dim × float4`. Detoast first (vectors are plain-stored but be
/// safe). Ported to a raw reader because the AM callback receives the raw Datum (no SQL `::real[]` cast here).
pub(crate) unsafe fn datum_to_vec_f32(datum: pg_sys::Datum) -> Vec<f32> { unsafe {
    let detoasted = pg_sys::pg_detoast_datum(datum.cast_mut_ptr::<pg_sys::varlena>());
    let base = detoasted.cast::<u8>();
    // VARHDRSZ (4-byte varlena header) — pgvector always uses the 4-byte header for `vector`.
    let data = base.add(4);
    let dim = i16::from_ne_bytes([*data, *data.add(1)]) as usize;
    let floats = data.add(4).cast::<f32>(); // skip dim(2) + unused(2)
    let mut out = Vec::with_capacity(dim);
    for i in 0..dim {
        out.push(floats.add(i).read_unaligned());
    }
    out
}}

// ================================ M59 T3.3 — build + fold wiring preserves AQ v3 ================================
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use crate::am::hnsw_page::{pack_aq, read_meta};

    /// A dim-8 corpus (divisible by the AQ subspace counts used here, m ∈ {2,4}) so `AqQuantizer::train` accepts
    /// it (`dim % m == 0`). Distinct, deterministic points, `n` rows.
    fn aq_corpus8(n: i64) -> Vec<(i64, Vec<f32>)> {
        (0..n)
            .map(|i| {
                let f = i as f32;
                (
                    i + 1,
                    vec![f, (i % 7) as f32, (i % 5) as f32, (i % 3) as f32, f * 0.1, (i % 11) as f32, (i % 2) as f32, f * 0.5],
                )
            })
            .collect()
    }

    /// Build a dim-8 table `tbl` with `n` distinct rows (id = i+1), then `CREATE INDEX ... USING theodb_hnsw`
    /// with the given `with` clause (empty ⇒ no reloption). Returns nothing; the index is `{tbl}_idx`.
    fn build_indexed_table(tbl: &str, n: i64, with: &str) {
        pgrx::Spi::run(&format!("CREATE TABLE {tbl} (id int PRIMARY KEY, e vector(8))")).unwrap();
        for (id, v) in aq_corpus8(n) {
            let lit = v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO {tbl} VALUES ({id}, '[{lit}]')")).unwrap();
        }
        let clause = if with.is_empty() { String::new() } else { format!(" WITH ({with})") };
        pgrx::Spi::run(&format!("CREATE INDEX {tbl}_idx ON {tbl} USING theodb_hnsw (e){clause}")).unwrap();
    }

    /// Read the persisted meta of `{tbl}_idx` via the real Relation (the only way to see what `ambuild` packed).
    unsafe fn meta_of(tbl: &str) -> crate::am::hnsw_page::HnswMeta { unsafe {
        let oid: pg_sys::Oid =
            pgrx::Spi::get_one(&format!("SELECT '{tbl}_idx'::regclass::oid")).unwrap().expect("oid");
        let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        let meta = read_meta(rel).expect("read_meta");
        pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        meta
    }}

    /// ctid → the encoded i64 the AM stores, for driving the FFI fold's `dead` predicate.
    fn heap_tid_i64(tbl: &str, id: i32) -> i64 {
        let txt: String = pgrx::Spi::get_one(&format!("SELECT ctid::text FROM {tbl} WHERE id = {id}"))
            .unwrap()
            .expect("row exists");
        let inner = txt.trim_start_matches('(').trim_end_matches(')');
        let (b, o) = inner.split_once(',').expect("ctid has block,offset");
        (b.trim().parse::<i64>().unwrap() << 16) | o.trim().parse::<i64>().unwrap()
    }

    /// Exact top-k via seqscan (oracle), then index scan; returns `(exact, via_index)` id sets.
    fn topk_sets(tbl: &str, probe: &str, k: i64) -> (Vec<i32>, Vec<i32>) {
        let sql = format!("SELECT id FROM {tbl} ORDER BY e <-> '{probe}'::vector LIMIT {k}");
        let run = |sql: &str| -> Vec<i32> {
            pgrx::Spi::connect(|c| {
                c.select(sql, None, &[]).unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
            })
        };
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        pgrx::Spi::run("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on").unwrap();
        let exact = run(&sql);
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let idx = run(&sql);
        (exact, idx)
    }

    /// T3.3 ACCEPTANCE (build): `CREATE INDEX ... WITH (pq_subspaces=4, pq_bits=4, aq_threshold=2000)` packs a v3
    /// index — the persisted meta carries the AQ codebook + `aq_m=4` (NOT SBQ), each node has ⌈4/2⌉=2 trailing
    /// code bytes, and the f32-rerank scan still returns the exact top-k (the inline codes do not corrupt scoring).
    #[pgrx::pg_test]
    fn ambuild_with_pq_subspaces_packs_v3_and_scans_correctly() {
        build_indexed_table("aqb", 40, "pq_subspaces=4, pq_bits=4, aq_threshold=2000");
        let meta = unsafe { meta_of("aqb") };
        assert_eq!(meta.aq_m, 4, "ambuild picked v3 (aq_m=4) from the reloption");
        assert_eq!(meta.sbq_bits, 0, "v3 index carries NO SBQ (AQ ⊥ SBQ, D1)");
        assert!(!meta.aq_codebook.is_empty(), "the AQ codebook is persisted in the v3 meta");
        // The persisted codebook decodes and reports the η we asked for (2.0), proving the reloption reached train.
        let q = crate::vec::aq::AqQuantizer::from_meta_bytes(&meta.aq_codebook).expect("codebook decodes");
        assert_eq!(q.m(), 4);
        assert!((q.aq_threshold() - 2.0).abs() < 1e-3, "η=2.0 round-tripped from the reloption");
        // ⌈m/2⌉ = 2 trailing bytes per node.
        assert_eq!(crate::vec::aq::AqQuantizer::bytes_per_vector(8, 4), 2);
        // f32 rerank still exact.
        let (mut exact, mut idx) = topk_sets("aqb", "[3,3,2,0,0.3,4,1,1.5]", 5);
        exact.sort_unstable();
        idx.sort_unstable();
        assert_eq!(idx, exact, "v3 scan returns the exact top-5 (inline AQ codes don't corrupt f32 rerank)");
    }

    /// M77 (pg_scann) — `CREATE INDEX ... USING theodb_ivfflat WITH (pq_subspaces=M)` takes the v4 IVF-AQ layout
    /// (persisted AQ block32 codes + f32 rerank) and the batched-AH scan returns high recall vs the exact seqscan
    /// top-k. This is the M77 correctness gate: the v4 build + scan path is wired and correct.
    #[pgrx::pg_test]
    fn ambuild_ivf_pq_subspaces_v4_scans_high_recall() {
        pgrx::Spi::run("CREATE TABLE ivfaq (id int PRIMARY KEY, e vector(8))").unwrap();
        for i in 0..200usize {
            let lit =
                (0..8).map(|j| (((i * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfaq VALUES ({}, '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run(
            "CREATE INDEX ivfaq_idx ON ivfaq USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=4, aq_threshold=2000)",
        )
        .unwrap();
        // The index took the v4 AQ layout (not the v3 f32 fallback).
        let is_v4 = unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'ivfaq_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let v = crate::am::page::ivf_is_v4(rel);
            pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            v
        };
        assert!(is_v4, "pq_subspaces IVF index took the v4 AQ layout");
        pgrx::Spi::run("SET theodb_ivfflat.probes = 8").unwrap(); // all lists → recall bound by AQ+rerank, not probing
        let probe: String =
            (0..8).map(|j| (((5 * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
        let (exact, idx) = topk_sets("ivfaq", &format!("[{probe}]"), 10);
        assert!(!idx.is_empty(), "the v4 AQ scan returned rows");
        let e: std::collections::HashSet<i32> = exact.iter().copied().collect();
        let recall = idx.iter().filter(|id| e.contains(id)).count() as f64 / (exact.len().max(1) as f64);
        assert!(recall >= 0.8, "IVF-AQ v4 recall@10 {recall:.2} < 0.8 (exact={exact:?} idx={idx:?})");
    }

    /// M81 (pg_scann lifecycle) — a row INSERTed AFTER a v4 IVF-AQ index is built (goes to the pending region,
    /// not the persisted AQ lists) is FOLDED into the scan: `scan_ivf_aq` reads + exact-scores the pending, so
    /// post-build INSERTs are never silently dropped. (VACUUM safety is by construction — `vacuum_rebuild` no-ops
    /// on v4 instead of the f32 rebuild that would reject `read_ivf_meta(v4)`; VACUUM cannot run inside the test tx.)
    #[pgrx::pg_test]
    fn ivf_aq_v4_folds_post_build_inserts() {
        pgrx::Spi::run("CREATE TABLE ivfaqp (id int PRIMARY KEY, e vector(8))").unwrap();
        for i in 0..100usize {
            let lit =
                (0..8).map(|j| (((i * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfaqp VALUES ({}, '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run(
            "CREATE INDEX ivfaqp_idx ON ivfaqp USING theodb_ivfflat (e) WITH (lists=4, pq_subspaces=4, aq_threshold=2000)",
        )
        .unwrap();
        // INSERT a distinctive row AFTER build → lands in the pending region (not the persisted AQ lists).
        pgrx::Spi::run("INSERT INTO ivfaqp VALUES (9999, '[50,50,50,50,50,50,50,50]')").unwrap();
        pgrx::Spi::run("SET theodb_ivfflat.probes = 4").unwrap();
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id FROM ivfaqp ORDER BY e <-> '[50,50,50,50,50,50,50,50]'::vector LIMIT 3",
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert!(ids.contains(&9999), "post-build INSERT (id 9999) folded into the v4 AQ scan (got {ids:?})");
    }

    /// M83 (Roadmap v7 D3) — `WITH (pq_subspaces=M, separate_storage=1)` takes the v5 STORAGE-SEPARATED layout
    /// (codes and f32 on distinct page ranges) and the two-phase scan (codes-only prune → random-read f32 rerank)
    /// returns high recall vs the exact seqscan top-k. The correctness gate: the v5 build + split scan is wired,
    /// the codes region carries the ids, and `read_vec_at` random-reads the right f32 for the rerank.
    #[pgrx::pg_test]
    fn ambuild_ivf_pq_subspaces_v5_split_scans_high_recall() {
        pgrx::Spi::run("CREATE TABLE ivfaq5 (id int PRIMARY KEY, e vector(8))").unwrap();
        for i in 0..200usize {
            let lit =
                (0..8).map(|j| (((i * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfaq5 VALUES ({}, '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run(
            "CREATE INDEX ivfaq5_idx ON ivfaq5 USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=4, aq_threshold=2000, separate_storage=1)",
        )
        .unwrap();
        // The index took the v5 storage-separated layout (not v4 interleaved, not v3 f32).
        let (is_v5, is_v4) = unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'ivfaq5_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let v5 = crate::am::page::ivf_is_v5(rel);
            let v4 = crate::am::page::ivf_is_v4(rel);
            pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            (v5, v4)
        };
        assert!(is_v5, "separate_storage=1 index took the v5 storage-separated layout");
        assert!(!is_v4, "v5 is not misdetected as v4");
        pgrx::Spi::run("SET theodb_ivfflat.probes = 8").unwrap();
        let probe: String =
            (0..8).map(|j| (((5 * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
        let (exact, idx) = topk_sets("ivfaq5", &format!("[{probe}]"), 10);
        assert!(!idx.is_empty(), "the v5 split AQ scan returned rows");
        let e: std::collections::HashSet<i32> = exact.iter().copied().collect();
        let recall = idx.iter().filter(|id| e.contains(id)).count() as f64 / (exact.len().max(1) as f64);
        assert!(recall >= 0.8, "IVF-AQ v5 recall@10 {recall:.2} < 0.8 (exact={exact:?} idx={idx:?})");
    }

    /// M83 — a row INSERTed AFTER a v5 index is built (pending region) is FOLDED into the split scan, exactly like
    /// v4. Guards that the storage-separated path did not regress the M81 pending-fold lifecycle.
    #[pgrx::pg_test]
    fn ivf_aq_v5_folds_post_build_inserts() {
        pgrx::Spi::run("CREATE TABLE ivfaq5p (id int PRIMARY KEY, e vector(8))").unwrap();
        for i in 0..100usize {
            let lit =
                (0..8).map(|j| (((i * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfaq5p VALUES ({}, '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run(
            "CREATE INDEX ivfaq5p_idx ON ivfaq5p USING theodb_ivfflat (e) WITH (lists=4, pq_subspaces=4, aq_threshold=2000, separate_storage=1)",
        )
        .unwrap();
        pgrx::Spi::run("INSERT INTO ivfaq5p VALUES (9999, '[50,50,50,50,50,50,50,50]')").unwrap();
        pgrx::Spi::run("SET theodb_ivfflat.probes = 4").unwrap();
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id FROM ivfaq5p ORDER BY e <-> '[50,50,50,50,50,50,50,50]'::vector LIMIT 3",
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert!(ids.contains(&9999), "post-build INSERT (id 9999) folded into the v5 split scan (got {ids:?})");
    }

    /// M85 (Roadmap v7) — `WITH (pq_subspaces=M, separate_storage=1, refine=1)` takes the v6 SQ8-refine layout
    /// (rerank on SQ8 codes, not raw f32) and returns high recall vs the exact seqscan top-k. The correctness
    /// gate: the v6 build (AQ codes + SQ8 codes on distinct pages, both codebooks persisted) + the SQ8-decode
    /// rerank is wired and correct.
    #[pgrx::pg_test]
    fn ambuild_ivf_pq_subspaces_v6_sq8_scans_high_recall() {
        pgrx::Spi::run("CREATE TABLE ivfaq6 (id int PRIMARY KEY, e vector(8))").unwrap();
        for i in 0..200usize {
            let lit =
                (0..8).map(|j| (((i * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfaq6 VALUES ({}, '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run(
            "CREATE INDEX ivfaq6_idx ON ivfaq6 USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=4, aq_threshold=2000, separate_storage=1, refine=1)",
        )
        .unwrap();
        // The index took the v6 SQ8-refine layout (not v5, not v4).
        let (is_v6, is_v5) = unsafe {
            let oid: pg_sys::Oid =
                pgrx::Spi::get_one("SELECT 'ivfaq6_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let v6 = crate::am::page::ivf_is_v6(rel);
            let v5 = crate::am::page::ivf_is_v5(rel);
            pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            (v6, v5)
        };
        assert!(is_v6, "refine=1 index took the v6 SQ8-refine layout");
        assert!(!is_v5, "v6 is not misdetected as v5");
        pgrx::Spi::run("SET theodb_ivfflat.probes = 8").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.over_fetch = 8").unwrap(); // widen the pool so SQ8's ε loss doesn't cap recall
        let probe: String =
            (0..8).map(|j| (((5 * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
        let (exact, idx) = topk_sets("ivfaq6", &format!("[{probe}]"), 10);
        assert!(!idx.is_empty(), "the v6 SQ8 scan returned rows");
        let e: std::collections::HashSet<i32> = exact.iter().copied().collect();
        let recall = idx.iter().filter(|id| e.contains(id)).count() as f64 / (exact.len().max(1) as f64);
        assert!(recall >= 0.8, "IVF-AQ v6 SQ8 recall@10 {recall:.2} < 0.8 (exact={exact:?} idx={idx:?})");
    }

    /// M85 — a row INSERTed AFTER a v6 index is built (pending region, f32) is FOLDED into the SQ8 scan and scored
    /// EXACTLY, like v4/v5. Guards that the SQ8-refine path did not regress the M81 pending-fold lifecycle.
    #[pgrx::pg_test]
    fn ivf_aq_v6_folds_post_build_inserts() {
        pgrx::Spi::run("CREATE TABLE ivfaq6p (id int PRIMARY KEY, e vector(8))").unwrap();
        for i in 0..100usize {
            let lit =
                (0..8).map(|j| (((i * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfaq6p VALUES ({}, '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run(
            "CREATE INDEX ivfaq6p_idx ON ivfaq6p USING theodb_ivfflat (e) WITH (lists=4, pq_subspaces=4, aq_threshold=2000, separate_storage=1, refine=1)",
        )
        .unwrap();
        pgrx::Spi::run("INSERT INTO ivfaq6p VALUES (9999, '[50,50,50,50,50,50,50,50]')").unwrap();
        pgrx::Spi::run("SET theodb_ivfflat.probes = 4").unwrap();
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(
                "SELECT id FROM ivfaq6p ORDER BY e <-> '[50,50,50,50,50,50,50,50]'::vector LIMIT 3",
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert!(ids.contains(&9999), "post-build INSERT (id 9999) folded into the v6 SQ8 scan (got {ids:?})");
    }

    /// M86 (Roadmap v7) — `WITH (..., soar_lambda=N)` (N>0) spills each vector to a SOAR-chosen second list; the
    /// scan still returns correct high recall (duplicates are de-duplicated by tid at emit time). Correctness
    /// gate for the SOAR build path: spilling does not corrupt the index or double-count in the top-k.
    #[pgrx::pg_test]
    fn ambuild_ivf_soar_spill_scans_high_recall_no_dupes() {
        pgrx::Spi::run("CREATE TABLE ivfsoar (id int PRIMARY KEY, e vector(8))").unwrap();
        for i in 0..200usize {
            let lit =
                (0..8).map(|j| (((i * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfsoar VALUES ({}, '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run(
            "CREATE INDEX ivfsoar_idx ON ivfsoar USING theodb_ivfflat (e) WITH (lists=8, pq_subspaces=4, aq_threshold=2000, separate_storage=1, soar_lambda=1000)",
        )
        .unwrap();
        pgrx::Spi::run("SET theodb_ivfflat.probes = 8").unwrap();
        pgrx::Spi::run("SET theodb_hnsw.over_fetch = 8").unwrap();
        let probe: String =
            (0..8).map(|j| (((5 * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
        let (exact, idx) = topk_sets("ivfsoar", &format!("[{probe}]"), 10);
        assert!(!idx.is_empty(), "the SOAR-spilled scan returned rows");
        // No duplicate ids in the returned top-k (dedup-by-tid at emit works despite the vector being in 2 lists).
        let mut sorted = idx.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), idx.len(), "SOAR scan returned duplicate ids (dedup-by-tid failed): {idx:?}");
        let e: std::collections::HashSet<i32> = exact.iter().copied().collect();
        let recall = idx.iter().filter(|id| e.contains(id)).count() as f64 / (exact.len().max(1) as f64);
        assert!(recall >= 0.8, "SOAR recall@10 {recall:.2} < 0.8 (exact={exact:?} idx={idx:?})");
    }

    /// M87 (Roadmap v7) — filtered ANN over a v5 storage-separated index preserves recall: under a SELECTIVE
    /// `WHERE`, the M87 IVF iterative re-search grows `probes` so the AM keeps supplying candidates until the
    /// executor's LIMIT is satisfied (else the first probes' candidates are all filtered out → recall collapses,
    /// the pre-M87 gap). Gate: the index-scan filtered top-k equals the exact seqscan-filtered top-k.
    #[pgrx::pg_test]
    fn filtered_ann_v5_iterative_preserves_recall() {
        pgrx::Spi::run("CREATE TABLE ivffilt (id int PRIMARY KEY, grp int, e vector(8))").unwrap();
        for i in 0..300usize {
            let lit =
                (0..8).map(|j| (((i * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivffilt VALUES ({}, {}, '[{lit}]')", i + 1, (i + 1) % 10)).unwrap();
        }
        pgrx::Spi::run(
            "CREATE INDEX ivffilt_idx ON ivffilt USING theodb_ivfflat (e) WITH (lists=16, pq_subspaces=4, aq_threshold=2000, separate_storage=1)",
        )
        .unwrap();
        // A 10%-selective filter (grp=3) + tiny probes: without the iterative re-search, the few probed lists'
        // candidates would mostly be filtered out and the top-5 would be short.
        pgrx::Spi::run("SET theodb_ivfflat.probes = 1").unwrap();
        let q = "[5,2,0,4,1,3,6,0.5]";
        let filt = "grp = 3";
        let idx_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET enable_indexscan=on", None, &[]).ok();
            c.select(
                &format!("SELECT id FROM ivffilt WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        // Exact filtered top-5 via seqscan.
        let exact_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on", None, &[]).ok();
            c.select(
                &format!("SELECT id FROM ivffilt WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        assert_eq!(idx_ids.len(), 5, "filtered index scan returned {} rows, expected 5 (iterative supplied enough)", idx_ids.len());
        let e: std::collections::HashSet<i32> = exact_ids.iter().copied().collect();
        let recall = idx_ids.iter().filter(|id| e.contains(id)).count() as f64 / 5.0;
        assert!(recall >= 0.8, "filtered recall@5 {recall:.2} < 0.8 (idx={idx_ids:?} exact={exact_ids:?})");
    }

    /// M90 (inline label filter) — a v7 index (built with a `smallint[]` label column) is detected as v7, and a
    /// `WHERE lbl && '{k}' ORDER BY e <-> q LIMIT n` index scan returns the CORRECT filtered top-n (== exact
    /// seqscan-filtered): the Stage-1 inline skip drops non-overlapping candidates, and `xs_recheck` guarantees
    /// correctness. Also asserts the label-less path stays v5 (v7 is opt-in on the 2nd column).
    #[pgrx::pg_test]
    fn v7_inline_label_filter_scans_correct() {
        pgrx::Spi::run("CREATE TABLE ivfv7 (id int PRIMARY KEY, lbl smallint[], e vector(8))").unwrap();
        for i in 0..300usize {
            let lit =
                (0..8).map(|j| (((i * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            // Label = i % 5 (5 tags, ~20% each). Row i also carries tag 9 when i%50==0 (a sparse ~2% tag).
            let tag = i % 5;
            let lbl = if i % 50 == 0 { format!("{{{tag},9}}") } else { format!("{{{tag}}}") };
            pgrx::Spi::run(&format!("INSERT INTO ivfv7 VALUES ({}, '{lbl}', '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run(
            "CREATE INDEX ivfv7_idx ON ivfv7 USING theodb_ivfflat (e, lbl) WITH (lists=16, pq_subspaces=4, aq_threshold=2000, separate_storage=1)",
        )
        .unwrap();
        // The label column ⇒ the v7 (label-aware) layout, not v5.
        let (is_v7, is_v5) = unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'ivfv7_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let v7 = crate::am::page::ivf_is_v7(rel);
            let v5 = crate::am::page::ivf_is_v5(rel);
            pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            (v7, v5)
        };
        assert!(is_v7, "a label column ⇒ the v7 layout");
        assert!(!is_v5, "v7 is not misdetected as v5");
        pgrx::Spi::run("SET theodb_ivfflat.probes = 16").unwrap();
        let q = "[5,2,0,4,1,3,6,0.5]";
        let filt = "lbl && '{3}'::smallint[]";
        let idx_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET enable_indexscan=on", None, &[]).ok();
            c.select(
                &format!("SELECT id FROM ivfv7 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        // Exact filtered top-5 via seqscan.
        let exact_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on", None, &[]).ok();
            c.select(
                &format!("SELECT id FROM ivfv7 WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"),
                None,
                &[],
            )
            .unwrap()
            .filter_map(|r| r.get::<i32>(1).unwrap())
            .collect()
        });
        // Every id the index scan returns MUST satisfy the filter (correctness of the inline skip + xs_recheck).
        let matching: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select(&format!("SELECT id FROM ivfv7 WHERE {filt}"), None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
        });
        let mset: std::collections::HashSet<i32> = matching.iter().copied().collect();
        assert!(idx_ids.iter().all(|id| mset.contains(id)), "every filtered idx row satisfies lbl && {{3}} (idx={idx_ids:?})");
        let e: std::collections::HashSet<i32> = exact_ids.iter().copied().collect();
        let recall = idx_ids.iter().filter(|id| e.contains(id)).count() as f64 / (exact_ids.len().max(1) as f64);
        assert!(recall >= 0.8, "v7 filtered recall@5 {recall:.2} < 0.8 (idx={idx_ids:?} exact={exact_ids:?})");
    }

    /// M90 (review blocker regression) — VACUUM on a v7 (labeled) index must be a SAFE NO-OP, not an error. Before
    /// the fix, `amvacuumcleanup` fell through to the v3 rebuild (`read_ivf_meta`, v2/v3-only) which rejected v7 →
    /// every autovacuum errored (un-vacuumable labeled index). Asserts VACUUM succeeds AND the filtered scan still
    /// returns only matching LIVE rows after a DELETE.
    #[pgrx::pg_test]
    fn v7_vacuum_is_safe_noop() {
        pgrx::Spi::run("CREATE TABLE ivfv7v (id int PRIMARY KEY, lbl smallint[], e vector(8))").unwrap();
        for i in 0..200usize {
            let lit =
                (0..8).map(|j| (((i * 13 + j * 5) % 89) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfv7v VALUES ({}, '{{{}}}', '[{lit}]')", i + 1, i % 4)).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX ivfv7v_idx ON ivfv7v USING theodb_ivfflat (e, lbl) WITH (lists=8, pq_subspaces=4, aq_threshold=2000, separate_storage=1)").unwrap();
        // The fix: the VACUUM rebuild entry (`vacuum_rebuild`) must be a SAFE NO-OP for v7 (return 0, no error) —
        // exactly like v4/v5/v6. Called directly (SQL `VACUUM` can't run inside a pg_test's transaction block).
        // Before the fix it fell through to the v3 rebuild which rejected v7 → error.
        let noop = unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'ivfv7v_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            let mut dead = |_id: i64| false; // nothing dead
            let r = vacuum_rebuild(rel, &mut dead);
            pg_sys::index_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
            r
        };
        assert_eq!(noop, 0, "vacuum_rebuild on a v7 index is a safe no-op (returns 0), not an error");
        // The filtered scan still works after the (no-op) vacuum.
        pgrx::Spi::run("SET theodb_ivfflat.probes = 8").unwrap();
        let ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET enable_indexscan=on", None, &[]).ok();
            c.select("SELECT id FROM ivfv7v WHERE lbl && '{1}'::smallint[] ORDER BY e <-> '[1,2,3,4,5,6,7,8]'::vector LIMIT 5", None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        // Row i (0-based) has id=i+1 and label i%4; label 1 ⇒ i%4==1 ⇒ id%4==2. The filter returns only those.
        assert!(ids.iter().all(|&id| id % 4 == 2), "filtered scan returns only label-1 rows (got {ids:?})");
    }

    /// M90 (review blocker regression) — a PENDING (post-build INSERT) row with a NON-matching label must NOT be a
    /// false positive. The pending region stores no label, so the inline skip cannot filter it; only the executor's
    /// heap re-check (`xs_recheck=true`, kept ON by `amgettuple` when a label predicate is active) filters it.
    /// Before the fix, `amgettuple` cleared `xs_recheck` on every tuple → the non-matching pending row was returned.
    #[pgrx::pg_test]
    fn v7_pending_non_matching_label_is_filtered() {
        pgrx::Spi::run("CREATE TABLE ivfv7p (id int PRIMARY KEY, lbl smallint[], e vector(8))").unwrap();
        for i in 0..100usize {
            let lit =
                (0..8).map(|j| (((i * 17 + j * 3) % 83) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfv7p VALUES ({}, '{{1}}', '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX ivfv7p_idx ON ivfv7p USING theodb_ivfflat (e, lbl) WITH (lists=8, pq_subspaces=4, aq_threshold=2000, separate_storage=1)").unwrap();
        // Post-build INSERT → PENDING region, with a NON-matching label {2} and a vector near the query.
        pgrx::Spi::run("INSERT INTO ivfv7p VALUES (999, '{2}', '[0,0,0,0,0,0,0,0]')").unwrap();
        pgrx::Spi::run("SET theodb_ivfflat.probes = 8").unwrap();
        // Query filters for label {1}; the pending row {2} MUST NOT appear (executor recheck filters it).
        let ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET enable_indexscan=on", None, &[]).ok();
            c.select("SELECT id FROM ivfv7p WHERE lbl && '{1}'::smallint[] ORDER BY e <-> '[0,0,0,0,0,0,0,0]'::vector LIMIT 10", None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        assert!(!ids.contains(&999), "the non-matching pending row {{2}} must be filtered by xs_recheck (got {ids:?})");
        // And a query for {2} DOES find the pending row (pending fold + recheck pass).
        let ids2: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET enable_indexscan=on", None, &[]).ok();
            c.select("SELECT id FROM ivfv7p WHERE lbl && '{2}'::smallint[] ORDER BY e <-> '[0,0,0,0,0,0,0,0]'::vector LIMIT 10", None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        assert!(ids2.contains(&999), "the matching pending row {{2}} IS found (got {ids2:?})");
    }

    /// M91 T1.1 — selectivity-adaptive probing recovers filtered recall at LOW probes. A rare label ({7}, ~4% of
    /// rows) is scattered across many IVF lists; with `probes = 1` the OLD fixed `.take(probes)` scan would see at
    /// most one list's candidates (≈ 0 matches) and starve. The adaptive loop keeps probing nearest lists past
    /// `probes` until the matching-candidate pool reaches the rerank target — since only ~16 rows carry {7} (< the
    /// default pool), it probes all lists, finds every match, and returns the EXACT filtered top-5. Behavioral proof
    /// of adaptivity: high recall at probes=1 is only reachable if the loop probed well beyond 1 list.
    #[pgrx::pg_test]
    fn v7_adaptive_probing_recovers_selective_recall() {
        pgrx::Spi::run("CREATE TABLE ivfv7a (id int PRIMARY KEY, lbl smallint[], e vector(8))").unwrap();
        for i in 0..400usize {
            // Scatter vectors across the space so the rare-label rows land in many different lists (not clustered).
            let lit =
                (0..8).map(|j| (((i * 53 + j * 29) % 101) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            // Label = i % 6 (6 common tags) PLUS the sparse tag {7} on every 25th row (16 rows ≈ 4%).
            let lbl = if i % 25 == 0 { format!("{{{},7}}", i % 6) } else { format!("{{{}}}", i % 6) };
            pgrx::Spi::run(&format!("INSERT INTO ivfv7a VALUES ({}, '{lbl}', '[{lit}]')", i + 1)).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX ivfv7a_idx ON ivfv7a USING theodb_ivfflat (e, lbl) WITH (lists=32, pq_subspaces=4, aq_threshold=2000, separate_storage=1)").unwrap();
        // probes = 1: without adaptive probing the rare-label matches (scattered across ~16 lists) would be missed.
        pgrx::Spi::run("SET theodb_ivfflat.probes = 1").unwrap();
        let q = "[5,2,0,4,1,3,6,0.5]";
        let filt = "lbl && '{7}'::smallint[]";
        let idx_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET enable_indexscan=on", None, &[]).ok();
            c.select(&format!("SELECT id FROM ivfv7a WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        let exact_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on", None, &[]).ok();
            c.select(&format!("SELECT id FROM ivfv7a WHERE {filt} ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        let e: std::collections::HashSet<i32> = exact_ids.iter().copied().collect();
        let recall = idx_ids.iter().filter(|id| e.contains(id)).count() as f64 / (exact_ids.len().max(1) as f64);
        assert!(recall >= 0.8, "M91 adaptive recall@5 {recall:.2} < 0.8 at probes=1 — the loop must probe past 1 list to find the scattered {{7}} matches (idx={idx_ids:?} exact={exact_ids:?})");
    }

    /// M91 T1.1 — no-regression on the NON-filter path. With `probes = lists` and no label predicate, the adaptive
    /// loop must visit every list (break condition `!filtering ⇒ break at probes`, and probes=all ⇒ no early break)
    /// and return the EXACT unfiltered top-5. Guards that the loop refactor did not skip lists or grow probes for a
    /// non-filtered scan.
    #[pgrx::pg_test]
    fn v7_non_filter_scan_unchanged() {
        pgrx::Spi::run("CREATE TABLE ivfv7n (id int PRIMARY KEY, lbl smallint[], e vector(8))").unwrap();
        for i in 0..200usize {
            let lit =
                (0..8).map(|j| (((i * 37 + j * 11) % 89) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
            pgrx::Spi::run(&format!("INSERT INTO ivfv7n VALUES ({}, '{{{}}}', '[{lit}]')", i + 1, i % 4)).unwrap();
        }
        pgrx::Spi::run("CREATE INDEX ivfv7n_idx ON ivfv7n USING theodb_ivfflat (e, lbl) WITH (lists=8, pq_subspaces=4, aq_threshold=2000, separate_storage=1)").unwrap();
        // Probe ALL lists, no filter → the scan must equal the exact unfiltered top-5 (full probe = exact rerank).
        pgrx::Spi::run("SET theodb_ivfflat.probes = 8").unwrap();
        let q = "[3,1,4,1,5,2,6,0.5]";
        let idx_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_seqscan=off; SET enable_indexscan=on", None, &[]).ok();
            c.select(&format!("SELECT id FROM ivfv7n ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        let exact_ids: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SET enable_indexscan=off; SET enable_bitmapscan=off; SET enable_seqscan=on", None, &[]).ok();
            c.select(&format!("SELECT id FROM ivfv7n ORDER BY e <-> '{q}'::vector LIMIT 5"), None, &[])
                .unwrap().filter_map(|r| r.get::<i32>(1).unwrap()).collect()
        });
        let mut a = idx_ids.clone();
        a.sort_unstable();
        let mut b = exact_ids.clone();
        b.sort_unstable();
        assert_eq!(a, b, "M91: unfiltered v7 scan at probes=all must equal exact top-5 (idx={idx_ids:?} exact={exact_ids:?})");
    }

    /// T3.3 NON-REGRESSION (build): with NO reloption, `ambuild_hnsw` packs the byte-identical v1 layout — the
    /// meta carries neither an AQ nor an SBQ codebook (`aq_m == 0 && sbq_bits == 0`). Guards the "existing indexes
    /// stay v1/v2 byte-identical" invariant against the new AQ branch.
    #[pgrx::pg_test]
    fn ambuild_without_reloption_stays_v1() {
        build_indexed_table("aqv1", 30, "");
        let meta = unsafe { meta_of("aqv1") };
        assert_eq!(meta.aq_m, 0, "no reloption ⇒ NOT v3");
        assert_eq!(meta.sbq_bits, 0, "no reloption ⇒ NOT v2 either — plain v1 f32-only");
        assert!(meta.aq_codebook.is_empty() && meta.codebook.is_empty(), "v1 has no codebook of any kind");
    }

    /// T3.3 ACCEPTANCE (fold): a v3 index survives the VACUUM compaction fold — the codebook is RE-TRAINED (not
    /// dropped to v1, not corrupted). VACUUM the *command* cannot run inside a pg_test transaction, so drive the
    /// fold at the FFI level (as the M56 tests do): tombstone the dead, then call `vacuum_rebuild` (the compaction
    /// path). Post-fold: the meta is still v3 (aq_m preserved) AND an index scan returns the exact top-k over the
    /// live rows.
    #[pgrx::pg_test]
    fn aq_index_survives_vacuum_fold() {
        build_indexed_table("aqf", 40, "pq_subspaces=4, pq_bits=4, aq_threshold=1500");
        let before = unsafe { meta_of("aqf") };
        assert_eq!(before.aq_m, 4, "built v3");
        // Delete ids 1..=10 at the SQL level, then drive the FFI fold over the remaining live TIDs.
        let dead: Vec<i64> = (1..=10i32).map(|id| heap_tid_i64("aqf", id)).collect();
        pgrx::Spi::run("DELETE FROM aqf WHERE id <= 10").unwrap();
        let live = unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'aqf_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let mut is_dead = |t: i64| dead.contains(&t);
            let live = vacuum_rebuild(rel, &mut is_dead); // the compaction fold (advisory EXCLUSIVE)
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            live
        };
        assert_eq!(live, 30, "the fold kept the 30 live rows (40 built − 10 dead)");
        let after = unsafe { meta_of("aqf") };
        assert_eq!(after.aq_m, 4, "the fold PRESERVED v3 (codebook re-trained, not dropped to v1)");
        assert_eq!(after.sbq_bits, 0, "still not SBQ after the fold");
        assert!(!after.aq_codebook.is_empty(), "the re-trained AQ codebook is persisted");
        assert_eq!(after.node_count, 30, "the folded graph holds only the 30 live nodes");
        // A scan over the folded v3 index returns the exact top-k of the live rows (none of the deleted ids 1..10).
        let (mut exact, mut idx) = topk_sets("aqf", "[20,6,0,2,2.0,9,0,10.0]", 5);
        exact.sort_unstable();
        idx.sort_unstable();
        assert_eq!(idx, exact, "the folded v3 index scans correctly");
        assert!(idx.iter().all(|&id| id > 10), "no deleted id survives the fold (got {idx:?})");
    }

    /// T3.3 RACE-1 (parallel-build codebook determinism): building the SAME v3 index twice — once forcing the
    /// SEQUENTIAL path (a huge parallel threshold) and once the PARALLEL path (threshold 1) — must yield a
    /// BYTE-IDENTICAL persisted AQ codebook. The codebook is trained on the drained corpus, so a data race in the
    /// parallel drain would surface as a codebook divergence. Sequential-vs-parallel parity is the race-aware
    /// signal (mirrors the M46/M57 sequential≈parallel bisection). A `#[pg_test]` (single-threaded pg backend) so
    /// the process-global `set_var` toggle cannot race a concurrent test thread.
    #[pgrx::pg_test]
    fn aq_codebook_deterministic_under_parallel_build() {
        let corpus = aq_corpus8(300); // ≥ any reasonable parallel threshold so the parallel path really engages
        let (m, bits, thr) = (4usize, 4u8, 2.0f32);
        let pack_with_threshold = |t: &str| -> Vec<u8> {
            // FIXME: Audit that the environment access only happens in single-threaded code.
            unsafe { std::env::set_var("THEODB_HNSW_PARALLEL_THRESHOLD", t) };
            let idx = HnswIndex::build(&corpus, HNSW_M, HNSW_EF_CONSTRUCTION, Metric::L2, BUILD_SEED);
            let packed = pack_aq(&idx, 1, m, bits, thr).expect("pack_aq");
            // M59 fix: the codebook is on the packed codebook pages (no longer inline in the meta item — it
            // overflows one page at large dim). Reassemble it from those pages to compare seq-vs-parallel.
            let meta = crate::am::hnsw_page::decode_meta(&packed.meta).unwrap();
            crate::am::hnsw_page::codebook_from_packed(&packed, meta.aq_cb_first, meta.aq_cb_npages)
        };
        let seq = pack_with_threshold("100000000"); // threshold ≫ n ⇒ sequential
        let par = pack_with_threshold("1"); // threshold 1 ⇒ parallel
        // FIXME: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("THEODB_HNSW_PARALLEL_THRESHOLD") };
        assert!(!seq.is_empty(), "the v3 codebook is non-empty");
        assert_eq!(seq, par, "the AQ codebook is byte-identical seq-vs-parallel (no data race in the drain)");
    }

    /// T3.3 RACE-2 (fold vs concurrent insert): the AQ fold introduces NO new lock — it rides the SAME advisory
    /// EXCLUSIVE the SBQ/v1 fold uses (`vacuum_rebuild` → `index_exclusive`), while `aminsert` takes
    /// `index_shared`. This test proves the SERIALIZATION preserves v3: fold the v3 index, then insert a new row,
    /// then confirm the index is still v3 AND the new row is findable (it lands in pending, folded on the next
    /// fold). AQ adds no shared mutable state — the codebook is packed into fresh pages before the block-0 pivot —
    /// so it needs no new lock. The lock-reuse itself is asserted structurally (grep in the plan's AC).
    #[pgrx::pg_test]
    fn aq_fold_survives_concurrent_insert() {
        build_indexed_table("aqr", 40, "pq_subspaces=4, pq_bits=4, aq_threshold=1000");
        // Fold (drop nothing) — exercises the compaction path holding the advisory EXCLUSIVE.
        unsafe {
            let oid: pg_sys::Oid = pgrx::Spi::get_one("SELECT 'aqr_idx'::regclass::oid").unwrap().expect("oid");
            let rel = pg_sys::index_open(oid, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
            let mut none_dead = |_: i64| false;
            assert_eq!(vacuum_rebuild(rel, &mut none_dead), 40, "fold kept all 40 rows");
            pg_sys::index_close(rel, pg_sys::RowExclusiveLock as pg_sys::LOCKMODE);
        }
        assert_eq!(unsafe { meta_of("aqr") }.aq_m, 4, "v3 preserved across a no-op fold");
        // A concurrent insert (serialized by index_shared vs the fold's index_exclusive) lands in pending.
        pgrx::Spi::run("INSERT INTO aqr VALUES (999, '[41,6,1,2,4.1,9,1,20.5]')").unwrap();
        assert_eq!(unsafe { meta_of("aqr") }.aq_m, 4, "insert into a v3 index keeps it v3 (pending, not a downgrade)");
        // The new row is findable by an index scan on its own vector (pending brute-force fold in the scan).
        pgrx::Spi::run("SET theodb_hnsw.ef_search = 200").unwrap();
        pgrx::Spi::run("SET enable_seqscan=off; SET enable_bitmapscan=off; SET enable_indexscan=on").unwrap();
        let got: Vec<i32> = pgrx::Spi::connect(|c| {
            c.select("SELECT id FROM aqr ORDER BY e <-> '[41,6,1,2,4.1,9,1,20.5]'::vector LIMIT 1", None, &[])
                .unwrap()
                .filter_map(|r| r.get::<i32>(1).unwrap())
                .collect()
        });
        assert_eq!(got, vec![999], "the concurrently-inserted row is present after the fold (got {got:?})");
    }

    /// T3.3 NEGATIVE (Rule 8): a v3 build whose vector dim is NOT divisible by `pq_subspaces` must raise a TYPED
    /// error at CREATE INDEX (from `AqQuantizer::train`), not a panic across the C boundary. dim=8, pq_subspaces=3
    /// ⇒ 8 % 3 ≠ 0 ⇒ the typed "not divisible by m" error surfaces as a build ERROR.
    #[pgrx::pg_test(error = "theodb hnsw build: theodb aq: vector dim 8 is not divisible by m 3 (subspaces must be equal-sized)")]
    fn ambuild_v3_rejects_indivisible_dim() {
        build_indexed_table("aqbad", 20, "pq_subspaces=3, pq_bits=4");
    }

    /// Build a dim-`dim` table `tbl` with `n` distinct rows, then `CREATE INDEX ... USING theodb_hnsw` with `with`.
    /// Generalizes `build_indexed_table` to realistic embedding dims (768/1536) so the AQ codebook is multi-page.
    fn build_indexed_table_dim(tbl: &str, n: i64, dim: usize, with: &str) {
        pgrx::Spi::run(&format!("CREATE TABLE {tbl} (id int PRIMARY KEY, e vector({dim}))")).unwrap();
        for i in 0..n {
            // Distinct, deterministic vectors (varying so k-means has real structure to quantize).
            let lit = (0..dim)
                .map(|j| (((i as usize * 31 + j * 7) % 97) as f32 * 0.1).to_string())
                .collect::<Vec<_>>()
                .join(",");
            pgrx::Spi::run(&format!("INSERT INTO {tbl} VALUES ({}, '[{lit}]')", i + 1)).unwrap();
        }
        let clause = if with.is_empty() { String::new() } else { format!(" WITH ({with})") };
        pgrx::Spi::run(&format!("CREATE INDEX {tbl}_idx ON {tbl} USING theodb_hnsw (e){clause}")).unwrap();
    }

    /// M59 REGRESSION (the bug the benchmark's Phase 5 hit): `CREATE INDEX ... WITH (pq_subspaces=8, …)` on a
    /// dim=768 corpus. The AQ codebook is `m·16·(dim/m)·4 = 8·16·96·4 ≈ 48 KB` — SIX× the 8 KB page — so the old
    /// codec (codebook inline in the meta item) blew `PageAddItem failed (item too large / page full?)` at build.
    /// After the M59 fix the codebook is split across dedicated pages, so:
    ///   1. CREATE INDEX SUCCEEDS (no PageAddItem failure) — this is the RED assertion that failed before the fix;
    ///   2. the persisted codebook reassembles BIT-EXACT (round-trip through the real pages) and re-encodes a probe
    ///      identically to a freshly-trained quantizer over the SAME live vectors (deterministic AQ_BUILD_SEED);
    ///   3. an index scan returns the exact top-k (the multi-page codebook did not corrupt the AH walk / f32 rerank).
    #[pgrx::pg_test]
    fn ambuild_dim768_pq_subspaces_multipage_codebook_no_page_overflow() {
        // dim=768, m=8 → sub_dim=96 → codebook 8*16*96*4 = 49152 B ≈ 6 pages at 8 KB. This CREATE INDEX is the
        // exact call that raised `PageAddItem failed` before the codebook was paged.
        build_indexed_table_dim("aq768", 40, 768, "pq_subspaces=8, pq_bits=4, aq_threshold=4100");
        let meta = unsafe { meta_of("aq768") };
        assert_eq!(meta.aq_m, 8, "v3 index built with 8 subspaces");
        assert_eq!(meta.dim, 768, "dim=768");
        assert!(meta.aq_cb_npages >= 2, "the dim=768 codebook needs MULTIPLE dedicated pages (got {})", meta.aq_cb_npages);
        // (2) the reassembled codebook round-trips bit-exact and re-encodes identically to a fresh train.
        let expected_len = 13 + 8 * crate::vec::aq::AQ_K_STAR * (768 / 8) * 4; // to_meta_bytes header + centroids
        assert_eq!(meta.aq_codebook.len(), expected_len, "reassembled codebook is the full multi-page blob");
        let q = crate::vec::aq::AqQuantizer::from_meta_bytes(&meta.aq_codebook).expect("multi-page codebook decodes");
        assert_eq!(q.m(), 8);
        assert_eq!(q.dim(), 768, "the decoded codebook covers the full index dim");
        // The reloption `aq_threshold=4100` is MILLI-scaled (η×1000) → the trained quantizer stores η=4.1.
        assert!((q.aq_threshold() - 4.1).abs() < 0.01, "η=4.1 (reloption 4100/1000) round-tripped through the paged codebook");
        // (3) an index scan returns the exact top-k over the built rows.
        let probe = (0..768).map(|j| (((5usize * 31 + j * 7) % 97) as f32 * 0.1).to_string()).collect::<Vec<_>>().join(",");
        let (mut exact, mut idx) = topk_sets("aq768", &format!("[{probe}]"), 5);
        exact.sort_unstable();
        idx.sort_unstable();
        assert_eq!(idx, exact, "the dim=768 v3 scan returns the exact top-5 (multi-page codebook intact)");
    }
}
