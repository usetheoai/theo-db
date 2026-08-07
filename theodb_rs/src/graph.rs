//! M108 — persisted-CSR graph structure (Phase 1 of the native graph pillar, ADR-0048).
//!
//! The M107 spike proved native CSR+BFS beats recursive-CTE 106–738× on traversal, but on-the-fly CSR build
//! dominates at 1M (build 31 ms vs traverse 1.4 ms → end-to-end collapses to ~7×). M108 persists the CSR ONCE
//! (`theodb.graph_build`) so per-query cost is load+traverse, not rebuild.
//!
//! Durability (Rule 9 / KISS): the compact CSR is serialized to a `bytea` in the `theodb.graph_csr` catalog
//! table — PostgreSQL makes that **WAL-logged + crash-safe + MVCC** natively, so we do NOT hand-roll a WAL'd
//! index-AM (the ADR-1 "estrutura CSR sobre a edge-table" branch). Incremental maintenance = `graph_refold`
//! rebuilds the bytea (fold-on-demand). The traversal (`theodb.graph_expand`) loads the persisted CSR and runs
//! the frontier BFS — the same reachable-set semantics as the theo-rag `graph-retriever.ts` (undirected, ≤H hops).
use pgrx::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// M108 — per-backend deserialized-CSR cache (the M101 Arrow-cache pattern). Persisting the CSR as a bytea
// removes the per-query SCAN+SORT *rebuild*, but a naive `graph_expand` still LOADs+DESERIALIZEs the whole CSR
// per query — which then dominates. This thread_local caches the deserialized `Csr` per edge-relation, keyed by
// the persisted `built_at` epoch so a `graph_refold` transparently invalidates it. Warm queries pay only the
// traverse (the M107 traverse-only win); the first query per backend pays the one-time deserialize.
thread_local! {
    static CSR_CACHE: RefCell<HashMap<pg_sys::Oid, (i64, Rc<Csr>)>> = RefCell::new(HashMap::new());
}

// Catalog: one persisted CSR per edge-relation. `csr` is the serialized adjacency; `nnodes`/`nedges` are stats.
extension_sql!(
    r#"
CREATE TABLE IF NOT EXISTS theodb.graph_csr (
    edge_rel   oid PRIMARY KEY,
    src_col    text NOT NULL,
    dst_col    text NOT NULL,
    nnodes     bigint NOT NULL,
    nedges     bigint NOT NULL,
    csr        bytea NOT NULL,
    built_at   timestamptz NOT NULL DEFAULT now()
);
"#,
    name = "theodb_graph_schema",
    requires = ["theodb_schema_bootstrap"],
);

/// Serialized CSR layout (little-endian): [u64 nnodes][u64 total_adj][ (nnodes+1) × u64 offsets ][ total_adj × u32 adj ].
/// Undirected: each edge (u,v) contributes v to u's list and u to v's list, so `total_adj == 2 × nedges_kept`.
struct Csr {
    nnodes: u64,
    offsets: Vec<u64>, // len nnodes+1
    adj: Vec<u32>,     // len total_adj
}

impl Csr {
    fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(16 + self.offsets.len() * 8 + self.adj.len() * 4);
        b.extend_from_slice(&self.nnodes.to_le_bytes());
        b.extend_from_slice(&(self.adj.len() as u64).to_le_bytes());
        for o in &self.offsets {
            b.extend_from_slice(&o.to_le_bytes());
        }
        for a in &self.adj {
            b.extend_from_slice(&a.to_le_bytes());
        }
        b
    }

    fn from_bytes(b: &[u8]) -> Result<Csr, String> {
        if b.len() < 16 {
            return Err("theodb graph: truncated CSR header".into());
        }
        let nnodes = u64::from_le_bytes(b[0..8].try_into().unwrap());
        let total_adj = u64::from_le_bytes(b[8..16].try_into().unwrap());
        let noff = (nnodes as usize) + 1;
        let off_end = 16 + noff * 8;
        let adj_end = off_end + (total_adj as usize) * 4;
        if b.len() != adj_end {
            return Err(format!(
                "theodb graph: CSR length mismatch (got {}, want {adj_end})",
                b.len()
            ));
        }
        let mut offsets = Vec::with_capacity(noff);
        for i in 0..noff {
            let p = 16 + i * 8;
            offsets.push(u64::from_le_bytes(b[p..p + 8].try_into().unwrap()));
        }
        let mut adj = Vec::with_capacity(total_adj as usize);
        for i in 0..(total_adj as usize) {
            let p = off_end + i * 4;
            adj.push(u32::from_le_bytes(b[p..p + 4].try_into().unwrap()));
        }
        Ok(Csr { nnodes, offsets, adj })
    }

    /// Frontier BFS from `seeds`, ≤`max_hops`, visited-bitset (no revisits). Returns the reachable node set.
    fn expand(&self, seeds: &[i64], max_hops: i32) -> Vec<i64> {
        let nn = self.nnodes as usize;
        let mut visited = vec![0u64; nn / 64 + 1];
        let set = |bs: &mut [u64], i: usize| bs[i >> 6] |= 1u64 << (i & 63);
        let is = |bs: &[u64], i: usize| (bs[i >> 6] >> (i & 63)) & 1 == 1;
        let mut frontier: Vec<u32> = Vec::new();
        for &s in seeds {
            if s < 0 || (s as u64) >= self.nnodes {
                continue;
            }
            let si = s as usize;
            if !is(&visited, si) {
                set(&mut visited, si);
                frontier.push(s as u32);
            }
        }
        let hops = max_hops.max(0);
        for _ in 0..hops {
            let mut next: Vec<u32> = Vec::new();
            for &node in &frontier {
                let (a0, a1) = (
                    self.offsets[node as usize] as usize,
                    self.offsets[node as usize + 1] as usize,
                );
                for &nbr in &self.adj[a0..a1] {
                    let ni = nbr as usize;
                    if !is(&visited, ni) {
                        set(&mut visited, ni);
                        next.push(nbr);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        (0..nn).filter(|&i| is(&visited, i)).map(|i| i as i64).collect()
    }

    /// M109 — Multi-Source BFS. `seed_sets[l]` is lane `l`'s source set; returns lane `l`'s reachable node
    /// set (undirected, ≤`max_hops`) at `result[l]`. Top-down Aggregated-Neighbor-Processing (Then et al.,
    /// VLDB'14): per-vertex `u64` source-mask (bit `l` = "lane l reached this vertex"), so ONE sweep of the
    /// CSR advances up to 64 BFSs at once via bitwise-OR (auto-vectorized plain Rust — the source-parallel
    /// mechanism, NOT the candidate-parallel `pshufb` of `vec/ah.rs`; ADR-1). Lanes are tiled at 64; the same
    /// deserialized `Csr` is reused across tiles. INVARIANT (ADR-3): `expand_multi([[s]],h)[0]` set-equals
    /// `expand(&[s],h)` — MS-BFS is exactly N single-source BFSs sharing edge traversal.
    fn expand_multi(&self, seed_sets: &[Vec<i64>], max_hops: i32) -> Vec<Vec<i64>> {
        let nn = self.nnodes as usize;
        let hops = max_hops.max(0);
        let nsets = seed_sets.len();
        let mut result: Vec<Vec<i64>> = vec![Vec::new(); nsets];
        let mut base = 0;
        while base < nsets {
            let tile = (nsets - base).min(64);
            // FRONTIER-DRIVEN MS-BFS (ADR-2): iterate only ACTIVE vertices — NOT a full O(nnodes) per-hop sweep
            // (a shared hub reached by many lanes has its high-degree adjacency traversed ONCE with the OR'd
            // lane bits, instead of once per lane in N sequential BFSs). MEASURED traversal-only (confound-free,
            // `wiki/benchmarks/m109-msbfs`): pure_speedup 1.33× @N=1 → ~7× @N=64+, oracle PASS at every N. The
            // growth-with-N is exactly Then et al.'s (VLDB'14) edge-sharing mechanism. `visit[v]` is a u64
            // source-mask (bit l = lane `base+l`), auto-vectorized bitwise-OR — source-parallel, NOT the
            // candidate-parallel `pshufb` of `vec/ah.rs` (ADR-1).
            let mut visit = vec![0u64; nn]; // bit l set ⇒ lane l propagates from this vertex next hop
            let mut seen = vec![0u64; nn]; // accumulated reached-by-lane bits
            let mut frontier: Vec<u32> = Vec::new();
            for l in 0..tile {
                let bit = 1u64 << l;
                for &s in &seed_sets[base + l] {
                    if s < 0 || (s as u64) >= self.nnodes {
                        continue; // out-of-range seed skipped (same as `expand`)
                    }
                    let si = s as usize;
                    if seen[si] == 0 {
                        frontier.push(si as u32); // add each seed vertex to the frontier once
                    }
                    seen[si] |= bit;
                    visit[si] |= bit;
                }
            }
            for _ in 0..hops {
                if frontier.is_empty() {
                    break;
                }
                let mut visit_next = vec![0u64; nn];
                let mut next_frontier: Vec<u32> = Vec::new();
                for &v in &frontier {
                    let vv = visit[v as usize];
                    let (a0, a1) =
                        (self.offsets[v as usize] as usize, self.offsets[v as usize + 1] as usize);
                    for &nbr in &self.adj[a0..a1] {
                        let ni = nbr as usize;
                        let new_bits = vv & !seen[ni]; // lanes that reach nbr and haven't seen it yet
                        if new_bits != 0 {
                            if visit_next[ni] == 0 {
                                next_frontier.push(nbr); // add nbr to next frontier once per hop
                            }
                            visit_next[ni] |= new_bits;
                            seen[ni] |= new_bits;
                        }
                    }
                }
                visit = visit_next;
                frontier = next_frontier;
            }
            // Extract: distribute each vertex's set bits to the owning lane's result.
            for (v, &mask) in seen.iter().enumerate() {
                let mut m = mask;
                while m != 0 {
                    let l = m.trailing_zeros() as usize;
                    result[base + l].push(v as i64);
                    m &= m - 1;
                }
            }
            base += tile;
        }
        result
    }

    /// M112 — Personalized PageRank from `seeds` (HippoRAG's ranking mechanism). Power iteration over the
    /// undirected CSR: `π = (1-d)·s + d·(Wᵀ·π)` where `s` is the restart distribution (uniform over seeds), `W`
    /// is the row-stochastic random walk (from u, uniform over its `deg(u)` neighbors), and `d` is the damping.
    /// Returns the stationary PPR score per node. Unweighted (M108 CSR stores adjacency, not weights).
    fn ppr(&self, seeds: &[i64], damping: f64, iters: i32) -> Vec<f64> {
        let nn = self.nnodes as usize;
        if nn == 0 {
            return Vec::new();
        }
        let d = damping.clamp(0.0, 0.999);
        let deg: Vec<f64> =
            (0..nn).map(|u| (self.offsets[u + 1] - self.offsets[u]) as f64).collect();
        // restart distribution: uniform over the valid seeds (sums to 1); if no valid seed, uniform over all.
        let mut s = vec![0.0f64; nn];
        let valid: Vec<usize> = seeds
            .iter()
            .filter(|&&x| x >= 0 && (x as u64) < self.nnodes)
            .map(|&x| x as usize)
            .collect();
        if valid.is_empty() {
            let u = 1.0 / nn as f64;
            s.iter_mut().for_each(|x| *x = u);
        } else {
            let w = 1.0 / valid.len() as f64;
            for &si in &valid {
                s[si] += w;
            }
        }
        let mut pi = s.clone();
        for _ in 0..iters.max(1) {
            let mut next = vec![0.0f64; nn];
            // π_next[v] = (1-d)·s[v] + d·Σ_{u ∈ neighbors(v)} π[u]/deg(u)   (undirected: neighbors(v) = adj(v))
            for v in 0..nn {
                let (a0, a1) = (self.offsets[v] as usize, self.offsets[v + 1] as usize);
                let mut acc = 0.0;
                for &nbr in &self.adj[a0..a1] {
                    let u = nbr as usize;
                    if deg[u] > 0.0 {
                        acc += pi[u] / deg[u];
                    }
                }
                next[v] = (1.0 - d) * s[v] + d * acc;
            }
            pi = next;
        }
        pi
    }
}

/// Scan the edge relation and build the undirected CSR. `edge_rel` is a table name; `src_col`/`dst_col` are the
/// integer endpoint columns.
///
/// Injection-safe on BOTH axes (M146 T1.2): the columns are quoted with `format('%I')`, and the RELATION goes
/// through `($3)::regclass::text`. The relation axis used to be a raw `%s` splice, which was a real, MEASURED
/// second-order SQL injection: `graph_build('(SELECT src, dst FROM other WHERE 1/0 = 1) x', 'src','dst')`
/// raised `division by zero`, proving the caller's SQL executed inside our scan. The `($1)::regclass::oid` in
/// the persist step below does NOT protect this — it runs only AFTER the injected scan has already executed.
///
/// `regclass` (not `%I`) is the right tool here: it validates that the name parses AND resolves through
/// `search_path`, raising 42P01 BEFORE any SQL is assembled, and `regclassout` re-renders the identifier from
/// `pg_class` — so the text spliced into the query comes from the catalog, never from the caller. `%I` would be
/// wrong: it treats the whole input as ONE identifier, mangling `schema.table` into `"schema.table"`.
/// Note: `regclass` performs no privilege check; authorization is still enforced when the scan executes, which
/// is correct while this function is SECURITY INVOKER. Making it SECURITY DEFINER would require an explicit
/// `has_table_privilege` gate.
fn build_csr(edge_rel: &str, src_col: &str, dst_col: &str) -> Csr {
    let scan_sql = Spi::get_one_with_args::<String>(
        "SELECT format('SELECT (%I)::bigint AS s, (%I)::bigint AS d FROM %s WHERE %I IS NOT NULL AND %I IS NOT NULL', $1,$2,($3)::regclass::text,$1,$2)",
        &[src_col.into(), dst_col.into(), edge_rel.into()],
    )
    .ok()
    .flatten()
    .unwrap_or_else(|| crate::pg::err_input("theodb.graph_build: could not build scan query"));

    let mut src: Vec<i64> = Vec::new();
    let mut dst: Vec<i64> = Vec::new();
    let mut maxn: i64 = -1;
    Spi::connect(|client| {
        let t = client.select(&scan_sql, None, &[]).unwrap_or_else(|e| {
            crate::pg::err_input(&format!("theodb.graph_build: scan failed: {e:?}"))
        });
        for row in t {
            let s = row.get::<i64>(1).ok().flatten();
            let d = row.get::<i64>(2).ok().flatten();
            if let (Some(s), Some(d)) = (s, d) {
                if s < 0 || d < 0 {
                    crate::pg::err_input(
                        "theodb.graph_build: node ids must be non-negative bigints",
                    );
                }
                // M144 T2.4: the CSR adjacency stores node ids as u32 (`v as u32` below); a bigint
                // node id above u32::MAX would truncate silently and corrupt the graph. Fail-closed at
                // the trust boundary (fresh-read from SPI) instead of at the cast.
                if s > u32::MAX as i64 || d > u32::MAX as i64 {
                    crate::pg::err_input(
                        "theodb.graph_build: node ids must fit in u32 (max 4294967295)",
                    );
                }
                maxn = maxn.max(s).max(d);
                src.push(s);
                dst.push(d);
            }
        }
    });
    let nnodes = (maxn + 1).max(0) as u64;
    let nn = nnodes as usize;
    let m = src.len();
    // degree (undirected)
    let mut deg = vec![0u64; nn + 1];
    for i in 0..m {
        deg[src[i] as usize] += 1;
        deg[dst[i] as usize] += 1;
    }
    let mut offsets = vec![0u64; nn + 1];
    let mut acc = 0u64;
    for i in 0..nn {
        offsets[i] = acc;
        acc += deg[i];
    }
    offsets[nn] = acc;
    let mut cur = offsets.clone();
    let mut adj = vec![0u32; acc as usize];
    for i in 0..m {
        let (u, v) = (src[i] as usize, dst[i] as usize);
        adj[cur[u] as usize] = v as u32;
        cur[u] += 1;
        adj[cur[v] as usize] = u as u32;
        cur[v] += 1;
    }
    Csr { nnodes, offsets, adj }
}

// All `#[pg_extern]` entrypoints live in the `theodb_rs` schema (project convention — pgrx merges same-named
// `#[pg_schema]` modules across files, e.g. vectorizer.rs / api.rs). The `theodb.graph_*` public wrappers below
// call these `theodb_rs.graph_*` internals.
#[pgrx::pg_schema]
mod theodb_rs {
    use super::{Csr, build_csr};
    use pgrx::prelude::*;
    use std::rc::Rc;

    /// `graph_build(edge_rel text, src_col text, dst_col text) -> bigint` — build + PERSIST the CSR once
    /// (WAL-safe bytea in `theodb.graph_csr`). Returns the edge count. Re-run to refold after inserts.
    #[pg_extern]
    fn graph_build(edge_rel: &str, src_col: &str, dst_col: &str) -> i64 {
        let csr = build_csr(edge_rel, src_col, dst_col);
        let nedges = (csr.adj.len() / 2) as i64;
        let nnodes = csr.nnodes as i64;
        let bytes = csr.to_bytes();
        Spi::run_with_args(
            "INSERT INTO theodb.graph_csr (edge_rel, src_col, dst_col, nnodes, nedges, csr, built_at) \
             VALUES (($1)::regclass::oid, $2, $3, $4, $5, $6, clock_timestamp()) \
             ON CONFLICT (edge_rel) DO UPDATE SET src_col=EXCLUDED.src_col, dst_col=EXCLUDED.dst_col, \
               nnodes=EXCLUDED.nnodes, nedges=EXCLUDED.nedges, csr=EXCLUDED.csr, built_at=clock_timestamp()",
            &[edge_rel.into(), src_col.into(), dst_col.into(), nnodes.into(), nedges.into(), bytes.into()],
        )
        .unwrap_or_else(|e| crate::pg::err_input(&format!("theodb.graph_build: persist failed: {e:?}")));
        nedges
    }

    /// `graph_refold(edge_rel text) -> bigint` — rebuild the persisted CSR from the current edges (fold-on-demand).
    #[pg_extern]
    fn graph_refold(edge_rel: &str) -> i64 {
        let cols = Spi::get_two_with_args::<String, String>(
            "SELECT src_col, dst_col FROM theodb.graph_csr WHERE edge_rel = ($1)::regclass::oid",
            &[edge_rel.into()],
        )
        .ok();
        match cols {
            Some((Some(s), Some(d))) => graph_build(edge_rel, &s, &d),
            _ => crate::pg::err_input(
                "theodb.graph_refold: no persisted CSR for this edge relation (run graph_build first)",
            ),
        }
    }

    /// Resolve the PERSISTED CSR for `edge_rel` from the per-backend cache (M108). Cheap oid+built_at resolve;
    /// on a miss (or stale epoch) loads the bytea, deserializes, and caches an `Rc<Csr>`. Fails fast (typed) if
    /// no CSR was built. Shared by `graph_expand` (single-source) and `graph_expand_multi` (M109).
    fn load_cached_csr(edge_rel: &str) -> Rc<Csr> {
        let (oid, epoch): (pg_sys::Oid, i64) = Spi::get_two_with_args::<pg_sys::Oid, i64>(
            "SELECT ($1)::regclass::oid, (extract(epoch from built_at)*1e6)::bigint \
             FROM theodb.graph_csr WHERE edge_rel = ($1)::regclass::oid",
            &[edge_rel.into()],
        )
        .ok()
        .and_then(|(o, e)| Some((o?, e?)))
        .unwrap_or_else(|| {
            crate::pg::err_unsupported(
                "theodb graph: no persisted CSR for this edge relation — run theodb.graph_build(edge_rel, src_col, dst_col) first",
            )
        });
        super::CSR_CACHE.with(|c| {
            if let Some((ep, rc)) = c.borrow().get(&oid) {
                if *ep == epoch {
                    return rc.clone();
                }
            }
            let bytes = Spi::get_one_with_args::<Vec<u8>>(
                "SELECT csr FROM theodb.graph_csr WHERE edge_rel = ($1)::regclass::oid",
                &[edge_rel.into()],
            )
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                crate::pg::err_input("theodb graph: persisted CSR row vanished mid-query")
            });
            let rc = Rc::new(
                Csr::from_bytes(&bytes)
                    .unwrap_or_else(|e| crate::pg::err_input(&format!("theodb graph: {e}"))),
            );
            c.borrow_mut().insert(oid, (epoch, rc.clone()));
            rc
        })
    }

    /// `graph_expand(edge_rel text, seeds bigint[], max_hops int) -> SETOF bigint` — load the PERSISTED CSR (no
    /// rebuild) and return the reachable node set within `max_hops` from `seeds` (undirected frontier BFS).
    #[pg_extern]
    fn graph_expand(
        edge_rel: &str,
        seeds: Vec<Option<i64>>,
        max_hops: i32,
    ) -> SetOfIterator<'static, i64> {
        let csr = load_cached_csr(edge_rel);
        let seed_ids: Vec<i64> = seeds.into_iter().flatten().collect();
        SetOfIterator::new(csr.expand(&seed_ids, max_hops).into_iter())
    }

    /// `graph_expand_card(edge_rel text, seeds bigint[], max_hops int) -> bigint` — single-source reachable-set
    /// CARDINALITY (count computed in Rust, one row out). The count-in-Rust twin of `graph_expand`; used to
    /// isolate MS-BFS traversal speedup from SQL row-streaming in the crossover benchmark (both sides count in Rust).
    #[pg_extern]
    fn graph_expand_card(edge_rel: &str, seeds: Vec<Option<i64>>, max_hops: i32) -> i64 {
        let csr = load_cached_csr(edge_rel);
        let seed_ids: Vec<i64> = seeds.into_iter().flatten().collect();
        csr.expand(&seed_ids, max_hops).len() as i64
    }

    /// `graph_ppr(edge_rel text, seeds bigint[], damping float8, iters int) -> TABLE(node bigint, score float8)`
    /// — M112 Personalized PageRank from `seeds` over the persisted CSR (HippoRAG ranking). Returns only nodes
    /// with score > 0, highest first.
    #[pg_extern]
    fn graph_ppr(
        edge_rel: &str,
        seeds: Vec<Option<i64>>,
        damping: f64,
        iters: i32,
    ) -> TableIterator<'static, (name!(node, i64), name!(score, f64))> {
        let csr = load_cached_csr(edge_rel);
        let seed_ids: Vec<i64> = seeds.into_iter().flatten().collect();
        let pi = csr.ppr(&seed_ids, damping, iters);
        let mut rows: Vec<(i64, f64)> = pi
            .into_iter()
            .enumerate()
            .filter(|(_, s)| *s > 1e-12)
            .map(|(i, s)| (i as i64, s))
            .collect();
        rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
        TableIterator::new(rows.into_iter())
    }

    /// `graph_expand_multi(edge_rel text, set_ids int[], seeds bigint[], max_hops int) -> TABLE(set_id int,
    /// node bigint)` — M109 batched Multi-Source BFS. `set_ids`/`seeds` are PARALLEL arrays: `seeds[i]` belongs
    /// to lane `set_ids[i]` (ragged sets, built from `SELECT array_agg(set_id), array_agg(seed) ...`). Returns,
    /// per input `set_id`, its reachable node set — one MS-BFS sweep advances up to 64 lanes at once (ADR-3).
    #[pg_extern]
    fn graph_expand_multi(
        edge_rel: &str,
        set_ids: Vec<Option<i32>>,
        seeds: Vec<Option<i64>>,
        max_hops: i32,
    ) -> TableIterator<'static, (name!(set_id, i32), name!(node, i64))> {
        let (lanes, lane_set_id) = super::group_seed_lanes(set_ids, seeds);
        let csr = load_cached_csr(edge_rel);
        let reached = csr.expand_multi(&lanes, max_hops);
        // Flatten (lane → original set_id) × reachable nodes into rows (no intermediate Vec — lazy iterator).
        TableIterator::new(reached.into_iter().enumerate().flat_map(move |(lane, nodes)| {
            let sid = lane_set_id[lane];
            nodes.into_iter().map(move |n| (sid, n))
        }))
    }

    /// `graph_expand_multi_card(edge_rel text, set_ids int[], seeds bigint[], max_hops int) -> TABLE(set_id
    /// int, card bigint)` — M109 batched MS-BFS returning only each lane's reachable-set CARDINALITY (not the
    /// nodes). Cheap: N rows out (one per lane), so the SQL row-return does NOT confound a traversal benchmark;
    /// it is also a lightweight per-entity ≤H-hop reach-count signal. Same MS-BFS traversal as `graph_expand_multi`.
    #[pg_extern]
    fn graph_expand_multi_card(
        edge_rel: &str,
        set_ids: Vec<Option<i32>>,
        seeds: Vec<Option<i64>>,
        max_hops: i32,
    ) -> TableIterator<'static, (name!(set_id, i32), name!(card, i64))> {
        let (lanes, lane_set_id) = super::group_seed_lanes(set_ids, seeds);
        let csr = load_cached_csr(edge_rel);
        let reached = csr.expand_multi(&lanes, max_hops);
        let rows: Vec<(i32, i64)> = reached
            .into_iter()
            .enumerate()
            .map(|(lane, nodes)| (lane_set_id[lane], nodes.len() as i64))
            .collect();
        TableIterator::new(rows.into_iter())
    }
}

/// Group parallel `set_ids`/`seeds` arrays into contiguous lanes (lane `k` ↔ `lane_set_id[k]`), preserving
/// first-appearance order. Fails fast (typed) on unequal lengths; skips NULL element pairs. Shared by the
/// `graph_expand_multi{,_card}` entrypoints (DRY).
fn group_seed_lanes(
    set_ids: Vec<Option<i32>>,
    seeds: Vec<Option<i64>>,
) -> (Vec<Vec<i64>>, Vec<i32>) {
    if set_ids.len() != seeds.len() {
        crate::pg::err_input(
            "theodb graph_expand_multi: set_ids and seeds must be parallel arrays of equal length",
        );
    }
    let mut lane_of: HashMap<i32, usize> = HashMap::new();
    let mut lane_set_id: Vec<i32> = Vec::new();
    let mut lanes: Vec<Vec<i64>> = Vec::new();
    for (sid, seed) in set_ids.into_iter().zip(seeds.into_iter()) {
        let (sid, seed) = match (sid, seed) {
            (Some(a), Some(b)) => (a, b),
            _ => continue, // NULL element in either array → skip that pair
        };
        let lane = *lane_of.entry(sid).or_insert_with(|| {
            lane_set_id.push(sid);
            lanes.push(Vec::new());
            lanes.len() - 1
        });
        lanes[lane].push(seed);
    }
    (lanes, lane_set_id)
}

// REVOKE the graph functions from PUBLIC (least-privilege — they read arbitrary edge relations; the caller
// needs SELECT on the edge table anyway, but keep the convention).
extension_sql!(
    r#"
DO $$
DECLARE r record;
BEGIN
    FOR r IN SELECT p.oid::regprocedure AS sig FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
             WHERE n.nspname='theodb_rs' AND p.proname ~ '^graph_'
    LOOP EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC', r.sig); END LOOP;
END $$;
"#,
    name = "theodb_graph_revoke",
    requires = [
        graph_build,
        graph_refold,
        graph_expand,
        graph_expand_card,
        graph_expand_multi,
        graph_expand_multi_card,
        graph_ppr
    ],
);

// Public `theodb.graph_*` surface (the pg_extern lives in schema `theodb_rs`; expose curated wrappers in `theodb`).
extension_sql!(
    r#"
CREATE FUNCTION theodb.graph_build(edge_rel text, src_col text, dst_col text) RETURNS bigint
  LANGUAGE sql VOLATILE AS $fn$ SELECT theodb_rs.graph_build($1,$2,$3) $fn$;
CREATE FUNCTION theodb.graph_refold(edge_rel text) RETURNS bigint
  LANGUAGE sql VOLATILE AS $fn$ SELECT theodb_rs.graph_refold($1) $fn$;
CREATE FUNCTION theodb.graph_expand(edge_rel text, seeds bigint[], max_hops int) RETURNS SETOF bigint
  LANGUAGE sql VOLATILE AS $fn$ SELECT theodb_rs.graph_expand($1,$2,$3) $fn$;
CREATE FUNCTION theodb.graph_expand_card(edge_rel text, seeds bigint[], max_hops int) RETURNS bigint
  LANGUAGE sql VOLATILE AS $fn$ SELECT theodb_rs.graph_expand_card($1,$2,$3) $fn$;
CREATE FUNCTION theodb.graph_expand_multi(edge_rel text, set_ids int[], seeds bigint[], max_hops int)
  RETURNS TABLE(set_id int, node bigint)
  LANGUAGE sql VOLATILE AS $fn$ SELECT * FROM theodb_rs.graph_expand_multi($1,$2,$3,$4) $fn$;
CREATE FUNCTION theodb.graph_expand_multi_card(edge_rel text, set_ids int[], seeds bigint[], max_hops int)
  RETURNS TABLE(set_id int, card bigint)
  LANGUAGE sql VOLATILE AS $fn$ SELECT * FROM theodb_rs.graph_expand_multi_card($1,$2,$3,$4) $fn$;
REVOKE ALL ON FUNCTION theodb.graph_build(text,text,text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.graph_refold(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.graph_expand(text,bigint[],int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.graph_expand_card(text,bigint[],int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.graph_expand_multi(text,int[],bigint[],int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.graph_expand_multi_card(text,int[],bigint[],int) FROM PUBLIC;
CREATE FUNCTION theodb.graph_ppr(edge_rel text, seeds bigint[], damping float8 DEFAULT 0.5, iters int DEFAULT 20)
  RETURNS TABLE(node bigint, score float8)
  LANGUAGE sql VOLATILE AS $fn$ SELECT * FROM theodb_rs.graph_ppr($1,$2,$3,$4) $fn$;
REVOKE ALL ON FUNCTION theodb.graph_ppr(text,bigint[],float8,int) FROM PUBLIC;
"#,
    name = "theodb_graph_wrappers",
    requires = [
        graph_build,
        graph_refold,
        graph_expand,
        graph_expand_card,
        graph_expand_multi,
        graph_expand_multi_card,
        graph_ppr,
        "theodb_graph_schema"
    ],
);

// M108 (review HIGH-1): drop the persisted CSR row when its edge relation is DROPped, so a later relation that
// REUSES the freed OID cannot be served the old graph's CSR (a silent wrong answer). An `sql_drop` event trigger
// is the correct hook — `graph_csr.edge_rel` is a bare oid with no pg_depend edge.
extension_sql!(
    r#"
CREATE FUNCTION theodb.graph_on_drop() RETURNS event_trigger LANGUAGE plpgsql AS $fn$
DECLARE r record;
BEGIN
    FOR r IN SELECT objid FROM pg_event_trigger_dropped_objects() WHERE object_type = 'table' LOOP
        DELETE FROM theodb.graph_csr WHERE edge_rel = r.objid;
    END LOOP;
END $fn$;
CREATE EVENT TRIGGER theodb_graph_drop ON sql_drop EXECUTE FUNCTION theodb.graph_on_drop();
"#,
    name = "theodb_graph_drop_trigger",
    requires = ["theodb_graph_schema"],
);

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    fn seed() {
        // small deterministic graph: a path 0-1-2-3-4 + a hub 0-2, 0-3 (undirected)
        Spi::run("CREATE TABLE g(src bigint, dst bigint)").unwrap();
        Spi::run("INSERT INTO g VALUES (0,1),(1,2),(2,3),(3,4),(0,2),(0,3)").unwrap();
    }

    // M108: build persists a CSR; expand reads it (no rebuild) and returns the correct reachable set.
    #[pg_test]
    fn m108_build_persists_and_expand_reads() {
        seed();
        let ne: i64 = Spi::get_one("SELECT theodb.graph_build('g','src','dst')").unwrap().unwrap();
        assert_eq!(ne, 6, "6 edges built");
        // a persisted row exists
        let cnt: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.graph_csr WHERE edge_rel='g'::regclass::oid")
                .unwrap()
                .unwrap();
        assert_eq!(cnt, 1, "CSR persisted as one bytea row");
        // 1 hop from node 4 → {4,3}
        let r1: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[4]::bigint[], 1)")
                .unwrap()
                .unwrap();
        assert_eq!(r1, 2, "1-hop from 4 reaches {{4,3}}");
        // 2 hops from node 4 → {4,3,2,0}
        let r2: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[4]::bigint[], 2)")
                .unwrap()
                .unwrap();
        assert_eq!(r2, 4, "2-hop from 4 reaches {{4,3,2,0}}");
        // 3 hops from node 4 → all 5 nodes
        let r3: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[4]::bigint[], 3)")
                .unwrap()
                .unwrap();
        assert_eq!(r3, 5, "3-hop from 4 reaches all nodes");
    }

    // M108: expand does NOT rebuild — it reflects the CSR AT BUILD TIME until a refold; a new edge appears only
    // after graph_refold. This is ALSO the same-transaction cache-invalidation regression test (review HIGH-2):
    // the first expand populates the per-backend cache at built_at epoch E1; the refold rewrites built_at with
    // clock_timestamp() → E2 ≠ E1 (within ONE txn — `now()` would give E1==E2 and serve the STALE cached CSR,
    // making `after` wrong). So this test fails with `now()` and passes with `clock_timestamp()`.
    #[pg_test]
    fn m108_refold_folds_new_edges() {
        seed();
        Spi::get_one::<i64>("SELECT theodb.graph_build('g','src','dst')").unwrap();
        // add a far node 9 connected to 4; NOT visible until refold (the persisted CSR was built with nnodes=5,
        // so seed 9 is out-of-range → reaches nothing — proving expand reads the SNAPSHOT, not a per-query rebuild).
        Spi::run("INSERT INTO g VALUES (4,9)").unwrap();
        let before: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[9]::bigint[], 1)")
                .unwrap()
                .unwrap();
        assert_eq!(
            before, 0,
            "before refold, node 9 is not in the persisted CSR (expand reads the snapshot, no rebuild)"
        );
        let ne: i64 = Spi::get_one("SELECT theodb.graph_refold('g')").unwrap().unwrap();
        assert_eq!(ne, 7, "refold rebuilt the CSR with the 7th edge");
        let after: i64 =
            Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[9]::bigint[], 1)")
                .unwrap()
                .unwrap();
        assert_eq!(after, 2, "after refold, node 9 reaches {{9,4}}");
    }

    // M108 GATE benchmark: build ONCE, then per-query cost is load+traverse (no rebuild) — measured vs the
    // recursive-CTE baseline on the SAME graph. Emits `M108_BENCH ...` via WARNING (visible in the test log).
    #[pg_test]
    fn m108_bench_persisted_vs_cte() {
        use std::time::Instant;
        // hub-y graph ~200k edges / 40k nodes, deterministic via hashint8.
        Spi::run(
            "CREATE TABLE ge AS \
             SELECT (abs(hashint8(g))%40000)::bigint AS src, \
                    (CASE WHEN abs(hashint8(g*7))%100 < 25 THEN abs(hashint8(g*3))%400 \
                          ELSE abs(hashint8(g*13))%40000 END)::bigint AS dst \
             FROM generate_series(1,200000) g",
        )
        .unwrap();
        Spi::run("CREATE INDEX ON ge(src); CREATE INDEX ON ge(dst); ANALYZE ge").unwrap();
        let seeds = "ARRAY[100,2000,15000,30000,7777]::bigint[]";

        let tb = Instant::now();
        let built: i64 =
            Spi::get_one("SELECT theodb.graph_build('ge','src','dst')").unwrap().unwrap();
        let build_ms = tb.elapsed().as_secs_f64() * 1000.0;

        // First query is COLD (load+deserialize+cache); queries 2..6 are WARM (cache hit → traverse only).
        let tc = Instant::now();
        let mut exp_reached: i64 =
            Spi::get_one(&format!("SELECT count(*) FROM theodb.graph_expand('ge', {seeds}, 3)"))
                .unwrap()
                .unwrap();
        let cold_ms = tc.elapsed().as_secs_f64() * 1000.0;
        let mut exp_ms = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            exp_reached = Spi::get_one(&format!(
                "SELECT count(*) FROM theodb.graph_expand('ge', {seeds}, 3)"
            ))
            .unwrap()
            .unwrap();
            exp_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let mut cte_ms = Vec::new();
        let mut cte_reached = 0i64;
        for _ in 0..5 {
            let t = Instant::now();
            cte_reached = Spi::get_one(&format!(
                "WITH RECURSIVE reach(node,hop) AS (SELECT unnest({seeds}),0 UNION \
                 SELECT CASE WHEN e.src=r.node THEN e.dst ELSE e.src END, r.hop+1 \
                 FROM reach r JOIN ge e ON (e.src=r.node OR e.dst=r.node) WHERE r.hop<3) \
                 SELECT count(DISTINCT node) FROM reach"
            ))
            .unwrap()
            .unwrap();
            cte_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let em = mean(&exp_ms);
        let cm = mean(&cte_ms);
        let _ = (exp_reached, cte_reached); // counts logged below; the SET-HASH oracle is the real check
        // Correctness oracle (review MEDIUM): SET-HASH — bit_xor(hashint8(node)) over the reachable SET, not just
        // count(==count can collide on unequal sets). Persisted-expand set-hash MUST equal the CTE's.
        let exp_hash: i64 = Spi::get_one(&format!(
            "SELECT coalesce(bit_xor(hashint8(node)),0) FROM theodb.graph_expand('ge', {seeds}, 3) AS t(node)"
        )).unwrap().unwrap();
        let cte_hash: i64 = Spi::get_one(&format!(
            "WITH RECURSIVE reach(node,hop) AS (SELECT unnest({seeds}),0 UNION \
             SELECT CASE WHEN e.src=r.node THEN e.dst ELSE e.src END, r.hop+1 \
             FROM reach r JOIN ge e ON (e.src=r.node OR e.dst=r.node) WHERE r.hop<3) \
             SELECT coalesce(bit_xor(hashint8(node)),0) FROM (SELECT DISTINCT node FROM reach) s"
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            exp_hash, cte_hash,
            "M108 set-hash oracle: persisted expand must yield the SAME reachable set as the CTE"
        );
        let line = format!(
            "M108_BENCH edges={built} build_ms={build_ms:.3} cold_expand_ms={cold_ms:.3} warm_expand_mean_ms={em:.4} cte_mean_ms={cm:.3} warm_speedup={:.1} reached={exp_reached}\n",
            cm / em.max(1e-9)
        );
        let _ = std::fs::write("/tmp/m108_bench_result.txt", &line);
        pgrx::warning!("{}", line.trim());
        // Gate: the WARM per-query persisted expand (cache hit → traverse only) is faster than the recursive CTE —
        // the M107 traverse-only win, now WITHOUT the per-query build (paid ONCE) and WITHOUT re-deserialize (cached).
        assert!(
            em < cm,
            "M108 gate: warm persisted expand ({em:.3}ms) must beat the recursive CTE ({cm:.3}ms) per query"
        );
    }

    // M108 (review HIGH-1): DROPping the edge relation removes its persisted CSR row (sql_drop event trigger),
    // so a reused OID can never be served the old graph's CSR. After DROP, expand fails fast (no stale answer).
    #[pg_test]
    fn m108_drop_removes_persisted_csr() {
        seed();
        Spi::get_one::<i64>("SELECT theodb.graph_build('g','src','dst')").unwrap();
        assert_eq!(
            Spi::get_one::<i64>(
                "SELECT count(*) FROM theodb.graph_csr WHERE edge_rel='g'::regclass::oid"
            )
            .unwrap()
            .unwrap(),
            1,
            "CSR row present before DROP"
        );
        Spi::run("DROP TABLE g").unwrap();
        // the sql_drop trigger deleted the row; nothing references the freed oid
        let orphans: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_csr WHERE edge_rel NOT IN (SELECT oid FROM pg_class)").unwrap().unwrap();
        assert_eq!(
            orphans, 0,
            "sql_drop trigger removes the orphaned CSR row (no OID-reuse wrong answer)"
        );
    }

    // M112: Personalized PageRank oracle — on a symmetric line 0-1-2-3-4 seeded at the CENTER (2), PPR must be
    // symmetric (score[1]==score[3], score[0]==score[4]) and monotonically decay from the seed
    // (score[2]>score[1]>score[0]). A rigorous behavioral oracle (no fabricated numbers).
    #[pg_test]
    fn m112_ppr_symmetry_and_decay() {
        Spi::run("CREATE TABLE gline(src bigint, dst bigint)").unwrap();
        Spi::run("INSERT INTO gline VALUES (0,1),(1,2),(2,3),(3,4)").unwrap();
        Spi::get_one::<i64>("SELECT theodb.graph_build('gline','src','dst')").unwrap();
        let sc = |n: i64| -> f64 {
            Spi::get_one(&format!("SELECT score FROM theodb.graph_ppr('gline', ARRAY[2]::bigint[], 0.5, 40) WHERE node={n}")).unwrap().unwrap_or(0.0)
        };
        let (s0, s1, s2, s3, s4) = (sc(0), sc(1), sc(2), sc(3), sc(4));
        assert!((s1 - s3).abs() < 1e-6, "PPR symmetric: score[1]={s1} == score[3]={s3}");
        assert!((s0 - s4).abs() < 1e-6, "PPR symmetric: score[0]={s0} == score[4]={s4}");
        assert!(s2 > s1 && s1 > s0, "PPR decays monotonically from the seed: {s2} > {s1} > {s0}");
        // seed at an endpoint (0): the seed keeps the most mass, decay toward the far end.
        let e = |n: i64| -> f64 {
            Spi::get_one(&format!("SELECT score FROM theodb.graph_ppr('gline', ARRAY[0]::bigint[], 0.5, 40) WHERE node={n}")).unwrap().unwrap_or(0.0)
        };
        assert!(
            e(0) > e(1) && e(1) > e(2) && e(2) > e(3),
            "endpoint-seeded PPR decays along the line"
        );
    }

    // M108: expand on an unbuilt relation fails fast (typed), not a silent empty.
    #[pg_test]
    fn m108_expand_without_build_errors() {
        Spi::run("CREATE TABLE g2(src bigint, dst bigint)").unwrap();
        let r = std::panic::catch_unwind(|| {
            Spi::run("SELECT theodb.graph_expand('g2', ARRAY[0]::bigint[], 1)")
        });
        assert!(r.is_err(), "expand without a prior build must raise, not return empty");
    }

    // M109 (T1.1/T2.1): the DECOMPOSITION invariant — each MS-BFS lane's reachable set is byte-identical
    // (set-hash) to the single-source `expand` of that lane's seed. This is the primary correctness gate; a
    // word-boundary / tail-padding / axis-conflation bug changes a lane's set → the set-hash diverges.
    #[pg_test]
    fn m109_expand_multi_matches_expand_per_lane() {
        seed();
        Spi::get_one::<i64>("SELECT theodb.graph_build('g','src','dst')").unwrap();
        // 3 lanes, single seed each: set 1={4}, set 2={0}, set 3={1}. Check H=1,2,3.
        for h in 1..=3 {
            for (sid, s) in [(1i32, 4i64), (2, 0), (3, 1)] {
                let multi: i64 = Spi::get_one(&format!(
                    "SELECT coalesce(bit_xor(hashint8(node)),0) FROM theodb.graph_expand_multi('g', ARRAY[1,2,3]::int[], ARRAY[4,0,1]::bigint[], {h}) WHERE set_id={sid}"
                )).unwrap().unwrap();
                let single: i64 = Spi::get_one(&format!(
                    "SELECT coalesce(bit_xor(hashint8(node)),0) FROM theodb.graph_expand('g', ARRAY[{s}]::bigint[], {h}) AS t(node)"
                )).unwrap().unwrap();
                assert_eq!(
                    multi, single,
                    "lane {sid} (seed {s}) H={h}: MS-BFS set-hash must equal single-source expand"
                );
            }
        }
    }

    // M109 (T1.1): a lane with MULTIPLE seeds equals the union single-source expand of those seeds.
    #[pg_test]
    fn m109_expand_multi_multiseed_lane() {
        seed();
        Spi::get_one::<i64>("SELECT theodb.graph_build('g','src','dst')").unwrap();
        let multi: i64 = Spi::get_one(
            "SELECT coalesce(bit_xor(hashint8(node)),0) FROM theodb.graph_expand_multi('g', ARRAY[7,7]::int[], ARRAY[4,1]::bigint[], 1) WHERE set_id=7"
        ).unwrap().unwrap();
        let single: i64 = Spi::get_one(
            "SELECT coalesce(bit_xor(hashint8(node)),0) FROM theodb.graph_expand('g', ARRAY[4,1]::bigint[], 1) AS t(node)"
        ).unwrap().unwrap();
        assert_eq!(multi, single, "multi-seed lane must equal the union single-source expand");
    }

    // M109 (T1.1): out-of-range seed contributes nothing; lanes are independent (no cross-lane bit leak).
    #[pg_test]
    fn m109_expand_multi_lanes_independent() {
        seed();
        Spi::get_one::<i64>("SELECT theodb.graph_build('g','src','dst')").unwrap();
        let l1: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_expand_multi('g', ARRAY[1,2]::int[], ARRAY[4,99]::bigint[], 3) WHERE set_id=1").unwrap().unwrap();
        let l2: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_expand_multi('g', ARRAY[1,2]::int[], ARRAY[4,99]::bigint[], 3) WHERE set_id=2").unwrap().unwrap();
        assert_eq!(l1, 5, "lane 1 (seed 4, 3 hops) reaches all 5 nodes");
        assert_eq!(l2, 0, "lane 2 (out-of-range seed) reaches nothing — lanes independent");
    }

    // M109 (T1.2): > 64 seed-sets force TILING (64 + 1). Each lane across the tile boundary still matches its
    // single-source expand — catches tile-boundary index / lane-map bugs.
    #[pg_test]
    fn m109_expand_multi_tiling_65_sets() {
        seed();
        Spi::get_one::<i64>("SELECT theodb.graph_build('g','src','dst')").unwrap();
        for k in [1i32, 64, 65] {
            let s = ((k - 1) % 5) as i64;
            let multi: i64 = Spi::get_one(&format!(
                "WITH q AS (SELECT g AS sid, ((g-1)%5)::bigint AS sd FROM generate_series(1,65) g) \
                 SELECT coalesce(bit_xor(hashint8(node)),0) FROM theodb.graph_expand_multi('g', \
                   (SELECT array_agg(sid ORDER BY sid)::int[] FROM q), (SELECT array_agg(sd ORDER BY sid) FROM q), 2) WHERE set_id={k}"
            )).unwrap().unwrap();
            let single: i64 = Spi::get_one(&format!(
                "SELECT coalesce(bit_xor(hashint8(node)),0) FROM theodb.graph_expand('g', ARRAY[{s}]::bigint[], 2) AS t(node)"
            )).unwrap().unwrap();
            assert_eq!(
                multi, single,
                "tiling lane {k} (seed {s}) crosses the 64-lane tile boundary correctly"
            );
        }
    }

    // M109 (T2.1 negative): unbuilt relation → typed error, not silent empty.
    #[pg_test]
    fn m109_expand_multi_without_build_errors() {
        Spi::run("CREATE TABLE g3(src bigint, dst bigint)").unwrap();
        let r = std::panic::catch_unwind(|| {
            Spi::run(
                "SELECT * FROM theodb.graph_expand_multi('g3', ARRAY[1]::int[], ARRAY[0]::bigint[], 1)",
            )
        });
        assert!(r.is_err(), "expand_multi without a prior build must raise, not return empty");
    }

    // M109 (T2.1 negative): unequal set_ids/seeds lengths → typed error at the boundary.
    #[pg_test]
    fn m109_expand_multi_length_mismatch_errors() {
        seed();
        Spi::get_one::<i64>("SELECT theodb.graph_build('g','src','dst')").unwrap();
        let r = std::panic::catch_unwind(|| {
            Spi::run(
                "SELECT * FROM theodb.graph_expand_multi('g', ARRAY[1,2]::int[], ARRAY[0]::bigint[], 1)",
            )
        });
        assert!(r.is_err(), "unequal set_ids/seeds lengths must raise a typed error");
    }

    // M109 GATE benchmark (T3.1): the MS-BFS CROSSOVER SWEEP. Measures batched MS-BFS vs N sequential
    // single-source BFS across N ∈ {1..512} on the M108 hub graph — TRAVERSAL-ONLY (per-lane cardinality via
    // graph_expand_multi_card vs count(*) per expand), so the ~1.28M-row SQL materialization does NOT confound
    // the traversal comparison. The set-hash oracle HARD-gates correctness at every N. This empirically locates
    // the crossover the literature predicts at ~100–256 sources (Then et al. VLDB'14 Fig 1/6): below it,
    // sequential wins (few-seed GraphRAG regime); above it, MS-BFS's shared-hub traversal amortizes. Writes
    // /tmp/m109_crossover.json (the curve). An honest-negative at small N is a VALID measured outcome (ADR-3).
    #[pg_test]
    fn m109_bench_crossover_sweep() {
        use std::time::Instant;
        Spi::run(
            "CREATE TABLE ge AS \
             SELECT (abs(hashint8(g))%40000)::bigint AS src, \
                    (CASE WHEN abs(hashint8(g*7))%100 < 25 THEN abs(hashint8(g*3))%400 \
                          ELSE abs(hashint8(g*13))%40000 END)::bigint AS dst \
             FROM generate_series(1,200000) g",
        )
        .unwrap();
        Spi::run("CREATE INDEX ON ge(src); CREATE INDEX ON ge(dst); ANALYZE ge").unwrap();
        let built: i64 =
            Spi::get_one("SELECT theodb.graph_build('ge','src','dst')").unwrap().unwrap();

        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let ns: [i64; 7] = [1, 4, 16, 64, 128, 256, 512];
        let mut points: Vec<String> = Vec::new();
        let mut crossover: i64 = -1;
        let mut max_pure_speedup: f64 = 0.0;
        for &n in &ns {
            let seeds_vec: Vec<i64> = (1..=n).map(|k| (k * 617) % 40000).collect();
            let sids_arr = format!(
                "ARRAY[{}]::int[]",
                (1..=n).map(|k| k.to_string()).collect::<Vec<_>>().join(",")
            );
            let sds_arr = format!(
                "ARRAY[{}]::bigint[]",
                seeds_vec.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",")
            );

            // Oracle FIRST (gates the number at THIS N): batched reachable sets == N sequential expands.
            let batched_hash: i64 = Spi::get_one(&format!(
                "SELECT coalesce(bit_xor(hashint8(set_id::bigint*1000003 + node)),0) FROM theodb.graph_expand_multi('ge', {sids_arr}, {sds_arr}, 3)"
            )).unwrap().unwrap();
            let mut seq_hash: i64 = 0;
            for (i, &sk) in seeds_vec.iter().enumerate() {
                let k = (i + 1) as i64;
                let h: i64 = Spi::get_one(&format!(
                    "SELECT coalesce(bit_xor(hashint8({k}::bigint*1000003 + node)),0) FROM theodb.graph_expand('ge', ARRAY[{sk}]::bigint[], 3) AS t(node)"
                )).unwrap().unwrap();
                seq_hash ^= h;
            }
            assert_eq!(
                batched_hash, seq_hash,
                "M109 oracle @ N={n}: batched reachable sets must equal N sequential expands"
            );

            // TRAVERSAL-ONLY timing. batched = graph_expand_multi_card (N cardinality rows out, count in Rust).
            // Two sequential baselines to DECOMPOSE the win honestly:
            //   seq_card  = N × graph_expand_card  → count in Rust too, so this isolates (a) MS-BFS edge-sharing
            //               + (b) 1 SPI call vs N — the PURE traversal+call speedup, no row-streaming asymmetry.
            //   seq_nodes = N × count(*) over graph_expand → streams every reachable node row (the naive caller).
            let _ = Spi::get_one::<i64>(&format!("SELECT count(*) FROM theodb.graph_expand_multi_card('ge', {sids_arr}, {sds_arr}, 3)")).unwrap();
            let mut batch_ms = Vec::new();
            for _ in 0..3 {
                let t = Instant::now();
                let _ = Spi::get_one::<i64>(&format!("SELECT count(*) FROM theodb.graph_expand_multi_card('ge', {sids_arr}, {sds_arr}, 3)")).unwrap();
                batch_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            let mut seq_card_ms = Vec::new();
            for _ in 0..3 {
                let t = Instant::now();
                for &sk in &seeds_vec {
                    let _ = Spi::get_one::<i64>(&format!(
                        "SELECT theodb.graph_expand_card('ge', ARRAY[{sk}]::bigint[], 3)"
                    ))
                    .unwrap();
                }
                seq_card_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            let mut seq_nodes_ms = Vec::new();
            for _ in 0..3 {
                let t = Instant::now();
                for &sk in &seeds_vec {
                    let _ = Spi::get_one::<i64>(&format!(
                        "SELECT count(*) FROM theodb.graph_expand('ge', ARRAY[{sk}]::bigint[], 3)"
                    ))
                    .unwrap();
                }
                seq_nodes_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            let std = |v: &[f64], m: f64| {
                (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
            };
            let (bm, scm, snm) = (mean(&batch_ms), mean(&seq_card_ms), mean(&seq_nodes_ms));
            let (bstd, scstd) = (std(&batch_ms, bm), std(&seq_card_ms, scm));
            let speedup_pure = scm / bm.max(1e-9); // isolates (a)+(b): both count in Rust
            let speedup_naive = snm / bm.max(1e-9); // vs the naive N-call node-streaming baseline
            if crossover < 0 && speedup_pure >= 1.0 {
                crossover = n;
            }
            max_pure_speedup = max_pure_speedup.max(speedup_pure);
            pgrx::warning!(
                "M109_SWEEP n={n} batched_ms={bm:.3}±{bstd:.3} seq_card_ms={scm:.3}±{scstd:.3} pure_speedup={speedup_pure:.3} naive_speedup={speedup_naive:.3}"
            );
            points.push(format!(
                "{{\"n\":{n},\"batched_ms\":{bm:.4},\"batched_std\":{bstd:.4},\"seq_card_ms\":{scm:.4},\"seq_card_std\":{scstd:.4},\"seq_nodes_ms\":{snm:.4},\"pure_speedup\":{speedup_pure:.3},\"naive_speedup\":{speedup_naive:.3}}}"
            ));
        }
        // TOPOLOGY-FLOOR datapoint (review LOW): the hub graph is MS-BFS's best case (lanes share hubs). A
        // UNIFORM-random graph (no hub concentration) bounds the floor — how much of the win survives when
        // seeds share little structure. Measured at N=64, same methodology, oracle PASS.
        Spi::run(
            "CREATE TABLE geu AS SELECT (abs(hashint8(g))%40000)::bigint AS src, (abs(hashint8(g*2654435761))%40000)::bigint AS dst \
             FROM generate_series(1,200000) g",
        ).unwrap();
        Spi::run("CREATE INDEX ON geu(src); CREATE INDEX ON geu(dst); ANALYZE geu").unwrap();
        Spi::get_one::<i64>("SELECT theodb.graph_build('geu','src','dst')").unwrap();
        let fn64: i64 = 64;
        let fseeds: Vec<i64> = (1..=fn64).map(|k| (k * 617) % 40000).collect();
        let fsids = format!(
            "ARRAY[{}]::int[]",
            (1..=fn64).map(|k| k.to_string()).collect::<Vec<_>>().join(",")
        );
        let fsds = format!(
            "ARRAY[{}]::bigint[]",
            fseeds.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(",")
        );
        let fbh: i64 = Spi::get_one(&format!("SELECT coalesce(bit_xor(hashint8(set_id::bigint*1000003 + node)),0) FROM theodb.graph_expand_multi('geu', {fsids}, {fsds}, 3)")).unwrap().unwrap();
        let mut fsh: i64 = 0;
        for (i, &sk) in fseeds.iter().enumerate() {
            let k = (i + 1) as i64;
            fsh ^= Spi::get_one::<i64>(&format!("SELECT coalesce(bit_xor(hashint8({k}::bigint*1000003 + node)),0) FROM theodb.graph_expand('geu', ARRAY[{sk}]::bigint[], 3) AS t(node)")).unwrap().unwrap();
        }
        assert_eq!(fbh, fsh, "M109 oracle @ uniform-floor N=64: batched == N sequential");
        let _ = Spi::get_one::<i64>(&format!(
            "SELECT count(*) FROM theodb.graph_expand_multi_card('geu', {fsids}, {fsds}, 3)"
        ))
        .unwrap();
        let mut fb = Vec::new();
        for _ in 0..3 {
            let t = std::time::Instant::now();
            let _ = Spi::get_one::<i64>(&format!(
                "SELECT count(*) FROM theodb.graph_expand_multi_card('geu', {fsids}, {fsds}, 3)"
            ))
            .unwrap();
            fb.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let mut fs = Vec::new();
        for _ in 0..3 {
            let t = std::time::Instant::now();
            for &sk in &fseeds {
                let _ = Spi::get_one::<i64>(&format!(
                    "SELECT theodb.graph_expand_card('geu', ARRAY[{sk}]::bigint[], 3)"
                ))
                .unwrap();
            }
            fs.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let floor_speedup = mean(&fs) / mean(&fb).max(1e-9);
        pgrx::warning!("M109_FLOOR uniform_graph N=64 pure_speedup={floor_speedup:.3}");

        let json = format!(
            "{{\"graph\":{{\"nodes\":40000,\"edges\":{built}}},\"hardware\":\"DO-Regular 4 vCPU @ 2.0GHz, 7.8GB (pgrx 0.19/PG17)\",\"uniform_floor_speedup_n64\":{floor_speedup:.3},\"max_hops\":3,\"runs\":3,\
             \"methodology\":\"traversal-only. batched=graph_expand_multi_card (count in Rust). pure_speedup vs seq_card (N x graph_expand_card, also count in Rust) isolates MS-BFS edge-sharing + 1-SPI-call-vs-N. naive_speedup vs N x count(*) over graph_expand (streams nodes).\",\
             \"crossover_n_pure\":{crossover},\"curve\":[{}]}}\n",
            points.join(",")
        );
        let _ = std::fs::write("/tmp/m109_crossover.json", &json);
        pgrx::warning!(
            "M109_CROSSOVER crossover_n={crossover} max_pure_speedup={max_pure_speedup:.2} (>=1.0x speedup first at this N; -1 = never within swept range)"
        );
        // GATE: the oracle (correctness at every N) already hard-gated each point. Regression gate: batched
        // MS-BFS traversal MUST beat the count-in-Rust sequential baseline by > 2× somewhere in the swept range
        // (measured ~7× at N=64; a conservative 2× floor guards against a regression that erases the win).
        assert!(
            max_pure_speedup > 2.0,
            "M109 GATE: batched MS-BFS pure-traversal speedup must exceed 2× (got {max_pure_speedup:.2}×)"
        );
    }

    // M144 T2.4 (EC-4 EDGE): a large id that fits in u32 must NOT trip the guard — the CSR builds.
    //
    // HONEST NOTE (proven on the droplet, 2026-07-23): the CSR offset array is DENSE, indexed by the raw
    // node id (`vec![0u64; nn + 1]`, `offsets[node as usize]`), so `nn == max_id + 1`. Building at the
    // literal u32::MAX (4294967295) allocates a ~34 GB offset array → OOM, unrelated to the guard. So the
    // ACCEPT side is exercised with a large-but-feasible id (1_000_000): it proves the guard fires ONLY on
    // ids strictly PAST u32::MAX (the guard is `>`, not `>=`) and never false-positives on a valid id. The
    // exact-boundary REJECT (u32::MAX + 1) is proven feasibly by the sibling NEGATIVE test — the guard
    // aborts BEFORE the allocation, so it needs no memory.
    #[pg_test]
    fn csr_build_accepts_large_valid_u32_id() {
        Spi::run("CREATE TABLE gmax(src bigint, dst bigint)").unwrap();
        // 1_000_000 is well within u32 and builds a ~8 MB offset array — feasible, and enough to prove the
        // guard does not misfire on a legitimately large id.
        Spi::run("INSERT INTO gmax VALUES (0, 1000000)").unwrap();
        let ne: i64 =
            Spi::get_one("SELECT theodb.graph_build('gmax','src','dst')").unwrap().unwrap();
        assert_eq!(ne, 1, "one edge built for a large valid u32 id");
    }

    // M144 T2.4 (EC-4 NEGATIVE): a node id past u32::MAX must fail-closed with a typed error,
    // never truncate silently into the u32 adjacency slot.
    #[pg_test(error = "must fit in u32")]
    fn csr_build_guards_u32_boundary() {
        Spi::run("CREATE TABLE gover(src bigint, dst bigint)").unwrap();
        // 4294967296 == u32::MAX + 1 — the first id past the boundary.
        Spi::run("INSERT INTO gover VALUES (0, 4294967296)").unwrap();
        // Must ereport "must fit in u32" — the #[pg_test(error=...)] attribute asserts it.
        let _ = Spi::get_one::<i64>("SELECT theodb.graph_build('gover','src','dst')");
    }
}
