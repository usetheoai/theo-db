# Discovery Plan: M0 Walking Skeleton — PostgreSQL 17 + pgvector Container

> **Version 1.1** — This discovery investigates how to assemble the thinnest deployable
> slice of TheoDB: a container image that starts PostgreSQL 17 with `pgvector` installed,
> accepts a wire connection, and passes a smoke test that runs `CREATE EXTENSION vector;`
> + an `<=>` cosine-similarity query. In scope: pgvector (container packaging, build deps,
> test framework), pgvectorscale (ANN technique gap vs HNSW, AlloyDB SOTA anchor),
> and supabase-postgres (container layer strategy cross-check). Output is a blueprint
> with a concrete Dockerfile recipe, smoke-test skeleton, and an ADR confirming the
> "no engine fork" constraint from `CLAUDE.md` Rule 2 / PRD D3.

**Slug:** `m0-walking-skeleton`
**Owner:** paulo / Claude Code
**Created:** 2026-06-26
**Time budget:** 4h total (breakdown in ADR D1 below)

---

## Context

M0 is the foundation milestone defined in `ROADMAP.md`:

> **Objective:** Thinnest deployable slice — container that starts PG17 + pgvector, accepts
> wire connections, and proves the "no engine fork" architecture works end-to-end.
>
> **DoD:**
> 1. Container builds and accepts a PostgreSQL wire connection.
> 2. `CREATE EXTENSION vector;` + `<=>` similarity query passes in an automated smoke test.
> 3. ADR "no engine fork" committed in `docs/adr/`.

M0 is entirely pre-code today (no source files yet). The primary risk is conflating
"container builds" with "container works" — a Dockerfile that compiles but fails at
`CREATE EXTENSION` is not done. The second risk is scope creep: M0 must NOT pre-build
pgvectorscale (Rust toolchain, pgrx) — that is M2 territory. pgvectorscale is studied
here only to understand the ANN technique gap that drives M0's `UNBENCHMARKED` status
for techniques corner.

Triggered by: `ROADMAP.md § M0`, `CLAUDE.md § Regras específicas do TheoDB` (Rules 1–7),
`PRD.md § 15 D3` (Fork Policy), and `rules/discover-phd-rigor.md` (SOTA-anchoring mandate).

---

## Objective

Produce a blueprint that enables the team to decide: **exactly which Dockerfile recipe,
smoke-test skeleton, and ADR text are needed to satisfy M0's three DoD bullets** —
with enough benchmark context to know what the ANN gap with AlloyDB is, even though
closing that gap is deferred to M2.

Success criteria:

- [ ] All 10 research questions answered OR explicitly BLOCKED with reason
- [ ] All four coverage corners have populated blueprint sections
- [ ] Every citation resolves under `.claude/knowledge-base/references/`
- [ ] Techniques corner: AlloyDB SOTA anchor present (R1), ≥ 2 primary sources per
      technique claim (R2), every perf claim benchmarked or `UNBENCHMARKED` (R3)
- [ ] `/discover-confidence` verdict ≥ `SHIPPABLE_WITH_CAVEATS`

---

## In-Scope / Out-of-Scope

### In-Scope

| Project | Subdirectories in scope | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvector/` | `Dockerfile`, `test/t/`, `test/sql/`, `README.md`, `src/`, `sql/` | Primary reference for M0: container build + smoke test pattern |
| `.claude/knowledge-base/references/pgvectorscale/` | `README.md`, `DEVELOPMENT.md`, `pgvectorscale/Cargo.toml`, `pgvectorscale/src/`, `pgvectorscale/benches/`, `pgvectorscale/vectorscale.control` | ANN technique gap (HNSW vs StreamingDiskANN vs AlloyDB ScaNN) — studied here; built in M2 |
| `.claude/knowledge-base/references/supabase-postgres/` | `Dockerfile-17`, `docker/pgctld/` | Container layer strategy cross-check; wire-compat acceptance criterion pattern |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/supabase-postgres/` (Nix pipeline, Nix store) | Nix-based build is heavy machinery; M0 needs a single-Dockerfile approach (KISS Rule 10) |
| `.claude/knowledge-base/references/cloudnative-pg/` | cloudnative-pg's `Dockerfile` is the operator binary, not a PostgreSQL container — wrong layer |
| `.claude/knowledge-base/references/pgvectorscale/` (Rust toolchain, pgrx build) | pgvectorscale runtime installation is M2 scope; M0 only studies the ANN technique gap |
| `.claude/knowledge-base/references/patroni/`, `pgbackrest/`, `duckdb/`, `pg_mooncake/` | M0 has no HA, no columnar — deferred to M4 and M3 respectively |
| Any project NOT in `.claude/knowledge-base/references/` | Cross-project rule: never cite a source not locally available |

---

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvector: 2h (primary — Dockerfile + test framework), pgvectorscale: 1h
(ANN technique gap only, no build), supabase-postgres: 1h (layer strategy + wire compat).

**Rationale:** pgvector is the M0 primary artifact source — deepest dive. pgvectorscale
is studied only for techniques corner (understanding the HNSW → StreamingDiskANN gap the
AlloyDB ScaNN SOTA reveals). supabase-postgres is the cross-check for layer strategy and
wire-compat acceptance criterion.

**Alternatives considered:** equal 1.3h split (wrong — pgvector carries the most weight),
skip pgvectorscale (wrong — techniques corner would be empty of SOTA anchor), deep dive
into cloudnative-pg (wrong — its Dockerfile is for the Go operator, not PG).

**Stop condition — per question:** When Fase A returns no matches after 3 query variants
(different patterns, different paths), mark the question BLOCKED with reason
"Fase A exhausted — no hotspots found" and continue to the next.

**Stop condition — per project:** When time budget is exhausted with questions still pending,
mark all remaining questions for that project BLOCKED with reason "budget exhausted".
If all questions are done or blocked: emit `<promise>BLUEPRINT_COMPLETE</promise>` when
the 4 corners are covered; otherwise emit `<promise>BLUEPRINT_BLOCKED</promise>` with the
honest blocked-questions report.

**Anti-pattern:** NEVER fabricate Fase B answers for a question whose Fase A was exhausted.
Honest BLOCKED > false PASS (Unbreakable Rule 3).

**Consequences:** Blocked questions become next-discovery seeds. The blueprint surfaces them
explicitly in `## Blocked questions (if any)`.

---

### D2 — Investigation depth

**Decision:** For code-shape questions (Perl tests, Dockerfile RUN layers, Rust control
files) use Fase A (ast-grep / grep / find) to locate hotspots, then Fase B (Read) to
capture intent and line-exact citations. For text-shape questions (README benchmark claims,
DEVELOPMENT.md toolchain prerequisites) skip Fase A and go directly to Fase B (Read in full).

**Rationale:** Text-shape documents do not benefit from AST traversal. Code-shape questions
need the hotspot map first to avoid reading irrelevant files (parsimony ladder rung 5).

**Alternatives considered:** Read all files in all reference projects top-to-bottom
(too slow, violates KISS Rule 10), grep-only without Read (misses intent + edge-case
comments that matter for the blueprint).

**Consequences:** Fase A may declare SKIP for text-shape questions — this is intentional
and documented per question.

---

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad) | Fase B (deep — Read) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How does pgvector's Perl tap framework (`PostgreSQL::Test::Cluster`) execute the minimal smoke sequence: node init → `CREATE EXTENSION vector;` → `<=>` similarity query → assertion? | tests | `.claude/knowledge-base/references/pgvector/test/t/` | `find .claude/knowledge-base/references/pgvector/test/t/ -name "*.pl"` — list all test scripts; grep for `CREATE EXTENSION vector` to identify which file(s) contain the minimal load sequence | Read `.claude/knowledge-base/references/pgvector/test/t/003_ivfflat_vector_build_recall.pl` in full to capture: `$node->init`, `$node->start`, `safe_psql("CREATE EXTENSION vector;")`, vector INSERT + ORDER BY `<=>` + assertion | Step-by-step annotated sequence with `file:line` citations; what `PostgreSQL::Test::Cluster` provides vs what we must write |
| Q2 | What SQL statements in pgvector's `test/sql/` validate the type cast (`::vector`) + distance operator (`<=>`) once the extension is pre-loaded by the harness? (EC-1 fix: `CREATE EXTENSION` is NOT in the SQL files — it is loaded by the Perl TAP harness in `test/t/*.pl`. SQL files presuppose extension already active.) | tests | `.claude/knowledge-base/references/pgvector/test/sql/` | `grep -rl "<=>" .claude/knowledge-base/references/pgvector/test/sql/` — list files exercising the distance operator (no `CREATE EXTENSION` in these files) | Read `vector_type.sql` (type casts: `::vector`, `::vector(N)`) + `hnsw_vector.sql` (distance op `<=>` in real ORDER BY query); note that `CREATE EXTENSION vector;` is handled at harness init (Perl `safe_psql`), not in `.sql` files | Minimal 3-statement psql replay: `CREATE TABLE t (val vector(3)); INSERT INTO t VALUES ('[1,2,3]'), ('[4,5,6]'); SELECT val FROM t ORDER BY val <=> '[1,2,3]' LIMIT 1;` — no `CREATE EXTENSION` needed here (loaded at smoke-test init); annotated with `file:line` |
| Q3 | Does supabase-postgres's pgctld wrapper include any extension health-check or smoke-test hook invocable from outside the container?  What does the pattern look like? | tests | `.claude/knowledge-base/references/supabase-postgres/docker/pgctld/` | `ls .claude/knowledge-base/references/supabase-postgres/docker/pgctld/` — enumerate wrapper scripts; grep for `healthcheck\|smoke\|check\|extension` | Read `.claude/knowledge-base/references/supabase-postgres/docker/pgctld/pgctld-wrapper.sh` and any companion shell scripts in full | Either "yes, pattern is X" (with `file:line`) or "no — must write a fresh `smoke.sh` for TheoDB M0" |
| Q4 | What are the exact apt build dependencies in pgvector's Dockerfile for PG17 (`build-essential`, `postgresql-server-dev-$PG_MAJOR`, etc.)? (EC-2 scope guard: ONLY extract the apt package list — the WHAT. Design decisions for `ADD`, `OPTFLAGS`, `apt-mark hold` rationale go to Q6.) | deps | `.claude/knowledge-base/references/pgvector/` | SKIP — text-shape (Dockerfile); `find .claude/knowledge-base/references/pgvector -name "Dockerfile*"` to confirm exactly one Dockerfile exists | Read `.claude/knowledge-base/references/pgvector/Dockerfile`; ONLY annotate apt package lines: which packages are installed, purpose of each, and cleanup order (`apt-get autoremove`, `rm /tmp/pgvector`); DO NOT describe `ADD` instruction or OPTFLAGS (those are Q6 scope) | TABLE: package → purpose → build-time-only vs runtime; cleanup sub-commands in order; `ARG PG_MAJOR` parameterisation note |
| Q5 | What additional build toolchain does pgvectorscale require beyond pgvector's C build (Rust stable, `cargo-pgrx` pinned version, `pkg-config`, `libssl-dev`)? What is the minimal toolchain installation sequence from `DEVELOPMENT.md`? | deps | `.claude/knowledge-base/references/pgvectorscale/` | SKIP — text-shape; `find .claude/knowledge-base/references/pgvectorscale -name "DEVELOPMENT.md" -o -name "Cargo.toml"` | Read `.claude/knowledge-base/references/pgvectorscale/DEVELOPMENT.md` for the prerequisite list + install commands; read `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml` for declared crate version | Ordered dependency list: OS packages → Rust toolchain → cargo-pgrx version → build command; M0 note: this toolchain is explicitly OUT of M0 scope (M2) — captured as a "deferred" block in the blueprint |
| Q6 | What are the container-image build DESIGN DECISIONS in pgvector's Dockerfile: the `ADD https://github.com/.../pgvector.git#v0.8.3` git-tag pin strategy, the `make OPTFLAGS=""` portability choice, and the `apt-mark hold locales` rationale? (EC-2 scope guard: ONLY extract the HOW/WHY design rationale. Apt package list is Q4 scope.) | tools | `.claude/knowledge-base/references/pgvector/` | SKIP — Dockerfile already available from Q4; read with design-decision focus | Re-read `.claude/knowledge-base/references/pgvector/Dockerfile`; focus EXCLUSIVELY on: (1) `ADD https://...#v0.8.3` — why git-tag pin vs commit-SHA pin (reproducibility argument); (2) `make OPTFLAGS=""` — why empty (disables `-march=native`, produces portable binary); (3) `apt-mark hold locales` — why hold (prevents locales upgrade from pulling in extra packages in single-RUN layer) | TABLE: instruction → design rationale → M0 carry-over decision (adopt / adapt / reject); answer: is the `ADD` git-tag pin portable to TheoDB M0 Dockerfile? |
| Q7 | How does supabase-postgres's `Dockerfile-17` package PostgreSQL 17 + extensions (Nix-based vs direct apt)?  Is its approach suitable for TheoDB's single-Dockerfile M0, or does it over-engineer the packaging? | tools | `.claude/knowledge-base/references/supabase-postgres/` | SKIP — text-shape; `find .claude/knowledge-base/references/supabase-postgres -name "Dockerfile-17"` | Read `.claude/knowledge-base/references/supabase-postgres/Dockerfile-17` in full; identify: base image strategy (Alpine + Nix), multi-stage count, extension install mechanism, Nix store size implications | Verdict table: supabase approach → complexity/size trade-off → "UNSUITABLE for M0 because Nix adds X MB and N stages; pgvector apt approach is the KISS rung-5 choice" with `file:line` evidence |
| Q8 | What ANN index types does pgvector expose (IVFFlat, HNSW), and what does AlloyDB Omni's ScaNN extension use as its SOTA approach? What is the M0-relevant gap (and why closing it is deferred)? | techniques | `.claude/knowledge-base/references/pgvector/` | `grep -n "HNSW\|IVFFlat\|ScaNN\|index" .claude/knowledge-base/references/pgvector/README.md \| head -30` | Read pgvector `README.md` §§ "Getting Started", "Index" in full; focus on HNSW parameters (`m`, `ef_construction`), IVFFlat (`lists`, `probes`), recall vs throughput trade-offs | Table: index type → algorithm paper → M0 status (HNSW in scope, IVFFlat in scope, ScaNN=AlloyDB SOTA=M2 gap); performance claims in README cited as `UNBENCHMARKED` for AlloyDB comparison (R3 — no reproducible benchmark available in `.claude/knowledge-base/references/` for AlloyDB) |
| Q9 | How does pgvectorscale's StreamingDiskANN differ from pgvector's HNSW in recall, latency, and build cost? What published benchmark evidence (Timescale vs Pinecone) justifies including it in TheoDB's ANN stack over plain pgvector? | techniques | `.claude/knowledge-base/references/pgvectorscale/` | `grep -n "DiskANN\|diskann\|HNSW\|benchmark\|recall\|latency\|throughput" .claude/knowledge-base/references/pgvectorscale/README.md \| head -40` | Read `.claude/knowledge-base/references/pgvectorscale/README.md` §§ "Why pgvectorscale?" + benchmark section; read `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/vectorscale.control` (`requires = 'vector'` — confirms pgvector is a hard dep); note that the Filtered DiskANN algorithm cites `dl.acm.org/doi/10.1145/3543507.3583552` (in allowlist) | Benchmark table: dataset → recall@k → p95 latency → methodology; SOTA anchor: AlloyDB uses ScaNN (in-process, not via SQL extension) — TheoDB uses StreamingDiskANN (permissive, SQL-native); gap acknowledged; M0 does NOT include pgvectorscale (M2 scope) |
| Q10 | What wire-protocol surface does PostgreSQL 17 expose by default (TCP 5432, `libpq` socket), and is a successful `psql -h localhost -c "SELECT 1;"` + `CREATE EXTENSION vector; SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;"` sufficient as M0's wire-compatibility acceptance criterion? What does the supabase-postgres pgctld say about the readiness signal? (EC-3 fix: AlloyDB wire-compat claim is `UNBENCHMARKED` — no `knowledge-base/references/alloydb/` directory exists; do NOT cite a local alloydb path; the R1 anchor for Q10 is structural reasoning only.) | techniques | `.claude/knowledge-base/references/supabase-postgres/docker/pgctld/` | `grep -n "port\|5432\|ready\|listening\|startup\|pg_isready" .claude/knowledge-base/references/supabase-postgres/docker/pgctld/pgctld-wrapper.sh` | Read `.claude/knowledge-base/references/supabase-postgres/docker/pgctld/pgctld-wrapper.sh` and `.claude/knowledge-base/references/supabase-postgres/docker/pgctld/postgresql.conf.tmpl` to confirm port defaults and readiness-signal pattern | Decision: `pg_isready` + extension smoke (`<=>` query returns a float) is the correct M0 acceptance criterion; AlloyDB wire-compat anchor (R1): PostgreSQL 17 `libpq` wire protocol IS the AlloyDB wire-compat surface by documented specification (AlloyDB is PostgreSQL-compatible — structural claim, NOT citeable from a local file); `UNBENCHMARKED` for AlloyDB comparison (no `knowledge-base/references/alloydb/`); `UNBENCHMARKED` for wire latency (not in scope for M0); any WebFetch to cloud.google.com/alloydb MUST use `EXTERNAL-FETCH` marker |

---

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q1, Q2, Q3 | Covered (3 questions) |
| Dependencies | Q4, Q5 | Covered (2 questions) |
| Tools | Q6, Q7 | Covered (2 questions) |
| Techniques | Q8, Q9, Q10 | Covered (3 questions, ≥ 2 per frontier mandate) |

**Coverage: 4/4 corners covered (100%)**

> **TheoDB frontier rigor** (`rules/discover-phd-rigor.md`): techniques corner carries 3
> questions, each anchored on AlloyDB/ScaNN SOTA (R1), backed by ≥ 2 sources: pgvector
> README + pgvectorscale README + dl.acm.org FilteredDiskANN (R2), every performance
> claim either benchmarked from the reference README or explicitly marked `UNBENCHMARKED`
> (R3). Budget: 10 questions total, max 3/corner — within 6-14 range, ≤ 5/corner ✓.

---

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before Fase A of any Q | Declared reference path exists under `.claude/knowledge-base/references/` | Mark Q BLOCKED "path not found", continue to next Q |
| Per-question Fase A budget | Fase A returned ≥ 1 match OR SKIP declared (text-shape) OR 3 query variants tried | After 3 retries empty: BLOCKED "Fase A exhausted"; continue |
| After Fase B of any Q | Blueprint section for that Q has ≥ 1 citation with `file:line` | Re-iterate Q (1 retry max) |
| After Q1 Corner 1 section | Blueprint Corner 1 includes a concrete `psql`-executable smoke sequence (2–5 SQL statements, no Perl, no TAP framework setup) (EC-4) | Iterate: add `psql -h localhost -U postgres -c "CREATE EXTENSION IF NOT EXISTS vector; SELECT '[1,2,3]'::vector <=> '[4,5,6]'::vector;"` as the M0 `smoke.sh` line explicitly |
| Mid-loop corner sanity | After Q5: corners 1+2 fully answered; after Q7: corners 3 answered | If any corner empty, prioritise remaining questions for that corner |
| Techniques R1/R2/R3 gate | Q8/Q9/Q10 each cite ≥ 2 sources; performance claims benchmarked or `UNBENCHMARKED` | Add a missing citation or add the `UNBENCHMARKED` marker; do NOT emit promise |
| Before `BLUEPRINT_COMPLETE` | All 4 corners populated; no fabricated citations; R1/R2/R3 satisfied | Refuse promise; continue iterating |

---

## Acceptance Criteria

- [ ] Q1–Q10: all answered with `file:line` citations OR explicitly BLOCKED with reason
- [ ] Corner 1 (Integration Tests): blueprint section documents the Perl tap smoke sequence AND the raw SQL smoke statement sequence
- [ ] Corner 2 (Dependencies): blueprint section documents exact apt build deps (Q4) AND pgvectorscale extra toolchain acknowledged as M2-deferred (Q5)
- [ ] Corner 3 (Tools): blueprint section confirms pgvector apt approach as M0 pattern (Q6) AND dismisses supabase-postgres Nix approach with evidence (Q7)
- [ ] Corner 4 (Techniques): HNSW/IVFFlat vs StreamingDiskANN vs AlloyDB ScaNN gap table (Q8+Q9), wire-compat criterion confirmed (Q10); AlloyDB anchor present (R1); ≥ 2 sources/technique (R2); `UNBENCHMARKED` markers for claims without local benchmark data (R3)
- [ ] At least one ADR in the blueprint: "no engine fork" + why pgvector apt (not Nix, not pgvectorscale at M0) satisfies M0 parsimony ladder
- [ ] `/discover-confidence` verdict ≥ `SHIPPABLE_WITH_CAVEATS`
- [ ] Blueprint saved at `.claude/knowledge-base/discoveries/blueprints/m0-walking-skeleton-blueprint.md`

---

## Global Definition of Done

- [ ] All phases completed: plan → edge-cases → plan-confidence → execute → confidence → improve if needed → re-score
- [ ] Final `/discover-confidence` verdict recorded in blueprint header
- [ ] No fabricated citations (all `knowledge-base/references/...` paths verified to exist)
- [ ] Coverage Matrix 100% — all 4 corners populated
- [ ] ADRs in blueprint reference at least one principle: `rules/parsimony-ladder.md` (KISS/YAGNI), `rules/architecture.md` (DIP), `CLAUDE.md` Rules 2+4 (Apache 2.0, no engine fork)
- [ ] `CHANGELOG.md [Unreleased]` updated with discovery entry
