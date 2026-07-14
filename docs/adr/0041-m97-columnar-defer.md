# ADR 0041 — M97: DEFER a new Columnar/HTAP pillar; keep the shipped pg_duckdb route

**Status:** Proposed (owner sign-off pending) · **Date:** 2026-07-13 · **Milestone:** M97 · **Decision:** DEFER

## Context

PRD decision D2 flagged Columnar/HTAP as a candidate pillar. M97 is a discovery-only cycle (zero product code)
answering: is building a NEW columnar pillar — beyond the pg_duckdb/HTAP surface TheoDB already ships (M61/M62/M64) —
a bet worth months, under the D1 license gate (Apache-2.0 / MIT / BSD / PostgreSQL License only)?

Evidence: `knowledge-base/discoveries/blueprints/columnar-htap-blueprint.md` (SHIPPABLE, council-research-adr,
R0 web-grounded) + `docs/benchmarks/m97-htap-viability.md`.

## Decision

**DEFER** a new columnar pillar. Keep the shipped pg_duckdb (MIT) + HTAP codegen surface (M61/M62, ADRs 0020/0021)
as the permissive columnar answer. Add a **moonlink-license watch-item**.

## Rationale

1. **Columnar value is real AND already delivered.** The M97 viability benchmark measured DuckDB columnar **15–23×**
   faster than PG row-store on analytical aggregations (20M rows, same box) — confirming columnar's value. But that
   value already ships permissively via pg_duckdb (embedded M61) + the HTAP surface (M62, ~31× OLAP measured).
2. **Every "go further" differentiator is barred or blocked** (web-verified licenses, 2026-07-13):
   - *auto row→columnar sync* — the only peer offering it is pg_mooncake, whose sync engine **moonlink is BSL 1.1
     (BARRED by D1)**; the MIT extension shell is not the value; last GA tag `v0.1.2` (2025-02) — immature too.
   - *in-Postgres columnar access method* — Hydra columnar and Citus columnar are both **AGPLv3 (BARRED by D1)**.
   - *single-plan row↔columnar routing* — **paradigm-blocked**: TheoDB is structurally two engines / two planners
     (ADR 0023 measured `ERROR: DuckDB execution is not supported inside functions`); a permissive extension cannot
     match AlloyDB's in-core in-memory single-planner engine (the M73 vector lesson: match capability, never claim
     superiority over the closed SOTA).
   - *unified vector+analytics in one query* — **already delivered** (M62/M64 statement-level codegen).
3. **GO would be accidental complexity** — months re-packaging what pg_duckdb already gives (the CLAUDE.md
   "multi-engine abstraction with one engine" anti-pattern). **NO-GO would be too strong** — it would retire a
   capability that is real, delivered, and measured. DEFER is the honest middle: the permissive design space is
   exhausted by what ships.

## Consequences

- No new product code (the milestone delivers KNOWLEDGE).
- The pg_duckdb/HTAP surface remains the permissive columnar answer; `duckdb.allow_community_extensions=off` stays.
- **Revisit trigger:** re-open M97 if (i) moonlink (or an equivalent) relicenses off BSL 1.1 to Apache/MIT (making
  auto-sync obtainable), OR (ii) the vector North Star reposition (ADR 0033) settles and marketing needs a second
  MEASURED pillar.
- Positioning discipline: "on-demand vectorized columnar analytics via pg_duckdb (lakehouse bet, D2)" — never the
  in-memory-auto columnar of AlloyDB.

## Alternatives considered

- **GO (new columnar pillar):** rejected — differentiators barred (moonlink BSL, Hydra/Citus AGPL) or paradigm-blocked.
- **NO-GO (retire columnar framing):** rejected — discards real, shipped, measured value.

## References

- Blueprint: `knowledge-base/discoveries/blueprints/columnar-htap-blueprint.md`
- Benchmark: `docs/benchmarks/m97-htap-viability.{md,json}`
- Prior columnar decisions: ADRs `0013`, `0020`, `0021`, `0023`
- Licenses (web-verified 2026-07-13): moonlink BSL 1.1, Hydra/Citus AGPLv3, pg_duckdb/DuckDB/pg_mooncake MIT
