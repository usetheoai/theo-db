# Discover-Confidence — m6-columnar-htap

**Date:** 2026-06-28 · **Verdict:** SHIPPABLE_WITH_CAVEATS (89)
**Blueprint:** .claude/knowledge-base/discoveries/blueprints/m6-columnar-htap-blueprint.md

## Key findings (M6)
- **pg_mooncake** (MIT) columnstore-mirror over live Postgres tables (DuckDB+Iceberg lakehouse); `CALL mooncake.create_table('mirror','base')` + query the mirror. **Risk #1 RESOLVED:** Makefile lists pg14–pg18 → PG17 supported (verbatim). `requires pg_duckdb`.
- **DoD-2 plan choice (live evidence):** columnstore-mirror query → `Custom Scan (DuckDBScan)`; row table → `Seq Scan` (EXPLAIN). Observable.
- **Capability proven live** on the canonical distribution (official image PG18): `avg(price) FROM trades_iceberg` = 208.5, matches row-store.
- **Build cost:** heavy (Rust+pgrx+DuckDB+pg_duckdb); no pg17 prebuilt .so → source build. pgduckdb:17-main exists as a PG17 base.
- **D2 honesty:** DuckDB+Iceberg lakehouse on disk, NOT AlloyDB's in-memory columnar — competitive-different bet (ADR 0002), not a copy.

## Recommendation (measurement-first, cf. BM25 S2)
Prove + measure the columnar capability (DuckDBScan vs SeqScan EXPLAIN + analytical query vs row-store); gate the heavy PG17 source-build embedding into theo-db:dev on the measurement. UNBENCHMARKED until TheoDB measures; AlloyDB in-memory framing from the allowlisted GCloud blog + ADR 0002 (docs page redirected off-allowlist).
