# Discovery Plan: Hybrid Search (FTS + vector) + Reciprocal Rank Fusion

> **Version 1.0** — Investigate how permissive OSS PostgreSQL stacks combine full-text search (FTS) with
> vector similarity and fuse the two rankings via Reciprocal Rank Fusion (RRF), so TheoDB can ship M7-S1
> (hybrid search) as a pure-OSS win on top of the M2 pgvector base — without the AGPL-barred `pg_search`
> (D1). The blueprint will decide: the FTS index (GIN vs RUM), the RRF formula + constant, the target API
> shape (`ai.hybrid_search` native fn + manual SQL path), and the recall methodology to claim the win.

**Slug:** `m7-hybrid-search-rrf`
**Owner:** paulohenriquevn
**Created:** 2026-06-28
**Time budget:** 6h (per-project breakdown in ADR D1)

## Context

M7 (IA avançada) ROADMAP fixes the sequence: **hybrid search first — pure OSS, immediate win**; BM25
permissivo is a real gap (`pg_search` is AGPL → barred by D1) that earns its *own* discovery later (M7-S2).
This discovery scopes **only** M7-S1: FTS+vector+RRF on PostgreSQL-native primitives. Triggers:

- ROADMAP `### M7` DoD-2: "Hybrid search (texto + semântico) + reranking (RRF) com recall medido (ex.: BEIR)".
- Target API spec at `docs/features/06-busca-hibrida.md` (`ai.hybrid_search()` + manual RRF SQL).
- North-star ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`: AlloyDB ships `ai.hybrid_search`;
  TheoDB matches the capability with permissive pieces and wins on model-agnosticism (CLAUDE.md TheoDB rule 1).
- M2 already shipped pgvector (HNSW/IVFFlat) + the recall@k harness (`vector-recall-benchmark-harness`),
  the measurement-first base this slice reuses (CLAUDE.md TheoDB rule 5 — performance is a claim, not opinion).

## Objective

Produce a blueprint that lets us decide the M7-S1 implementation: FTS index choice, RRF fusion contract,
target API surface, and the recall methodology — entirely on permissive OSS.

- [ ] All research questions answered with citations to `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison table populated for every in-scope reference project
- [ ] Recommendations section provides at least one concrete decision proposal per research question
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS (frontier threshold ≥ 75)

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvector/` | `README.md` (hybrid-search section) | Canonical FTS+vector hybrid + RRF example on the exact base TheoDB ships |
| `.claude/knowledge-base/references/supabase-postgres/` | `nix/tests/sql/docs-full-text-search.sql`, `nix/tests/sql/z_15_rum.sql` | PG-native FTS (GIN) + RUM index — the permissive FTS ranking options |
| `.claude/knowledge-base/references/paradedb/` | `LICENSE`, `tests/tests/hybrid.rs` | Confirm AGPL (barred — D1) AND read its RRF technique as field reference (technique is not copyrightable; we borrow the math, not the code) |
| `.claude/knowledge-base/references/pgvectorscale/` | `README.md` (rescore section) | Whether DiskANN rescore is a reranking lever complementary to RRF |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `paradedb/pg_search/` **as an implementation choice** | AGPL-3.0 → barred from the TheoDB distribution by D1. Read for technique only, never vendored. |
| BM25 permissive alternatives (pg_bm25 forks, VectorChord-bm25, etc.) | Deferred to M7-S2 (its own discovery, per ROADMAP) — not needed for the PG-native FTS baseline |
| `citus/` distributed text-search internals | Distribution/scale is M8, not M7-S1 |
| Any `*/build/`, `*/dist/`, `*/target/`, `*/.venv/` | Build artifacts |
| Any project NOT cloned into `.claude/knowledge-base/references/` | Cross-Project Rule: never claim a feature without reading source |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvector 1.5h, supabase-postgres 1.5h, paradedb 1.5h (technique + license), pgvectorscale 0.5h,
allowlisted web (RRF paper + AlloyDB doc + BEIR) 1h.

**Rationale:** pgvector is the base we ship and carries the canonical hybrid example; supabase carries the
PG-native FTS+RUM options; paradedb carries a working RRF reference (and the license that proves why we do
NOT vendor it). pgvectorscale is a quick check (rescore lever). Web budget gathers the ≥ 2 primary sources
(R2) and the SOTA anchor (R1).

**Alternatives considered:** equal split (rejected — paradedb's license check is cheap, its technique is
the valuable part); single-project deep dive (rejected — RRF fusion must be cross-validated across ≥ 2 refs).

**Stop condition — per question (mandatory):** when a question's Fase A returns empty after 3 query-variant
retries, mark BLOCKED with reason "Fase A exhausted"; continue. Never pad with unrelated hotspots.

**Stop condition — per project (mandatory):** when a project's budget is exhausted with questions pending,
mark them BLOCKED with reason "budget exhausted". If every remaining question is `done` or honest `blocked`,
emit `<promise>BLUEPRINT_BLOCKED</promise>` (never `BLUEPRINT_COMPLETE` from a blocked state).

**Anti-pattern:** NEVER fabricate Fase B answers (Unbreakable Rule 3).

**Consequences:** blocked questions surface in the blueprint's `## Blocked questions` section as M7-S2 seeds.

### D2 — Investigation depth

**Decision:** Read the cited FTS/RRF SQL + the paradedb RRF test end-to-end (small, dense files); Grep/skim
for everything else. WebFetch only allowlisted domains for the RRF paper, AlloyDB doc, and BEIR.

**Rationale:** the RRF math and the FTS ranking SQL are the load-bearing techniques — they must be read
exactly to cite line-precise, not skimmed. License files are a one-line confirmation.

**Consequences:** trade depth-everywhere for depth-where-it-bears; license confirmation is shallow by design.

### D3 — Borrow technique, never AGPL code (D1 compliance)

**Decision:** paradedb is read ONLY to understand the RRF algorithm + hybrid query shape. No AGPL code,
schema, or test is copied into TheoDB. The RRF formula (`1/(k+rank)`) is the public Cormack et al. 2009
technique, independently citable from the primary paper.

**Rationale:** D1 bars AGPL from the distribution; algorithms are not the licensed artifact. We anchor RRF on
the primary paper (R2), using paradedb only as a field-confirmation of the SQL shape.

**Consequences:** the blueprint's RRF section cites the paper as the authority + paradedb/pgvector as field
witnesses — not paradedb as the source.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad map) | Fase B (deep Read) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How does the OSS stack test FTS+vector hybrid retrieval against a real Postgres (fixtures, assertions, what's asserted)? | tests | `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql`, `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs` | SKIP (text/code-shape known) — Grep `to_tsvector\|ts_rank\|@@\|RRF\|FULL OUTER JOIN` in the two files | Read both: capture how the hybrid result set is asserted (ordering, score, recall) | Table: test → setup (index/data) → assertion type, with `path:line` per row |
| Q2 | What runtime pieces does PG-native FTS+vector+RRF require, and which are permissive vs AGPL-barred? | deps | `.claude/knowledge-base/references/paradedb/LICENSE`, `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/z_15_rum.sql` | SKIP — text-shape. Read `LICENSE` header; Grep `rum` extension usage | Confirm: PG-native FTS = built-in (zero dep); RUM = optional permissive; pg_search = AGPL (barred) | Dep table: piece → license → in-distribution? (yes/no + reason) |
| Q3 | What's the local-dev / smoke story to exercise a hybrid query end-to-end against `theo-db:dev`? | tools | `.claude/knowledge-base/references/pgvector/README.md`, repo root (`smoke.sh`, `Dockerfile`) | SKIP — Glob `smoke.sh`, `docker-compose*.yml`; Grep hybrid example in pgvector README | Read the pgvector hybrid example + the repo smoke to define the reproduction recipe | Step-by-step: container up → CREATE EXTENSION → seed → hybrid query → assert |
| Q4 | What is the canonical RRF fusion contract (formula, the `k`/60 constant, FULL OUTER JOIN, RANK() window) and how do field implementations realize it? | techniques | `.claude/knowledge-base/references/pgvector/README.md`, `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs`, allowlisted RRF paper | Grep `1.0/(60\|RANK() OVER\|FULL OUTER JOIN\|rrf` across both refs | Read both RRF realizations side-by-side; WebFetch Cormack et al. 2009 (R2 primary source) | Side-by-side: source → RRF formula → k value → join shape → citation; SOTA-anchored (R1) on AlloyDB `ai.hybrid_search` |
| Q5 | For the FTS leg, which PG-native ranking + index is right: `ts_rank`/`ts_rank_cd` + GIN vs RUM `<=>` distance — trade-offs (build, query, score quality)? | techniques | `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql`, `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/z_15_rum.sql`, `.claude/knowledge-base/references/pgvector/README.md` | Grep `gin\|rum\|ts_rank\|<=>` in the three files | Read GIN+ts_rank vs RUM+`<=>` examples; capture index DDL + ranking semantics | Table: option → index DDL → ranking fn → trade-off; recommendation with rationale |
| Q6 | What recall does hybrid (RRF) deliver vs pure-vector vs pure-keyword, under what benchmark — and what does the SOTA anchor (AlloyDB `ai.hybrid_search`) expose (weighted components)? | techniques | allowlisted BEIR + AlloyDB doc; `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs` | WebFetch BEIR (recall methodology) + AlloyDB hybrid-search doc (component/weight API); Grep weight/score in paradedb | Read BEIR methodology + AlloyDB API; capture the metric + the weighted-component shape | Table: source → dataset → metric (recall@k/nDCG) → methodology; bare claims marked `UNBENCHMARKED` (R3) |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q1 | Covered |
| Dependencies | Q2 | Covered |
| Tools | Q3 | Covered |
| Techniques | Q4, Q5, Q6 | Covered (≥ 2 — frontier R4) |

**Coverage: 4/4 corners covered (100%)**

> **TheoDB frontier rigor** (`rules/discover-phd-rigor.md`): techniques corner = 3 questions; each (R1)
> anchored on AlloyDB `ai.hybrid_search` SOTA with the gap stated, (R2) backed by ≥ 2 primary sources
> (Cormack et al. 2009 RRF paper + BEIR + official AlloyDB doc, cross-validated against ≥ 2 cloned refs),
> (R3) recall claims carry methodology + numbers + source OR the literal `UNBENCHMARKED` marker.
> Budget: 6 total (≤ 14), ≤ 5 per corner. ✅

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | each `.claude/knowledge-base/references/{path}` declared in Fase A exists | mark Qx BLOCKED "path not found", continue |
| Per-question Fase A budget | Fase A returned ≥ 1 hotspot OR 3 retries attempted | after 3 retries empty, BLOCKED "Fase A exhausted" |
| After answering Qx | blueprint section under Qx has ≥ 1 citation | re-iterate Qx (1 retry) |
| Web claim (Q4/Q6) | every performance claim has methodology+source OR `UNBENCHMARKED` (R3) | add methodology or flag UNBENCHMARKED |
| Before promising complete | all 4 coverage corners populated | refuse promise, continue |

## Acceptance Criteria

- [ ] All research questions answered OR explicitly BLOCKED with reason
- [ ] All four coverage corners populated in the blueprint
- [ ] Every citation points to a real `.claude/knowledge-base/references/{...}` path (verified)
- [ ] Frontier rigor (R1/R2/R3): RRF + FTS anchored on SOTA + ≥ 2 primary sources; recall claims benchmarked OR `UNBENCHMARKED`
- [ ] ≥ 1 ADR section in the blueprint synthesizes decisions (FTS index, RRF contract, API surface)
- [ ] Time budget respected per project
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint at `.claude/knowledge-base/discoveries/blueprints/m7-hybrid-search-rrf-blueprint.md`

## Edge Cases & MUST-FIX (from /discover-edge-cases)

Absorbed into the questions above; each MUST be answered (or honestly BLOCKED) in the blueprint:

| # | Edge case / risk | MUST-FIX (which question carries it) + reference | Acceptance |
|---|---|---|---|
| E1 | RRF constant `k` (Cormack default 60) — hardcode vs expose as param | Q4: state the contract + cite the paper for 60; field witness `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs` uses `1.0/(60+rank)` | Blueprint Q4 names the `k` decision + primary citation |
| E2 | FTS language config (`to_tsvector('english', …)`) — non-english / `simple` config; generated `tsvector` column language fixed at DDL time | Q5: recommend a default + escape hatch, cite `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql` | Blueprint Q5 covers language config explicitly |
| E3 | paradedb is AGPL — risk of borrowing code, not just technique (D1/D3) | Q2 cites `.claude/knowledge-base/references/paradedb/LICENSE`; Q4 borrows only the published RRF math (paper), never AGPL SQL/code | Blueprint states "technique from paper; paradedb = field witness only" |
| E4 | Recall measurement reuse — does the M2 harness support a keyword/hybrid leg, or is that a gap? | Q6: state whether the harness extends to hybrid OR flag the extension as an M7-S1 implementation task (no fabricated recall — R3) | Blueprint Q6 names the harness gap honestly |
| E5 | `ai.hybrid_search` JSON-array API (spec `docs/features/06-busca-hibrida.md`) is complex — over-engineering vs the manual SQL/RRF path (KISS, parsimony-ladder) | Q4: compare native-fn surface (`.claude/knowledge-base/references/pgvector/README.md` hybrid example) vs manual SQL; recommend the minimal first deliverable | Blueprint Q4 recommends the MVP surface (manual SQL first, native fn as wrapper) |
| E6 | Empty-leg fusion — a query may match only the vector leg OR only the FTS leg (FULL OUTER JOIN + COALESCE) | Q1/Q4: confirm the fusion handles a missing leg, cite `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs` (FULL OUTER JOIN) + `.claude/knowledge-base/references/pgvector/README.md` | Blueprint shows the COALESCE/empty-leg handling with citation |

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed)
- [ ] Final `/discover-confidence` verdict recorded in the blueprint header
- [ ] No fabricated citations
- [ ] Coverage Matrix 100%
- [ ] ADRs reference ≥ 1 project rule/principle (D1 license rule, KISS for PG-native-first, `discover-phd-rigor.md`)
