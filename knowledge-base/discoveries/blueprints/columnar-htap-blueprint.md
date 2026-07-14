# Blueprint: M97 Columnar/HTAP (D2) discovery

**Date:** 2026-07-13 · **Milestone:** M97 · **Cycle:** discover (council-research-adr, R0 web-grounded) ·
**Recommendation: DEFER** (signed ADR `docs/adr/0041-m97-columnar-defer.md`).

## Executive summary

TheoDB **already ships** the only D1-permissive columnar route: `pg_duckdb` embedded in M61 (ADR 0020), the HTAP
codegen surface in M62 (ADR 0021, ~31× OLAP measured), the two-engine planner ceiling recorded in M64 (ADR 0023).
M97 is therefore NOT "should we do columnar" — it is "is going FURTHER than what we ship worth months?" The answer
is **DEFER**: every "go further" differentiator is either **license-barred** (D1) or **paradigm-blocked** (two
engines), and the residual ergonomic delta is already built. A rigorous DEFER is the milestone's KNOWLEDGE
deliverable — zero product code.

## Context

TheoDB's PRD decision D2 flagged Columnar/HTAP as a potential pillar. This discovery (R0 web-grounded) answers
whether going FURTHER than the already-shipped pg_duckdb/HTAP surface (M61/M62/M64) is a bet worth months — under
the D1 license gate (Apache-2.0/MIT/BSD/PostgreSQL only). It produces KNOWLEDGE + a GO/NO-GO/DEFER ADR, no code.

## Objective

Recommend GO / NO-GO / DEFER on a new columnar pillar, with web-verified license evidence for every permissive peer,
an honest gap analysis vs a bare `CREATE EXTENSION pg_duckdb`, and a viability-benchmark anchor number — success =
a SHIPPABLE blueprint + a signed decision ADR.

## Coverage Corner 1 — Integration Tests

The "test" of this discovery is the decision gate: what evidence justifies each verdict.

- **GO** (add columnar as a NEW pillar with a milestone roadmap) requires a differentiator beyond what's shipped —
  auto row→columnar sync OR single-plan row↔columnar routing. Neither is permissively achievable (see below). GO
  would commit months to RE-PACKAGING what `pg_duckdb` already delivers — accidental complexity (CLAUDE.md
  "multi-engine abstraction with one engine" anti-pattern).
- **NO-GO** (retire the columnar framing) is stronger than the evidence needs — columnar capability is real and
  already delivered/measured.
- **DEFER ← recommended.** The capability ships (M61/M62), is measured (ADRs 0013/0020/0021), and the new-pillar
  differentiators are barred/blocked. Nothing is achievable NOW that isn't already delivered. Revisit only if
  (i) a permissive real-time-sync engine emerges (moonlink relicenses off BSL), or (ii) the vector North Star
  reposition (ADR 0033) settles and marketing needs a second MEASURED pillar. DEFER commits to: this blueprint +
  the viability benchmark anchor number + a signed ADR + a moonlink-license watch-item — **zero product code**.

Prior-art decision evidence (resolves on disk): `docs/adr/0013-v1-legacy-columnar-bm25-scope.md`,
`docs/adr/0020-m61-embed-pgduckdb.md`, `docs/adr/0021-m62-htap-codegen-surface.md`,
`docs/adr/0023-m64-rag-unified-not-columnar-planner.md`.

## Coverage Corner 2 — Dependencies

D1 admits only Apache-2.0 / MIT / BSD / PostgreSQL License. Verified live (WebFetch, 2026-07-13):

| Peer | License | Evidence (URL) | D1 verdict |
|---|---|---|---|
| **pg_duckdb** | MIT | `api.github.com/repos/duckdb/pg_duckdb` → `spdx_id: MIT` | **GO** (already shipped) |
| **DuckDB** (engine) | MIT | `api.github.com/repos/duckdb/duckdb` → `spdx_id: MIT` | **GO** (already shipped) |
| **pg_mooncake** (extension) | MIT | `api.github.com/repos/Mooncake-Labs/pg_mooncake` → `spdx_id: MIT` | extension MIT, but… |
| **moonlink** (pg_mooncake sync engine) | **BSL 1.1** | `raw.githubusercontent.com/Mooncake-Labs/moonlink/main/LICENSE` → "Business Source License 1.1" | **BARRED (D1)** |
| **Hydra columnar** | AGPLv3 | `raw.githubusercontent.com/hydradatabase/hydra/main/columnar/LICENSE` → AGPLv3 | **BARRED (D1)** |
| **Citus columnar** | AGPLv3 | `citus/LICENSE` → AGPLv3 | **BARRED (D1)** |

**Decisive fact:** the ONLY peer offering the M97 differentiator "automatic real-time row→columnar sync" is
pg_mooncake — and its sync value lives in **moonlink (BSL 1.1, barred)**. The MIT shell is not the value. pg_mooncake's
last tagged release is `v0.1.2` (2025-02-12) — 17 months without a GA tag. The differentiator is license-poisoned AND
immature. DuckDB community extensions remain a supply-chain vector → keep `duckdb.allow_community_extensions=off`
(the shipped default, ADR 0020).

## Coverage Corner 3 — Tools

**Goal:** honestly measure "is columnar worth a NEW pillar" — same analytical data + queries, same box.

- **Dataset:** an analytical `hits`-shaped table (ClickBench idiom — `github.com/ClickHouse/ClickBench`, Apache-2.0):
  20M rows, GROUP BY aggregations + a filtered scan (columnar's home turf).
- **Arms (same box, same data):** (A) vanilla PG row-store; (B) DuckDB columnar (MIT) on the same exported data.
- **Measure:** warm latency per query, ≥1 warm run after a warm-up; the honest caveat printed (single query ≠
  workload; DuckDB is columnar+vectorized+multicore vs PG row-store; in-memory-AlloyDB not comparable).
- **Result:** `docs/benchmarks/m97-htap-viability.{md,json}` — DuckDB/columnar shows the expected large speedup on
  GROUP BY aggregations, RE-CONFIRMING the M61/M62 measured columnar value (ADR 0020 heap 0.63–0.89× / Parquet ~9×;
  M62 ~31× OLAP). **The benchmark confirms the shipped route is right; it discovers nothing new** — itself evidence
  for DEFER (no new differentiator to chase).

## Coverage Corner 4 — Techniques

- **AlloyDB SOTA (the bar):** in-memory columnar engine, AUTOMATIC workload-learned row↔column organization,
  planner-chosen, "up to 100× vs standard PostgreSQL", zero schema/ETL change —
  `cloud.google.com/blog/products/databases/alloydb-for-postgresql-columnar-engine`.
- **The permissive route (achievable):** pg_duckdb = on-demand vectorized columnar execution over live heap / Parquet
  — `raw.githubusercontent.com/duckdb/pg_duckdb/main/README.md`. A lakehouse/vectorized-on-disk bet (D2),
  DELIBERATELY DIFFERENT from AlloyDB's in-memory-auto engine.
- **The paradigm ceiling (≥2 sources):** (i) AlloyDB uses ONE engine + ONE planner owning both stores [Google blog];
  (ii) HTAP survey `arXiv:2404.15670` — first-class hybrid row+columnar plans require a single planner over both
  stores. TheoDB is structurally two engines (Postgres executor + DuckDB), two planners (ADR 0023 measured
  `ERROR: DuckDB execution is not supported inside functions`). **A permissive PG extension cannot match an in-core
  in-memory columnar engine** — the M73 vector lesson applied (match capability, never claim superiority over the
  closed SOTA).
- **The honest gap (what TheoDB adds beyond bare `CREATE EXTENSION pg_duckdb`):** (a) auto row→columnar sync →
  **barred** (moonlink BSL); (b) single-plan row↔columnar routing → **paradigm-blocked** (two engines, ADR 0023);
  (c) unified vector+analytics in one query → **already delivered** (M62/M64 statement-level codegen,
  `theodb.olap_sql`, RAG-over-SQL). The residual is ergonomics (freshness catalog + codegen) — **already shipped**.
  Honest statement: `CREATE EXTENSION pg_duckdb` already delivers ~90% of the achievable permissive value; TheoDB's
  own-code delta is real but small and already built.

## Cross-cutting Comparison

| Axis | AlloyDB (closed SOTA) | pg_duckdb (shipped, MIT) | moonlink sync (BSL — barred) | Hydra/Citus columnar AM (AGPL — barred) |
|---|---|---|---|---|
| License vs D1 | closed | **GO** | **BARRED** | **BARRED** |
| Model | in-memory auto columnar, one planner | on-demand vectorized over heap/Parquet, two engines | real-time row→columnar CDC sync | in-Postgres columnar pages |
| Planner routing | single-plan row↔column | two engines (ADR 0023 ceiling) | n/a | single-AM but AGPL |
| TheoDB status | the bar (paradigm ceiling) | **already delivered** (M61/M62) | unobtainable permissively | unobtainable permissively |
| New-pillar delta | — | thin (ergonomics, already built) | would be the differentiator — barred | would be the differentiator — barred |

The comparison shows the permissive design space is EXHAUSTED by what TheoDB already ships: the only routes that
would differentiate a new pillar (auto-sync, in-Postgres columnar AM) are all AGPL/BSL — barred by D1.

## Recommendations

1. **DEFER the new columnar pillar** (ADR 0041) — keep the shipped pg_duckdb/HTAP surface as the permissive answer.
2. **Watch moonlink's license** — if it relicenses off BSL 1.1 to Apache/MIT, the auto-sync differentiator becomes
   obtainable and M97 should be re-opened.
3. **Position honestly** (M73 pattern) — "TheoDB does on-demand vectorized columnar analytics via pg_duckdb
   (lakehouse bet, D2); it does NOT claim the in-memory auto-columnar of AlloyDB" — never over-claim.
4. **Keep `duckdb.allow_community_extensions=off`** (shipped default) — supply-chain hygiene.

## ADRs

### D1 — DEFER the columnar/HTAP "new pillar", KEEP the shipped pg_duckdb route

**Decision:** DEFER any new columnar pillar. Record the analysis in `docs/adr/0041-m97-columnar-defer.md`; keep the
M61/M62 pg_duckdb/HTAP surface as the permissive columnar answer; add a moonlink-license watch-item.

**Rejected alternatives:** (a) **GO** — REJECTED: every differentiator beyond shipped is license-barred (moonlink
BSL, Hydra/Citus AGPL — D1) or paradigm-blocked (two engines, ADR 0023); GO = months re-packaging pg_duckdb
(accidental complexity). (b) **NO-GO** — REJECTED: stronger than the evidence needs; columnar capability is real,
delivered, and measured — retiring the framing would discard shipped value.

## Prior Art & Related Work

- `.claude/knowledge-base/discoveries/blueprints/m61-columnar-htap-adoption-blueprint.md` — the earlier web-cited
  license/peer analysis this extends.
- ADRs 0013/0020/0021/0023 (the shipped columnar/HTAP decisions).
- Benchmarks `docs/benchmarks/{m30-columnar-scale,m61-columnar-adoption,m62-htap}.{md,json}` (measured columnar value).

## Drawbacks & Risks

| # | Risk | Severity | Mitigation |
|---|---|---|---|
| 1 | DEFER mistaken as "columnar abandoned" | MEDIUM | the ADR states columnar SHIPS (pg_duckdb); DEFER is about a NEW pillar only |
| 2 | A permissive sync engine emerges and we miss it | LOW | moonlink-license watch-item in the ADR |
| 3 | DuckDB community-extension supply chain | LOW | `allow_community_extensions=off` (shipped default) |

## Unresolved Questions

- (none — the decision is DEFER with a clear revisit trigger; the owner signs ADR 0041.)
