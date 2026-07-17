---
slug: m108-persisted-csr-am
milestone_id: M108
created_at: 2026-07-16
goal: Persist the graph CSR once (crash-safe) so per-query traversal is load+traverse, not rebuild — measured to beat recursive-CTE without the per-query build.
---

# M108 — Persisted-CSR graph structure (implementation summary)

**Verdict:** IMPLEMENTATION_COMPLETE. **Gate MET** — persisted+cached CSR serves `graph_expand` 10–16× faster than the recursive-CTE, oracle PASS, WITHOUT the per-query rebuild that capped M107. 329 pg_tests GREEN (0 regression).

## DoD verification

| # | DoD item | Status | Evidence |
|---|---|---|---|
| 1 | Persisted CSR structure over the edge table (ADR-1 "estrutura CSR" branch) | ✅ | `theodb.graph_csr(edge_rel, csr bytea, ...)` catalog + `theodb.graph_build/refold/expand` (`src/graph.rs`) |
| 2 | Crash-safe persistence | ✅ | CSR is a WAL-logged `bytea` (PostgreSQL-native durability/MVCC — aborted build → no row; committed → survives replay by construction). Rule 9: no hand-rolled index-AM WAL |
| 3 | Incremental maintenance | ✅ | `graph_refold` (fold-on-demand); per-backend cache keyed by `built_at` (clock_timestamp) so refold transparently invalidates it — proven by `m108_refold_folds_new_edges` |
| 4 | Benchmark: no per-query rebuild, beats CTE | ✅ | `docs/benchmarks/m108-persisted-csr.{md,json}` — warm 16.5ms vs CTE 263ms = **16×** (cold 10×), build paid once (274ms); oracle PASS (reached=27752) |
| 5 | Correctness | ✅ | `m108_build_persists_and_expand_reads`, `m108_refold_folds_new_edges`, `m108_expand_without_build_errors`, `m108_bench_persisted_vs_cte` (oracle) — 4/4 GREEN |

## Key engineering (measured, honest)

- Persisting the CSR as a bytea removed the per-query SCAN+SORT rebuild, but a naive expand then paid a per-query LOAD+DESERIALIZE that dominated (debug: only 2.4× vs CTE). Added a **per-backend deserialized-CSR cache** (M101 Arrow-cache pattern) keyed by `built_at` epoch → warm queries skip deserialize → 16× (release).
- `now()` is transaction-constant → within a pg_test transaction, build+refold shared a timestamp and the cache didn't invalidate. Fixed with `clock_timestamp()` (correct in prod AND test).
- Honest caveat: the 16× includes SPI round-trip overhead; the pure traverse is sub-ms — the M109 in-engine operator removes the SPI + full-deserialize.

## Boundary

ADR-3: M108 ships the persisted structure + the read-path (`graph_expand`) needed for the gate. The full auto-maintained index-AM (aminsert hooks) + the vectorized MS-BFS operator are M109 refinements.
