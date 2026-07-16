# M108 — Persisted-CSR: build-once, query-many (no per-query rebuild)

**Verdict: GATE MET.** The persisted CSR (built once, cached per backend) serves `graph_expand` at **16× the recursive-CTE** on warm queries (10× cold) — the M107 traverse win, now WITHOUT the per-query rebuild — with the reachable-set correctness oracle PASSing.

## What M108 closes

The M107 spike proved native CSR+BFS beats recursive-CTE 106–738× on traversal, but on-the-fly CSR *build* dominated at scale (build ≫ traverse → end-to-end collapsed to ~7×). M108 persists the CSR ONCE as a `bytea` in `theodb.graph_csr` — PostgreSQL makes that WAL-logged + crash-safe + MVCC natively (Rule 9 / KISS: no hand-rolled index-AM WAL) — and caches the deserialized CSR per backend (the M101 Arrow-cache pattern), keyed by the `built_at` epoch so `graph_refold` transparently invalidates it.

## Method

- **Graph:** ~200k edges / 40k nodes, deterministic hub-y (25% of dst land on the top-1% hub set via `hashint8`). Same graph for both engines.
- **Query:** 5 seeds, ≤3 hops → reachable set.
- **Native persisted:** `theodb.graph_build` once, then `theodb.graph_expand` — first query COLD (load bytea + deserialize + cache), subsequent WARM (cache hit → traverse only). Both include SPI round-trip overhead.
- **Baseline:** the SAME graph, the `UNION`-dedup recursive CTE (fairer than `UNION ALL`), indexed src+dst.
- **Oracle:** `graph_expand` reachable-set count == the CTE's `count(DISTINCT node)` — asserted every run.
- **Build profile:** the extension is built `--release` (fair vs the CTE's native C execution — a debug build would understate the Rust side). Host: DigitalOcean `s-4vcpu-8gb`, pgrx 0.19.0 / PostgreSQL 17.10.

## Results (release, edges=200000, reached=27752, oracle PASS)

| Metric | Value | vs recursive-CTE |
|---|---|---|
| `graph_build` (once) | 273.8 ms | — (amortized over all queries) |
| `graph_expand` **cold** (first: load+deserialize+cache) | 26.4 ms | **10.0×** |
| `graph_expand` **warm** (cache hit → traverse) | **16.5 ms** | **16.0×** |
| recursive-CTE (`UNION` dedup) per query | 263.4 ms | 1× (baseline) |

*(A debug build measures warm ≈ 36 ms → 8.2× — reported for completeness; release is the representative number since the CTE runs in native C either way.)*

## Honest caveats (Rule 3)

1. **SPI overhead in the number.** The 16.5 ms warm figure includes the `built_at` freshness check + the `count(*)` SETOF materialization over SPI; the pure in-memory traverse is sub-ms (M107). So 16× is a *conservative floor* for the cached path — a direct in-engine operator (M109) removes the SPI round-trips.
2. **Cold first query.** The first query per backend pays the one-time load+deserialize (26 ms); the cache makes queries 2..N warm. A long-lived backend amortizes the cold cost immediately.
3. **`bytea` durability, not a custom index-AM.** M108 takes the ADR-1 "estrutura CSR sobre a edge-table" branch: the CSR is a WAL-logged `bytea`, so crash-safety is PostgreSQL's own (an aborted `graph_build` leaves no row; a committed one survives replay by construction — MVCC). A full auto-maintained index-AM (aminsert hooks) is a documented refinement; `graph_refold` is the explicit fold-on-demand.
4. **Deserialize-per-cold-query** is the residual cost the cache mitigates; the M109 in-engine operator (reading CSR pages directly, no full-deserialize) is the next step.

## Verdict: **GATE MET**

The persisted + cached CSR serves traversal **10–16× faster than the recursive-CTE**, correctness-proven (oracle PASS), WITHOUT the per-query rebuild that capped M107's end-to-end win — the build (274 ms) is paid once and amortized. M108's gate ("persisted CSR preserves the win without per-query build, crash-safe") is met.

## Reproduction

`cargo pgrx test pg17 m108 --release` runs `m108_bench_persisted_vs_cte`, which builds the graph, times cold/warm `graph_expand` vs the CTE, asserts the oracle, and writes the `M108_BENCH` line. 4 M108 pg_tests + the full 328-test suite GREEN (0 regression).
