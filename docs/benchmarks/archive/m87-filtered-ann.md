# M87 — pg_scann filtered ANN + planner: iterative IVF scan (SIFT1M)

**Date:** 2026-07-12 · **Dataset:** SIFT1M + synthetic label column · **Verdict:** `GO`

M87 makes filtered ANN (`WHERE … ORDER BY e <-> q LIMIT k`) work over the storage-separated layout, and verifies
the planner picks the AM correctly.

## The gap + fix

The M52 iterative scan (which recovers recall under a selective `WHERE` by growing the search) was **HNSW-only**.
The IVF-AQ split scans (v5/v6) and the v3 f32 IVF did **not** participate → a selective `WHERE` collapsed recall
(the first probed lists' candidates get filtered out, the AM returns `false`, the `LIMIT` is never satisfied).

**M87** refactors the IVF scans to return a `Vec` + take `probes`/`rerank_pool` params, and arms the iterative
re-search for **all IVF** (v3/v4/v5/v6): under a selective `WHERE` it grows **probes** (reach unprobed lists) **and**
the **rerank pool** (once all lists are probed, the pool caps distinct emitted — a selective filter needs more
reranked to find k) until `max_scan_tuples` distinct tids are emitted. Dedup-by-tid via the existing `amgettuple`
emitted-`HashSet`. `amcostestimate` was already v5/v6-aware (the fallback chain) — verified, not changed.

## Correctness

**248 pg_tests GREEN** (247 + 1 M87: `filtered_ann_v5_iterative_preserves_recall` — asserts the index-scan
filtered top-k equals the exact seqscan-filtered top-k), **0 failed** — zero regression. The refactor
(Vec-returning scans + params) is covered by the full v3/v4/v5/v6 suites.

## Results (SIFT1M, v5 index, filtered queries)

**Planner (EXPLAIN):** `Limit → Index Scan using sift_idx on sift, Order By: (e <-> q)` — the planner **picks the AM
index scan** for the filtered ordered query. ✓

| selectivity | probes | filtered recall@10 | QPS |
|---|---:|---:|---:|
| 10% (label=3) | 32 | **0.894** | 205.7 |
| 10% | 64 | 0.894 | 180.3 |
| 30% (label∈{1,2,3}) | 32 | **0.942** | 213.1 |
| 30% | 64 | 0.942 | 159.0 |

Build: v5 828.9s (lists=500).

## Findings

1. **Filtered ANN preserves recall** via the iterative scan: 0.894 @ 10% selectivity, 0.942 @ 30%. Without the fix,
   a selective `WHERE` collapses recall (candidates filtered out, AM returns `false`) — the pg_test proves the fix,
   the benchmark proves it at 1M scale.
2. **The planner picks the AM** for a filtered `ORDER BY <-> LIMIT` query (EXPLAIN). The v5/v6-aware startup-cost
   ratio drives the LIMIT-favors-index choice; Postgres's own `WHERE`-selectivity estimate prefers seqscan when the
   filter is very selective (pgvector-0.8 behavior).
3. Higher selectivity → higher filtered recall (more matching rows in the candidate pool).

## Honest caveats

- Filtered recall (0.89-0.94) is slightly below unfiltered v5 (~0.98 at probes=32/of=8, M84) — the AH prune + filter
  interaction; widening `over_fetch` raises it (at a QPS cost). This is **pgvector-relaxed-order class**, not AlloyDB
  **inline/adaptive filtering** (bitmap-in-traversal + runtime plan switching — a paradigm feature IVF-as-a-PG-AM
  cannot reach, same class as the ScaNN QPS ceiling, M73/ADR-0035).
- The iterative re-search re-scans probed lists each grow (re-reads already-emitted candidates, filtered by tid) —
  correct but O(N) per grow at high selectivity; pgvector's iterative has similar cost. A cursor that skips probed
  lists is a future optimization.
- Warm-cache SIFT1M; a secondary B-tree on the label would let Postgres pre-filter at high selectivity (orthogonal
  to this AM-side change).

## Verdict

**GO.** Filtered ANN (pgvector relaxed_order class) works over the storage-separated layout, and the planner picks
the AM correctly — the **class-AlloyDB-in-Postgres** bar for filtered vector search. Closes the M85-M87 goal scope.
NOT AlloyDB inline/adaptive filtering (paradigm gap); NOT a ScaNN-library beat (M73/ADR-0035). Next: M88 (billion-scale).

See also: `docs/benchmarks/m85-sq8-refine.md`, `docs/benchmarks/m84-recall-confirmation.md`, `.claude/knowledge-base/discoveries/blueprints/m52-filtered-ann-blueprint.md`.
