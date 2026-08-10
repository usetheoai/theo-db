//! Scan hooks (M26 Phase 3): `ambeginscan` / `amrescan` / `amgettuple` / `amendscan`.
//!
//! `amrescan` deserializes the persisted `IvfflatIndex` from pages (once), reads the ORDER-BY query vector from
//! the scan key, and runs the reused search — producing `(encoded_tid, distance)` in ascending-distance order.
//! `amgettuple` hands the executor one heap TID at a time (in that order) until exhausted.
use crate::am::build::datum_to_vec_f32;
use crate::am::index::Persisted;
use crate::am::{page, tid};
use crate::ann::Metric;
use pgrx::prelude::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

// Lists probed per structured IVFFlat scan (bounds the pages read — the partial-read win, M31). M34: read from the
// `theodb_ivfflat.probes` GUC (default 10) instead of a fixed constant; still clamped to the actual list count.

/// Route a `build_lut16` failure to the SQLSTATE that matches its CAUSE. Diverges.
///
/// M146 (review F-db-1/F-arch-2 — a defect introduced by M146's own taxonomy pass). `build_lut16`
/// (`vec/ah.rs:63,68`) fails for two unrelated reasons, and the first pass sent both to `err_corrupt`:
///
/// - **empty codebook** — the persisted AQ quantizer has `dim == 0`: genuinely a storage problem → `XX002`;
/// - **query dim != codebook dim** — the CALLER passed a vector of the wrong width: an input problem → `22023`.
///
/// Collapsing them means a mistyped query against a perfectly healthy IVF-AQ index tells the operator
/// `index_corrupted` — the SQLSTATE they use to decide "REINDEX now", and the REINDEX will not help.
/// That is strictly worse than the XX000 it replaced: XX000 carried no information, this carries WRONG
/// information. The five `scan_ivf_aq*` bodies never validate the query width (unlike HNSW at `:289` and
/// SymQG at `:358`), so `build_lut16` is where that check actually lands.
///
/// The discriminator is the quantizer's STATE (`codebook_dim == 0`), not the text of the message. The first
/// version matched the substring `"query dim"` and its doc claimed `ah_tests.rs` pinned that literal —
/// **false**, caught by the review's second pass: `ah_tests.rs:94,104` assert only `is_err()`. Rewording
/// `vec/ah.rs:70` to "query dimension" would have re-inverted the taxonomy with the whole suite green. The
/// caller already holds the quantizer, so the structural check costs nothing and cannot rot.
/// M147 (Fase 2, bullet 2) — o erro tipado que os 8 gather helpers propagam com `?` até UM boundary
/// (`amrescan`/re-search), no lugar dos ~56 `match { Ok(v) => v, Err(e) => crate::pg::err_*(...) }` C-style.
/// Carrega a CLASSE (Corrupt → XX002, Input → 22023) até o boundary, preservando a taxonomia do M146.
/// `From<String>` faz o default = `Corrupt` (51 dos 56 sítios são decode-de-bytes = corrupção); os 5 sítios de
/// `build_lut16` escolhem a classe por `codebook_dim` (dim-mismatch = erro do chamador = Input) via
/// `lut_scan_error`. Idioma do crate: `columnar_agg.rs:939` (closure-`Result`); padrão pgvectorscale
/// (`next<S>` retorna `Option`, o callback traduz). O `raise` diverge no boundary.
pub(crate) enum ScanError {
    Corrupt(String),
    Input(String),
}

impl From<String> for ScanError {
    fn from(s: String) -> Self {
        ScanError::Corrupt(s)
    }
}

impl ScanError {
    /// Converte a classe no `ereport` correspondente, no boundary. Diverge (nunca retorna).
    fn raise(self) -> ! {
        match self {
            ScanError::Corrupt(m) => crate::pg::err_corrupt(&format!("theodb am scan: {m}")),
            ScanError::Input(m) => crate::pg::err_input(&format!("theodb am scan: {m}")),
        }
    }
}

/// `build_lut16` falha por duas causas: codebook vazio (`dim == 0`) = storage → Corrupt (XX002); dim-mismatch
/// = entrada do chamador → Input (22023). A mesma lógica de `lut_error` do M146 (F-db-1), agora produzindo um
/// `ScanError` propagável por `?` em vez de divergir no sítio.
fn lut_scan_error(codebook_dim: usize, e: String) -> ScanError {
    if codebook_dim == 0 {
        ScanError::Corrupt(format!("aq lut: {e}"))
    } else {
        ScanError::Input(format!("aq lut: {e}"))
    }
}

/// M147 (bullet 3) — o kernel Stage-1 compartilhado pelos 5 corpos `scan_ivf_aq*`. Itera os blocos de 32 da
/// lista, chama `ah_score_block` (o scorer SIMD), e invoca `on_candidate(ordinal, score)` por candidato.
/// RECEBE `codes_off` do chamador — NUNCA o recomputa: `codes_off` é conhecimento ON-DISK por-versão (v4
/// `8n+entry_f32·n`; v5/v6/v8 `8n`; v7 `8n+n·label_bytes`), e recomputá-lo aqui vazaria o layout on-disk para o
/// kernel domain, reintroduzindo o risco misparse→data-loss que a ADR-2 do M145 barra. A divergência por-versão
/// (decode de tid inline, membership, label) vive na closure `on_candidate`, não em branches de versão aqui.
#[inline]
fn stage1_score_blocks<F: FnMut(usize, i32)>(
    lut: &crate::vec::ah::Lut16,
    bytes: &[u8],
    codes_off: usize,
    n: usize,
    pairs: usize,
    mut on_candidate: F,
) {
    let nblocks = n.div_ceil(32);
    let mut out = [0i32; 32];
    for b in 0..nblocks {
        let bn = (n - b * 32).min(32);
        let base = codes_off + b * pairs * 32;
        if bytes.len() < base + pairs * 32 {
            break; // página mais curta que a contagem do diretório declara — para esta lista
        }
        crate::vec::ah::ah_score_block(lut, &bytes[base..base + pairs * 32], bn, &mut out[..bn]);
        for (j, &score) in out.iter().enumerate().take(bn) {
            on_candidate(b * 32 + j, score);
        }
    }
}

/// One scored candidate: its distance-to-query key + heap TID. Ordered ascending by (distance, tid) — the exact
/// order the old `results.sort_by` produced, so the emitted top-K is byte-identical (recall unchanged, M36 ADR-1).
/// `f64::total_cmp` is a TOTAL order — under M49 cosine a zero-norm vector yields NaN, which `total_cmp` orders
/// LAST (EC-3), so a degenerate vector sinks to the end instead of panicking; the `tid` tiebreak reproduces the
/// stable order of the previous sort.
#[derive(PartialEq)]
struct Scored {
    d: f64,
    tid: i64,
}
impl Eq for Scored {}
impl Ord for Scored {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.d.total_cmp(&o.d).then(self.tid.cmp(&o.tid))
    }
}
impl PartialOrd for Scored {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

/// M36: a lazy MIN-heap over the scan candidates instead of a fully-sorted Vec. `amrescan` heapifies in O(C)
/// (`BinaryHeap::from`); `amgettuple` pops the next-nearest in O(log C). The executor pulls ~k times for an
/// `ORDER BY <-> q LIMIT k`, so total work is O(C + k·log C) vs the old O(C·log C) full sort — the ~38% `sort`
/// phase the M36 measurement flagged. `Reverse` turns the max-heap into a min-heap (nearest first).
struct ScanState {
    heap: BinaryHeap<Reverse<Scored>>,
    // M52 iterative scan (HNSW only): when the heap exhausts and the executor keeps pulling (a selective WHERE
    // rejected the emitted tuples), re-search with a growing `ef`, dedup already-emitted tids, up to
    // `max_scan_tuples`. Empty query / non-HNSW index ⇒ `iterative = false` (pre-M52 behavior: at most ef_search).
    query: Vec<f32>,
    rel: pg_sys::Relation,
    ef: usize,
    // M87 — IVF iterative cursor: the `probes` the next re-search grows from. `> 0` ⇒ the index is IVF (grow
    // probes); `0` ⇒ HNSW (grow `ef`). Under a selective WHERE the executor keeps pulling, so we probe MORE lists
    // (the true NN may be in an unprobed list) AND widen the AQ rerank pool (once all lists are probed, the pool
    // caps the distinct emitted candidates — a selective filter needs more reranked to find k survivors) until
    // `max_scan_tuples` distinct tids are emitted (recall preserved).
    probes: usize,
    rerank_pool: usize,
    lists: usize, // M87 — total IVF list count; the iterative re-search grows `probes` up to this, then stops.
    last_total: usize, // M87 — candidates the last IVF scan returned; growth stops once it stops increasing.
    emitted: std::collections::HashSet<i64>,
    iterative: bool,
    exhausted: bool,
    // M90 (inline label filter): the query's sorted-deduped label set parsed from the `labels && '{…}'` ScanKey in
    // `amrescan`. Empty = no label predicate (unchanged v3/v4/v5/v6 scan). A v7 index consults it in Stage-1 to skip
    // non-overlapping candidates before the rerank.
    query_labels: Vec<i16>,
    // M90: a label predicate is active → `amgettuple` MUST keep `xs_recheck=true` so the executor re-checks
    // `labels && …` on the heap tuple. Load-bearing for the PENDING region (post-build INSERTs carry no stored
    // label → the inline skip cannot filter them; only the executor recheck can). Without this the pending row
    // would be a false positive (review blocker).
    filtering: bool,
    // M118: resume-from-discarded state (V1 exact-f32 HNSW path only). `Some` after the first gather when the
    // index is a plain HNSW; the iterative scan then pulls the next batch from the RETAINED frontier
    // (`resumable_next`) instead of re-searching the graph with a doubled `ef` (the ~3× M52 cost). SBQ/AQ indexes
    // keep `None` and fall back to the M52 re-search (their per-batch rerank is a tracked follow-up).
    resume: Option<crate::am::hnsw_page::HnswResume>,
    // M146 (#177): o `amrescan` recebeu zero ScanKeys de ORDER BY. Um índice vetorial não tem ordem própria,
    // então este scan é irrespondível — `amgettuple` levanta erro tipado em vez de fingir "zero linhas".
    unordered: bool,
}

#[pg_guard]
pub extern "C-unwind" fn ambeginscan(
    index_relation: pg_sys::Relation,
    nkeys: ::std::os::raw::c_int,
    norderbys: ::std::os::raw::c_int,
) -> pg_sys::IndexScanDesc {
    let scandesc = unsafe { pg_sys::RelationGetIndexScan(index_relation, nkeys, norderbys) };
    let state = Box::new(ScanState {
        unordered: false,
        heap: BinaryHeap::new(),
        query: Vec::new(),
        rel: index_relation,
        ef: 0,
        probes: 0,
        rerank_pool: 0,
        lists: 0,
        last_total: 0,
        emitted: std::collections::HashSet::new(),
        iterative: false,
        exhausted: false,
        query_labels: Vec::new(),
        filtering: false,
        resume: None,
    });
    unsafe { (*scandesc).opaque = Box::into_raw(state).cast::<std::os::raw::c_void>() };
    scandesc
}

/// Heapify the scan candidates into a lazy min-heap (M36) — O(C), replacing the old O(C·log C) full sort.
fn heapify(candidates: Vec<(i64, f64)>) -> BinaryHeap<Reverse<Scored>> {
    BinaryHeap::from(
        candidates.into_iter().map(|(tid, d)| Reverse(Scored { d, tid })).collect::<Vec<_>>(),
    )
}

#[pg_guard]
pub extern "C-unwind" fn amrescan(
    scan: pg_sys::IndexScanDesc,
    keys: pg_sys::ScanKey,
    nkeys: ::std::os::raw::c_int,
    orderbys: pg_sys::ScanKey,
    norderbys: ::std::os::raw::c_int,
) {
    unsafe {
        let scan_ref = &mut *scan;
        let state = &mut *scan_ref.opaque.cast::<ScanState>();
        // M52: reset the iterative-scan state on every rescan (the executor may rescan the same descriptor).
        state.heap.clear();
        state.emitted.clear();
        state.exhausted = false;
        state.iterative = false;
        state.resume = None; // M118: drop any retained frontier — a rescan restarts the search from scratch
        // M90: parse the `labels && '{…}'` ScanKey (our only pushable search predicate — the label opclass's
        // `OPERATOR 1 &&`) into the query label set. A non-NULL key argument is the query `smallint[]`. `xs_recheck`
        // is set so the executor re-checks the predicate on the heap tuple (correctness even if the inline skip is
        // lossy). No keys → empty set → unchanged scan.
        state.query_labels.clear();
        if nkeys > 0 && !keys.is_null() {
            let ks = std::slice::from_raw_parts(keys, nkeys as usize);
            for k in ks {
                if k.sk_flags as u32 & pg_sys::SK_ISNULL == 0 {
                    if let Some(mut v) = Vec::<i16>::from_datum(k.sk_argument, false) {
                        v.sort_unstable();
                        v.dedup();
                        state.query_labels = v;
                        (*scan).xs_recheck = true;
                        break;
                    }
                }
            }
        }
        // M92 v1a: a TID membership set (from a Custom Scan node's bitmap sub-plan) is an additional inline filter.
        // Like the label predicate it keeps `xs_recheck` on — membership is an ADMISSION filter (lossy bitmap pages
        // + the label-less pending region can over-admit), so the executor / Custom Scan node re-checks the real qual.
        let has_membership = crate::am::customscan::has_membership();
        if has_membership {
            (*scan).xs_recheck = true;
        }
        state.filtering = !state.query_labels.is_empty() || has_membership;

        // M146 (#177) — um scan SEM `ORDER BY <->` NÃO pode ser respondido por este AM: sem vetor de consulta
        // não existe ordem, e um grafo HNSW não é uma lista de todas as TIDs. O erro é levantado em
        // `amgettuple`, não aqui — ver o comentário longo lá. Marcar o estado e retornar mantém o `amrescan`
        // livre de `ereport`, exatamente como o `hnswrescan` do pgvector (`src/hnswscan.c:167`), que só copia
        // as ScanKeys; o guard equivalente do upstream vive no `hnswgettuple` (`:214`).
        state.unordered = norderbys < 1 || orderbys.is_null();
        if state.unordered {
            return;
        }
        // A NULL query vector (`ORDER BY col <-> NULL`) has SK_ISNULL set and a 0 sk_argument — dereferencing it
        // would segfault in pg_detoast_datum. Treat it as an empty scan (matches pgvector's ivfscan/hnswscan).
        if (*orderbys).sk_flags as u32 & pg_sys::SK_ISNULL != 0 {
            return;
        }
        // Serialize against a concurrent VACUUM fold (share mode — compatible with other scans/inserts, blocks the
        // exclusive rewrite). Prevents a torn read of the index/pending region mid-rewrite.
        crate::am::lock::index_shared(scan_ref.indexRelation);
        let query = datum_to_vec_f32((*orderbys).sk_argument);
        let rel = scan_ref.indexRelation;

        // Dispatch on the persisted layout: structured IVFFlat (M31 — partial-page read) vs the M26 blob (HNSW).
        let magic = match page::peek_magic(rel) {
            Ok(m) => m,
            Err(e) => crate::pg::err_corrupt(&format!("theodb am scan: {e}")),
        };
        if magic == 0 {
            return; // empty / unbuilt index
        }
        // M36: the scan functions heapify their candidates into a lazy min-heap (O(C)) so `amgettuple` pops the
        // top-K in O(k·log C) — replacing the old O(C·log C) full sort (the measured ~38% `sort` phase).
        state.heap = if magic == page::IVF_STRUCT_MAGIC {
            // M87: IVF (v3/v4/v5/v6) now participates in iterative scan — under a selective WHERE the executor keeps
            // pulling, so `amgettuple` re-searches with a GROWN `probes` (more lists) until `max_scan_tuples`
            // distinct tids are emitted (recall preserved). Armed only when max_scan_tuples > 0.
            let probes0 = crate::am::guc::probes();
            let pool0 = 64 * crate::am::guc::over_fetch();
            state.query = query.clone();
            state.rel = rel;
            state.probes = probes0;
            state.rerank_pool = pool0;
            state.lists = page::ivf_list_count(rel);
            state.iterative = crate::am::guc::max_scan_tuples() > 0;
            let init = scan_ivf_structured(rel, &query, probes0, pool0, &state.query_labels)
                .unwrap_or_else(|e| e.raise());
            state.last_total = init.len();
            heapify(init)
        } else if magic == crate::am::hnsw_page::HNSW_STRUCT_MAGIC {
            // M52: HNSW is the only layout with iterative scan. Record the query + starting ef so `amgettuple`
            // can grow the search under a selective WHERE. iterative is armed only when max_scan_tuples > 0.
            let ef = crate::am::guc::ef_search();
            state.query = query.clone();
            state.rel = rel;
            state.ef = ef;
            state.iterative = crate::am::guc::max_scan_tuples() > 0;
            // M118: try resume-from-discarded (V1 exact-f32). `resumable_init` returns None for SBQ/AQ/empty →
            // fall back to the M52 gather (traverse + pending fold). On Some, keep the retained frontier in
            // `state.resume` so `amgettuple` resumes from it instead of re-searching with a doubled `ef`.
            let meta = match crate::am::hnsw_page::read_meta(rel) {
                Ok(m) => m,
                Err(e) => crate::pg::err_corrupt(&format!("theodb am scan: {e}")),
            };
            if meta.node_count > 0 && query.len() != meta.dim as usize {
                crate::pg::err_input(&format!(
                    "theodb hnsw: query dim {} != index dim {}",
                    query.len(),
                    meta.dim
                ));
            }
            let init: Vec<(i64, f64)> =
                match crate::am::hnsw_page::resumable_init(rel, &meta, &query, ef) {
                    Ok(Some(mut rg)) => {
                        let mut b =
                            match crate::am::hnsw_page::resumable_next(rel, &meta, &query, &mut rg)
                            {
                                Ok(b) => b,
                                Err(e) => crate::pg::err_corrupt(&format!("theodb am scan: {e}")),
                            };
                        // Fold pending (post-build INSERTs) into the FIRST batch only; later resumed batches never
                        // re-fold — the scan dedups tids. Mirrors `gather_hnsw_candidates`.
                        let metric = match Metric::from_tag(meta.metric_tag) {
                            Some(m) => m,
                            None => crate::pg::err_corrupt(
                                "theodb am scan: unknown metric tag in persisted index meta",
                            ),
                        };
                        let pending = match page::read_pending(rel) {
                            Ok(p) => p,
                            Err(e) => crate::pg::err_corrupt(&format!("theodb am scan: {e}")),
                        };
                        for (tidv, v) in pending {
                            b.push((tidv, metric.dist(&query, &v)));
                        }
                        state.resume = Some(rg);
                        b
                    }
                    Ok(None) => {
                        gather_hnsw_candidates(rel, &query, ef).unwrap_or_else(|e| e.raise())
                    }
                    Err(e) => crate::pg::err_corrupt(&format!("theodb am scan: {e}")),
                };
            heapify(init)
        } else {
            scan_blob(rel, &query)
        };
    }
}

// M146 T2.4: `scan_hnsw_structured` foi removida aqui — era dead code (zero callers no repo inteiro),
// superseded pelo refactor do amrescan, que chama `gather_hnsw_candidates` direto para poder crescer o `ef`
// entre iterações. Os helpers que ela usava seguem vivos e usados por outros caminhos (#169).

/// M52: gather the HNSW scan candidates (traverse at `ef` + pending fold) as a raw `(tid, dist)` Vec. Extracted
/// so the iterative scan (`amgettuple`) can re-run it with a growing `ef` and dedup already-emitted tids, without
/// re-heapifying inside. `ef` is a parameter (not the GUC) precisely so the iterative loop can grow it.
unsafe fn gather_hnsw_candidates(
    rel: pg_sys::Relation,
    query: &[f32],
    ef: usize,
) -> Result<Vec<(i64, f64)>, ScanError> {
    let meta = crate::am::hnsw_page::read_meta(rel)?;
    let metric = Metric::from_tag(meta.metric_tag)
        .ok_or_else(|| ScanError::Corrupt("unknown metric tag in persisted index meta".into()))?;
    // Fail-fast with a typed error (mirrors pgvector's "different vector dimensions") instead of letting a
    // cross-dim query reach the SIMD scorer's length assertion as a bare panic across the C boundary. Only when
    // the index has nodes: an empty index carries dim=0 and traverse short-circuits to [] regardless of the query.
    if meta.node_count > 0 && query.len() != meta.dim as usize {
        crate::pg::err_input(&format!(
            "theodb hnsw: query dim {} != index dim {}",
            query.len(),
            meta.dim
        ));
    }
    let mut results = crate::am::hnsw_page::traverse(rel, &meta, query, ef)?;
    // Fold in pending (INSERTed after build) — no rebuild (mirror the IVF path).
    let pending = page::read_pending(rel)?;
    for (tidv, v) in pending {
        results.push((tidv, metric.dist(query, &v)));
    }
    Ok(results)
}

/// M31 partial-page scan: read the meta + centroids (∝ nlists), pick the `SCAN_PROBES` nearest centroids, and read
/// ONLY those lists' pages (∝ probes) — never the whole index. Merge the pending region. Ascending distance.
// M87 — returns the candidate `(tid, distance)` Vec (NOT a heap) so the caller can dedup already-emitted tids on
// an iterative re-search under a selective WHERE. `probes` is passed in (the iterative cursor grows it), not read
// from the GUC. Dispatches to the v4/v5/v6 AQ gathers or the v3 f32 path.
unsafe fn scan_ivf_structured(
    rel: pg_sys::Relation,
    query: &[f32],
    probes: usize,
    rerank_pool: usize,
    query_labels: &[i16],
) -> Result<Vec<(i64, f64)>, ScanError> {
    // M147 — the version discriminant is read ONCE (`ivf_version` reads block 0 a single time) and matched,
    // replacing the M31..E1 if-ladder that re-read block 0 up to 5×. Each arm dispatches to the per-version
    // body whose ON-DISK decode stays separate (ADR-2). A `V3` falls through to the f32 body below; an
    // unknown discriminant on a TIVS-magic index is corruption → XX002 (the operator REINDEXes), never
    // silently read as v3. The comment history of each path: v7=M90 inline label filter, v4=M77 pg_scann,
    // v5=M83 storage-separated, v6=M85 SQ8 refine, v8=E1 f32-free RaBitQ.
    match page::ivf_version(rel) {
        Ok(page::IvfVersion::V7) => {
            return scan_ivf_aq_split_v7(rel, query, probes, rerank_pool, query_labels);
        }
        Ok(page::IvfVersion::V4) => return scan_ivf_aq(rel, query, probes, rerank_pool),
        Ok(page::IvfVersion::V5) => return scan_ivf_aq_split(rel, query, probes, rerank_pool),
        Ok(page::IvfVersion::V6) => return scan_ivf_aq_split_sq8(rel, query, probes, rerank_pool),
        Ok(page::IvfVersion::V8) => {
            return scan_ivf_aq_split_rabitq(rel, query, probes, rerank_pool);
        }
        Ok(page::IvfVersion::V3) => {} // f32 structured — falls through to the body below
        Err(e) => return Err(e.into()),
    }
    // v3 f32 IVF reranks ALL probed candidates exactly (no AH prune pool) — `rerank_pool` unused here.
    let _ = rerank_pool;
    let meta = page::read_ivf_meta(rel)?;
    let metric = Metric::from_tag(meta.metric_tag)
        .ok_or_else(|| ScanError::Corrupt("unknown metric tag in persisted index meta".into()))?;
    // NOTE: do NOT early-return on empty centroids — an index built (or vacuumed) empty still has a pending
    // region with INSERTed rows that must be folded in (else those rows are silently dropped). The probe loop
    // below is simply empty when there are no centroids, and the pending fold still runs.
    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    let dim = meta.dim as usize;
    let entry = 8 + dim * 4;

    let mut results: Vec<(i64, f64)> = Vec::new();
    // M49: every metric (L2/IP/cosine) scores DIRECTLY off the page bytes via a fused zero-alloc kernel — no
    // per-entry `scratch` decode. (L2 keeps its AVX2+FMA path; IP/cosine are scalar-from-bytes, still zero-alloc.)
    // Opt-in phase profiler (THEODB_SCAN_PROFILE=1): attribute the scan latency across {reads, score, sort} so the
    // optimization targets the REAL bottleneck, not an assumed one (measurement-first — ADR D3/D4). Off by default:
    // std::time::Instant is only sampled at list granularity (≤ probes pairs), never per-candidate.
    let profile = std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1");
    let mut read_us = 0u128;
    let mut score_us = 0u128;
    let mut cand = 0usize;
    for &(_, ci) in cd.iter().take(probes) {
        let (fb, np, cnt) = meta.dir[ci];
        let t_read = std::time::Instant::now();
        let bytes = page::read_ivf_list_bytes(rel, fb, np)?;
        if profile {
            read_us += t_read.elapsed().as_micros();
        }
        let t_score = std::time::Instant::now();
        for i in 0..cnt as usize {
            let o = i * entry;
            if bytes.len() < o + entry {
                break; // page shorter than the directory count claims — stop this list
            }
            let tidv = i64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
            // M49: 3-way fused, zero-alloc for all metrics (was: L2 fused; cosine/ip decoded `scratch` per entry).
            let raw = &bytes[o + 8..o + entry];
            let d = match metric {
                Metric::L2 => crate::vec::l2_dist_from_bytes(query, raw),
                Metric::Ip => crate::vec::ip_dist_from_bytes(query, raw),
                Metric::Cosine => crate::vec::cosine_dist_from_bytes(query, raw),
            };
            results.push((tidv, d));
            cand += 1;
        }
        if profile {
            score_us += t_score.elapsed().as_micros();
        }
    }
    // Fold in pending (INSERTed after build) — no rebuild.
    let pending = page::read_pending(rel)?;
    for (tidv, v) in pending {
        results.push((tidv, metric.dist(query, &v)));
    }
    if profile {
        // Phase attribution for the scan (opt-in observability — the wiring-triad runtime metric). `nonempty`
        // surfaces list-balance health: a near-1 value on distinct data signals a degenerate build/corpus. M87:
        // heapify moved to the caller (the iterative re-search needs the raw Vec to dedup already-emitted tids).
        let nonempty = meta.dir.iter().filter(|(_, _, c)| *c > 0).count();
        pgrx::log!(
            "theodb scan profile: cand={cand} nonempty_lists={nonempty}/{} probes={probes} \
             reads={read_us}us score={score_us}us",
            meta.centroids.len()
        );
    }
    Ok(results)
}

/// M77 (pg_scann) — the IVF-AQ v4 scan: probe nprobe lists → batched AH-LUT scan over the block32 codes (the
/// ~5-7× QPS lever the M75 spike proved) → rerank the `over_fetch` best by exact f32. O(probes) page reads, like
/// the f32 IVF path, but the per-candidate score is a near-free LUT lookup instead of 128 mults. Ports
/// `ann::ivf_aqah::search` to read from the persisted v4 pages.
unsafe fn scan_ivf_aq(
    rel: pg_sys::Relation,
    query: &[f32],
    probes: usize,
    rerank_pool: usize,
) -> Result<Vec<(i64, f64)>, ScanError> {
    let meta = page::read_ivf_aq_meta(rel)?;
    let quant = crate::vec::aq::AqQuantizer::from_meta_bytes(&meta.codebook)
        .map_err(|e| ScanError::Corrupt(format!("aq codebook: {e}")))?;
    let lut =
        crate::vec::ah::build_lut16(query, &quant).map_err(|e| lut_scan_error(quant.dim(), e))?;
    let metric = Metric::from_tag(meta.metric_tag)
        .ok_or_else(|| ScanError::Corrupt("unknown metric tag in persisted index meta".into()))?;
    let dim = meta.dim as usize;
    let entry_f32 = dim * 4;
    let pairs = (meta.m as usize).div_ceil(2);

    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    // M84 — AQ rerank pool = base 64 × over_fetch factor. Fixes a latent no-op: the old `over_fetch().max(64)`
    // was ALWAYS 64 (over_fetch ≤ 64, so the .max(64) floor always won), so `theodb_hnsw.over_fetch` never
    // widened the AQ rerank pool. `64 * over_fetch()` keeps the default (over_fetch=1 → 64) but lets a wider pool
    // recover recall when the AH prune quality caps it (the M83 recall-ceiling lever).
    // M87: `rerank_pool` is a parameter — the iterative re-search widens it (once all lists are probed, the pool
    // caps the distinct emitted; a selective filter needs more reranked). Floored at 1 for a degenerate call.
    let rerank_pool = rerank_pool.max(1);

    // Stage 1 — batched AH scan of probed lists. Cache each list's bytes for the rerank (read once, O(probes)).
    let mut listbytes: Vec<(usize, Vec<u8>)> = Vec::new(); // (n, bytes)
    let mut cands: Vec<(i32, usize, usize)> = Vec::new(); // (ah_score, listbytes_idx, entry)
    for &(_, ci) in cd.iter().take(probes) {
        let (fb, np, cnt) = meta.dir[ci];
        let n = cnt as usize;
        if n == 0 {
            continue;
        }
        let bytes = page::read_ivf_list_bytes(rel, fb, np)?;
        let codes_off = 8 * n + entry_f32 * n;
        let lbidx = listbytes.len();
        stage1_score_blocks(&lut, &bytes, codes_off, n, pairs, |ordinal, score| {
            cands.push((score, lbidx, ordinal));
        });
        listbytes.push((n, bytes));
    }

    // Stage 2 — rerank the `rerank_pool` best (smaller AH score = closer) with the exact f32 distance.
    let rn = rerank_pool.min(cands.len());
    if rn < cands.len() {
        cands.select_nth_unstable_by_key(rn, |c| c.0);
        cands.truncate(rn);
    }
    let mut results: Vec<(i64, f64)> = Vec::with_capacity(cands.len());
    for (_, lbidx, entry) in &cands {
        let (n, bytes) = &listbytes[*lbidx];
        let tid = i64::from_le_bytes(bytes[entry * 8..entry * 8 + 8].try_into().unwrap());
        let f32_off = 8 * n + entry * entry_f32;
        let raw = &bytes[f32_off..f32_off + entry_f32];
        let d = match metric {
            Metric::L2 => crate::vec::l2_dist_from_bytes(query, raw),
            Metric::Ip => crate::vec::ip_dist_from_bytes(query, raw),
            Metric::Cosine => crate::vec::cosine_dist_from_bytes(query, raw),
        };
        results.push((tid, d));
    }
    // M81 — fold in pending (rows INSERTed after build): f32, scored EXACTLY (never quantized), no rebuild. Same
    // contract as the v3 f32 path — else post-build INSERTs are silently dropped from a WITH (pq_subspaces) index.
    let pending =
        page::read_pending(rel).map_err(|e| ScanError::Corrupt(format!("pending: {e}")))?;
    for (tidv, v) in pending {
        results.push((tidv, metric.dist(query, &v)));
    }
    Ok(results)
}

/// M83 (Roadmap v7 D3 spike) — the IVF-AQ v5 STORAGE-SEPARATED scan: Stage 1 reads ONLY each probed list's
/// compact CODE pages (ids + block32 codes, ~24 B/vec) and AH-prunes; Stage 2 random-reads the f32 for the
/// `over_fetch` survivors ONLY (~512 B/vec × R). Fixes the v4 interleaving (ADR-0037) that forced the full f32
/// I/O on every candidate. Recall is byte-identical to v4/v3 (same AH prune + exact rerank); the win is the far
/// smaller working set paged in during the scan-heavy Stage 1.
unsafe fn scan_ivf_aq_split(
    rel: pg_sys::Relation,
    query: &[f32],
    probes: usize,
    rerank_pool: usize,
) -> Result<Vec<(i64, f64)>, ScanError> {
    let meta = page::read_ivf_aq_meta_split(rel)?;
    let quant = crate::vec::aq::AqQuantizer::from_meta_bytes(&meta.codebook)
        .map_err(|e| ScanError::Corrupt(format!("aq codebook: {e}")))?;
    let lut =
        crate::vec::ah::build_lut16(query, &quant).map_err(|e| lut_scan_error(quant.dim(), e))?;
    let metric = Metric::from_tag(meta.metric_tag)
        .ok_or_else(|| ScanError::Corrupt("unknown metric tag in persisted index meta".into()))?;
    let dim = meta.dim as usize;
    let pairs = (meta.m as usize).div_ceil(2);

    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    // M84 — AQ rerank pool = base 64 × over_fetch factor. Fixes a latent no-op: the old `over_fetch().max(64)`
    // was ALWAYS 64 (over_fetch ≤ 64, so the .max(64) floor always won), so `theodb_hnsw.over_fetch` never
    // widened the AQ rerank pool. `64 * over_fetch()` keeps the default (over_fetch=1 → 64) but lets a wider pool
    // recover recall when the AH prune quality caps it (the M83 recall-ceiling lever).
    // M87: `rerank_pool` is a parameter — the iterative re-search widens it (once all lists are probed, the pool
    // caps the distinct emitted; a selective filter needs more reranked). Floored at 1 for a degenerate call.
    let rerank_pool = rerank_pool.max(1);

    // M92: arbitrary-WHERE TID membership (from a Custom Scan node's bitmap sub-plan) — the inline pre-filter also
    // engages on the plain-vector v5 layout (not just the v7 label layout): Stage-1 skips candidates whose TID is
    // not a member, so the rerank pool fills with matching candidates instead of being starved by the post-filter.
    let membership = crate::am::customscan::membership();
    // M91 selectivity-adaptive probing (generalized to v5): a selective membership starves the fixed `probes` (the
    // true matching NN hide in unprobed lists — measured INLINE recall 0.91 vs POST 1.0 at fixed probes), so when
    // filtering, keep probing nearest lists past `probes` until the matching-candidate pool reaches `rerank_pool`.
    // Empty membership ⇒ break at exactly `probes` (byte-identical to the old `.take(probes)` scan).
    let filtering = membership.is_some();

    // Stage 1 — read ONLY the CODE pages of each probed list; AH-score; keep (score, tid, list_ci, ordinal).
    let mut cands: Vec<(i32, i64, usize, usize)> = Vec::new();
    let mut probed = 0usize;
    for &(_, ci) in cd.iter() {
        if probed >= probes && (!filtering || cands.len() >= rerank_pool) {
            break;
        }
        probed += 1;
        let (cfb, cnp, _vfb, _vnp, cnt) = meta.dir[ci];
        let n = cnt as usize;
        if n == 0 {
            continue;
        }
        let cbytes = page::read_ivf_list_bytes(rel, cfb, cnp)?;
        let codes_off = 8 * n; // codes start right after the n ids
        stage1_score_blocks(&lut, &cbytes, codes_off, n, pairs, |ordinal, score| {
            let tid = i64::from_le_bytes(cbytes[ordinal * 8..ordinal * 8 + 8].try_into().unwrap());
            // M92 arbitrary-WHERE inline skip — admit exact members OR lossy-bitmap blocks (recheck filters the
            // over-admit); empty membership ⇒ unchanged v5 scan.
            if let Some(m) = &membership {
                let block = (tid >> 16) as u32;
                if !m.exact.contains(&tid) && !m.lossy.contains(&block) {
                    return;
                }
            }
            cands.push((score, tid, ci, ordinal));
        });
    }

    // Stage 2 — rerank the `rerank_pool` best (smaller AH score = closer); random-read f32 for survivors ONLY.
    let rn = rerank_pool.min(cands.len());
    if rn < cands.len() {
        cands.select_nth_unstable_by_key(rn, |c| c.0);
        cands.truncate(rn);
    }
    let mut results: Vec<(i64, f64)> = Vec::with_capacity(cands.len());
    for (_, tid, ci, ordinal) in &cands {
        let (_cfb, _cnp, vfb, vnp, _cnt) = meta.dir[*ci];
        let raw = page::read_vec_at(rel, vfb, vnp, *ordinal, dim)
            .map_err(|e| ScanError::Corrupt(format!("v5 rerank: {e}")))?;
        let d = match metric {
            Metric::L2 => crate::vec::l2_dist_from_bytes(query, &raw),
            Metric::Ip => crate::vec::ip_dist_from_bytes(query, &raw),
            Metric::Cosine => crate::vec::cosine_dist_from_bytes(query, &raw),
        };
        results.push((*tid, d));
    }
    // M81 — fold in pending (rows INSERTed after build): f32, scored EXACTLY, no rebuild. Same contract as v3/v4.
    let pending =
        page::read_pending(rel).map_err(|e| ScanError::Corrupt(format!("pending: {e}")))?;
    for (tidv, v) in pending {
        results.push((tidv, metric.dist(query, &v)));
    }
    Ok(results)
}

/// M90 (inline label filter) — the IVF-AQ v7 scan: identical to the v5 split scan, EXCEPT the per-list CODE blob is
/// `[ids][labels_fixed][codes]`, so (a) the codes start `n·label_bytes` further in, and (b) each candidate's label
/// set is read inline in Stage 1 and, when the query carries a label predicate, non-overlapping candidates are
/// SKIPPED before they cost a rerank slot (the inline filter). `xs_recheck=true` (set in `amrescan`) guarantees
/// correctness even for the label-less pending region. Empty `query_labels` ⇒ no filter (behaves like v5).
unsafe fn scan_ivf_aq_split_v7(
    rel: pg_sys::Relation,
    query: &[f32],
    probes: usize,
    rerank_pool: usize,
    query_labels: &[i16],
) -> Result<Vec<(i64, f64)>, ScanError> {
    let meta = page::read_ivf_aq_meta_split(rel)?;
    let quant = crate::vec::aq::AqQuantizer::from_meta_bytes(&meta.codebook)
        .map_err(|e| ScanError::Corrupt(format!("aq codebook: {e}")))?;
    let lut =
        crate::vec::ah::build_lut16(query, &quant).map_err(|e| lut_scan_error(quant.dim(), e))?;
    let metric = Metric::from_tag(meta.metric_tag)
        .ok_or_else(|| ScanError::Corrupt("unknown metric tag in persisted index meta".into()))?;
    let dim = meta.dim as usize;
    let pairs = (meta.m as usize).div_ceil(2);
    let label_bytes = 2 + page::LABEL_K * 2;
    let label_filtering = !query_labels.is_empty();
    // M92 v1a: arbitrary-WHERE TID membership (from a Custom Scan node) — an inline skip orthogonal to the label
    // predicate; works on any IVF-AQ index (no label column required). Read once per scan (backend-local).
    let membership = crate::am::customscan::membership();
    // `filtering` drives the M91 adaptive probing: a selective membership must probe MORE lists too (else it
    // starves exactly like a selective label). Either an active label OR membership arms it.
    let filtering = label_filtering || membership.is_some();

    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    let rerank_pool = rerank_pool.max(1);

    // Stage 1 — read ONLY the CODE pages; AH-score; INLINE-SKIP non-overlapping labels before they cost a slot.
    // M91: selectivity-adaptive probing. A selective label filter starves the default `probes` — the true filtered
    // NN hide in lists the default never visits (measured: recall 0.741 @ 0.01% sel, recovered to ~1.0 by probing
    // more lists; wiki/benchmarks/m91-adaptive-filter.md). So when filtering, keep probing nearest lists PAST
    // `probes` until the matching-candidate pool reaches the rerank target `rerank_pool` (naturally bounded by
    // cd.len()). A non-filter or loose-filter query breaks at exactly `probes` (target already met) ⇒ byte-identical
    // to the previous fixed `.take(probes)` scan (the no-regression guarantee). Extreme selectivity (matches ≪
    // rerank_pool) degenerates toward a near-full-list scan — bounded and correct (ADR M91-3).
    let mut cands: Vec<(i32, i64, usize, usize)> = Vec::new();
    let mut probed = 0usize;
    for &(_, ci) in cd.iter() {
        if probed >= probes && (!filtering || cands.len() >= rerank_pool) {
            break;
        }
        probed += 1;
        let (cfb, cnp, _vfb, _vnp, cnt) = meta.dir[ci];
        let n = cnt as usize;
        if n == 0 {
            continue;
        }
        let cbytes = page::read_ivf_list_bytes(rel, cfb, cnp)?;
        let labels_off = 8 * n; // labels start right after the n ids
        let codes_off = 8 * n + n * label_bytes; // codes start after ids + the fixed label region
        stage1_score_blocks(&lut, &cbytes, codes_off, n, pairs, |ordinal, score| {
            // INLINE FILTER: skip candidates whose stored label set does not overlap the query's.
            if label_filtering
                && !v7_label_overlaps(&cbytes, labels_off, ordinal, label_bytes, query_labels)
            {
                return;
            }
            let tid = i64::from_le_bytes(cbytes[ordinal * 8..ordinal * 8 + 8].try_into().unwrap());
            // M92 arbitrary-WHERE inline skip — admit a candidate whose exact TID is a member OR whose block is a
            // lossy-bitmap block (over-admit → the node's ExecQual recheck filters it, ADR M93-2). Dropping the
            // lossy case would under-admit (silently miss valid rows on lossy pages).
            if let Some(m) = &membership {
                let block = (tid >> 16) as u32;
                if !m.exact.contains(&tid) && !m.lossy.contains(&block) {
                    return;
                }
            }
            cands.push((score, tid, ci, ordinal));
        });
    }
    // M91 — wiring-triad runtime metric (opt-in via THEODB_SCAN_PROFILE=1): the adaptive loop grew the probe count
    // past the default ONLY when the filter was selective, so `probes_effective > probes_default` is the observable
    // proof the adaptivity fired. Zero cost in production (env var unset).
    if std::env::var("THEODB_SCAN_PROFILE").is_ok_and(|v| v == "1") {
        pgrx::log!(
            "theodb v7 inline scan: filtering={filtering} probes_default={probes} probes_effective={probed} \
             matching_cands={} lists={}",
            cands.len(),
            cd.len()
        );
    }

    // Stage 2 — rerank the `rerank_pool` best; random-read f32 for survivors ONLY.
    let rn = rerank_pool.min(cands.len());
    if rn < cands.len() {
        cands.select_nth_unstable_by_key(rn, |c| c.0);
        cands.truncate(rn);
    }
    let mut results: Vec<(i64, f64)> = Vec::with_capacity(cands.len());
    for (_, tid, ci, ordinal) in &cands {
        let (_cfb, _cnp, vfb, vnp, _cnt) = meta.dir[*ci];
        let raw = page::read_vec_at(rel, vfb, vnp, *ordinal, dim)
            .map_err(|e| ScanError::Corrupt(format!("v7 rerank: {e}")))?;
        let d = match metric {
            Metric::L2 => crate::vec::l2_dist_from_bytes(query, &raw),
            Metric::Ip => crate::vec::ip_dist_from_bytes(query, &raw),
            Metric::Cosine => crate::vec::cosine_dist_from_bytes(query, &raw),
        };
        results.push((*tid, d));
    }
    // Pending region (post-build INSERTs) has NO stored label — emit it and let `xs_recheck` filter on the heap.
    let pending =
        page::read_pending(rel).map_err(|e| ScanError::Corrupt(format!("pending: {e}")))?;
    for (tidv, v) in pending {
        results.push((tidv, metric.dist(query, &v)));
    }
    Ok(results)
}

/// M90 — does the label set stored for `ordinal` (a `[u16 count][LABEL_K × i16]` record at `labels_off + ordinal·
/// label_bytes`) share any element with `query_labels`? Both label sets are tiny (≤ LABEL_K), so a nested scan is
/// cheapest. A stored count of 0 (NULL label at build) never overlaps.
#[inline]
fn v7_label_overlaps(
    cbytes: &[u8],
    labels_off: usize,
    ordinal: usize,
    label_bytes: usize,
    query_labels: &[i16],
) -> bool {
    let rec = labels_off + ordinal * label_bytes;
    if cbytes.len() < rec + label_bytes {
        return false;
    }
    let cnt = u16::from_le_bytes(cbytes[rec..rec + 2].try_into().unwrap()) as usize;
    for i in 0..cnt.min(page::LABEL_K) {
        let o = rec + 2 + i * 2;
        let lbl = i16::from_le_bytes(cbytes[o..o + 2].try_into().unwrap());
        if query_labels.contains(&lbl) {
            return true;
        }
    }
    false
}

/// M85 (Roadmap v7) — the IVF-AQ v6 SQ8-REFINE scan: identical to the v5 split scan (Stage 1 AH prune over
/// codes-only pages) but Stage 2 reranks the `over_fetch` survivors on SQ8 codes (`dim` B/vec) instead of raw f32
/// (`dim·4` B/vec) — 4× less Stage-2 random-read I/O at the high-recall frontier (M84). Rerank is asymmetric: the
/// SQ8 code is decoded to an approximate f32, then the EXACT `Metric::dist` is applied against the f32 query, so
/// the query is never quantized (recall degrades on one side only). Recall is SQ8-approximate for built rows,
/// EXACT for pending rows (the pending region stays f32).
unsafe fn scan_ivf_aq_split_sq8(
    rel: pg_sys::Relation,
    query: &[f32],
    probes: usize,
    rerank_pool: usize,
) -> Result<Vec<(i64, f64)>, ScanError> {
    let meta = page::read_ivf_aq_meta_split_sq8(rel)?;
    let quant = crate::vec::aq::AqQuantizer::from_meta_bytes(&meta.aq_codebook)
        .map_err(|e| ScanError::Corrupt(format!("aq codebook: {e}")))?;
    let sq8 = crate::sq8::Sq8Quantizer::from_meta_bytes(&meta.sq8_codebook)
        .map_err(|e| ScanError::Corrupt(format!("sq8 codebook: {e}")))?;
    let lut =
        crate::vec::ah::build_lut16(query, &quant).map_err(|e| lut_scan_error(quant.dim(), e))?;
    let metric = Metric::from_tag(meta.metric_tag)
        .ok_or_else(|| ScanError::Corrupt("unknown metric tag in persisted index meta".into()))?;
    let dim = meta.dim as usize;
    let pairs = (meta.m as usize).div_ceil(2);

    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    // M87: `rerank_pool` is a parameter — the iterative re-search widens it (once all lists are probed, the pool
    // caps the distinct emitted; a selective filter needs more reranked). Floored at 1 for a degenerate call.
    let rerank_pool = rerank_pool.max(1);

    // Stage 1 — read ONLY the CODE pages of each probed list; AH-score; keep (score, tid, list_ci, ordinal).
    let mut cands: Vec<(i32, i64, usize, usize)> = Vec::new();
    for &(_, ci) in cd.iter().take(probes) {
        let (cfb, cnp, _sfb, _snp, cnt) = meta.dir[ci];
        let n = cnt as usize;
        if n == 0 {
            continue;
        }
        let cbytes = page::read_ivf_list_bytes(rel, cfb, cnp)?;
        let codes_off = 8 * n;
        stage1_score_blocks(&lut, &cbytes, codes_off, n, pairs, |ordinal, score| {
            let tid = i64::from_le_bytes(cbytes[ordinal * 8..ordinal * 8 + 8].try_into().unwrap());
            cands.push((score, tid, ci, ordinal));
        });
    }

    // Stage 2 — rerank the `rerank_pool` best; random-read the SQ8 code for those survivors ONLY, decode, and
    // apply the exact metric against the f32 query (asymmetric). 128 B/survivor at dim=128, not 512 B.
    let rn = rerank_pool.min(cands.len());
    if rn < cands.len() {
        cands.select_nth_unstable_by_key(rn, |c| c.0);
        cands.truncate(rn);
    }
    let mut results: Vec<(i64, f64)> = Vec::with_capacity(cands.len());
    for (_, tid, ci, ordinal) in &cands {
        let (_cfb, _cnp, sfb, snp, _cnt) = meta.dir[*ci];
        let code = page::read_sq8_at(rel, sfb, snp, *ordinal, dim)
            .map_err(|e| ScanError::Corrupt(format!("v6 sq8 rerank: {e}")))?;
        let approx = sq8.decode(&code);
        results.push((*tid, metric.dist(query, &approx)));
    }
    // M81 — fold in pending (rows INSERTed after build): f32, scored EXACTLY (never quantized). Same as v3/v4/v5.
    let pending =
        page::read_pending(rel).map_err(|e| ScanError::Corrupt(format!("pending: {e}")))?;
    for (tidv, v) in pending {
        results.push((tidv, metric.dist(query, &v)));
    }
    Ok(results)
}

/// E1 — the IVF-AQ v8 RaBitQ-REFINE scan: identical Stage-1 to v5/v6 (AH prune over codes-only pages) but Stage-2
/// reranks the `rerank_pool` survivors on **f32-FREE** RaBitQ residual codes (`estimate_l2_sq`, no raw vector
/// touched — removing the exact M82/v5 Stage-2 f32 random-read bind). Residual-based: per PROBED list, precompute
/// `q_r = P·(query − centroid[ci])` and `qc2 = ‖query − centroid[ci]‖²`, then estimate per survivor. L2-only
/// (guarded at build). Pending rows stay f32-EXACT (never RaBitQ-encoded — they carry no centroid assignment).
unsafe fn scan_ivf_aq_split_rabitq(
    rel: pg_sys::Relation,
    query: &[f32],
    probes: usize,
    rerank_pool: usize,
) -> Result<Vec<(i64, f64)>, ScanError> {
    let meta = page::read_ivf_aq_meta_split_rabitq(rel)?;
    let quant = crate::vec::aq::AqQuantizer::from_meta_bytes(&meta.aq_codebook)
        .map_err(|e| ScanError::Corrupt(format!("aq codebook: {e}")))?;
    let rq = crate::vec::rabitq::RabitqQuantizer::from_meta_bytes(&meta.rabitq_codebook)
        .map_err(|e| ScanError::Corrupt(format!("rabitq codebook: {e}")))?;
    let lut =
        crate::vec::ah::build_lut16(query, &quant).map_err(|e| lut_scan_error(quant.dim(), e))?;
    let metric = Metric::from_tag(meta.metric_tag)
        .ok_or_else(|| ScanError::Corrupt("unknown metric tag in persisted index meta".into()))?;
    let dim = meta.dim as usize;
    let pairs = (meta.m as usize).div_ceil(2);

    let mut cd: Vec<(f64, usize)> =
        meta.centroids.iter().enumerate().map(|(i, c)| (metric.dist(query, c), i)).collect();
    cd.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let probes = probes.clamp(1, meta.centroids.len().max(1));
    let rerank_pool = rerank_pool.max(1);

    // Stage 1 — read ONLY the CODE pages of each probed list; AH-score; keep (score, tid, list_ci, ordinal).
    let mut cands: Vec<(i32, i64, usize, usize)> = Vec::new();
    for &(_, ci) in cd.iter().take(probes) {
        let (cfb, cnp, _rfb, _rnp, cnt) = meta.dir[ci];
        let n = cnt as usize;
        if n == 0 {
            continue;
        }
        let cbytes = page::read_ivf_list_bytes(rel, cfb, cnp)?;
        let codes_off = 8 * n;
        stage1_score_blocks(&lut, &cbytes, codes_off, n, pairs, |ordinal, score| {
            let tid = i64::from_le_bytes(cbytes[ordinal * 8..ordinal * 8 + 8].try_into().unwrap());
            cands.push((score, tid, ci, ordinal));
        });
    }

    // Stage 2 — rerank the best `rerank_pool` survivors on f32-FREE RaBitQ residual codes. Per distinct probed list
    // `ci`, precompute (q_r = P·(query − centroid[ci]), qc2 = ‖query − centroid[ci]‖²) once (rotate is O(dim²)).
    let rn = rerank_pool.min(cands.len());
    if rn < cands.len() {
        cands.select_nth_unstable_by_key(rn, |c| c.0);
        cands.truncate(rn);
    }
    let mut qcache: Vec<Option<(Vec<f32>, f64)>> = vec![None; meta.centroids.len()];
    let mut results: Vec<(i64, f64)> = Vec::with_capacity(cands.len());
    for (_, tid, ci, ordinal) in &cands {
        let (_cfb, _cnp, rfb, rnp, _cnt) = meta.dir[*ci];
        if qcache[*ci].is_none() {
            let c = &meta.centroids[*ci];
            let qmc: Vec<f32> = query.iter().zip(c).map(|(&x, &cc)| x - cc).collect();
            let q_r = rq.rotate(&qmc);
            // qc2 = ‖q−c‖² (SQUARED L2). NOT metric.dist — that returns sqrt(‖·‖²) for L2, and the estimator
            // mixes qc2 with the squared residual norm nr², so a sqrt here corrupts the cross-list ranking.
            let qc2: f64 = qmc.iter().map(|&d| (d as f64) * (d as f64)).sum();
            qcache[*ci] = Some((q_r, qc2));
        }
        let (q_r, qc2) = qcache[*ci].as_ref().unwrap();
        let rec = page::read_rabitq_at(rel, rfb, rnp, *ordinal, dim)
            .map_err(|e| ScanError::Corrupt(format!("v8 rabitq rerank: {e}")))?;
        let code = crate::vec::rabitq::RabitqQuantizer::record_to_code(&rec, dim)
            .map_err(|e| ScanError::Corrupt(format!("v8 rabitq record: {e}")))?;
        // estimate_l2_sq returns SQUARED L2; sqrt it so v8 distances share the sqrt-L2 scale of the pending rows
        // (metric.dist) and of v5/v6 — the ORDER BY comparison must not mix squared and sqrt scales. Clamp the
        // rare negative estimate (quantization noise near d≈0) before the sqrt.
        results.push((*tid, rq.estimate_l2_sq(&code, q_r, *qc2).max(0.0).sqrt()));
    }
    // Pending rows (INSERTed after build): f32, scored EXACTLY (same as v5/v6). Never RaBitQ-encoded.
    let pending =
        page::read_pending(rel).map_err(|e| ScanError::Corrupt(format!("pending: {e}")))?;
    for (tidv, v) in pending {
        results.push((tidv, metric.dist(query, &v)));
    }
    Ok(results)
}

/// The M26 blob scan path — HNSW (and any legacy blob index): deserialize the whole index + search. O(N).
unsafe fn scan_blob(rel: pg_sys::Relation, query: &[f32]) -> BinaryHeap<Reverse<Scored>> {
    let blob = match page::read_blob(rel) {
        Ok(b) => b,
        Err(e) => crate::pg::err_corrupt(&format!("theodb am scan: {e}")),
    };
    if blob.is_empty() {
        return BinaryHeap::new();
    }
    let idx = match Persisted::from_bytes(&blob) {
        Ok(i) => i,
        Err(e) => crate::pg::err_corrupt(&format!("theodb am scan: {e}")),
    };
    let pending = match page::read_pending(rel) {
        Ok(p) => p,
        Err(e) => crate::pg::err_corrupt(&format!("theodb am scan: {e}")),
    };
    // `search_merged` already returns ascending-sorted; heapify is O(C) and keeps the uniform pop path (M36).
    heapify(idx.search_merged(query, &pending))
}

#[pg_guard]
pub extern "C-unwind" fn amgettuple(
    scan: pg_sys::IndexScanDesc,
    _direction: pg_sys::ScanDirection::Type,
) -> bool {
    unsafe {
        let scan_ref = &mut *scan;
        let state = &mut *scan_ref.opaque.cast::<ScanState>();
        // M146 (#177) — o executor está pedindo tuplas de um scan sem `ORDER BY <->`. Antes disto o AM
        // devolvia `false` na primeira chamada e o executor concluía, corretamente do ponto de vista dele,
        // que **a relação não tem linhas**. Medido em PG 18.4, `enable_seqscan=off`, tabela de 500 linhas:
        //
        //   SELECT count(*) FROM t;   -- B-tree: 500  |  theodb_hnsw: 0   ← resposta ERRADA, sem erro algum
        //   SELECT id     FROM t;     -- idem: zero linhas
        //   EXPLAIN: Aggregate -> Index Only Scan using t_hnsw on t
        //
        // O planner não está errado: para `count(*)` nenhuma coluna é necessária, então o teste de index-only
        // é vacuosamente satisfeito e qualquer índice serve. Errado estava o AM, que promete
        // `amoptionalkey = true` ("sei varrer sem ScanKey") e entregava zero linhas.
        //
        // O guard vive AQUI, e não no `amrescan`, porque é aqui que o executor de fato pede linhas — o
        // `amrescan` pode ser chamado em caminhos que nunca puxam tupla. É onde o pgvector o coloca
        // (`src/hnswscan.c:214`: `if (scan->orderByData == NULL) elog(ERROR, "cannot scan hnsw index
        // without order")`), e seguir o upstream aqui é mais seguro que inovar.
        // Lê a FONTE PRIMÁRIA (`numberOfOrderBys` do próprio descritor), não o flag derivado do argumento do
        // rescan — review pass-2 (F2-db-4). O pgvector faz o mesmo (`hnswscan.c:214` consulta
        // `scan->orderByData`, não um campo do opaque). O flag continua sendo mantido no rescan para leitura
        // humana, mas a decisão não depende de o rescan ter rodado antes: se `amgettuple` for alcançado por
        // um caminho que não passou pelo rescan, o guard ainda vale.
        if scan_ref.numberOfOrderBys < 1 || scan_ref.orderByData.is_null() || state.unordered {
            crate::pg::err_input(
                "theodb am: cannot scan a vector index without ORDER BY <distance operator> — \
                 a vector index has no total order of its own. Add the ORDER BY, or let the planner \
                 use a sequential/heap scan (this index cannot answer an unordered scan).",
            );
        }
        loop {
            // M36: pop the next-nearest candidate from the lazy min-heap — O(log C) per call, and the executor
            // only pulls ~k times for a `LIMIT k`, so the scan never pays the full O(C·log C) sort.
            if let Some(Reverse(scored)) = state.heap.pop() {
                // M52: dedup across iterative re-searches (a grown-ef search re-emits earlier candidates).
                if !state.emitted.insert(scored.tid) {
                    continue;
                }
                tid::set_on(scored.tid, &mut scan_ref.xs_heaptid);
                // Our stored vectors are the heap vectors, so within a batch the order is exact. Across iterative
                // batches the order is RELAXED (pgvector-0.8 default) — acceptable for a filtered scan; no recheck.
                scan_ref.xs_recheckorderby = false;
                // M90: keep the executor's heap re-check ON when a label predicate is active — the PENDING region
                // emits label-less rows the inline skip cannot filter (review blocker: else a non-matching pending
                // row is a false positive). No predicate ⇒ false (unchanged).
                scan_ref.xs_recheck = state.filtering;
                return true;
            }
            // M52 iterative scan: the heap is empty. Under a selective WHERE the executor keeps pulling, so grow
            // `ef` and re-search until `max_scan_tuples` distinct candidates are emitted (recall preserved).
            if !state.iterative || state.exhausted {
                return false;
            }
            let cap = crate::am::guc::max_scan_tuples();
            if state.emitted.len() >= cap {
                state.exhausted = true;
                return false;
            }
            // M87: grow the search. HNSW (state.probes == 0) grows `ef`; IVF (state.probes > 0) LOOPS growing both
            // `probes` (reach unprobed lists) and the AQ rerank pool (once all lists are probed the pool caps the
            // distinct emitted — a selective filter needs more reranked to find k). An empty intermediate list (or
            // a grow whose candidates were all already emitted) must NOT terminate — keep growing until fresh
            // candidates appear OR the scan saturates (all lists probed AND the returned total stops increasing).
            if state.probes == 0 {
                // M118: V1 resume-from-discarded — pull the next batch(es) from the RETAINED frontier instead of
                // re-searching the graph with a doubled `ef` (the ~3× M52 cost). SBQ/AQ indexes carry
                // `state.resume == None` and fall through to the M52 ef-doubling re-search below.
                if state.resume.is_some() {
                    let rel = state.rel;
                    let meta = match crate::am::hnsw_page::read_meta(rel) {
                        Ok(m) => m,
                        Err(e) => crate::pg::err_corrupt(&format!("theodb am scan: {e}")),
                    };
                    let resume_ceiling = crate::am::guc::hnsw_resume_max_bytes();
                    loop {
                        if state.emitted.len() >= cap {
                            state.exhausted = true;
                            return false;
                        }
                        // M118 T2.2: memory-ceiling fail-safe — if the retained frontier grew past
                        // `theodb_hnsw.resume_max_mb`, stop resuming and return what is already emitted (correctness
                        // preserved by the executor MVCC recheck + max_scan_tuples; EC-5 — a clean stop, no panic).
                        if resume_ceiling > 0
                            && state.resume.as_ref().map(|rg| rg.approx_bytes()).unwrap_or(0)
                                > resume_ceiling
                        {
                            state.exhausted = true;
                            return false;
                        }
                        // Disjoint field borrows: `&state.query` (immutable) + `&mut state.resume` (mutable).
                        // `if let` (not `unwrap`) — structurally cannot panic across the C boundary if a future
                        // edit ever clears `state.resume` mid-loop (review LOW: defense-in-depth). None today.
                        let (batch, frontier_empty) = {
                            let q = &state.query;
                            match state.resume.as_mut() {
                                Some(rg) => {
                                    let b = match crate::am::hnsw_page::resumable_next(
                                        rel, &meta, q, rg,
                                    ) {
                                        Ok(b) => b,
                                        Err(e) => {
                                            crate::pg::err_corrupt(&format!("theodb am scan: {e}"))
                                        }
                                    };
                                    (b, rg.exhausted())
                                }
                                None => (Vec::new(), true), // unreachable today (guarded by is_some) — treat as exhausted
                            }
                        };
                        let fresh: Vec<(i64, f64)> = batch
                            .into_iter()
                            .filter(|(tid, _)| !state.emitted.contains(tid))
                            .collect();
                        if !fresh.is_empty() {
                            state.heap = heapify(fresh);
                            break;
                        }
                        if frontier_empty {
                            state.exhausted = true; // frontier exhausted — the reachable graph is fully emitted
                            return false;
                        }
                        // this batch's candidates were all already emitted, but the frontier continues → next batch
                    }
                } else {
                    let new_ef = state.ef.saturating_mul(2).min(cap);
                    if new_ef <= state.ef {
                        state.exhausted = true; // ef already at the ceiling — no wider search possible
                        return false;
                    }
                    state.ef = new_ef;
                    let fresh: Vec<(i64, f64)> =
                        gather_hnsw_candidates(state.rel, &state.query, new_ef)
                            .unwrap_or_else(|e| e.raise())
                            .into_iter()
                            .filter(|(tid, _)| !state.emitted.contains(tid))
                            .collect();
                    if fresh.is_empty() {
                        state.exhausted = true; // grew ef but found nothing new — the whole graph is emitted
                        return false;
                    }
                    state.heap = heapify(fresh);
                }
            } else {
                loop {
                    if state.emitted.len() >= cap {
                        state.exhausted = true;
                        return false;
                    }
                    state.probes = state.probes.saturating_mul(2);
                    state.rerank_pool = state.rerank_pool.saturating_mul(2);
                    let all = scan_ivf_structured(
                        state.rel,
                        &state.query,
                        state.probes,
                        state.rerank_pool,
                        &state.query_labels,
                    )
                    .unwrap_or_else(|e| e.raise());
                    let total = all.len();
                    let fresh: Vec<(i64, f64)> =
                        all.into_iter().filter(|(tid, _)| !state.emitted.contains(tid)).collect();
                    if !fresh.is_empty() {
                        state.heap = heapify(fresh);
                        break;
                    }
                    // Nothing new. Exhaust only when fully saturated: every list probed AND the returned total
                    // stopped growing (the pool now covers all candidates). Else keep growing.
                    if state.probes >= state.lists.max(1) && total <= state.last_total {
                        state.exhausted = true;
                        return false;
                    }
                    state.last_total = total;
                }
            }
        }
    }
}

#[pg_guard]
pub extern "C-unwind" fn amendscan(scan: pg_sys::IndexScanDesc) {
    unsafe {
        let scan_ref = &mut *scan;
        if !scan_ref.opaque.is_null() {
            drop(Box::from_raw(scan_ref.opaque.cast::<ScanState>()));
            scan_ref.opaque = std::ptr::null_mut();
        }
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;

    /// The lazy min-heap MUST emit candidates in the exact order the old `results.sort_by` did — ascending by
    /// (distance, tid) — so the top-K (and thus recall) is byte-identical (M36 ADR-1). This is THE recall-preserved
    /// gate for the sort→heap change.
    #[pgrx::pg_test]
    fn heap_pops_same_order_as_sort_with_ties() {
        // Candidates with distance ties (2.0 twice) + an out-of-order input — the heap must sort them.
        let candidates: Vec<(i64, f64)> =
            vec![(30, 2.0), (10, 1.0), (50, 3.0), (20, 2.0), (40, 0.5)];

        // Reference: the exact comparator the old scan used (partial_cmp by dist, then tid).
        let mut expected = candidates.clone();
        expected.sort_by(|a, b| {
            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
        });

        // Under test: heapify + pop-all.
        let mut heap = heapify(candidates);
        let mut popped: Vec<(i64, f64)> = Vec::new();
        while let Some(Reverse(s)) = heap.pop() {
            popped.push((s.tid, s.d));
        }

        assert_eq!(
            popped, expected,
            "heap pop order must equal the old sort order (ties broken by tid)"
        );
        // Spot-check the tie is broken by tid ascending: 20 (d=2.0) before 30 (d=2.0).
        let pos20 = popped.iter().position(|&(t, _)| t == 20).unwrap();
        let pos30 = popped.iter().position(|&(t, _)| t == 30).unwrap();
        assert!(pos20 < pos30, "distance tie must break by tid ascending");
    }

    /// An empty candidate set → empty heap → the first pop returns None (empty scan; same as before).
    #[pgrx::pg_test]
    fn empty_heap_pops_none() {
        let mut heap = heapify(Vec::new());
        assert!(heap.pop().is_none());
    }
}
