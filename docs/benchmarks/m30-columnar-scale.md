# M30 — columnar-at-scale: pg_mooncake columnstore vs row-store (the crossover)

**Substrate:** mooncakelabs/pg_mooncake (PG18, MIT) — measurement substrate — shipping columnar on the PG17 TheoDB image is the gated adoption step.  
**Query:** `SELECT category, count(*), avg(amount) FROM t GROUP BY category (analytics rollup)`.  
**Type:** measurement — fills the large-scale gap the M6 benchmark (`docs/benchmarks/m6-columnar-vs-row.md`) marked UNBENCHMARKED (it measured only 100k, where the row-store won). Grounds the M30 KEEP-columnar decision (ADR 0013).

| rows (n) | row-store ms (Seq Scan) | columnstore ms (DuckDBScan) | speedup | correct? |
|---|---|---|---|---|
| 100,000 | 9.2 | 4.0 (DuckDBScan) | 2.33× | yes |
| 1,000,000 | 62.3 | 7.2 (DuckDBScan) | 8.65× | yes |
| 5,000,000 | 397.4 | 26.6 (DuckDBScan) | 14.94× | yes |

## Verdict

**Crossover: columnar beats the row-store from n = 100,000.** Correctness (`match`) holds at every point (count exact + per-group avg within 1e-3 cross-engine tolerance). The columnstore plans as `DuckDBScan` (vectorized), the row-store as `Seq Scan`.

## Honest caveats
- Substrate is `mooncakelabs/pg_mooncake` (**PG18**); the shipped TheoDB image is **PG17**. This proves the columnar CAPABILITY + the crossover; shipping columnar on PG17 is the gated adoption step (fix the from-source build OR bump to PG18) — a separate milestone.
- Synthetic 5-category group-by (analytics/observability-style rollup); real workloads vary.
- The columnstore is a DuckDB+Iceberg lakehouse on disk (ADR 0002 D2 — NOT AlloyDB in-memory).

## Reproduce
```
docker run -d --name mooncake -p <port>:5432 -e POSTGRES_PASSWORD=postgres mooncakelabs/pg_mooncake:latest
python3 benchmarks/run_m30_columnar_scale.py --port <port> --write-doc
```
