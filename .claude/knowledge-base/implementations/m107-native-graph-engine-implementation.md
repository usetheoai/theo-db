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

## Measured evidence (the gate)

| Scale | Native traverse | Native total | Recursive-CTE | Speedup traverse | Speedup total | Oracle |
|---|---|---|---|---|---|---|
| 100k edges | 0.27 ms ± 0.04 | 1.89 ms | 190.9 ms ± 32.8 | **732× ± 167** | 108× ± 37 | PASS 4/4 |
| 1M edges | 1.08 ms ± 0.30 | 39.27 ms | 283.1 ms ± 90.5 | **262× ± 38** | 8.3× ± 3.5 | PASS 4/4 |

Fairer `UNION`-dedup CTE (182 ms @1M) still ~170× slower than native traverse — no strawman.

## Key finding shaping Phase 1

On-the-fly CSR build dominates at 1M (38 ms build vs 1 ms traverse → end-to-end win collapses to 8.3×) — the DuckPGQ result. **Phase 1 MUST persist the CSR** (index-AM, built once + incremental) so the operative number is the traverse-only 262×. Recorded as ADR-0048's design-shaping consequence + follow-on milestone #1.

## Boundary honesty

This is **Phase 0** — a discovery + measurement gate, NOT the engine. It ships a blueprint + spike + GO verdict + ADR. The engine (persisted-CSR index-AM, MS-BFS operator, SQL/PGQ surface, vector-on-nodes, PPR) are follow-on milestones authorized by the GO. Graph *quality* (extraction) is a separate eval.

## Artifacts

- Blueprint: `knowledge-base/discoveries/blueprints/m107-native-graph-engine-blueprint.md`
- Spike: `benchmarks/m107_graph_spike/{Cargo.toml,src/main.rs,run_bench.py}`
- Benchmark: `docs/benchmarks/m107-graph-spike.{md,json}`
- ADR: `docs/adr/0048-m107-native-graph-engine-go.md`
