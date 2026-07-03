# M6 — Columnar (pg_mooncake) vs row-store, measured

> **Measured, not asserted** (CLAUDE.md TheoDB rule 5 / `rules/public-copy.md`). Numbers from an actual run of
> the columnar harness against a live **pg_mooncake** columnstore mirror. This is the **measurement-first gate**
> (ADR 0002) informing whether to embed pg_mooncake into the shipped TheoDB image.

## What is measured

`benchmarks/theodb_bench/columnar.py::run_columnar_vs_row` seeds a row table `metrics(id, category, amount)`
with 100,000 rows (5 categories), creates a pg_mooncake **columnstore mirror** (`CALL mooncake.create_table`),
and runs the same scan-heavy aggregate on both:

```sql
SELECT category, count(*), round(avg(amount)::numeric,4) FROM <metrics | metrics_cs> GROUP BY category ORDER BY category;
```

## Substrate (honest)

The measurement runs on the **canonical pg_mooncake distribution** (`mooncakelabs/pg_mooncake`, MIT — wrapped
as the throwaway `packaging/Dockerfile.columnar`), which ships PostgreSQL **18** + pg_duckdb + pg_mooncake.
TheoDB ships PostgreSQL **17**; pg_mooncake supports pg17 (its Makefile lists pg14–18), but ships **no pg17
prebuilt artifact**, so a PG17 build is from-source (Rust + cargo-pgrx + DuckDB). A from-source PG17 build was
attempted and **failed at the then-current upstream HEAD on a pinned-rustc MSRV mismatch**
(`error: rustc 1.88.0 is not supported by the following package`) — a resolvable toolchain issue,
not a capability gap. Per measurement-first, embedding pg_mooncake into the shipped PG17 image is the **gated
adoption step**; the capability + plan choice are proven here on the canonical distribution.

## Results (run 2026-06-28, mooncakelabs/pg_mooncake, 100k rows)

| Path | Plan (EXPLAIN) | Aggregate latency |
|---|---|---|
| row-store (`metrics`) | `Sort → HashAggregate → Seq Scan` | **10.9 ms** |
| columnstore mirror (`metrics_cs`) | **`Custom Scan (DuckDBScan)`** (DuckDB vectorized) | 44.3 ms |

**Correctness:** the columnstore mirror aggregate **equals** the row-store aggregate, group-for-group
(`match = True`; e.g. `cat0 → count 20000, avg 746.25`). The mirror stays in sync with the base.

## Honest reading

- **DoD-2 (row-vs-columnar plan choice) is proven:** the mirror query plans as `Custom Scan (DuckDBScan)` —
  executed by DuckDB's vectorized columnar engine — while the row query uses a heap `Seq Scan`. This is the
  load-bearing, observable plan-choice evidence, independent of timing.
- **The columnar path is NOT faster at this scale (100k rows, 5 groups): row-store 10.9 ms vs columnar 44.3 ms.**
  This is the expected regime: DuckDB's columnar/vectorized advantage materializes on **large, wide, scan-heavy**
  workloads (millions of rows, many columns, selective projections), where a heap Seq Scan + sort dominates;
  at 100k narrow rows the cross-engine + DuckDB scan setup overhead exceeds the heap aggregate. We report the
  measured numbers honestly (Rule 5) — **no columnar speed win is claimed at this scale.** The large-scale win
  (e.g. ClickBench) is upstream's published result, **not reproduced here → UNBENCHMARKED at TheoDB scale.**
- **Sync overhead (risk #2): UNBENCHMARKED** — the mirror auto-syncs from the base; its sync cost is not measured here.

## Decision status (measurement-first)

| Question | Status |
|---|---|
| Permissive columnar piece (MIT)? | ✅ pg_mooncake (MIT) + pg_duckdb (MIT) — license-sweep § (e) |
| Columnar capability functional on a real engine? | ✅ columnstore mirror created; mirror == row (correctness) |
| Row-vs-columnar plan choice (DoD-2)? | ✅ DuckDBScan (mirror) vs Seq Scan (row) — EXPLAIN |
| Columnar speed win measured? | ❌ not at 100k (row-store faster); large-scale win UNBENCHMARKED |
| PG17 support? | ✅ supported (Makefile pg14–18); from-source PG17 build blocked on a pinned-rustc MSRV (resolvable) |
| Embed pg_mooncake in the shipped image? | ⏳ gated — future ADR, pending the PG17 build fix + a large-scale measurement |
| Honesty (D2)? | ✅ DuckDB+Iceberg lakehouse on disk, NOT AlloyDB in-memory |

## Reproduce

```bash
docker build -f packaging/Dockerfile.columnar -t theo-db-columnar .
docker run -d --name col -e POSTGRES_PASSWORD=postgres -p 5432:5432 theo-db-columnar   # wait for ready
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  python3 -c "from theodb_bench.db import VectorDB; from theodb_bench.columnar import run_columnar_vs_row; \
d=VectorDB('host=localhost port=5432 dbname=postgres user=postgres password=postgres').connect(); d.ping(); \
r=run_columnar_vs_row(d,100000); print('match',r['match'],'row_ms',round(r['row']['ms'],1),'col_ms',round(r['columnar']['ms'],1))"
```
