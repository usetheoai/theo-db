//! M44 — parallel HNSW graph build. Constructs the same layered proximity graph as the sequential `hnsw::build`
//! but inserts nodes concurrently across CPU cores with `std::thread::scope` (borrows the read-only corpus without
//! `Arc`; a worker panic re-raises on join → fail-loud) + a per-node `RwLock` on the neighbor lists (readers/search
//! share, writers/link are exclusive). The persisted AM build is L2-only, pure Rust (no PG calls in this loop), so
//! workers never touch Postgres. The build is NON-DETERMINISTIC (racy insert order → a different but recall-
//! equivalent graph each run; level assignment stays deterministic — ADR D2). Mirrors hnswlib's parallel build.
//!
//! Deadlock-free by construction: each `insert_node` holds AT MOST ONE node lock at a time (search read-locks each
//! neighbor slice briefly to clone it, then works lock-free; linking write-locks one neighbor at a time, releasing
//! before the next). No nested cross-node locking → no lock cycle.
use super::{Cand, Metric};
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::RwLock;

/// Corpora with at least this many nodes build in parallel; smaller ones build sequentially (deterministic,
/// thread-overhead-free — the tiny AM test corpora stay on the unchanged sequential path).
pub(super) const PARALLEL_BUILD_THRESHOLD: usize = 4096;

/// Build the HNSW graph concurrently over `vectors` (all equal-dim) with pre-assigned `levels` (deterministic).
/// Node 0 is the seed entry. Returns `(neighbors, entry, max_level)` in the same shape the sequential build yields.
/// Every this many nodes, the leader joins all workers and calls `check_interrupt` (M48 T4.1). Batching is what
/// makes cancellation possible AND safe: a `pg_sys::check_for_interrupts!` in the injected closure can
/// `ereport(ERROR)` → longjmp, which MUST NOT cross a live worker thread (UB) — so it runs only between batches,
/// when `thread::scope` has joined every worker. 4096 bounds the cancel latency to one batch without adding
/// meaningful join overhead over a multi-thousand-node build.
const CANCEL_CHECK_BATCH: usize = 4096;

pub(super) fn build_parallel(
    vectors: &[Vec<f32>],
    levels: &[usize],
    m: usize,
    m0: usize,
    ef_construction: usize,
    metric: Metric,
    check_interrupt: &(dyn Fn() + Sync),
) -> (Vec<Vec<Vec<usize>>>, usize, usize) {
    let n = vectors.len();
    // One lock per node, guarding its per-layer neighbor lists (init empty, sized to the node's level).
    let neighbors: Vec<RwLock<Vec<Vec<usize>>>> =
        (0..n).map(|i| RwLock::new(vec![Vec::new(); levels[i] + 1])).collect();
    // Shared (entry, max_level). Node 0 is the seed; it is placed before the parallel phase (empty neighbors).
    let state = RwLock::new((0usize, levels[0]));

    let nthreads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(1)
        .min(n.max(1));

    // Batched worker pool: each batch `[start, end)` gets a fresh counter so a worker's final over-fetch never
    // leaks into the next batch. `scope` joins all workers (and re-raises any panic) before the batch returns —
    // the ONLY point where `check_interrupt` (leader-only) is longjmp-safe.
    let mut start = 1usize; // node 0 already placed
    while start < n {
        let end = (start + CANCEL_CHECK_BATCH).min(n);
        let counter = AtomicUsize::new(start);
        std::thread::scope(|s| {
            for _ in 0..nthreads {
                s.spawn(|| loop {
                    let node = counter.fetch_add(1, Ordering::Relaxed);
                    if node >= end {
                        break;
                    }
                    insert_node(node, vectors, levels, &neighbors, &state, m, m0, ef_construction, metric);
                });
            }
        });
        // All workers joined — safe to run the injected cancellation check (it may ereport(ERROR)→longjmp).
        check_interrupt();
        start = end;
    }

    let neighbors_plain: Vec<Vec<Vec<usize>>> =
        neighbors.into_iter().map(|rw| rw.into_inner().unwrap_or_else(|e| e.into_inner())).collect();
    let (entry, max_level) = state.into_inner().unwrap_or_else(|e| e.into_inner());
    (neighbors_plain, entry, max_level)
}

/// Insert `node` into the shared graph: greedy-descend the upper layers, then link on each layer ≤ its level.
fn insert_node(
    node: usize,
    vectors: &[Vec<f32>],
    levels: &[usize],
    neighbors: &[RwLock<Vec<Vec<usize>>>],
    state: &RwLock<(usize, usize)>,
    m: usize,
    m0: usize,
    ef_construction: usize,
    metric: Metric,
) {
    let level = levels[node];
    let q = &vectors[node];
    // Snapshot the entry + max_level (short read lock). A slightly-stale entry still finds neighbors (connected
    // graph) → recall-equivalent (ADR D2).
    let (mut ep, max_level) = *state.read().unwrap();

    let mut lc = max_level;
    while lc > level {
        ep = greedy_descend(q, ep, lc, vectors, neighbors, metric);
        lc -= 1; // safe: `lc > level >= 0` ⇒ `lc >= 1` before the decrement (mirrors the sequential build)
    }

    let mut lc = level.min(max_level) as isize;
    while lc >= 0 {
        let layer = lc as usize;
        let candidates = search_layer(q, &[ep], ef_construction, layer, vectors, neighbors, metric);
        let m_layer = if layer == 0 { m0 } else { m };
        let selected = select_from(vectors, metric, candidates, m_layer);

        // This node's own neighbors on this layer. Honest note (M44 review, LOW): the assignment OVERWRITES the
        // list, so an in-flight back-link another thread pushed to `node[layer]` (having reached `node` via a
        // higher layer) can be clobbered — a LOGICAL lost-update (not a data race: both under this write lock).
        // It only slightly reduces in-edges → a recall effect, well inside the accepted racy-build envelope (ADR
        // D2) and empirically covered by the recall-parity gate (Δ +0.0055 @50k). If recall ever regresses under
        // high contention, the minimal fix is to MERGE (extend+prune) instead of overwrite — YAGNI until measured.
        {
            let mut nn = neighbors[node].write().unwrap();
            if layer < nn.len() {
                nn[layer] = selected.clone();
            }
        }
        // Back-link: add `node` to each selected neighbor, pruning if it overflows. One node lock at a time.
        for &nb in &selected {
            let mut nbn = neighbors[nb].write().unwrap();
            if layer >= nbn.len() {
                continue;
            }
            nbn[layer].push(node);
            if nbn[layer].len() > m_layer {
                // Prune WHILE holding nb's write lock (select_from reads only `vectors`, never a neighbor lock →
                // no deadlock; avoids a lost-update that a drop-then-relock would allow).
                let cand: Vec<Cand> = nbn[layer]
                    .iter()
                    .map(|&x| Cand { d: metric.dist_simd(&vectors[nb], &vectors[x]), i: x })
                    .collect();
                nbn[layer] = select_from(vectors, metric, cand, m_layer);
            }
        }
        if let Some(&best) = selected.first() {
            ep = best;
        }
        lc -= 1;
    }

    // Promote the entry if this node reaches a new max level.
    if level > max_level {
        let mut st = state.write().unwrap();
        if level > st.1 {
            *st = (node, level);
        }
    }
}

/// Greedy-descend `layer` from `ep`, read-locking each visited node's neighbor slice briefly to clone it.
fn greedy_descend(
    q: &[f32],
    mut ep: usize,
    layer: usize,
    vectors: &[Vec<f32>],
    neighbors: &[RwLock<Vec<Vec<usize>>>],
    metric: Metric,
) -> usize {
    let mut best_d = metric.dist_simd(q, &vectors[ep]);
    loop {
        let nbs: Vec<usize> = {
            let g = neighbors[ep].read().unwrap();
            let li = layer.min(g.len().saturating_sub(1));
            g[li].clone()
        };
        let mut improved = false;
        for nb in nbs {
            let d = metric.dist_simd(q, &vectors[nb]);
            if d < best_d {
                best_d = d;
                ep = nb;
                improved = true;
            }
        }
        if !improved {
            return ep;
        }
    }
}

/// Search `layer` from `entries`, keeping the `ef` nearest; returns them sorted (nearest first). Concurrent
/// variant: each candidate's neighbor slice is cloned under a brief read lock, then scored lock-free.
fn search_layer(
    q: &[f32],
    entries: &[usize],
    ef: usize,
    layer: usize,
    vectors: &[Vec<f32>],
    neighbors: &[RwLock<Vec<Vec<usize>>>],
    metric: Metric,
) -> Vec<Cand> {
    let n = vectors.len();
    let mut visited = vec![false; n];
    let mut cand: BinaryHeap<std::cmp::Reverse<Cand>> = BinaryHeap::new();
    let mut result: BinaryHeap<Cand> = BinaryHeap::new();
    for &e in entries {
        if e >= n {
            continue;
        }
        let d = metric.dist_simd(q, &vectors[e]);
        visited[e] = true;
        cand.push(std::cmp::Reverse(Cand { d, i: e }));
        result.push(Cand { d, i: e });
    }
    while let Some(std::cmp::Reverse(c)) = cand.pop() {
        let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
        if c.d > worst && result.len() >= ef {
            break;
        }
        let nbs: Vec<usize> = {
            let g = neighbors[c.i].read().unwrap();
            if layer >= g.len() {
                continue;
            }
            g[layer].clone()
        };
        for nb in nbs {
            if visited[nb] {
                continue;
            }
            visited[nb] = true;
            let d = metric.dist_simd(q, &vectors[nb]);
            let worst = result.peek().map(|w| w.d).unwrap_or(f64::INFINITY);
            if d < worst || result.len() < ef {
                cand.push(std::cmp::Reverse(Cand { d, i: nb }));
                result.push(Cand { d, i: nb });
                if result.len() > ef {
                    result.pop();
                }
            }
        }
    }
    result.into_sorted_vec()
}

/// pgvector "closer" neighbor heuristic (free-fn form; reads only `vectors`, never a neighbor lock → safe to call
/// while holding a node's write lock). Identical logic to the sequential `HnswIndex::select_from`.
fn select_from(vectors: &[Vec<f32>], metric: Metric, mut candidates: Vec<Cand>, m: usize) -> Vec<usize> {
    candidates.sort();
    let mut kept: Vec<usize> = Vec::new();
    for c in &candidates {
        if kept.len() >= m {
            break;
        }
        let cv = &vectors[c.i];
        let closer_to_kept = kept.iter().any(|&k| metric.dist_simd(cv, &vectors[k]) < c.d);
        if !closer_to_kept {
            kept.push(c.i);
        }
    }
    if kept.len() < m {
        for c in &candidates {
            if kept.len() >= m {
                break;
            }
            if !kept.contains(&c.i) {
                kept.push(c.i);
            }
        }
    }
    kept
}
