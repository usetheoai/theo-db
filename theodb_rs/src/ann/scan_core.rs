//! FU-1: the pure ground-layer HNSW search, extracted behind a `NeighborSource` seam.
//!
//! The M46 change (pre-size the 3 per-query structures + reuse one neighbor scratch) lives in the ground loop
//! of the on-disk `am/hnsw_page.rs::traverse`. To measure its allocation cost cleanly — same-graph,
//! box-noise-immune, without page I/O confounds — the loop is extracted here as [`ground_search`], generic over
//! a [`NeighborSource`]. Production implements the seam over PostgreSQL pages (`PageNeighborSource`); the
//! criterion bench + these tests implement it over an in-memory `HnswIndex` ([`MemNeighborSource`]). This module
//! MUST NOT reference `pg_sys` (the bench link invariant — blueprint Q5): it is pure domain logic
//! (`architecture.md § 1` — the domain does not depend on infrastructure).
//!
//! The seam preserves production's page-read pattern: [`NeighborSource::neighbors_into`] lists neighbor *refs*
//! (one read), the loop dedups on the ref key, and ONLY non-visited refs are [`NeighborSource::load`]ed (one
//! read each). So `pages_read` is unchanged — the extraction is recall-neutral (proven by the M46 oracle on the
//! production side + the brute-force oracle on the in-memory side).

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashSet};

/// The DIP seam between the pure ground search and its storage (PostgreSQL pages OR an in-memory graph).
///
/// `Node` is a *loaded* candidate (its distance is known, it can be expanded, it carries the result tid).
/// `Ref` is an *unloaded* neighbor reference (an address / index) — cheap, produced by `neighbors_into` and
/// deduped by the loop BEFORE `load` is called, mirroring the on-disk scan (dedup-before-read).
pub(crate) trait NeighborSource {
    /// A loaded node — distance known, expandable. Must be `Copy` (pushed into both heaps).
    type Node: Copy;
    /// An unloaded neighbor reference — cheap, not yet scored. Must be `Copy` (held in the scratch buffer).
    type Ref: Copy;

    /// Distance of a loaded node to the query (cached in the node — never a re-score).
    fn dist(&self, node: &Self::Node) -> f64;
    /// The heap tid of a loaded node (the result payload).
    fn tid(&self, node: &Self::Node) -> i64;
    /// Dedup key of a loaded node (its own address / index).
    fn node_key(&self, node: &Self::Node) -> u64;
    /// Dedup key of an unloaded ref (production: packed `(blk,off)`; in-memory: the node index).
    fn ref_key(&self, r: &Self::Ref) -> u64;
    /// Ground-layer (`layer 0`) neighbor refs of a loaded node, appended into `out` (cleared first). One read.
    fn neighbors_into(&self, node: &Self::Node, out: &mut Vec<Self::Ref>) -> Result<(), String>;
    /// Load an unloaded ref into a node (distance + tid + expansion handle). One read. Only for fresh refs.
    fn load(&self, r: &Self::Ref) -> Result<Self::Node, String>;
    /// M56: whether a loaded node may enter the RESULT set. A tombstoned node is still navigated THROUGH (it
    /// enters the candidate heap and its neighbors are expanded — connectivity is preserved) but is NOT emitted.
    /// Default `true` (a graph without tombstones behaves byte-identically to the pre-M56 search).
    fn emittable(&self, _node: &Self::Node) -> bool {
        true
    }
}

/// A `(distance, node)` pair ordered by distance, NaN LAST (worst) — mirrors the on-disk `Cand` ordering
/// (`am/hnsw_page.rs`) so a zero-norm cosine vector (NaN distance) falls to the end instead of corrupting the
/// heaps (edge-case EC-5).
struct Ranked<N> {
    d: f64,
    node: N,
}
impl<N> PartialEq for Ranked<N> {
    fn eq(&self, o: &Self) -> bool {
        self.d == o.d
    }
}
impl<N> Eq for Ranked<N> {}
impl<N> PartialOrd for Ranked<N> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl<N> Ord for Ranked<N> {
    fn cmp(&self, o: &Self) -> Ordering {
        self.d.partial_cmp(&o.d).unwrap_or(Ordering::Equal)
    }
}

/// On-demand top-`ef` ground-layer search (the M46 hot loop): a min-heap of candidates to expand, a max-heap of
/// the `ef` best found, a `visited` dedup set, and one reused neighbor `scratch`. Returns `(tid, dist)` ascending.
///
/// `entry` is the loaded starting node (production passes the upper-layer-descended entry point; the in-memory
/// bench passes the graph's entry via [`MemNeighborSource::entry_node`]). `presize` is the SOLE axis the FU-1
/// bench flips: `true` pre-sizes the three per-query structures + scratch with
/// `with_capacity(ef·m0·2 / ef·m0 / ef+1 / m0)` (the M46 change — anchors pgvector `tidhash_create(ef*m*2)` +
/// pgvectorscale `with_capacity(search_list_size*neigbors)`); `false` uses `::new()` (the pre-M46 baseline).
/// The RESULT is byte-identical for both — pre-sizing is a capacity hint that cannot change visit order.
pub(crate) fn ground_search<S: NeighborSource>(
    src: &S,
    entry: S::Node,
    ef: usize,
    m0: usize,
    presize: bool,
) -> Result<Vec<(i64, f64)>, String> {
    // Thin wrapper: map the raw nodes to (tid, walk-distance). The SBQ rerank path (M51) uses
    // `ground_search_nodes` directly so it can re-score the survivors. The candidates_seen count (M68) is
    // discarded here — only the production HNSW scan (hnsw_page.rs) reports it to the observability collector.
    Ok(ground_search_nodes(src, entry, ef, m0, presize)?
        .0
        .into_iter()
        .map(|(node, d)| (src.tid(&node), d))
        .collect())
}

/// Like [`ground_search`] but returns the raw loaded nodes (ascending by walk-distance) instead of `(tid, dist)`,
/// so a caller can re-score the survivors (M51: rerank the SBQ-Hamming top-`ef` by exact f32). Nodes carry their
/// storage handle (production `Cand` has `(blk,off)`), which the collapsed `(tid, dist)` form discards.
pub(crate) fn ground_search_nodes<S: NeighborSource>(
    src: &S,
    entry: S::Node,
    ef: usize,
    m0: usize,
    presize: bool,
) -> Result<(Vec<(S::Node, f64)>, usize), String> {
    let ef = ef.max(1); // ef=0 clamp (edge-case) — never an empty search
    let (mut visited, mut cands, mut result, mut scratch): (
        HashSet<u64>,
        BinaryHeap<Reverse<Ranked<S::Node>>>,
        BinaryHeap<Ranked<S::Node>>,
        Vec<S::Ref>,
    ) = if presize {
        let cap = ef.saturating_mul(m0.max(1)).max(1);
        (
            HashSet::with_capacity(cap.saturating_mul(2)),
            BinaryHeap::with_capacity(cap),
            BinaryHeap::with_capacity(ef + 1),
            Vec::with_capacity(m0.max(1)),
        )
    } else {
        (HashSet::new(), BinaryHeap::new(), BinaryHeap::new(), Vec::new())
    };

    let entry_d = src.dist(&entry);
    visited.insert(src.node_key(&entry));
    // M56: navigate FROM the entry always (it may be a tombstone that bridges live regions), but only emit it
    // when live. The result heap holds only emittable nodes, so the ef/worst bound counts LIVE results — the
    // search naturally widens until `ef` live nodes are found (the tombstone over-fetch is automatic).
    cands.push(Reverse(Ranked { d: entry_d, node: entry }));
    if src.emittable(&entry) {
        result.push(Ranked { d: entry_d, node: entry });
    }

    while let Some(Reverse(Ranked { d: cd, node: c })) = cands.pop() {
        let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
        if cd > worst && result.len() >= ef {
            break;
        }
        src.neighbors_into(&c, &mut scratch)?;
        for i in 0..scratch.len() {
            let r = scratch[i];
            if !visited.insert(src.ref_key(&r)) {
                continue; // dedup BEFORE load — keeps pages_read identical (recall-neutral)
            }
            let cand = src.load(&r)?;
            let nd = src.dist(&cand);
            let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
            if nd < worst || result.len() < ef {
                // Navigate through this node regardless of tombstone status (preserves connectivity)…
                cands.push(Reverse(Ranked { d: nd, node: cand }));
                // …but only a live node enters the result set (M56 tombstone filter).
                if src.emittable(&cand) {
                    result.push(Ranked { d: nd, node: cand });
                    if result.len() > ef {
                        result.pop();
                    }
                }
            }
        }
    }

    // M68: candidates_seen = the unique nodes navigated in the beam (the `visited` dedup set — the candidate
    // pool, NOT the result `ef`). Captured before `visited` drops; returned as a pure `usize` so `scan_core`
    // keeps its "no `crate::`" invariant (the criterion bench includes this file by #[path]).
    let candidates_seen = visited.len();
    let mut out: Vec<(S::Node, f64)> = result.into_iter().map(|r| (r.node, r.d)).collect();
    // Ascending by distance, tie-broken by tid for determinism (same order the collapsed form produced).
    out.sort_by(|a, b| {
        a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal).then(src.tid(&a.0).cmp(&src.tid(&b.0)))
    });
    Ok((out, candidates_seen))
}

/// M118: resume-from-discarded state for the iterative filtered scan. Holds the ground-layer beam frontier
/// (`cands` = pgvector 0.8.5's `so->discarded`) + `visited` (`so->v`) so the search can be RESUMED from the
/// retained frontier across `amgettuple` batches instead of re-searching the whole graph with a doubled `ef`
/// (the M52 cost measured at ~3× vs pgvector in `docs/benchmarks/m52-filtered-ann.md`). Each [`next_batch`]
/// runs ONE bounded-`ef` beam pass seeded from the retained frontier and returns its emittable top-`ef`
/// (ascending); the frontier + visited persist for the next call, mirroring pgvector `ResumeScanItems`.
///
/// The original one-shot [`ground_search_nodes`] is UNCHANGED (zero regression for `traverse` + the SBQ/AQ
/// rerank paths); this is an additive path used ONLY by the iterative scan. Re-emission of a frontier head
/// across batches is harmless: the scan dedups TIDs (`am/scan.rs` `state.emitted`). Generic only over the Node
/// `N` (the scratch buffer is local per-batch, so no `S` lifetime entangles the caller's `ScanState`).
///
/// [`next_batch`]: ResumableGround::next_batch
/// [`ground_search_nodes`]: ground_search_nodes
pub(crate) struct ResumableGround<N> {
    visited: HashSet<u64>,
    cands: BinaryHeap<Reverse<Ranked<N>>>,
    ef: usize,
}

impl<N: Copy> ResumableGround<N> {
    /// Seed the resumable search at the (upper-layer-descended) `entry`. Mirrors the head of
    /// [`ground_search_nodes`]: insert the entry into `visited` and push it onto the frontier. `presize`
    /// pre-sizes the two persistent structures identically to the one-shot search.
    pub(crate) fn init<S: NeighborSource<Node = N>>(
        src: &S,
        entry: S::Node,
        ef: usize,
        m0: usize,
        presize: bool,
    ) -> Self {
        let ef = ef.max(1);
        let (mut visited, mut cands): (HashSet<u64>, BinaryHeap<Reverse<Ranked<N>>>) = if presize {
            let cap = ef.saturating_mul(m0.max(1)).max(1);
            (HashSet::with_capacity(cap.saturating_mul(2)), BinaryHeap::with_capacity(cap))
        } else {
            (HashSet::new(), BinaryHeap::new())
        };
        let entry_d = src.dist(&entry);
        visited.insert(src.node_key(&entry));
        cands.push(Reverse(Ranked { d: entry_d, node: entry }));
        Self { visited, cands, ef }
    }

    /// The frontier is empty ⇒ the reachable graph is exhausted; no further resume is possible (EC-1).
    pub(crate) fn exhausted(&self) -> bool {
        self.cands.is_empty()
    }

    /// Unique nodes navigated so far (the `visited` dedup set) — observability parity with `candidates_seen`.
    pub(crate) fn candidates_seen(&self) -> usize {
        self.visited.len()
    }

    /// M118 (T2.2): approximate retained-state size in bytes (frontier heap + visited set) — the caller enforces
    /// the `theodb_hnsw.resume_max_mb` ceiling against this (fail-safe: stop resuming when the frontier grows too
    /// large at scale, rather than risk an OOM). A heap element is `Reverse<Ranked<N>>`; a visited entry is a `u64`.
    pub(crate) fn approx_bytes(&self) -> usize {
        self.cands.len().saturating_mul(std::mem::size_of::<Reverse<Ranked<N>>>())
            + self.visited.len().saturating_mul(std::mem::size_of::<u64>())
    }

    /// Run ONE bounded-`ef` beam pass seeded from the retained frontier; return the emittable results
    /// (ascending by distance, tid-tie-broken). The frontier (`cands`) + `visited` persist for the next call.
    /// On the `ef`-worst early-break the triggering frontier head is re-pushed (it IS the discarded set the
    /// next resume continues from). Returns `[]` when the frontier is already exhausted.
    pub(crate) fn next_batch<S: NeighborSource<Node = N>>(
        &mut self,
        src: &S,
    ) -> Result<Vec<(N, f64)>, String> {
        let ef = self.ef;
        let mut result: BinaryHeap<Ranked<N>> = BinaryHeap::with_capacity(ef + 1);
        let mut scratch: Vec<S::Ref> = Vec::new();
        while let Some(Reverse(Ranked { d: cd, node: c })) = self.cands.pop() {
            let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
            if cd > worst && result.len() >= ef {
                self.cands.push(Reverse(Ranked { d: cd, node: c })); // re-push: the discarded frontier head
                break;
            }
            // The popped node is a candidate result for THIS batch (it may have been emitted in a prior batch —
            // the scan dedups TIDs, so re-emission is harmless and keeps recall a superset of the re-search).
            if src.emittable(&c) {
                result.push(Ranked { d: cd, node: c });
                if result.len() > ef {
                    result.pop();
                }
            }
            src.neighbors_into(&c, &mut scratch)?;
            for i in 0..scratch.len() {
                let r = scratch[i];
                if !self.visited.insert(src.ref_key(&r)) {
                    continue; // dedup BEFORE load — recall-neutral page-read pattern
                }
                let cand = src.load(&r)?;
                let nd = src.dist(&cand);
                let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
                // Always retain the discovered candidate in the persistent frontier (pgvector 0.8.5's `discarded`
                // set) so a LATER resume can expand it — even when it is pruned from THIS batch's ef beam. Dropping
                // pruned candidates (the single-shot HNSW beam behavior) causes premature frontier exhaustion and
                // recall loss under a selective resume (measured 0.8 recall in the A/B before this fix). This is the
                // essence of resume-from-discarded: the frontier is the discarded set, never thrown away.
                self.cands.push(Reverse(Ranked { d: nd, node: cand }));
                if (nd < worst || result.len() < ef) && src.emittable(&cand) {
                    result.push(Ranked { d: nd, node: cand });
                    if result.len() > ef {
                        result.pop();
                    }
                }
            }
        }
        let mut out: Vec<(N, f64)> = result.into_iter().map(|r| (r.node, r.d)).collect();
        out.sort_by(|a, b| {
            a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal).then(src.tid(&a.0).cmp(&src.tid(&b.0)))
        });
        Ok(out)
    }
}

/// An in-memory [`NeighborSource`] over a built [`HnswIndex`] — used by the equivalence tests (the D3 guard:
/// `ground_search` over the REAL graph == brute exact kNN). Node ids are the graph's node indices; distances are
/// the metric score against the query. Gated to test/pg_test so the pure `ground_search` above can be
/// `#[path]`-included by the criterion bench WITHOUT pulling `crate::ann` / `pg_sys` (blueprint Q5 — the bench
/// links standalone; the module above this line has ZERO `crate::` references).
#[cfg(feature = "pg_test")]
pub(crate) struct MemNeighborSource<'a> {
    idx: &'a crate::ann::HnswIndex,
    query: &'a [f32],
    metric: crate::ann::Metric,
    n: u32,
}

/// A loaded in-memory node: its index + its cached distance to the query.
#[cfg(feature = "pg_test")]
#[derive(Clone, Copy, Debug)]
pub(crate) struct MemNode {
    idx: u32,
    d: f64,
}

#[cfg(feature = "pg_test")]
impl<'a> MemNeighborSource<'a> {
    pub(crate) fn new(idx: &'a crate::ann::HnswIndex, query: &'a [f32]) -> Self {
        let (metric, _m, _m0, _ef) = idx.params();
        Self { idx, query, metric, n: idx.node_count() as u32 }
    }
    fn score(&self, node: u32) -> f64 {
        self.metric.dist(self.query, self.idx.node_vector(node as usize))
    }
    /// The loaded entry node for the ground search (the graph's global entry point). The bench measures the
    /// ground-loop allocation from here — it does NOT replicate the upper-layer descent (not what M46 changed);
    /// starting from the global entry visits ef nodes, a fair/representative ground-loop workload.
    pub(crate) fn entry_node(&self) -> Result<MemNode, String> {
        let e = self.idx.entry().ok_or("scan_core: empty index has no entry point")? as u32;
        Ok(MemNode { idx: e, d: self.score(e) })
    }
}

#[cfg(feature = "pg_test")]
impl<'a> NeighborSource for MemNeighborSource<'a> {
    type Node = MemNode;
    type Ref = u32;

    fn dist(&self, node: &MemNode) -> f64 {
        node.d
    }
    fn tid(&self, node: &MemNode) -> i64 {
        self.idx.node_id(node.idx as usize)
    }
    fn node_key(&self, node: &MemNode) -> u64 {
        node.idx as u64
    }
    fn ref_key(&self, r: &u32) -> u64 {
        *r as u64
    }
    fn neighbors_into(&self, node: &MemNode, out: &mut Vec<u32>) -> Result<(), String> {
        out.clear();
        for &nb in self.idx.node_neighbors(node.idx as usize, 0) {
            out.push(nb as u32);
        }
        Ok(())
    }
    fn load(&self, r: &u32) -> Result<MemNode, String> {
        if *r >= self.n {
            // negative case (EC-3): a ref outside the graph is a typed error, never an OOB panic
            return Err(format!("scan_core: node id out of range ({r} >= {})", self.n));
        }
        Ok(MemNode { idx: *r, d: self.score(*r) })
    }
}

#[cfg(feature = "pg_test")]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use crate::ann::{HnswIndex, Metric};

    /// Seeded deterministic corpus (mirror of the `ann/mod.rs` test helper) — reused via the crate-internal RNG.
    fn seeded_corpus(n: usize, dim: usize, seed: u64) -> Vec<(i64, Vec<f32>)> {
        let mut r = crate::ann::Rng::new(seed);
        (0..n)
            .map(|i| {
                let v: Vec<f32> = (0..dim).map(|_| (r.next_f64() as f32) * 10.0).collect();
                (i as i64 + 1, v)
            })
            .collect()
    }

    /// Brute-force exact kNN (the independent oracle) — the same math as `ann/mod.rs::brute`.
    fn brute(corpus: &[(i64, Vec<f32>)], q: &[f32], k: usize, metric: Metric) -> Vec<i64> {
        let mut d: Vec<(f64, i64)> =
            corpus.iter().map(|(id, v)| (metric.dist(q, v), *id)).collect();
        d.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal).then(a.1.cmp(&b.1)));
        d.into_iter().take(k).map(|(_, id)| id).collect()
    }

    /// D3 guard: `ground_search` over the in-memory seam returns EXACTLY the brute-force exact-kNN order on a
    /// small distinct corpus at high ef (100% recall) — proves the pure traversal is correct (not a bogus graph).
    #[pgrx::pg_test]
    fn ground_search_matches_brute_exact_knn() {
        let corpus = seeded_corpus(2000, 16, 42);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 42);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[7].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let got = ground_search(&src, src.entry_node().unwrap(), 200, m0, true).unwrap();
        let got_ids: Vec<i64> = got.iter().take(10).map(|(tid, _)| *tid).collect();
        let want = brute(&corpus, &q, 10, Metric::L2);
        let (mut a, mut b) = (got_ids.clone(), want.clone());
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "ground_search top-10 must equal brute exact-kNN set at ef=200 (100% recall)");
    }

    /// M118 recall-neutral invariant: the union of resumed `next_batch` results (batch ef=50, accumulated) is a
    /// SUPERSET of the single-shot ef=200 top-10. Resume-from-discarded finds at least as good as the re-search
    /// it replaces (the frontier is retained, never discarded).
    #[pgrx::pg_test]
    fn resumed_batches_union_superset_of_single_ef() {
        let corpus = seeded_corpus(2000, 16, 42);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 42);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[7].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let single = ground_search(&src, src.entry_node().unwrap(), 200, m0, true).unwrap();
        let single_ids: HashSet<i64> = single.iter().take(10).map(|(t, _)| *t).collect();

        let mut rg = ResumableGround::init(&src, src.entry_node().unwrap(), 50, m0, true);
        let mut union: HashSet<i64> = HashSet::new();
        let mut passes = 0;
        while !rg.exhausted() && passes < 40 {
            for (node, _d) in rg.next_batch(&src).unwrap() {
                union.insert(src.tid(&node));
            }
            passes += 1;
            if union.len() >= 200 {
                break;
            }
        }
        assert!(
            single_ids.is_subset(&union),
            "resumed union (n={}) must contain the single-ef top-10 (missing: {:?})",
            union.len(),
            single_ids.difference(&union).collect::<Vec<_>>()
        );
    }

    /// EC-1: a small graph exhausts the frontier in finite passes; once exhausted, `next_batch` yields `[]`
    /// (no spin, no re-search fallback — the highly-selective case M118 targets).
    #[pgrx::pg_test]
    fn resume_exhausts_when_frontier_empty() {
        let corpus = seeded_corpus(200, 16, 3);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 3);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let mut rg = ResumableGround::init(&src, src.entry_node().unwrap(), 20, m0, true);
        let mut passes = 0;
        while !rg.exhausted() {
            let _ = rg.next_batch(&src).unwrap();
            passes += 1;
            assert!(passes < 200, "resume must terminate on a finite graph");
        }
        assert!(rg.exhausted(), "frontier must empty on a small reachable graph");
        assert!(rg.next_batch(&src).unwrap().is_empty(), "exhausted frontier yields an empty batch");
    }

    /// EC-3: a single-node index with ef=1 returns the node once, then exhausts (smallest-graph boundary).
    #[pgrx::pg_test]
    fn resume_single_node_index_ef1() {
        let corpus = seeded_corpus(1, 16, 5);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 5);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let mut rg = ResumableGround::init(&src, src.entry_node().unwrap(), 1, m0, true);
        let first = rg.next_batch(&src).unwrap();
        assert_eq!(first.len(), 1, "single-node index returns the node once");
        assert!(rg.exhausted(), "single node exhausts after one batch");
        assert!(rg.next_batch(&src).unwrap().is_empty(), "second batch is empty");
    }

    /// The bench's SOLE axis is result-neutral: presize=true and presize=false return byte-identical output.
    #[pgrx::pg_test]
    fn ground_search_presize_is_result_neutral() {
        let corpus = seeded_corpus(3000, 16, 7);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 7);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[13].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        for ef in [50usize, 200, 400] {
            let presized = ground_search(&src, src.entry_node().unwrap(), ef, m0, true).unwrap();
            let plain = ground_search(&src, src.entry_node().unwrap(), ef, m0, false).unwrap();
            assert_eq!(presized, plain, "presize must not change the result at ef={ef}");
        }
    }

    /// Edge (EC-2): ef larger than the graph returns ≤ node_count results, no panic, no ef padding.
    #[pgrx::pg_test]
    fn ground_search_ef_exceeds_node_count_returns_all() {
        let corpus = seeded_corpus(5, 4, 3);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 3);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let got = ground_search(&src, src.entry_node().unwrap(), 200, m0, true).unwrap();
        assert!(got.len() <= 5, "ef>node_count must return <= node_count, got {}", got.len());
        assert!(!got.is_empty(), "a non-empty graph must return at least the entry");
    }

    /// Edge: ef=0 is clamped to 1 (no empty/degenerate search, no panic).
    #[pgrx::pg_test]
    fn ground_search_ef_zero_clamped() {
        let corpus = seeded_corpus(100, 8, 1);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 1);
        let (_metric, _m, m0, _ef) = idx.params();
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let got = ground_search(&src, src.entry_node().unwrap(), 0, m0, true).unwrap();
        assert!(got.len() <= 1, "ef=0 clamps to 1 → <= 1 result, got {}", got.len());
    }

    /// Negative (EC-3): a ref outside the graph is a typed error, not an index-out-of-bounds panic.
    #[pgrx::pg_test]
    fn mem_neighbor_source_out_of_range_node_is_typed_err() {
        let corpus = seeded_corpus(10, 4, 5);
        let idx = HnswIndex::build(&corpus, 16, 64, Metric::L2, 5);
        let q = corpus[0].1.clone();
        let src = MemNeighborSource::new(&idx, &q);
        let err = src.load(&u32::MAX).unwrap_err();
        assert!(err.contains("out of range"), "out-of-range ref must be a typed error, got: {err}");
    }
}
