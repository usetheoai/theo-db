# Discover Edge Case Review — m0-walking-skeleton

Date: 2026-06-26
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/m0-walking-skeleton-plan.md
Research questions analyzed: 10 (Q1–Q10)
Edge cases found: 5 (MUST FIX: 3, SHOULD TEST: 1, DOCUMENT: 1)

---

## MUST FIX

### EC-1: Q2 — `test/sql/*.sql` files do NOT contain `CREATE EXTENSION vector;`

- **Affected question:** Q2
- **Family:** Method + Citation
- **Scenario:** Q2's Fase A grep (`grep -l "CREATE EXTENSION|<=>" .../test/sql/*.sql`) will
  return **zero matches** for `CREATE EXTENSION`. Confirmed: `hnsw_vector.sql` starts with
  `SET enable_seqscan = off;` — no extension load. `vector_type.sql` starts directly with
  `SELECT '[1,2,3]'::vector;`. `cast.sql` starts with `SELECT ARRAY[1,2,3]::vector;`. **None**
  of the 14 files in `test/sql/` contain `CREATE EXTENSION vector;`. The extension is loaded
  by the Perl TAP harness (`$node->safe_psql("postgres", "CREATE EXTENSION vector;")` in
  every `test/t/*.pl` file) — not by the SQL files. The SQL files presuppose the extension is
  already loaded. Furthermore, the plan's expected answer references "e.g. `vector.sql`,
  `hnsw.sql`" — neither file exists. Actual files: `vector_type.sql`, `hnsw_vector.sql`,
  `hnsw_halfvec.sql`, etc.
- **Impact:** During `/discover-execute`, Q2's Fase A will either (a) return zero results and
  trigger the "Fase A exhausted → BLOCKED" stop condition wrongly (the files exist; the grep
  pattern is wrong), or (b) the execute agent changes the grep pattern to `grep -l "<=>"` and
  finds files, but then tries to find `CREATE EXTENSION vector;` inside them and fails,
  producing a fabricated or empty blueprint section for the canonical SQL smoke sequence.
  Either outcome produces an invalid Corner 1 (Integration Tests) blueprint section.
- **Suggested fix:** Reframe Q2: remove `CREATE EXTENSION` from the Fase A grep; change the
  question focus from "extension load + type cast + distance operator" to "minimal SQL
  statements that validate type cast (`::vector`) and distance operator (`<=>`) once the
  extension is pre-loaded (as done by the Perl TAP harness)". New Fase A: `grep -rl "<=>"
  .claude/knowledge-base/references/pgvector/test/sql/`. New Fase B read targets:
  `vector_type.sql` (type casts) + `hnsw_vector.sql` (distance op in a real query). Expected
  answer shape changes to: "minimal 3-statement psql replay: `CREATE TABLE t (val vector(3));`
  + `INSERT INTO t VALUES ('[1,2,3]'), ('[4,5,6]');` + `SELECT val FROM t ORDER BY val <=>
  '[1,2,3]' LIMIT 1;` — no `CREATE EXTENSION` needed here (loaded at harness init)."

---

### EC-2: Q4/Q6 content overlap — both Fase B read the same Dockerfile with no explicit split

- **Affected question:** Q4, Q6
- **Family:** Method + Scope
- **Scenario:** Q4 Fase B says "Read `.claude/knowledge-base/references/pgvector/Dockerfile`
  in full; annotate every RUN directive". Q6 Fase B says "Re-read (or reference from Q4
  answer) `.../Dockerfile`; focus on `ADD https://...#v0.8.3`, `apt-mark hold locales`,
  `make OPTFLAGS=`". During `/discover-execute`, the execute agent will likely:
  (a) read the Dockerfile once in Q4 and produce a comprehensive annotation covering
  everything including the `ADD` instruction and `OPTFLAGS`, then (b) at Q6 either duplicate
  the same content in a second blueprint section (inflating Corner 3: Tools with content
  already in Corner 2: Dependencies) or emit a thin "see Q4" placeholder (leaving Corner 3
  without real content). Both outcomes break the blueprint's corner separation: Corner 2
  should contain the apt dependency table, Corner 3 should contain the container-build
  design decisions.
- **Impact:** Either: (a) duplicate content across two corners, or (b) Corner 3 blueprint
  section with no substantive content → `check_research_coverage.py` may flag corner as
  "zero non-placeholder content" → INVALID score cap (49).
- **Suggested fix:** Add explicit read-scope guard to each question. Q4 reads the Dockerfile
  but ONLY extracts the apt package list (the WHAT): `build-essential`,
  `postgresql-server-dev-$PG_MAJOR`, cleanup order, `apt-get autoremove`, `rm /tmp/pgvector`.
  Q6 reads the Dockerfile but ONLY extracts the design decisions (the HOW/WHY): why
  `ADD https://...#v0.8.3` (git-tag pin vs commit-SHA pin — reproducibility), why
  `make OPTFLAGS=""` (disables -march=native, portable binary), why `apt-mark hold locales`
  (prevents locales upgrade pulling in extra packages). Add to Q4's expected answer: "TABLE:
  package → purpose → build-time-only vs runtime". Add to Q6's expected answer: "TABLE:
  instruction → design rationale → M0 carry-over decision (adopt / adapt / reject)".

---

### EC-3: Q10 — AlloyDB wire-compat claim has no local reference; fabrication risk

- **Affected question:** Q10
- **Family:** Reference path + Citation
- **Scenario:** Q10's expected answer states "AlloyDB Omni wire-compat claim = 'any
  `libpq`-compatible client'". There is no `.claude/knowledge-base/references/alloydb/`
  directory — AlloyDB was NOT cloned at `/roadmap-init` time (no open-source repository
  exists for AlloyDB Omni itself). During `/discover-execute`, the execute agent will attempt
  to provide a `file:line` citation for the AlloyDB wire-compat claim, find no local path,
  and risk either (a) fabricating a file reference (hard cap: `fabricated_citation` → score
  ≤ 49) or (b) citing `cloud.google.com` via WebFetch without the required `UNBENCHMARKED`
  or `EXTERNAL-FETCH` marker, violating R3.
- **Impact:** Hard citation gate failure (`discover-blueprint-golden-rule.md § 1, bullet 2`):
  any `knowledge-base/references/{project}/{path}` that does not exist when checked via
  `Path.exists()` triggers INVALID (score ≤ 49). The AlloyDB anchor (R1 of PhD-rigor profile)
  becomes a liability if cited with a fabricated local path.
- **Suggested fix:** Add an explicit escape clause to Q10: "AlloyDB wire-compat is
  `UNBENCHMARKED` (no `knowledge-base/references/alloydb/` exists). The M0 acceptance
  criterion relies solely on PostgreSQL 17's documented `libpq` wire protocol — no AlloyDB
  citation needed. The AlloyDB anchor (R1) for Q10 is satisfied by the statement: 'PostgreSQL
  17 wire protocol (`libpq`) is the AlloyDB wire-compat surface by documented specification
  (AlloyDB is PostgreSQL-compatible; the claim is structural, not benchmarked).' Any
  WebFetch to `cloud.google.com/alloydb` MUST use the `UNBENCHMARKED` / `EXTERNAL-FETCH`
  marker in the blueprint." Also change the expected answer shape: remove "AlloyDB Omni
  wire-compat claim = 'any libpq-compatible client'" as a citeable source and replace with
  the structural reasoning above.

---

## SHOULD TEST

### EC-4: Q1 blueprint section must document the `psql`-based smoke test as M0 approach, not just describe Perl TAP

- **Affected question:** Q1
- **Suggested halt-loop checkpoint:** After writing the Corner 1 blueprint section from Q1,
  assert: "Does the section include a concrete `psql`-executable smoke sequence (2–5 SQL
  statements, no Perl, no TAP framework setup)? If yes → proceed. If the section only
  describes what `PostgreSQL::Test::Cluster` does without a standalone psql-based sequence
  → iterate to add the psql alternative explicitly." The risk: Q1 is framed as "how does
  pgvector's Perl TAP framework execute the smoke sequence?" — the execute agent may faithfully
  document the Perl TAP pattern (correct answer to the question as written) without deriving
  the simpler psql one-liner that M0's Docker smoke test will actually use. The blueprint
  should explicitly say: "For M0's `smoke.sh`, we do NOT replicate the Perl TAP harness — we
  use: `psql -h localhost -U postgres -c \"CREATE EXTENSION IF NOT EXISTS vector; SELECT
  '[1,2,3]'::vector <=> '[4,5,6]'::vector;\"`". This is needed by Corner 1 acceptance
  criterion: "blueprint documents the raw SQL smoke statement sequence."

---

## DOCUMENT

### EC-5: AlloyDB ScaNN gap is consciously UNBENCHMARKED — no local AlloyDB reference

- **Accepted risk:** Q8 and Q9 both compare pgvector HNSW / pgvectorscale StreamingDiskANN
  against AlloyDB ScaNN as the SOTA anchor (R1 of PhD-rigor profile). AlloyDB is a managed
  Google Cloud product — no open-source repository was cloned. Performance claims between
  pgvector/pgvectorscale and AlloyDB ScaNN cannot be reproduced locally. The plan already
  acknowledges this in Q8's expected answer: "AlloyDB comparison: `UNBENCHMARKED` (R3 — no
  reproducible benchmark available in `.claude/knowledge-base/references/` for AlloyDB)."
  This is the correct and honest treatment per `rules/discover-phd-rigor.md § 1 R3` and
  `public-copy.md § 4`. The `UNBENCHMARKED` marker allows the techniques corner to be fully
  populated while remaining honest about the evidence gap. No fix needed — document as
  accepted design choice for this discovery cycle; AlloyDB ScaNN benchmark data is a
  next-cycle seed.

---

## Summary

| Question | Edges found | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|-------------|----------|-------------|----------|
| Q1 | 1 | 0 | 1 (EC-4) | 0 |
| Q2 | 2 | 1 (EC-1) | 0 | 0 |
| Q3 | 0 | 0 | 0 | 0 |
| Q4 | 1 | 1 (EC-2, shared) | 0 | 0 |
| Q5 | 0 | 0 | 0 | 0 |
| Q6 | 1 | 1 (EC-2, shared) | 0 | 0 |
| Q7 | 0 | 0 | 0 | 0 |
| Q8 | 1 | 0 | 0 | 1 (EC-5) |
| Q9 | 0 | 0 | 0 | 0 |
| Q10 | 1 | 1 (EC-3) | 0 | 0 |

**Verdict: DISCOVERY PLAN NEEDS ADJUSTMENT**

Three MUST FIX items must be absorbed into plan v1.1 before `/discover-execute` can run:
- EC-1: Q2 grep pattern and focus reframed (no `CREATE EXTENSION` in SQL files)
- EC-2: Q4/Q6 explicit content-split guards added (Dockerfile read scope)
- EC-3: Q10 AlloyDB wire-compat escape clause added (`UNBENCHMARKED`, no fabricated citation)

One SHOULD TEST checkpoint added to the halt-loop (EC-4: Q1 psql smoke derivation).
One DOCUMENT entry confirming consciously accepted evidence gap (EC-5: AlloyDB UNBENCHMARKED).
