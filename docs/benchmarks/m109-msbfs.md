# M109 — Vectorized Multi-Source BFS: crossover benchmark

**Date:** 2026-07-16 · **Milestone:** M109 (native graph pillar Phase 2) · **Raw data:** [`m109-msbfs.json`](./m109-msbfs.json)

## What is measured

Batched Multi-Source BFS (`theodb.graph_expand_multi_card`, one call, N lanes advanced together by 64-wide
u64 source-masks) vs **N sequential single-source BFS** — the reachable-set-within-≤3-hops of each of N seeds
on a hub graph (40 000 nodes, 200 000 edges, ~25% edges to 400 hubs). Correctness is hard-gated at **every N**
by the per-lane **set-hash oracle** (`bit_xor(hashint8(node))`): each MS-BFS lane's reachable set is proven
byte-identical to that seed's single-source `expand`.

### Methodology — the row-materialization confound and how it is removed

A first, naive benchmark timed `count(*)` over the **node rows** returned by both paths (~1.28 M rows at
N=64). That measured SQL row-streaming, **not traversal**, and reported a spurious *loss* (0.44–0.62×). The
honest measurement isolates traversal by returning **per-lane cardinality** (count computed in Rust, N rows
out), and decomposes the win against two baselines:

- **`pure_speedup`** vs `N × graph_expand_card` (sequential, **also count-in-Rust**) — isolates the genuine
  MS-BFS edge-sharing + the 1-SPI-call-vs-N amortization, with the row-streaming asymmetry removed from *both*
  sides. This is the honest traversal number.
- **`naive_speedup`** vs `N × count(*) over graph_expand` (streams every reached node row) — what a naive
  caller issuing N single-seed neighborhood queries actually experiences.

Each point is mean of 3 runs, warm cache. Hardware: DigitalOcean dedicated droplet (pgrx 0.19 / PG17).

## Result

| N (seeds) | batched ms | seq count-in-Rust ms | **pure_speedup** | naive_speedup |
|---:|---:|---:|---:|---:|
| 1   | 2.22   | 2.96    | **1.33×** | 1.98× |
| 4   | 2.80   | 7.04    | **2.51×** | 7.46× |
| 16  | 10.70  | 52.54   | **4.91×** | 8.25× |
| 64  | 25.74  | 185.26  | **7.20×** | 12.21× |
| 128 | 41.10  | 266.26  | **6.48×** | 18.83× |
| 256 | 95.66  | 673.10  | **7.04×** | 17.81× |
| 512 | 277.29 | 1713.78 | **6.18×** | 13.86× |

**Crossover N = 1** — batched wins at every source count. The `pure_speedup` climbs **1.33× → 7×** as N grows
1 → 64 and plateaus at ~6–7× through 512: the growth-with-N is precisely the edge-sharing mechanism Then et
al. (VLDB 2014) describe — a hub reached by many lanes has its high-degree adjacency traversed **once** with
the OR'd lane bits instead of once per lane. Oracle PASS at every N.

## Honest scope & caveats

- The absolute speedup is **topology-dependent**: it comes from lanes sharing hub traversals. A graph with no
  shared structure between seeds would show less. The hub graph here models a realistic clustered knowledge
  graph.
- `pure_speedup` folds in the 1-SPI-call-vs-N amortization (a real architectural advantage of the batched
  API); the *growth* above ~1.3× as N rises is the genuine algorithmic edge-sharing.
- This is the **reachable-set** workload (no early termination). It is distinct from DuckPGQ's
  shortest-path-length workload; both are legitimate, and MS-BFS helps here because many lanes converge on the
  same hubs within ≤3 hops.
- What the batched operator delivers that a loop cannot cheaply: **per-seed** answers (reach-count or
  reach-set) in one traversal. For the pure **union** neighborhood (all seeds → one set), `expand(all_seeds)`
  (M108) already suffices; M109 is the faster primitive when per-seed separation is needed (per-entity signals,
  future per-seed PPR).

## Reproduce

```
cargo pgrx test pg17 m109_bench_crossover_sweep   # writes /tmp/m109_crossover.json
```
The set-hash oracle asserts correctness at every N; the regression gate asserts `max pure_speedup > 2×`.
