# Discovery Plan: Permissive BM25 lexical ranking for PostgreSQL (non-AGPL)

> **Version 1.0** — Investigate a **permissive (non-AGPL)** BM25 / lexical full-text ranking option for
> TheoDB, since ParadeDB `pg_search` (the BM25 SOTA witness) is AGPL-3.0 and barred by D1. The blueprint
> decides M7-S2's deliverable: adopt a truly-permissive BM25 extension IF one exists and is mature, OR own a
> permissive BM25 scoring implemented over PostgreSQL's native FTS statistics (`ts_stat`/tsvector) — both
> 100% permissive, zero AGPL. The DoD of M7-S2 (ROADMAP) is "alternativa permissiva a BM25 full-text
> identificada"; this discovery produces that identification with license evidence + the algorithm.

**Slug:** `m7-bm25-permissive`
**Owner:** paulohenriquevn
**Created:** 2026-06-28
**Time budget:** 6h (per-project breakdown in ADR D1)

## Context

ROADMAP `### M7` top-risk #1 + DoD: "Sem peça permissiva madura para BM25 full-text (paradedb `pg_search` é
AGPL)" → "**alternativa permissiva** identificada para full-text BM25". M7-S1 already confirmed
`paradedb/LICENSE` is AGPL-3.0 (barred by PRD D1 / CLAUDE.md TheoDB rule 2). M7-S1 shipped hybrid search with
the FTS leg on PostgreSQL-native `ts_rank_cd`+GIN; the open question it deferred is whether a **true BM25**
lexical leg (Okapi TF-IDF with document-length normalization, the field-standard the SOTA `pg_search` and
AlloyDB hybrid use) can be obtained on permissive terms. BM25 generalizes better than `ts_rank_cd` on
heterogeneous corpora (BEIR, Thakur et al. 2021), so closing this gap strengthens the hybrid leg's recall.
The SOTA anchor (ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`) is AlloyDB + ParadeDB
`pg_search` (AGPL). This discovery investigates the permissive replacement, with license due-diligence as a
first-class corner (D1 is a release gate — PRD §11).

## Objective

Produce a blueprint that decides the permissive BM25 path for TheoDB: which option (adopt-extension vs
own-in-SQL), with license evidence + the Okapi BM25 algorithm + an integration shape.

- [ ] All research questions answered with citations to `.claude/knowledge-base/references/` or allowlisted sources
- [ ] Cross-cutting comparison table populated for every candidate BM25 option
- [ ] Recommendations section gives one concrete decision proposal (adopt vs own) with license rationale
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS (frontier threshold ≥ 75)

## In-Scope / Out-of-Scope

### In-Scope (per reference project / source)

| Project / source | In-scope | Reason |
|---|---|---|
| `.claude/knowledge-base/references/paradedb/` | `LICENSE`, `pg_search/README.md`, `pg_search/.../columnar_advanced_06_score_function.sql`, `tests/tests/bm25_search.rs` | The BM25 SOTA witness — confirm AGPL (barred) + read the BM25 surface/technique (study-only, never copy AGPL code — D3) |
| `.claude/knowledge-base/references/supabase-postgres/` | `nix/tests/sql/docs-full-text-search.sql`, `nix/tests/sql/z_15_rum.sql` | PG-native FTS (`ts_rank`/`ts_rank_cd`/GIN) + RUM — the permissive built-in ranking baseline to compare BM25 against |
| `.claude/knowledge-base/references/pgvector/` | `README.md` | Hybrid-search context (the leg BM25 would replace `ts_rank_cd` in) |
| Allowlisted web (`github.com`, `www.postgresql.org`, `arxiv.org`, `dl.acm.org`) | VectorChord-bm25 / candidate extensions' LICENSE + README; PostgreSQL `ts_stat`/textsearch docs; BM25 primary source | Candidate permissive BM25 extensions are NOT cloned — license + maturity verified via their canonical repos/docs |

### Out-of-Scope (explicit)

| Project / source | Why excluded |
|---|---|
| `paradedb/pg_search/` **as an implementation to vendor** | AGPL-3.0 → barred by D1. Read for technique only, never copied. |
| Non-BM25 lexical methods (pure trigram `pg_trgm`, fuzzy) | Out of the BM25-specific scope |
| Hybrid fusion / RRF | Already delivered in M7-S1 — this discovery is only the BM25 *leg* |
| Any source NOT under `references/` and NOT in `rules/discover-web-allowlist.txt` | Cross-Project Rule + allowlist (R5) |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** paradedb 1h (license + BM25 surface), supabase-postgres 1h (native FTS baseline), allowlisted
web 3h (candidate-extension license/maturity + `ts_stat` docs + BM25 paper), synthesis 1h.

**Rationale:** the load-bearing work is the web license/maturity audit of candidate BM25 extensions (the
"is there a permissive mature piece?" question) + the BM25 algorithm primary source — these decide adopt-vs-own.

**Alternatives considered:** equal split (rejected — paradedb's license is a 1-line confirm); single-source
(rejected — the permissive-vs-AGPL decision needs cross-checking ≥ 2 candidates + the native baseline).

**Stop condition — per question (mandatory):** when a question's Fase A returns empty after 3 query-variant
retries, mark BLOCKED with reason "Fase A exhausted"; continue. Never pad with unrelated hits.

**Stop condition — per source (mandatory):** when a source's budget is exhausted with questions pending,
mark them BLOCKED with reason "budget exhausted". If every remaining question is `done` or honest `blocked`,
emit `<promise>BLUEPRINT_BLOCKED</promise>` (never `BLUEPRINT_COMPLETE` from a blocked state).

**Anti-pattern:** NEVER fabricate a license verdict or a benchmark number (Unbreakable Rule 3). A license
that cannot be confirmed from the canonical repo is marked `UNVERIFIED`, not assumed permissive.

**Consequences:** blocked questions surface in the blueprint as seeds; an unconfirmed-license candidate is
explicitly excluded from the recommendation (fail-closed on D1).

### D2 — Investigation depth

**Decision:** Read the cited BM25 SQL/algorithm sources end-to-end; for web candidates, fetch the LICENSE file
+ README maturity signals (release cadence, archived?, last commit) — license verdict is binary and must be
sourced from the canonical repo, never memory.

**Rationale:** D1 (no AGPL) is a release gate; a license verdict from memory is exactly the Rule-3 violation
this discovery must avoid. The BM25 formula must be read line-exact to be implementable.

**Consequences:** trade breadth for license precision on the candidates that matter.

### D3 — Borrow the BM25 technique (public algorithm), never AGPL code

**Decision:** ParadeDB `pg_search` is read ONLY to understand the BM25 surface (index DDL, `k1`/`b` params,
score operator). No AGPL code/schema/test is copied. The Okapi BM25 formula is the public Robertson/Spärck
Jones technique, citable from the primary source.

**Rationale:** D1 bars AGPL from the distribution; algorithms are not the licensed artifact.

**Consequences:** the blueprint cites the paper/PostgreSQL docs as authority + paradedb as a field witness of
the surface only.

## Research Questions

| # | Question | Corner | Source(s) | Fase A (broad map) | Fase B (deep) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How does the OSS stack test lexical-ranking correctness (what is asserted: order, score, recall)? | tests | `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql`, `.claude/knowledge-base/references/paradedb/pg_search/tests/pg_regress/sql/columnar_advanced_06_score_function.sql` | Grep `ts_rank\|@@\|pdb.score\|ORDER BY` in both | Read both: capture the ranking-assertion style (membership vs exact score vs order) | Table: test → what it asserts, with `path:line` per row |
| Q2 | License + maturity of every candidate BM25 piece — which are permissive (D1-clean) vs AGPL-barred? | deps | `.claude/knowledge-base/references/paradedb/LICENSE`; allowlisted `github.com` LICENSE/README of candidates (e.g. VectorChord-bm25), `www.postgresql.org` | Read `paradedb/LICENSE`; WebFetch each candidate's `LICENSE` + README (release/last-commit/archived) | Confirm each license verbatim from the canonical repo; flag `UNVERIFIED` if not resolvable | Dep table: piece → license → mature? → in-distribution? (yes/no + reason) |
| Q3 | Install/build cost: native FTS (zero-install) vs a BM25 extension (build/PGXS/rust) vs SQL-owned BM25 | tools | `.claude/knowledge-base/references/paradedb/pg_search/README.md`; allowlisted candidate READMEs; `www.postgresql.org` (textsearch) | Read pg_search README install section; WebFetch candidate install docs | Capture the build/runtime cost + whether it ships permissively in our image | Comparison: option → install cost → ships in TheoDB image? |
| Q4 | The Okapi BM25 formula (TF, IDF, doc-length norm, `k1`/`b` defaults) — the algorithm to own | techniques | allowlisted `dl.acm.org`/`arxiv.org` (Robertson & Zaragoza BM25 primary) ; `.claude/knowledge-base/references/paradedb/tests/tests/bm25_search.rs` (surface witness) | WebFetch the BM25 primary source; Grep `k1\|b \|bm25` in the paradedb test | Read the formula + the default `k1=1.2,b=0.75` justification; capture the surface from the witness | The BM25 score formula + parameter defaults + citation |
| Q5 | Can BM25 be computed over PostgreSQL's native FTS statistics (`ts_stat`, tsvector, doc length) on permissive terms — and how does that compare to `ts_rank_cd`? | techniques | `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql`; allowlisted `www.postgresql.org` (`ts_stat`, textsearch internals) | Grep `ts_rank\|tsvector\|ts_stat` in the supabase SQL; WebFetch PG `ts_stat`/textsearch docs | Read what term-frequency + document statistics PG exposes natively; assess feasibility of BM25-in-SQL | Feasibility verdict: native stats available for TF/IDF/doclen? + gap vs ts_rank_cd |
| Q6 | What does the SOTA expose (paradedb BM25 surface, AlloyDB) and what is the permissive equivalent TheoDB should own (adopt-vs-own decision inputs)? | techniques | `.claude/knowledge-base/references/paradedb/pg_search/README.md`, `.claude/knowledge-base/references/pgvector/README.md`; allowlisted `cloud.google.com` (AlloyDB) | Grep the pg_search README BM25 surface; WebFetch AlloyDB lexical/hybrid doc | Capture the SOTA surface + the gap TheoDB closes permissively | Table: SOTA surface → permissive-equivalent option → gap; bare perf claims marked `UNBENCHMARKED` (R3) |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q1 | Covered |
| Dependencies | Q2 | Covered |
| Tools | Q3 | Covered |
| Techniques | Q4, Q5, Q6 | Covered (≥ 2 — frontier R4) |

**Coverage: 4/4 corners covered (100%)**

> **TheoDB frontier rigor** (`rules/discover-phd-rigor.md`): techniques = 3 questions; each (R1) anchored on
> the ParadeDB/AlloyDB BM25 SOTA with the gap stated, (R2) backed by ≥ 2 primary sources (BM25 primary paper +
> PostgreSQL textsearch docs + cloned reference witnesses), (R3) any ranking-quality claim carries methodology
> + source OR the literal `UNBENCHMARKED` marker. License verdicts are sourced from canonical repos, never
> memory (D2). Budget: 6 total (≤ 14), ≤ 5 per corner. ✅

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | each `.claude/knowledge-base/references/{path}` declared in Fase A exists | mark Qx BLOCKED "path not found", continue |
| Web source (Q2/Q4/Q6) | the source is in `rules/discover-web-allowlist.txt` | do not cite; find an allowlisted equivalent or mark BLOCKED |
| License verdict (Q2) | the license is quoted from the candidate's canonical repo | mark the candidate `UNVERIFIED` (excluded from recommendation), never assume permissive |
| Perf/recall claim (Q4/Q5/Q6) | methodology+source OR `UNBENCHMARKED` | add methodology or flag UNBENCHMARKED (R3) |
| Before promising complete | all 4 coverage corners populated | refuse promise, continue |

## Acceptance Criteria

- [ ] All research questions answered OR explicitly BLOCKED with reason
- [ ] All four coverage corners populated in the blueprint
- [ ] Every reference citation points to a real `.claude/knowledge-base/references/{...}` path; every web citation is allowlisted
- [ ] Frontier rigor (R1/R2/R3): BM25 anchored on SOTA + ≥ 2 primary sources; ranking claims benchmarked OR `UNBENCHMARKED`
- [ ] License due-diligence: every candidate has a verbatim-sourced license verdict OR is `UNVERIFIED` (D1 fail-closed)
- [ ] ≥ 1 ADR in the blueprint synthesizes the adopt-vs-own decision
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint at `.claude/knowledge-base/discoveries/blueprints/m7-bm25-permissive-blueprint.md`

## Edge Cases & MUST-FIX (from /discover-edge-cases)

| # | Edge case / risk | MUST-FIX (which question) | Acceptance |
|---|---|---|---|
| E1 | A candidate BM25 extension claims permissive but a transitive component (or a dual-license) is AGPL | Q2 — verify the LICENSE file verbatim from the canonical repo (`.claude/knowledge-base/references/paradedb/LICENSE` is the confirmed-AGPL baseline); flag `UNVERIFIED` on any non-resolvable license | no candidate enters the recommendation without a sourced license verdict |
| E2 | "BM25 in SQL" over `ts_stat` may be too slow / not index-served (only a post-filter compute) | Q5 — state the performance posture honestly (`UNBENCHMARKED` until measured); compare to `ts_rank_cd` | Q5 marks any perf claim `UNBENCHMARKED` (R3) |
| E3 | Studying paradedb (AGPL) risks copying code, not just the BM25 technique (D1/D3) | Q4 cites the BM25 *paper* as authority; `.claude/knowledge-base/references/paradedb/tests/tests/bm25_search.rs` is a surface witness only | blueprint states "formula from paper; paradedb = surface witness, never copied" |
| E4 | A candidate may be mature-looking but archived / unmaintained | Q2/Q3 — capture last-commit / release / archived signals from the canonical repo | maturity signal recorded per candidate |
| E5 | Native `ts_rank_cd` (already shipped in M7-S1) might already be "good enough" → BM25 may be YAGNI | Q5/Q6 — state the concrete recall gap BM25 closes vs `ts_rank_cd` (or honestly conclude native is sufficient for now) | the adopt-vs-own-vs-keep-native decision is explicit + justified |
| E6 | The recommendation must be implementable for M7-S2 with D1-clean evidence (no AGPL, real test) | Q6 — the recommended option must be one TheoDB can ship + test without AGPL | recommendation names a concrete, D1-clean, testable deliverable |

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed)
- [ ] Final `/discover-confidence` verdict recorded in the blueprint header
- [ ] No fabricated citations; no assumed license verdicts
- [ ] Coverage Matrix 100%
- [ ] ADRs reference ≥ 1 project rule/principle (D1 license rule, `discover-phd-rigor.md`, parsimony-ladder for adopt-vs-own)
