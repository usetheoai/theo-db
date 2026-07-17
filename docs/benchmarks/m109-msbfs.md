# M109 — Vectorized Multi-Source BFS: crossover benchmark

**Date:** 2026-07-16 · **Milestone:** M109 (native graph pillar Phase 2) · **Raw data:** [`m109-msbfs.json`](./m109-msbfs.json)
**Hardware:** DigitalOcean DO-Regular, 4 vCPU @ 2.0GHz, 7.8 GB · pgrx 0.19 / PostgreSQL 17 · release build, default `target-cpu`.

## What is measured

Batched Multi-Source BFS (`theodb.graph_expand_multi_card`, one call, N lanes advanced together by 64-wide
u64 source-masks) vs **N sequential single-source BFS** — the reachable-set-within-≤3-hops of each of N seeds.
Correctness is hard-gated at **every N** by the per-lane **set-hash oracle** (`bit_xor(hashint8(set_id*P +
node))`, lane-keyed): each MS-BFS lane's reachable set is proven byte-identical to that seed's single-source
`expand`. Each point is mean ± std over 3 warm runs.

### Methodology — the row-materialization confound and how it is removed

A first, naive benchmark timed `count(*)` over the **node rows** returned by both paths (~1.28 M rows at
N=64). That measured SQL row-streaming, **not traversal**, and reported a spurious *loss* (0.44–0.62×). The
honest measurement isolates traversal by returning **per-lane cardinality** (count computed in Rust, N rows
out), decomposed against two baselines:

- **`pure_speedup`** vs `N × graph_expand_card` (sequential, **also count-in-Rust**) — isolates the genuine
  MS-BFS edge-sharing + the 1-SPI-call-vs-N amortization, row-streaming asymmetry removed from *both* sides.
  The 1-call amortization is the constant floor (visible at N=1 ≈ 1.3–1.7×); the **growth with N** is the
  genuine edge-sharing.
- **`naive_speedup`** vs `N × count(*) over graph_expand` (streams every reached node row) — what a naive
  caller issuing N single-seed neighborhood queries experiences.

## Result (hub graph, 40k nodes / 200k edges, ~25% edges → 400 hubs)

| N (seeds) | batched ms (±std) | seq count-in-Rust ms (±std) | **pure_speedup** | naive_speedup |
|---:|---:|---:|---:|---:|
| 1   | 1.63 ±0.08  | 2.77 ±0.81   | **1.70×** | 2.71× |
| 4   | 3.69 ±0.44  | 8.60 ±0.09   | **2.33×** | 4.84× |
| 16  | 8.49 ±0.59  | 61.40 ±8.18  | **7.24×** | 19.16× |
| 64  | 33.04 ±1.26 | 213.75 ±14.78 | **6.47×** | 12.60× |
| 128 | 69.88 ±5.36 | 398.04 ±18.67 | **5.70×** | 11.10× |
| 256 | 145.23 ±9.62 | 723.22 ±12.11 | **4.98×** | 10.41× |
| 512 | 183.06 ±14.85 | 1430.11 ±5.39 | **7.81×** | 17.09× |

**Crossover N = 1** — batched wins at every source count (N=1 pure 1.70×, std small: 1.63±0.08 vs 2.77±0.81).
The `pure_speedup` sits at **~5–8×** across N ≥ 16 (VM run-to-run variance is real on a shared vCPU — hence
the std column; the win is far outside it). The growth from ~1.7× (N=1, call-amortization only) to 5–8× is the
edge-sharing mechanism Then et al. (VLDB 2014) describe: a vertex reached by many lanes has its adjacency
traversed **once** with the OR'd lane bits instead of once per lane.

## Topology floor — the win is NOT hub-gamed

The concern that the hub graph is MS-BFS's best case is **refuted by measurement**: a **uniform-random** graph
(no hub concentration, same 40k/200k) measured at N=64 gives **pure_speedup ≈ 10.2×** — *stronger* than the hub
graph, because at avg-degree 10 over ≤3 hops the lanes' neighborhoods overlap broadly regardless of hubs.
`uniform_floor_speedup_n64` in the json. The edge-sharing win holds across both topologies; it is a property of
overlapping bounded-hop neighborhoods, not of hub structure.

## Honest scope & caveats

- `pure_speedup` folds in the 1-SPI-call-vs-N amortization (a real architectural advantage of the batched
  API); the **growth above ~1.7× as N rises** is the genuine algorithmic edge-sharing.
- Numbers vary run-to-run on the shared DO-Regular vCPU (see ±std); the ~5–8× headline is robust, individual
  points shift.
- This is the **reachable-set** workload (no early termination), distinct from DuckPGQ's shortest-path-length
  workload; both are legitimate.
- What the batched operator delivers that a loop cannot cheaply: **per-seed** answers (reach-count or
  reach-set) in one traversal. For the pure **union** neighborhood (all seeds → one set), `expand(all_seeds)`
  (M108) already suffices (HippoRAG joint-neighborhood); M109 is the faster primitive when **per-seed**
  separation is needed (per-entity signals, future per-seed PPR).

## Reproduce

```
cargo pgrx test pg17 m109_bench_crossover_sweep   # writes /tmp/m109_crossover.json
```
The set-hash oracle asserts correctness at every N (hub + uniform floor); the regression gate asserts
`max pure_speedup > 2×`.
