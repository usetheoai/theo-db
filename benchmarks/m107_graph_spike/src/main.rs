//! M107 Phase-0 spike — native CSR adjacency + frontier BFS, measured against the recursive-CTE
//! baseline (theo-rag `graph-retriever.ts`). Zero deps: deterministic LCG RNG, own CSR + BFS.
//!
//! Semantics match the baseline: undirected weighted graph; from K seed nodes, expand ≤H hops;
//! the answer is the REACHABLE SET (nodes within H hops). The native BFS tracks a visited bitset
//! (no revisits); the SQL baseline uses `WITH RECURSIVE ... UNION ALL ... WHERE hop < H` +
//! `count(DISTINCT)` (revisits — the inefficiency we are measuring). Both must return the SAME set
//! (the correctness oracle: reached_count + reached_checksum must match).
//!
//! Usage: m107_graph_spike <edges> <nodes> <seed> <hops> <num_seeds> [csv_out]
//! Prints one JSON line: {edges,nodes,hops,build_ms,traverse_ms,reached_count,reached_checksum,seeds_csv}

use std::io::Write;
use std::time::Instant;

/// Tiny deterministic LCG (Numerical Recipes constants) — no `rand` dependency, fully reproducible.
struct Lcg(u64);
impl Lcg {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n.max(1)
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 6 {
        eprintln!("usage: m107_graph_spike <edges> <nodes> <seed> <hops> <num_seeds> [csv_out]");
        std::process::exit(2);
    }
    let n_edges: usize = a[1].parse().unwrap();
    let n_nodes: u64 = a[2].parse().unwrap();
    let seed: u64 = a[3].parse().unwrap();
    let hops: usize = a[4].parse().unwrap();
    let n_seeds: usize = a[5].parse().unwrap();
    let csv_out = a.get(6).cloned();

    // ── Generate a deterministic, hub-y undirected graph (entity-graph-like) ─────────────────────
    // Endpoints are mostly uniform, but ~25% of destinations land on a small "hub" set (id < nodes/100)
    // so the BFS frontier actually grows across hops — the regime where recursive-CTE revisits explode.
    let hub_ceil = (n_nodes / 100).max(1);
    let mut rng = Lcg(seed.wrapping_mul(2862933555777941757).wrapping_add(3037000493));
    let mut src = Vec::with_capacity(n_edges);
    let mut dst = Vec::with_capacity(n_edges);
    let mut wgt = Vec::with_capacity(n_edges);
    for _ in 0..n_edges {
        let u = rng.below(n_nodes);
        let v = if rng.below(100) < 25 { rng.below(hub_ceil) } else { rng.below(n_nodes) };
        if u == v { continue; }
        src.push(u);
        dst.push(v);
        wgt.push(1u32 + (rng.below(9) as u32)); // weight 1..=9
    }
    let m = src.len();

    // Optional: emit the edge list as CSV for the Postgres COPY (src,dst,weight).
    if let Some(path) = &csv_out {
        let f = std::fs::File::create(path).unwrap();
        let mut w = std::io::BufWriter::new(f);
        for i in 0..m {
            writeln!(w, "{},{},{}", src[i], dst[i], wgt[i]).unwrap();
        }
        w.flush().unwrap();
    }

    // ── Build CSR adjacency (undirected: each edge contributes to both endpoints) ────────────────
    let t_build = Instant::now();
    let nn = n_nodes as usize;
    let mut deg = vec![0u32; nn + 1];
    for i in 0..m {
        deg[src[i] as usize] += 1;
        deg[dst[i] as usize] += 1;
    }
    // prefix sum -> offsets
    let mut off = vec![0u64; nn + 1];
    let mut acc = 0u64;
    for i in 0..nn {
        off[i] = acc;
        acc += deg[i] as u64;
    }
    off[nn] = acc;
    let mut cur = off.clone();
    let mut adj = vec![0u32; acc as usize]; // edge array: destination node ids
    for i in 0..m {
        let (u, v) = (src[i] as usize, dst[i] as usize);
        adj[cur[u] as usize] = v as u32; cur[u] += 1;
        adj[cur[v] as usize] = u as u32; cur[v] += 1;
    }
    let build_ms = t_build.elapsed().as_secs_f64() * 1000.0;

    // ── Pick K deterministic seed nodes (same set the SQL baseline will use) ─────────────────────
    let mut seeds: Vec<u64> = Vec::with_capacity(n_seeds);
    let mut sr = Lcg(seed ^ 0xD1B54A32D192ED03);
    for _ in 0..n_seeds { seeds.push(sr.below(n_nodes)); }
    seeds.sort_unstable(); seeds.dedup();

    // ── Frontier BFS ≤H hops, visited bitset (no revisits) ───────────────────────────────────────
    let t_tr = Instant::now();
    let mut visited = vec![0u64; nn / 64 + 1];
    let set = |bs: &mut [u64], i: usize| bs[i >> 6] |= 1u64 << (i & 63);
    let is = |bs: &[u64], i: usize| (bs[i >> 6] >> (i & 63)) & 1 == 1;
    let mut frontier: Vec<u32> = Vec::new();
    for &s in &seeds {
        let si = s as usize;
        if !is(&visited, si) { set(&mut visited, si); frontier.push(s as u32); }
    }
    for _ in 0..hops {
        let mut next: Vec<u32> = Vec::new();
        for &node in &frontier {
            let (a0, a1) = (off[node as usize] as usize, off[node as usize + 1] as usize);
            for &nbr in &adj[a0..a1] {
                let ni = nbr as usize;
                if !is(&visited, ni) { set(&mut visited, ni); next.push(nbr); }
            }
        }
        if next.is_empty() { break; }
        frontier = next;
    }
    let traverse_ms = t_tr.elapsed().as_secs_f64() * 1000.0;

    // Reachable-set oracle: count + a checksum (sum of reached ids, wrapping) so we can diff against SQL.
    let mut reached_count: u64 = 0;
    let mut checksum: u64 = 0;
    for i in 0..nn {
        if is(&visited, i) { reached_count += 1; checksum = checksum.wrapping_add(i as u64); }
    }

    let seeds_csv = seeds.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(" ");
    println!(
        "{{\"edges\":{},\"nodes\":{},\"hops\":{},\"build_ms\":{:.3},\"traverse_ms\":{:.3},\"reached_count\":{},\"reached_checksum\":{},\"seeds_csv\":\"{}\"}}",
        m, n_nodes, hops, build_ms, traverse_ms, reached_count, checksum, seeds_csv
    );
}
