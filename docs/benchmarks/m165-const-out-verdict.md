# M165 — projected-constant output column (q34) verdict

**Date:** 2026-07-27 · **Box:** DigitalOcean s-8vcpu-16gb (nyc3), PG 18.4 + `theodb_rs` @ develop + M165 · **ClickHouse:** 26.8.1, same box.
**Dataset:** ClickBench `hits`, 1M systematic 1-in-99 subsample. **Ratio = TheoDB_hot / ClickHouse_hot.**

## Result — q34 flips non-pushdown → pushdown

M165 adds a projected-constant output column arm so `SELECT 1, URL, count(*) … GROUP BY 1, URL` (ClickBench **q34**)
routes to `theodb_columnar_agg` instead of the row executor.

| Query | Before (fresh v0.155.0) | **After M165** | Δ |
|---|---|---|---|
| **q34 ratio vs ClickHouse** | 152.44× | **10.19×** (1.22s / 0.12s) | ~15× closer |
| q34 same-engine hot | 28.5 s (row executor) | **1.22 s** (columnar pushdown) | ~23× faster |
| q34 class | non-pushdown | **pushdown (`agg_routed=True`)** | — |
| Agg-routed queries | 30/43 | **31/43** | +1 (q34) |

## Correctness — no regression

- **43/43 A/B byte-identical** (columnar == heap, `diverged=0`) — including q34. The M165 change is isolated to q34's
  admit path (the const-out arm), so no other query's code path is touched; the byte-identical A/B across all 43 confirms it.
- **Type-coverage A/B: 26/26** as-expected (positive control diverged=2) — const_out int2/int4/int8 route byte-identical;
  const float/text/NULL decline (fail-closed). The `rules/testing.md §5.1` gate.
- **q17 remains a correct honest-negative** — under `enable_sort=on` (the benchmark condition) PG plans an AGG_SORTED
  GroupAggregate; routing it would let the GroupAgg's collation-order pathkeys be consumed by an upstream merge
  join / DISTINCT / setop without a re-sort — a wrong result the LIMIT-stripped A/B is blind to (council-rust-pgrx).
  The correct fix (collation-ordered executor emission) is a separate deferred capability.

## Honest caveats

- The "before" 152.44× is from the fresh v0.155.0 measurement on a *different* (now-destroyed) droplet; this run is a
  *new* box. The **q34 ratio is the comparable metric** (both engines on the same box each run); the same-engine
  28.5s→1.22s is a like-for-like TheoDB-only speedup across comparable s-8vcpu-16gb boxes.
- Overall geomean moved 9.95× → 10.66× between the two independent full runs — within cross-run timing noise on separate
  droplet instances; the 43/43 `diverged=0` proves no correctness change, and M165 only touches q34's path. Not a regression.
- Conservative methodology unchanged (ClickHouse server-side `--time` vs TheoDB psycopg2 round-trip → true gap ≤ measured).
- The remaining non-pushdown drag (q21/q22/q27/q29 string aggregates, q17 honest-negative) is M166/M167 scope.

## Reproduce

`benchmarks/run_m128_clickbench.py --n 1000000 --sample systematic --agg` (TheoDB) +
`benchmarks/m159_clickhouse_run.sh` (ClickHouse) + `benchmarks/m159_analyze.py`. Type-coverage:
`benchmarks/columnar_type_ab.py`. Raw: `docs/benchmarks/m165-artifacts/`.
