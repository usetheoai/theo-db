# M107 Phase-0 spike — native CSR + BFS vs recursive-CTE (D3 gate)

**Verdict: GO** (with a build-cost caveat that dictates the Phase-1 design). Native CSR adjacency + frontier BFS beats the theo-rag recursive-CTE baseline by **262–732× on traversal** and **8–108× end-to-end**, with the reachable-set **correctness oracle PASSing on every trial**.

## Question

Does native CSR+BFS traversal beat the recursive-CTE baseline (theo-rag `graph-retriever.ts`) enough to justify building a native graph engine? (Measurement-first D3 gate — honest-negative would close the pillar.)

## Method

- **Graph:** deterministic (seeded LCG), undirected, weighted, hub-y (25% of edges land on the top-1% "hub" nodes — the entity-graph regime where BFS frontiers grow). Same graph fed to BOTH engines.
- **Query:** from 5 seed nodes, expand ≤3 hops → the **reachable set** (theo-rag semantics).
- **Native:** own-code Rust (release) — CSR build (vertex-offset + edge arrays) + frontier BFS with a visited bitset. Build and traverse timed separately.
- **Baseline:** the SAME graph in PostgreSQL 17.10, `edges(src,dst,weight)` indexed on `src` and `dst`, `ANALYZE`d, the theo-rag `WITH RECURSIVE … UNION ALL … WHERE hop<3` + `count(DISTINCT)`.
- **Oracle:** native reachable-set (count + Σ ids) MUST equal the CTE's (`count(DISTINCT node)`, `sum(DISTINCT node)`) — a faster-but-wrong traversal is worthless.
- **Rigor:** 4 trials/scale (distinct RNG seeds → distinct graphs+seeds), mean ± std (population). Host: local docker `postgres:17-alpine` (PG 17.10) + native Rust release. Reproducible: `benchmarks/m107_graph_spike/` (`cargo build --release && python3 run_bench.py`).

## Results

| Scale (edges) | Native build (ms) | Native traverse (ms) | Native total (ms) | Recursive-CTE (ms) | **Speedup traverse** | **Speedup total** | Oracle |
|---|---|---|---|---|---|---|---|
| 100 000 | 1.63 ± 0.43 | **0.27 ± 0.04** | 1.89 ± 0.47 | 190.9 ± 32.8 | **732× ± 167** | **108× ± 37** | PASS (4/4) |
| 1 000 000 | 38.19 ± 16.28 | **1.08 ± 0.30** | 39.27 ± 16.32 | 283.1 ± 90.5 | **262× ± 38** | **8.3× ± 3.5** | PASS (4/4) |

**Fairness check (not strawmanning the baseline):** the theo-rag baseline uses `UNION ALL` (revisits). A *fairer* `UNION` (dedup = visited-tracking) recursive CTE at 1M measured **181.9 ms** (vs 288.8 ms `UNION ALL`, same graph/seeds, reached=37 899, oracle PASS) — the dedup helps, but native traverse (1.06 ms) is still **~170×** faster. The dominant CTE cost is the per-iteration `(e.src=r.node OR e.dst=r.node)` bitmap-OR join, which CSR replaces with an O(1) offset + sequential neighbor scan — so the conclusion survives the fairer baseline.

## Honest caveats (Rule 3)

1. **CSR build cost dominates at scale.** At 1M the on-the-fly CSR build (38 ms) is 35× the traverse (1.1 ms), collapsing the *end-to-end* win from 732× to 8.3×. This is exactly the DuckPGQ finding (on-the-fly CSR construction can dominate runtime). **Design implication (Phase-1 ADR):** persist the CSR as an index-AM built once + maintained incrementally — then the operative number is the **traverse-only 262×**, not the rebuild-per-query 8.3×.
2. **Single-node, synthetic graph.** The spike measures the traversal primitive on a representative synthetic graph, not a real GraphRAG entity corpus or a distributed setting. It proves the traversal-engine thesis, not end-to-end GraphRAG quality (which depends on extraction — a separate eval).
3. **The baseline is a real one.** The recursive-CTE is theo-rag's shipped code; the fairer `UNION` variant was also tested. No strawman.

## Verdict: **GO**

Native CSR+BFS traversal is **170–732× faster than the recursive-CTE** on the core GraphRAG op (bounded neighborhood expansion), correctness-proven (oracle PASS on all 8 trials + the fairness check). The build-dominates-at-1M result is not a counter-argument — it is the **evidence for persisting the CSR** (index-AM), which is the Phase-1 architecture decision. The native-graph-engine pillar is justified; the follow-on milestones (persisted-CSR index-AM, vectorized MS-BFS operator, SQL/PGQ surface, vector-on-nodes, PPR) are GO.

## Reproduction

```
cd benchmarks/m107_graph_spike
cargo build --release
python3 run_bench.py     # needs a local PostgreSQL 17 (here: docker theo-workspace-pg-cloud-1, db m107_spike)
# artifacts: docs/benchmarks/m107-graph-spike.{md,json}
```
