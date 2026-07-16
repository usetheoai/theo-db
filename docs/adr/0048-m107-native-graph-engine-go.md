# ADR-0048: Native graph engine (CSR + vectorized BFS + SQL/PGQ) over the columnar+vector substrate — D3 gate = GO

- **Status:** accepted
- **Date:** 2026-07-16
- **Deciders:** graph-pillar owner (M107 Phase 0)
- **Relates:** M107 blueprint (`knowledge-base/discoveries/blueprints/m107-native-graph-engine-blueprint.md`), the spike (`docs/benchmarks/m107-graph-spike.{md,json}`), the columnar pillar (ADR-0042/0044, M99–M103), the vector pillar, `../../CLAUDE.md § North Star` (AI-native)
- **Tags:** graph, graphrag, architecture, measurement-first, d3-gate

> M107 Phase 0 is the measurement-first gate for a native graph pillar (mirrors the vector pillar's M75 spike gating M76–M82). This ADR records the architecture decision + the D3 verdict backed by the spike.

## Context

Graph is a recurring, cross-system, frequently-used capability (not YAGNI), and TheoDB's mandate is an AI-native database. The SOTA convergence (DuckPGQ CIDR/VLDB 2023, Kùzu CIDR 2023) is that efficient graph+vector lives in ONE engine pairing **columnar storage + CSR adjacency + vectorized traversal (MS-BFS) + WCOJ/factorization**, exposed via **SQL/PGQ (SQL:2023)**. TheoDB already owns three of the four ingredients (columnar DataFusion/Arrow M99–M103; own vector AM + SIMD kernels; `ai.*`). The missing piece is native graph traversal. The theo-rag GraphRAG today runs on **recursive-CTE over relational tables** — the baseline to beat.

## Decision

**Build a native graph engine as own-code CSR adjacency + vectorized frontier/MS-BFS traversal operators FUSED with the existing columnar + vector substrate** — adopting the DuckPGQ/Kùzu *techniques* (public papers), NOT vendoring their engines, NOT using recursive-CTE, NOT using Apache AGE. **D3 verdict: GO** — the follow-on engine milestones are authorized.

## Evidence (the D3 gate)

Reproducible spike (`docs/benchmarks/m107-graph-spike.md`, 4 trials/scale, oracle PASS on all 8):

| Scale | Native traverse | Recursive-CTE | Speedup (traverse) | Speedup (end-to-end) |
|---|---|---|---|---|
| 100k edges | 0.27 ms | 190.9 ms | **732×** | 108× |
| 1M edges | 1.08 ms | 283.1 ms | **262×** | 8.3× |

The reachable-set correctness oracle (count + checksum) matched the CTE on every trial. The fairer `UNION`-dedup CTE (181.9 ms @1M) is still ~170× slower than native traverse — the conclusion survives a non-strawman baseline.

## Alternatives considered

- **Recursive-CTE** (the theo-rag baseline) — per-hop `(src=node OR dst=node)` bitmap-OR join + intermediate blow-up. **Rejected:** 170–732× slower than native on the core op (measured).
- **Apache AGE** (Apache-2.0, passes D1) — compiles Cypher to recursive relational joins over `agtype` tables → the SAME per-hop-join tax, plus a separate query silo, plus no managed-service support (RDS/Aurora). **Rejected on architecture, not license.**
- **Bundle Kùzu/DuckPGQ** (MIT) — excellent engines, but SEPARATE single-node stores; bundling forks the storage substrate away from TheoDB's columnar+vector. **Rejected:** Rule 9 = reuse OUR engine; GRFusion (arXiv 1709.06715) shows native traversal operators OVER existing storage beat native graph DBs — the gap is the execution model, not storage.

## Consequences

- **Positive:** a native graph pillar reusing the columnar+vector+SIMD investment; the GraphRAG flow (vector-entry → bounded traversal → rerank/PPR, LazyGraphRAG/HippoRAG) runs zero-copy in one engine — the AI-native win. theo-rag (and other systems) stop reimplementing traversal.
- **Design-shaping caveat (from the spike):** on-the-fly CSR build dominates at 1M (38 ms build vs 1 ms traverse → end-to-end win collapses to 8.3×). **Therefore Phase 1 MUST persist the CSR** as an index-AM (built once, maintained incrementally on VACUUM/insert) so the operative number is the traverse-only 262×.
- **Scope:** Phase 0 proved the traversal primitive. The engine phases (persisted-CSR index-AM → vectorized MS-BFS operator → `theodb.graph_expand`/SQL/PGQ surface → vector-on-nodes → PPR/community) are follow-on milestones, each with its own gate. Graph *quality* (extraction) is a separate eval, not solved by the engine.

## Follow-on milestones (proposed, gated on this GO)

1. Persisted CSR adjacency index-AM over an edge table (built once, incremental maintenance).
2. Vectorized MS-BFS traversal operator reusing the vector-pillar SIMD kernels.
3. `theodb.graph_expand(seeds, max_hops)` + `ai.extract_graph(text)` surface (pragmatic, pre-SQL/PGQ).
4. Vector index on graph nodes + the vector-entry→bounded-traversal→rerank retrieval flow.
5. (later) SQL/PGQ parser surface; PPR / community summarization if a measured eval justifies it.
