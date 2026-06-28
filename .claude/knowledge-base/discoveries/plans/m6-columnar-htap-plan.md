# Discovery Plan: Columnar / HTAP analytics for PostgreSQL (pg_mooncake, permissive)

> **Version 1.0** — Investigate the permissive columnar/HTAP path for TheoDB (M6): how `pg_mooncake` (MIT,
> DuckDB-powered Iceberg columnstore) gives fast analytics over live transactional Postgres tables, how the
> planner chooses row vs columnar, the PG-version support (risk #1), the build/adoption cost, and the honest
> framing (lakehouse DuckDB+Iceberg, NOT in-memory like AlloyDB — D2). The blueprint decides the M6 deliverable:
> the columnar capability + the row-vs-columnar plan evidence + the measurement-first adoption gate.

**Slug:** `m6-columnar-htap`
**Owner:** paulohenriquevn
**Created:** 2026-06-28
**Time budget:** 6h (per-project breakdown in ADR D1)

## Context

ROADMAP `### M6` (Analytics colunar / HTAP) objective: "Camada de armazenamento colunar (DuckDB-powered,
`pg_mooncake` MIT) para analytics rápido sobre dados transacionais vivos, com escolha de plano row vs
colunar." DoD: (1) `pg_mooncake` enabled for selected tables/columns with an analytical query measured vs
row-store; (2) row-vs-columnar plan choice documented; (3) honesty: it is a columnar **lakehouse**
(DuckDB+Iceberg), NOT in-memory like AlloyDB (PRD D2). Dependency M1 (the PostgreSQL-compatible distribution)
is satisfied. The north-star ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` already frames the
columnar pillar as a *different, competitive* bet (permissive lakehouse) forced by D1 (AlloyDB's in-memory
columnar peers — Citus/Hydra — are AGPL-barred), not a literal copy. Risk #1 is whether `pg_mooncake` supports
the PG major TheoDB ships (17). This discovery establishes the capability, the plan-choice evidence, the PG17
support, the build/adoption cost, and the honest scope, with measurement-first discipline (measure before
embedding a heavy dependency, cf. the M7-S2 BM25 precedent).

## Objective

Produce a blueprint that decides the M6 columnar deliverable: the pg_mooncake capability + the row-vs-columnar
plan evidence + the PG17 support + the measurement-first adoption gate + the honesty framing.

- [ ] All research questions answered with citations to `.claude/knowledge-base/references/` or allowlisted sources
- [ ] Cross-cutting comparison populated (columnstore mirror vs row-store; pg_mooncake vs in-memory AlloyDB)
- [ ] Recommendations give one concrete M6 deliverable proposal + the adoption gate
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS (frontier ≥ 75)

## In-Scope / Out-of-Scope

### In-Scope (per source)

| Source | In-scope | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pg_mooncake/` | `README.md`, `Makefile`, `Dockerfile`, `LICENSE`, `pg_mooncake.control` | The columnstore-mirror surface, PG-version support, build recipe, MIT license, pg_duckdb dependency |
| `.claude/knowledge-base/references/duckdb/` | `README.md` | The columnar execution engine underneath (DuckDB) — the lakehouse honesty (D2) |
| Allowlisted web (`duckdb.org`, `github.com`, `www.postgresql.org`) | pg_mooncake/pg_duckdb docs + releases (PG17 support, prebuilt artifacts); PostgreSQL custom-scan / planner docs | PG-version support (risk #1), build/adoption cost, the row-vs-columnar plan mechanism |

### Out-of-Scope (explicit)

| Source | Why excluded |
|---|---|
| In-memory columnar (Citus columnar / Hydra) | AGPL-barred (D1) and a different architecture — not the TheoDB bet (ADR 0002) |
| Full Iceberg catalog / external-engine integration | Beyond the M6 capability MVP (lakehouse interop is a follow-up) |
| Any source not under `references/` and not allowlisted | Cross-Project Rule + allowlist (R5) |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pg_mooncake (surface + PG17 support + build cost + license) 2.5h; DuckDB lakehouse honesty 0.5h;
allowlisted web (pg17 support/artifacts + custom-scan planner) 2.5h; synthesis 0.5h.

**Rationale:** the load-bearing questions are risk #1 (PG17 support — sourced from the Makefile + repo, not
memory) and the row-vs-columnar plan mechanism (the DoD-2 evidence).

**Stop condition — per question:** Fase A empty after 3 retries → BLOCKED "Fase A exhausted"; continue.
**Stop condition — per source:** budget exhausted → remaining BLOCKED; if all `done`/`blocked` →
`<promise>BLUEPRINT_BLOCKED</promise>` (never COMPLETE from a blocked state).
**Anti-pattern:** NEVER assert PG-version support or a performance number from memory — source it (Makefile /
repo / docs) or mark `UNVERIFIED`/`UNBENCHMARKED` (Rule 3).

**Consequences:** an unverifiable support claim is excluded from the recommendation (fail-closed).

### D2 — Investigation depth

**Decision:** Read the pg_mooncake README/Makefile/Dockerfile end-to-end (the support matrix + build recipe +
columnstore DDL must be exact); skim DuckDB for the lakehouse framing.

**Rationale:** the PG17 build/adoption decision depends on the exact recipe + support matrix.

**Consequences:** depth where the adoption decision bears.

### D3 — Measurement-first + honesty (D2 framing)

**Decision:** the blueprint MUST (a) measure the columnar analytical query vs row-store with real numbers OR
mark `UNBENCHMARKED`, and (b) state plainly that this is a DuckDB+Iceberg **lakehouse** columnstore, not the
in-memory columnar of AlloyDB (PRD D2). Adopting a heavy dependency (pg_duckdb+DuckDB) into the shipped image
is gated on the measurement (cf. M7-S2 BM25).

**Rationale:** CLAUDE.md TheoDB rule 5 (performance is a claim, not opinion) + rule 7 (honesty about the
lakehouse-vs-in-memory trade-off) + measurement-first (ADR 0002).

**Consequences:** the recommendation separates "capability proven + measured" from "embedded in the shipped
image" (the latter gated).

## Research Questions

| # | Question | Corner | Source(s) | Fase A | Fase B | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How does pg_mooncake test the columnstore-mirror correctness / freshness (the pattern for a columnar-vs-row eval)? | tests | `.claude/knowledge-base/references/pg_mooncake/README.md` (quickstart), allowlisted `github.com` (pg_mooncake tests) | Grep `create_table\|trades_iceberg\|SELECT` in the README; WebFetch the repo test dir | Read the mirror create + query example; capture how correctness/freshness is shown | Table: example → what it demonstrates, with citation |
| Q2 | License + PG-version support + dependency of pg_mooncake (risk #1: does it support PG17?) | deps | `.claude/knowledge-base/references/pg_mooncake/LICENSE`, `Makefile`, `pg_mooncake.control`; allowlisted `github.com` releases | Read LICENSE (MIT); read Makefile `PG_VERSION` list; read control `requires` | Confirm MIT; confirm pg17 in the supported list (verbatim); confirm pg_duckdb dependency | Dep table: piece → license → PG17? → requires |
| Q3 | Build / adoption cost: source build (Rust+pgrx+DuckDB) vs prebuilt (official image / pgduckdb base) | tools | `.claude/knowledge-base/references/pg_mooncake/Dockerfile`, `Makefile`; allowlisted `github.com`/`duckdb.org` | Read their Dockerfile (build stages + runtime base); WebFetch prebuilt-image availability | Capture the heavy-build reality + the prebuilt options (official image PG18; pgduckdb:17 base) | Comparison: option → cost → ships in TheoDB image? (gated) |
| Q4 | How does the planner choose row vs columnar (the DoD-2 mechanism) — what does EXPLAIN show? | techniques | allowlisted `github.com` (pg_mooncake) / `www.postgresql.org` (custom scan); `.claude/knowledge-base/references/pg_mooncake/README.md` | Grep `custom scan\|duckdb\|columnstore` ; WebFetch the planner/custom-scan behavior | Read how the columnstore mirror query is planned (Custom Scan / DuckDB) vs the heap Seq Scan | The plan-choice evidence: columnstore → DuckDBScan; row → SeqScan (EXPLAIN shapes) |
| Q5 | What analytical workload shows the columnar win, and how is it measured vs row-store (DoD-1)? | techniques | allowlisted `duckdb.org` (columnar analytics), `github.com` (pg_mooncake clickbench); `.claude/knowledge-base/references/duckdb/README.md` | WebFetch the columnar-analytics rationale + any pg_mooncake benchmark | Read the workload shape (scan-heavy aggregate/group-by) + the measurement method; mark `UNBENCHMARKED` until TheoDB measures | Workload + measurement method; SOTA-anchored on AlloyDB columnar (R1) |
| Q6 | What does the SOTA expose (AlloyDB in-memory columnar) and what is the honest TheoDB equivalent (lakehouse) — the D2 framing? | techniques | allowlisted `cloud.google.com` (AlloyDB columnar); `.claude/knowledge-base/references/duckdb/README.md`, `pg_mooncake/README.md` | WebFetch AlloyDB columnar engine; read pg_mooncake/DuckDB lakehouse framing | Capture the SOTA in-memory columnar vs the permissive DuckDB+Iceberg lakehouse; state the honest trade-off | Table: AlloyDB in-memory columnar → TheoDB lakehouse → honest delta (D2) |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q1 | Covered |
| Dependencies | Q2 | Covered |
| Tools | Q3 | Covered |
| Techniques | Q4, Q5, Q6 | Covered (≥ 2 — frontier R4) |

**Coverage: 4/4 corners covered (100%)**

> **TheoDB frontier rigor** (`rules/discover-phd-rigor.md`): techniques = 3 questions; each (R1) anchored on the
> AlloyDB in-memory columnar SOTA with the honest lakehouse delta (D2) stated, (R2) ≥ 2 primary sources
> (pg_mooncake repo + DuckDB docs + PostgreSQL custom-scan docs + cloned witnesses), (R3) any perf claim sourced
> OR `UNBENCHMARKED`. PG-version support sourced from the Makefile/repo, never memory (D1). Budget: 6 (≤ 14),
> ≤ 5/corner. ✅

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | each `.claude/knowledge-base/references/{path}` in Fase A exists | mark Qx BLOCKED "path not found", continue |
| Web source (Q2..Q6) | source ∈ `rules/discover-web-allowlist.txt` | do not cite; find allowlisted equivalent or BLOCKED |
| PG17 support claim (Q2) | sourced verbatim from the Makefile/repo | mark `UNVERIFIED`, exclude from the recommendation |
| Perf claim (Q5) | methodology+source OR `UNBENCHMARKED` (R3) | add methodology or flag UNBENCHMARKED |
| Before promising complete | all 4 corners populated + the D2 honesty stated | refuse promise, continue |

## Acceptance Criteria

- [ ] All research questions answered OR BLOCKED with reason
- [ ] All four coverage corners populated
- [ ] Every reference citation resolves; every web citation is allowlisted
- [ ] Frontier rigor (R1/R2/R3): SOTA-anchored + ≥ 2 primary sources; perf claims benchmarked OR `UNBENCHMARKED`
- [ ] PG17 support verified from the repo (risk #1) OR explicitly `UNVERIFIED`
- [ ] D2 honesty stated (lakehouse DuckDB+Iceberg, not in-memory)
- [ ] ≥ 1 ADR synthesizing the M6 deliverable + the measurement-first adoption gate
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint at `.claude/knowledge-base/discoveries/blueprints/m6-columnar-htap-blueprint.md`

## Edge Cases & MUST-FIX (from /discover-edge-cases)

| # | Edge case / risk | MUST-FIX (which question) | Acceptance |
|---|---|---|---|
| E1 | pg_mooncake does not support PG17 (risk #1) | Q2 — verify the Makefile/repo support matrix verbatim (the cloned `pg_mooncake/Makefile` lists pg14–18) | blueprint states PG17 support sourced from the repo, or `UNVERIFIED` |
| E2 | Heavy build (Rust+pgrx+DuckDB+pg_duckdb) blocks shipping into theo-db:dev | Q3 — capture the build cost honestly; recommend measurement-first (measure on the canonical distribution; gate the PG17 build/adoption — cf. BM25 S2) | blueprint separates "capability measured" from "embedded in shipped image (gated)" |
| E3 | Row↔columnar sync overhead (risk #2) | Q1/Q4 — note the mirror-sync model (sub-second freshness claim) + that it is `UNBENCHMARKED` until measured | blueprint flags the sync-overhead honestly |
| E4 | Over-claiming AlloyDB parity (in-memory) | Q6 — the D2 honesty: lakehouse DuckDB+Iceberg, NOT in-memory; competitive-different bet, not a copy | blueprint states the honest delta (D2) |
| E5 | Planner does not actually pick columnar (DoD-2 unverifiable) | Q4 — capture the EXPLAIN evidence (Custom Scan/DuckDBScan on the mirror vs SeqScan on the row table) | blueprint shows the row-vs-columnar EXPLAIN shapes with citation |
| E6 | No measured columnar-vs-row number (DoD-1) | Q5 — define the analytical workload + measurement method; mark `UNBENCHMARKED` until TheoDB measures it | blueprint defines the measurement, no fabricated number |

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed)
- [ ] Final `/discover-confidence` verdict recorded in the blueprint header
- [ ] No fabricated citations; no PG-support/perf claim asserted from memory
- [ ] Coverage Matrix 100%
- [ ] ADRs reference ≥ 1 project rule/principle (D2 honesty, measurement-first, parsimony-ladder, `discover-phd-rigor.md`)
