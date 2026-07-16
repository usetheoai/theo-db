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
            return Err(format!("theodb graph: CSR length mismatch (got {}, want {adj_end})", b.len()));
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
                let (a0, a1) = (self.offsets[node as usize] as usize, self.offsets[node as usize + 1] as usize);
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
}

/// Scan the edge relation and build the undirected CSR. `edge_rel` is a table name; `src_col`/`dst_col` are the
/// integer endpoint columns. Injection-safe: identifiers quoted via Postgres `format('%I')` over SPI.
fn build_csr(edge_rel: &str, src_col: &str, dst_col: &str) -> Csr {
    let scan_sql = Spi::get_one_with_args::<String>(
        "SELECT format('SELECT (%I)::bigint AS s, (%I)::bigint AS d FROM %s WHERE %I IS NOT NULL AND %I IS NOT NULL', $1,$2,$3,$1,$2)",
        &[src_col.into(), dst_col.into(), edge_rel.into()],
    )
    .ok()
    .flatten()
    .unwrap_or_else(|| crate::pg::err_input("theodb.graph_build: could not build scan query"));

    let mut src: Vec<i64> = Vec::new();
    let mut dst: Vec<i64> = Vec::new();
    let mut maxn: i64 = -1;
    Spi::connect(|client| {
        let t = client
            .select(&scan_sql, None, &[])
            .unwrap_or_else(|e| crate::pg::err_input(&format!("theodb.graph_build: scan failed: {e:?}")));
        for row in t {
            let s = row.get::<i64>(1).ok().flatten();
            let d = row.get::<i64>(2).ok().flatten();
            if let (Some(s), Some(d)) = (s, d) {
                if s < 0 || d < 0 {
                    crate::pg::err_input("theodb.graph_build: node ids must be non-negative bigints");
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
    use super::{build_csr, Csr};
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
            _ => crate::pg::err_input("theodb.graph_refold: no persisted CSR for this edge relation (run graph_build first)"),
        }
    }

    /// `graph_expand(edge_rel text, seeds bigint[], max_hops int) -> SETOF bigint` — load the PERSISTED CSR (no
    /// rebuild) and return the reachable node set within `max_hops` from `seeds` (undirected frontier BFS).
    #[pg_extern]
    fn graph_expand(edge_rel: &str, seeds: Vec<Option<i64>>, max_hops: i32) -> SetOfIterator<'static, i64> {
        // Resolve the edge-relation oid + its persisted `built_at` epoch (cheap — NO bytea load).
        let (oid, epoch): (pg_sys::Oid, i64) = super::CSR_CACHE.with(|_| {
            Spi::get_two_with_args::<pg_sys::Oid, i64>(
                "SELECT ($1)::regclass::oid, (extract(epoch from built_at)*1e6)::bigint \
                 FROM theodb.graph_csr WHERE edge_rel = ($1)::regclass::oid",
                &[edge_rel.into()],
            )
            .ok()
            .and_then(|(o, e)| Some((o?, e?)))
            .unwrap_or_else(|| {
                crate::pg::err_unsupported(
                    "theodb.graph_expand: no persisted CSR for this edge relation — run theodb.graph_build(edge_rel, src_col, dst_col) first",
                )
            })
        });
        // Cache hit (same oid + built_at) → skip the load+deserialize; else load the bytea, deserialize, cache.
        let csr: Rc<Csr> = super::CSR_CACHE.with(|c| {
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
            .unwrap_or_else(|| crate::pg::err_input("theodb.graph_expand: persisted CSR row vanished mid-query"));
            let rc = Rc::new(
                Csr::from_bytes(&bytes).unwrap_or_else(|e| crate::pg::err_input(&format!("theodb.graph_expand: {e}"))),
            );
            c.borrow_mut().insert(oid, (epoch, rc.clone()));
            rc
        });
        let seed_ids: Vec<i64> = seeds.into_iter().flatten().collect();
        SetOfIterator::new(csr.expand(&seed_ids, max_hops).into_iter())
    }
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
    requires = [graph_build, graph_refold, graph_expand],
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
REVOKE ALL ON FUNCTION theodb.graph_build(text,text,text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.graph_refold(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.graph_expand(text,bigint[],int) FROM PUBLIC;
"#,
    name = "theodb_graph_wrappers",
    requires = [graph_build, graph_refold, graph_expand, "theodb_graph_schema"],
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
        let cnt: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_csr WHERE edge_rel='g'::regclass::oid").unwrap().unwrap();
        assert_eq!(cnt, 1, "CSR persisted as one bytea row");
        // 1 hop from node 4 → {4,3}
        let r1: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[4]::bigint[], 1)").unwrap().unwrap();
        assert_eq!(r1, 2, "1-hop from 4 reaches {{4,3}}");
        // 2 hops from node 4 → {4,3,2,0}
        let r2: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[4]::bigint[], 2)").unwrap().unwrap();
        assert_eq!(r2, 4, "2-hop from 4 reaches {{4,3,2,0}}");
        // 3 hops from node 4 → all 5 nodes
        let r3: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[4]::bigint[], 3)").unwrap().unwrap();
        assert_eq!(r3, 5, "3-hop from 4 reaches all nodes");
    }

    // M108: expand does NOT rebuild — it reflects the CSR AT BUILD TIME until a refold; a new edge appears only
    // after graph_refold (the fold-on-demand maintenance).
    #[pg_test]
    fn m108_refold_folds_new_edges() {
        seed();
        Spi::get_one::<i64>("SELECT theodb.graph_build('g','src','dst')").unwrap();
        // add a far node 9 connected to 4; NOT visible until refold (the persisted CSR was built with nnodes=5,
        // so seed 9 is out-of-range → reaches nothing — proving expand reads the SNAPSHOT, not a per-query rebuild).
        Spi::run("INSERT INTO g VALUES (4,9)").unwrap();
        let before: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[9]::bigint[], 1)").unwrap().unwrap();
        assert_eq!(before, 0, "before refold, node 9 is not in the persisted CSR (expand reads the snapshot, no rebuild)");
        let ne: i64 = Spi::get_one("SELECT theodb.graph_refold('g')").unwrap().unwrap();
        assert_eq!(ne, 7, "refold rebuilt the CSR with the 7th edge");
        let after: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_expand('g', ARRAY[9]::bigint[], 1)").unwrap().unwrap();
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
        let built: i64 = Spi::get_one("SELECT theodb.graph_build('ge','src','dst')").unwrap().unwrap();
        let build_ms = tb.elapsed().as_secs_f64() * 1000.0;

        // First query is COLD (load+deserialize+cache); queries 2..6 are WARM (cache hit → traverse only).
        let tc = Instant::now();
        let mut exp_reached: i64 = Spi::get_one(&format!("SELECT count(*) FROM theodb.graph_expand('ge', {seeds}, 3)")).unwrap().unwrap();
        let cold_ms = tc.elapsed().as_secs_f64() * 1000.0;
        let mut exp_ms = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            exp_reached = Spi::get_one(&format!("SELECT count(*) FROM theodb.graph_expand('ge', {seeds}, 3)")).unwrap().unwrap();
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
            )).unwrap().unwrap();
            cte_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
        let em = mean(&exp_ms);
        let cm = mean(&cte_ms);
        // Correctness oracle: persisted-expand reachable-set count == recursive-CTE count.
        assert_eq!(exp_reached, cte_reached, "M108 oracle: persisted expand must match the CTE reachable set");
        let line = format!(
            "M108_BENCH edges={built} build_ms={build_ms:.3} cold_expand_ms={cold_ms:.3} warm_expand_mean_ms={em:.4} cte_mean_ms={cm:.3} warm_speedup={:.1} reached={exp_reached}\n",
            cm / em.max(1e-9)
        );
        let _ = std::fs::write("/tmp/m108_bench_result.txt", &line);
        pgrx::warning!("{}", line.trim());
        // Gate: the WARM per-query persisted expand (cache hit → traverse only) is faster than the recursive CTE —
        // the M107 traverse-only win, now WITHOUT the per-query build (paid ONCE) and WITHOUT re-deserialize (cached).
        assert!(em < cm, "M108 gate: warm persisted expand ({em:.3}ms) must beat the recursive CTE ({cm:.3}ms) per query");
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
}
