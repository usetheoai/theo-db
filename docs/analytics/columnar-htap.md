# Columnar / HTAP analytics on TheoDB (pg_mooncake) — M6

TheoDB's analytics pillar uses **pg_mooncake** (MIT, by Mooncake Labs): a Postgres extension that keeps a
**columnstore mirror** of your row tables in **DuckDB + Apache Iceberg**, so analytical queries run on a
vectorized columnar engine while your transactional tables stay row-store. This is **HTAP** — fast analytics
over live transactional data.

> No performance number on this page is unbenchmarked: measured results live in
> `docs/benchmarks/m6-columnar-vs-row.md` (CLAUDE.md TheoDB rule 5).

## Honesty: lakehouse on disk, NOT in-memory (PRD D2)

TheoDB's columnar is a **DuckDB + Iceberg lakehouse on disk** — a deliberate, competitive-**different** bet from
AlloyDB's **in-memory** columnar engine. The in-memory columnar peers in the Postgres ecosystem (Citus columnar,
Hydra) are **AGPL-licensed → barred by D1** (`docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`). The
lakehouse trade-off: data is on disk + Iceberg-interoperable (other engines can read it), not RAM-resident. We
do **not** claim AlloyDB in-memory parity.

## Enable + use

```sql
CREATE EXTENSION pg_mooncake CASCADE;          -- pulls pg_duckdb; needs shared_preload_libraries='pg_duckdb,pg_mooncake' + wal_level=logical
CREATE TABLE trades(id bigint PRIMARY KEY, symbol text, time timestamp, price real);
CALL mooncake.create_table('trades_iceberg', 'trades');   -- columnstore mirror, auto-synced
INSERT INTO trades VALUES (1,'AMZN','2024-06-05 10:05',207), (2,'AMZN','2024-06-05 10:15',210);
SELECT avg(price) FROM trades_iceberg WHERE symbol='AMZN';  -- runs in DuckDB's columnar engine
```

## Row vs columnar plan choice (DoD-2)

The planner routes a query on the columnstore mirror through pg_duckdb's `Custom Scan (DuckDBScan)` (DuckDB
vectorized columnar execution); the same query on the row table uses a heap `Seq Scan`. Choose the path by
querying the row table (transactional, point lookups / writes) or the mirror (analytical scans):

```
EXPLAIN SELECT category, avg(amount) FROM metrics_cs GROUP BY category;  -- Custom Scan (DuckDBScan)
EXPLAIN SELECT category, avg(amount) FROM metrics    GROUP BY category;  -- Seq Scan + HashAggregate
```

## When columnar wins (measured honesty)

DuckDB's columnar/vectorized advantage shows on **large, wide, scan-heavy** aggregates (millions of rows, many
columns). On small/narrow data the row-store can be faster (the cross-engine + scan-setup overhead dominates) —
see the measured 100k-row result in `docs/benchmarks/m6-columnar-vs-row.md` (row-store faster at that scale).
Use the columnstore mirror for big analytical scans, the row table for OLTP.

## Status (measurement-first)

The columnar capability + the row-vs-columnar plan choice are proven on the canonical pg_mooncake distribution
(MIT). Embedding pg_mooncake into the shipped TheoDB PG17 image is the **gated adoption step** — pg_mooncake
supports pg17, but the from-source PG17 build is heavy (Rust+pgrx+DuckDB) and currently blocked on a pinned-rustc
MSRV at upstream HEAD (`packaging/Dockerfile.columnar-pg17probe`; recorded in the benchmark report). Adoption is
a future ADR gated on the PG17 build fix + a large-scale measurement.
