# M107 Phase-0 spike — native CSR + BFS vs recursive-CTE (D3 gate)

**Verdict: GO** (with a build-cost caveat that dictates the Phase-1 design). Native CSR adjacency + frontier BFS beats **both** recursive-CTE baselines on the reachable-set expansion — **169–738× vs the theo-rag `UNION ALL` baseline and 106–232× vs a fairer `UNION`-dedup baseline** (traverse-only) — with the reachable-set **correctness oracle PASSing on every trial against both baselines**.

## Question

Does native CSR+BFS traversal beat the recursive-CTE enough to justify building a native graph engine? (Measurement-first D3 gate — honest-negative would close the pillar.)

## Scope — what is (and is NOT) measured

This spike benchmarks the **reachable-set expansion** — from K seeds, all nodes within ≤H hops — which is the *dominant cost* of the theo-rag graph retriever's recursive CTE. It is **NOT** the full retriever query: the real `graph-retriever.ts` runs `reach` **then** a chunk-scoring tail (`edge_chunks` unnest + double join, `SUM(weight)` GROUP BY, join `chunks`/`documents`, ORDER BY + LIMIT). That tail is *additional* SQL work, so the true end-to-end retriever CTE cost is **higher** than the numbers here — i.e. this spike is **conservative** against itself. We isolate the traversal primitive on purpose (it is the engine-gate question).

## Method

- **Graph:** deterministic (seeded LCG), undirected, weighted, hub-y (25% of edges land on the top-1% "hub" nodes — the entity-graph regime where BFS frontiers grow). Same graph fed to native + both baselines.
- **Query:** from 5 seed nodes, ≤3 hops → the reachable set.
- **Native:** own-code Rust (release) — CSR build (vertex-offset + edge arrays) + frontier BFS with a visited bitset; build and traverse timed separately.
- **Baselines (both reproducible in the harness):** the SAME graph in PostgreSQL 17.10, `edges(src,dst,weight)` indexed on `src` + `dst`, `ANALYZE`d. (a) **`UNION ALL`** — the theo-rag shipped semantics (revisits). (b) **`UNION` (dedup)** — a fairer visited-tracking formulation. Both use the undirected `(src=node OR dst=node)` join + `WHERE hop<3`.
- **Oracle:** native reachable-set (count + Σ ids) MUST equal each CTE's (`count(DISTINCT node)`, `sum(DISTINCT node)`) on every trial. *(Note: count+Σ is not injective; for the spike — same graph/seeds/budget on both engines — a divergence preserving both is astronomically unlikely, and an independent set-hash re-check confirmed true set-identity. The Phase-1 engine's differential test should use a set-hash, not count+Σ.)*
- **Rigor:** 4 trials/scale (distinct RNG seeds → distinct graphs+seeds), mean ± population-std. Host: local docker `postgres:17-alpine` (PG 17.10) + native Rust release (shared dev box — build-ms std is loose, see caveats). Reproducible: `benchmarks/m107_graph_spike/` (`cargo build --release && python3 run_bench.py`).

## Results (4 trials/scale, oracle PASS on ALL vs BOTH baselines)

| Scale (edges) | Native build | Native **traverse** | Native total | CTE `UNION ALL` | CTE `UNION` (dedup) | **traverse vs UNION ALL** | **traverse vs dedup** | total vs UNION ALL |
|---|---|---|---|---|---|---|---|---|
| 100 000 | 1.18 ± 0.35 ms | **0.25 ± 0.07 ms** | 1.43 ms | 181.6 ± 40.8 ms | 55.2 ± 10.4 ms | **738× ± 94** | **232× ± 52** | 131× ± 32 |
| 1 000 000 | 31.47 ± 5.69 ms | **1.38 ± 0.43 ms** | 32.85 ms | 222.5 ± 39.0 ms | 139.2 ± 23.4 ms | **169× ± 29** | **106× ± 19** | 6.9× ± 1.5 |

The `UNION`-dedup baseline is materially faster than `UNION ALL` (55 vs 182 ms @100k; 139 vs 222 ms @1M) — dedup *does* help — but native traverse still wins **106–232×**. The dominant CTE cost is the per-iteration `(e.src=node OR e.dst=node)` bitmap-OR join, which CSR replaces with an O(1) offset + sequential neighbor scan; splitting the OR into two directed index scans was independently measured and does NOT materially narrow the gap. The conclusion survives the fair baseline.

## Honest caveats (Rule 3)

1. **CSR build cost dominates at scale.** At 1M the on-the-fly CSR build (31 ms) is ~23× the traverse (1.4 ms), collapsing the *end-to-end* win vs `UNION ALL` from 738× to 6.9×. This is exactly the DuckPGQ finding (on-the-fly CSR construction can dominate runtime). **Design implication (ADR-0048):** persist the CSR as an index-AM built once + maintained incrementally — then the operative number is the **traverse-only 106–232×**, not the rebuild-per-query 6.9×.
2. **Materiality depends on graph density + hop budget.** The *ratio* is robust (an independently-measured sparse avg-degree-2 graph still gave ~140× traverse), but in the low-density regime the *absolute* CTE cost is already single-digit ms — a real but not user-visible win. The win is material for hub-heavy entity graphs at ≥10⁵ edges / ≥2 hops; for tiny/sparse graphs the recursive CTE is "good enough" (the blueprint's honest-negative boundary).
3. **Reduced query, not the full retriever** (see § Scope) — the spike is conservative against itself.
4. **Weak-but-adequate oracle** (count+Σ, not injective) — set-identity independently confirmed; Phase-1 must use a set-hash.
5. **Loose build-ms std + 4 trials** — build-ms std is ~18–29% of mean (allocator/OS noise on a shared box); 4 trials is the floor. The traverse std is tight (±0.07–0.43 ms). A pillar-build milestone should use 8–10 trials to tighten the build number the ADR depends on.

## Verdict: **GO**

Native CSR+BFS traversal is **106–738× faster than the recursive-CTE** (both the shipped `UNION ALL` and a fairer `UNION`-dedup baseline) on the core GraphRAG op (bounded neighborhood expansion), correctness-proven (oracle PASS on all 8 trials against both baselines + an independent set-hash re-check). The build-dominates-at-1M result is not a counter-argument — it is the **evidence for persisting the CSR** (index-AM), the Phase-1 architecture decision. The native-graph-engine pillar is justified; the follow-on milestones are GO.

## Reproduction

```
cd benchmarks/m107_graph_spike
cargo build --release
python3 run_bench.py     # needs a local PostgreSQL 17 (here: docker theo-workspace-pg-cloud-1, db m107_spike)
# artifacts: docs/benchmarks/m107-graph-spike.{md,json}
```
