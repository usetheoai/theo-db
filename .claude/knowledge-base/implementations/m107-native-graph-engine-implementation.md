---
slug: m107-native-graph-engine
milestone_id: M107
created_at: 2026-07-16
goal: Phase-0 gate of the native graph pillar — SOTA blueprint + reproducible spike proving native CSR+BFS beats recursive-CTE, with a D3 GO/honest-partial/honest-negative verdict.
---

# M107 — Native graph engine Phase 0 (implementation summary)

**Verdict:** IMPLEMENTATION_COMPLETE. **D3 gate = GO** — native CSR+BFS beats the theo-rag recursive-CTE baseline by **262–732× on traversal** (8–108× end-to-end), correctness-oracle PASS on all 8 trials.

## DoD verification (5/5)

| # | DoD item | Status | Evidence |
|---|---|---|---|
| 1 | SOTA-anchored blueprint (≥2 web sources/technique, 4 coverage corners, ADRs) | ✅ | `knowledge-base/discoveries/blueprints/m107-native-graph-engine-blueprint.md` — DuckPGQ, Kùzu, WCOJ, GRFusion, SQL/PGQ, GraphRAG/LazyGraphRAG/HippoRAG (all cited with URLs) |
| 2 | Reproducible own-code spike + benchmark | ✅ | `benchmarks/m107_graph_spike/` (zero-dep Rust CSR+BFS + `run_bench.py` driver) → `docs/benchmarks/m107-graph-spike.{md,json}` (4 trials/scale, mean±std, oracle) |
| 3 | Explicit D3 verdict | ✅ | **GO** — in the benchmark MD + ADR-0048 |
| 4 | ADR of the architecture decision | ✅ | `docs/adr/0048-m107-native-graph-engine-go.md` (native-over-columnar vs Apache AGE vs recursive-CTE; license note) |
| 5 | Rule-9 reuse discipline | ✅ | spike is own-code (no new deps); blueprint mandates reuse of columnar M99–M103 + vector AM + SIMD kernels — no columnar reimplementation, no PG rewrite |

## Measured evidence (the gate — both baselines reproducible, oracle PASS on all 8)

| Scale | Native traverse | CTE `UNION ALL` | CTE `UNION` dedup | traverse vs UNION ALL | traverse vs dedup | total vs UNION ALL |
|---|---|---|---|---|---|---|
| 100k | 0.25 ± 0.07 ms | 181.6 ± 40.8 ms | 55.2 ± 10.4 ms | **738× ± 94** | **232× ± 52** | 131× ± 32 |
| 1M | 1.38 ± 0.43 ms | 222.5 ± 39.0 ms | 139.2 ± 23.4 ms | **169× ± 29** | **106× ± 19** | 6.9× ± 1.5 |

Both baselines (theo-rag `UNION ALL` + fairer `UNION`-dedup) are in the harness with mean±std; native traverse wins 106–232× even vs the fairer baseline — no strawman. The spike isolates the reachable-set expansion (dominant CTE cost), not the full retriever tail (conservative).

## Key finding shaping Phase 1

On-the-fly CSR build dominates at 1M (38 ms build vs 1 ms traverse → end-to-end win collapses to 8.3×) — the DuckPGQ result. **Phase 1 MUST persist the CSR** (index-AM, built once + incremental) so the operative number is the traverse-only 262×. Recorded as ADR-0048's design-shaping consequence + follow-on milestone #1.

## Boundary honesty

This is **Phase 0** — a discovery + measurement gate, NOT the engine. It ships a blueprint + spike + GO verdict + ADR. The engine (persisted-CSR index-AM, MS-BFS operator, SQL/PGQ surface, vector-on-nodes, PPR) are follow-on milestones authorized by the GO. Graph *quality* (extraction) is a separate eval.

## Artifacts

- Blueprint: `knowledge-base/discoveries/blueprints/m107-native-graph-engine-blueprint.md`
- Spike: `benchmarks/m107_graph_spike/{Cargo.toml,src/main.rs,run_bench.py}`
- Benchmark: `docs/benchmarks/m107-graph-spike.{md,json}`
- ADR: `docs/adr/0048-m107-native-graph-engine-go.md`
