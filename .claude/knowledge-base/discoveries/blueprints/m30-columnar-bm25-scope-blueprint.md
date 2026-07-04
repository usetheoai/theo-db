# Blueprint — M30: v1-legacy scope decision (columnar M6 + BM25 M7)

**Slug:** m30-columnar-bm25-scope
**Date:** 2026-07-03
**Type:** Discovery blueprint (internal — the "prior art" is OUR own m6/m7 benchmarks + the shipped surface).
**Milestone:** M30 (the only open `[ ]` in the active ROADMAP). Decision-grade; the DoD requires an ADR + execution.

## Context

M30 resolves whether the two **v1-legacy pillars** — columnar (M6, `pg_mooncake`/`pg_duckdb`) and BM25 (M7,
`pg_textsearch`), built under the v1 "composition of third-party extensions" thesis — stay in the v2 mandate
(own-code, minimal deps) or are deprecated. `ROADMAP.md § Fora de escopo do v2` says columnar reopening
requires an ADR — **this is that ADR**. Directive: max rigor, benchmark-grounded, 100% functional evidence.

## The load-bearing finding: neither is SHIPPED; they are throwaway MEASUREMENT

Verified in code:
- **Shipped image (`Dockerfile`)** ships PostgreSQL + `pgvector` + `pgvectorscale` + `theodb_rs` (+ `vector`
  type). It ships **NO** `pg_mooncake`/`pg_duckdb` and **NO** `pg_textsearch`.
- The hybrid-search **FTS leg is PostgreSQL NATIVE** `ts_rank_cd` over a `tsvector` + GIN
  (`theodb_rs/src/hybrid.rs:9`, `api.rs:457`, `sql/40-theodb-hybrid.sql:3`) — own composition over a native
  platform feature (Rule 9). **This is a KEEPER and is NOT what M30 touches.**
- Columnar (`pg_mooncake`) and BM25 (`pg_textsearch`) exist ONLY as **throwaway measurement substrates**:
  `packaging/Dockerfile.columnar`, `packaging/Dockerfile.bm25`, the CI jobs `columnar-measure` /
  `bm25-measure`, `benchmarks/tests/test_columnar.py` / `test_bm25.py`, and the docs
  `docs/analytics/columnar-htap.md`, `docs/benchmarks/m6-columnar-vs-row.md`, `m7-bm25-vs-tsrank.md`.

So M30 decides the fate of the **exploration/measurement artifacts**, not any shipped product surface.

## Evidence splits the two pillars (measurement-first)

### Columnar (M6) — NO measured win → DEPRECATE
`docs/benchmarks/m6-columnar-vs-row.md` (honest reading):
- **No columnar speed win at 100k** — the row-store is faster (10.9ms vs 44.3ms).
- **Large-scale win UNBENCHMARKED** at TheoDB scale (only upstream's published ClickBench result, not reproduced).
- **Sync overhead UNBENCHMARKED**.
- It is a **DuckDB+Iceberg lakehouse on disk** (D2), NOT AlloyDB in-memory — a *different* bet, and **off the
  v2 own-code path** (pg_mooncake+pg_duckdb are heavy third-party from-source builds; the PG17 build was even
  blocked on a rustc/MSRV pin — the retired `Dockerfile.columnar-pg17probe`).
- ⇒ No demonstrated value at our scale + off-path + un-shipped ⇒ **deprecate the exploration**.

### BM25 (M7) — measured WIN → KEEP (permissive exception, gated for adoption)
`docs/benchmarks/m7-bm25-vs-tsrank.md` (honest reading):
- **nDCG@10: BM25 (`pg_textsearch`) 0.9546 vs native `ts_rank_cd` 0.5143** — a large lexical-quality win.
- `pg_textsearch` is **permissive** (vetted in `docs/adr/0003-permissive-bm25-pg-textsearch.md`).
- The doc's own conclusion: "pg_textsearch should be adopted into the shipped distribution … on the strength
  of this measurement."
- ⇒ A real, permissive, measured lexical lever ⇒ **do NOT deprecate**; keep as a Rule-9 permissive exception,
  gated for a future adoption milestone. Deprecating it would throw away a measured win (a Rule-3 dishonesty).

## DECISION (updated 2026-07-03 by the CTO): KEEP BOTH

The CTO steer: **columnar is a general analytics/HTAP capability** (AlloyDB parity — AlloyDB ships columnar),
NOT an observability-specific feature. Observability (append-only spans/metrics → large analytical
aggregations) is ONE strong workload among many (dashboards, HTAP over live transactional data). This
supersedes my initial "deprecate columnar" lean.

| Pillar | Decision | Grounding |
|---|---|---|
| Columnar (M6, pg_mooncake) | **KEEP** as a permissive analytics/HTAP pillar (Rule-9 exception, gated adoption) | AlloyDB-parity (North Star); general analytics workloads incl. observability; permissive (MIT). The m6 "no win at 100k" is the WRONG scale — columnar's win is at large analytical scale (m6 marked it UNBENCHMARKED). |
| BM25 (M7, pg_textsearch) | **KEEP** as a permissive exception, gated for adoption | m7: nDCG 0.95 vs 0.51 measured win; permissive (ADR 0003); real adoption candidate |
| Hybrid FTS leg (native `ts_rank_cd`) | **KEEP** (untouched — shipped, own composition over native Postgres) | Rule 9; it IS the shipped lexical leg |

**Nothing is deprecated/removed.** The ADR records KEEP for both, as **permissive exceptions to the own-code
mandate (Rule 9)**, gated for a future adoption milestone. Feasibility of shipping columnar: gated on either
(a) fixing the PG17 from-source build (rustc/MSRV — "resolvable toolchain issue") OR (b) a PG17→PG18 bump
(pg_mooncake ships prebuilt on PG18). Documented as the adoption path; not shipped in M30.

## Benchmark that VALIDATES the KEEP (the goal's data requirement)

The decision to keep columnar is honest ONLY if columnar actually wins at analytical scale (m6 measured just
100k, where row-store won, and marked the large-scale win UNBENCHMARKED). M30 fills that gap: measure
columnar-mirror vs row-store on the canonical `mooncakelabs/pg_mooncake` substrate at **increasing scale**
(e.g. 100k → 1M → 5M rows, a group-by aggregation representative of analytics/observability rollups),
reporting the crossover where the DuckDB columnstore beats the row-store `Seq Scan`. This is the reproducible
evidence the "keep" rests on (`docs/benchmarks/m30-columnar-scale.{md,json}`).

## ADR to write

`docs/adr/0013-v1-legacy-columnar-bm25-scope.md` (MADR 3.0). NOTE: the M30 DoD says "ADR 0007" but **0007 is
already taken** (`0007-synchronous-per-row-model-http.md`); the next free number is **0013** (0001–0012 exist).

## Validation (100% functional evidence)

Deprecating columnar removes only throwaway measurement infra — the shipped image does NOT contain columnar, so
removal cannot break the product. Proof required: rebuild the shipped image + `scripts/smoke.sh` SMOKE PASSED +
the remaining CI-covered test suites (`test_unified`, `test_bm25` kept, `test_extension_install`) green +
`check_xrefs.py` clean after removing the columnar doc/refs (no dangling reference).

## Prior art (internal)

- `docs/benchmarks/m6-columnar-vs-row.md`, `docs/benchmarks/m7-bm25-vs-tsrank.md` — the decision evidence.
- `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (columnar out-of-scope; reopening = this ADR),
  `docs/adr/0003-permissive-bm25-pg-textsearch.md` (BM25 permissive vetting).
- `ROADMAP.md § M30` (the DoD) + `§ Fora de escopo do v2`.
