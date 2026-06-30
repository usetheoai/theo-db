# Discovery Plan: M20 — Own `vector` type + distance operators in Rust/pgrx (pgvector parity)

> **Version 1.1** — Investigate how to implement a custom PostgreSQL `vector` type and the three distance
> operators (`<=>` cosine, `<->` L2, `<#>` negative inner product) in Rust/pgrx with **proven numeric parity**
> vs pgvector, for TheoDB M20 (ROADMAP-v2). In scope: `pgvector` (C — the SOTA reference + binary format),
> `pgvectorscale` and `vectorchord` (both pgrx/Rust — how peers represent vectors and whether they FFI-interop
> with pgvector's layout vs define a competing type). Expected blueprint output: a parity-preserving design +
> an evidence-backed coexistence-vs-substitution recommendation that protects existing HNSW/IVFFlat +
> pgvectorscale DiskANN indexes and the `theodb.embed`/`hybrid`/`import` surface (all built on pgvector `vector`).

**Slug:** `m20-own-vector-type`
**Owner:** paulohenriquevn
**Created:** 2026-06-30
**Time budget:** 6h (pgvector 3h · pgvectorscale 1.5h · vectorchord 1.5h — breakdown in ADR D1)

## Context

M20 (ROADMAP-v2.md:104) wants an own `vector` type + 3 distance ops in Rust, **measurement-first** — "só
substitui pgvector quando a paridade for provada". The whole TheoDB surface is built on pgvector's `vector`:
`theodb.embed` returns `vector` (`theodb_rs/src/lib.rs` extension_sql), `ai.hybrid_search_rrf` uses `<=>`
(`sql/40` → now Rust `theodb_rs/src/hybrid.rs`), `theodb.import_pinecone` casts `$2::vector`
(`theodb_rs/src/migrate.rs`), and HNSW/IVFFlat + pgvectorscale DiskANN indexes are built on pgvector's type.
A naive competing type would fork data + break indexes. This discovery must surface the parity-preserving path
BEFORE any code. It anchors on the SOTA (pgvector is the de-facto OSS standard; CLAUDE.md TheoDB rule 1 —
anchor on SOTA) and respects the parsimony ladder (`.claude/rules/parsimony-ladder.md` — reuse the binary
layout before inventing one) and measurement-first / public-copy (`.claude/rules/public-copy.md` — no perf
claim without a benchmark; R3 of `discover-phd-rigor.md`).

## Objective

Decide HOW to implement the own `vector` type + 3 operators in Rust at numeric parity, and WHETHER to coexist
with or substitute pgvector's type — with evidence. Measurable success criteria for the blueprint:

- [ ] All research questions below answered with citations to `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison table populated for pgvector vs pgvectorscale vs vectorchord (type representation, distance accumulation, operator/opclass wiring)
- [ ] At least one concrete decision proposal per research question (incl. the coexistence-vs-substitution recommendation with data/index-compat evidence)
- [ ] Every distance formula stated with its exact accumulation order (numeric-parity-bearing) + ≥2 source references (R2)
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvector/` | `src/vector.c`, `src/vector.h`, `sql/vector.sql`, `test/sql/vector_type.sql`, `test/expected/vector_type.out` | The SOTA reference: exact varlena layout, distance formulas/accumulation order, I/O funcs, operator/opclass SQL, and the regression-test oracle for parity |
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/src/access_method/pg_vector.rs`, `pgvectorscale/src/access_method/distance/mod.rs`, `pgvectorscale/Cargo.toml`, `pgvectorscale/tests/` | A pgrx peer (pgrx 0.16.1 — same as theodb_rs) that FFI-reads pgvector's `#[repr(C)]` layout; the parity + coexistence pattern |
| `.claude/knowledge-base/references/vectorchord/` | `src/datatype/memory_vector.rs`, `src/datatype/operators_vector.rs`, `sql/install/vchord--1.1.1.sql`, `Cargo.toml`, `tests/general/` | A second pgrx peer (pgrx 0.17.0) — independent confirmation of the FFI-wrap pattern + how a pgrx project declares operators/opclasses |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `references/pgvector/src/{hnsw*,ivf*}.c` | Index access methods are M21, not M20 (type+ops only) |
| `references/pgvectorscale/**` except the two `pg_vector.rs`/`distance/mod.rs` files + Cargo + tests | DiskANN AM internals are M21/M22 scope |
| `references/vectorchord/**` except datatype + operators + install SQL + Cargo + tests | RaBitQ/index internals are M21/M22 scope |
| `references/{citus,cloudnative-pg,duckdb,hydra,paradedb,patroni,pgbackrest,pg_mooncake,pinecone-python-client,supabase-postgres}/` | Not vector-type implementers; out of M20 scope |
| Any build artifacts (`target/`, `*.o`, `regression.diffs`) | Generated, not source of truth |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvector 3h (the parity oracle + exact formulas), pgvectorscale 1.5h, vectorchord 1.5h.

**Rationale:** pgvector is the SOTA + the numeric-parity ground truth (its accumulation order IS the spec), so it gets the deepest dive; the two pgrx peers are read to confirm the FFI pattern and operator-wiring idiom (pgvectorscale shares theodb_rs's exact pgrx 0.16.1, so it is the closest analog).

**Stop condition — per question (mandatory):** when a question's search returns empty after 3 query variants (symbol → grep pattern → alternate path), mark it BLOCKED with reason and continue. Never pad with unrelated hotspots.

**Stop condition — per project (mandatory):** when a project's budget is exhausted with questions pending, mark them BLOCKED ("budget exhausted") and continue. If every remaining question across all projects is `done` or honestly `blocked`, emit `<promise>BLUEPRINT_BLOCKED</promise>` (never `BLUEPRINT_COMPLETE` from a blocked state).

**Anti-pattern:** NEVER fabricate an answer for a question whose search was exhausted (Unbreakable Rule 3 / R6).

**Consequences:** the halt-loop stops on budget; blocked questions become next-discovery seed in the blueprint's `## Blocked questions` section.

### D2 — Investigation depth

**Decision:** Read end-to-end the distance functions + the type struct + the I/O funcs in pgvector (`vector.c`/`vector.h`); for the pgrx peers, Read the type-representation struct + the distance module + the operator-declaration sites; grep-then-read for the SQL operator/opclass wiring.

**Rationale:** numeric parity is decided by the EXACT accumulation order + edge-case handling (zero-norm cosine, NaN/inf) — these are only visible by reading the loop bodies, not by grepping signatures. Alternatives (grep-only) rejected: would miss the accumulation order that R3/parity hinges on.

**Consequences:** deeper read cost on pgvector (justified by D1's 3h); the blueprint can state each formula's exact order with line-exact citations.

### D3 — Benchmark deferred to PLAN/IMPLEMENT (discovery is read-only)

**Decision:** this discovery does NOT run benchmarks; it identifies the parity oracle (`test/expected/vector_type.out`) and the parity methodology. The reproducible numeric-parity + perf benchmark vs pgvector is an IMPLEMENT/PLAN deliverable (M20 DoD), marked `UNBENCHMARKED` in the blueprint per R3.

**Rationale:** `/discover-execute` is read-only over references (`hooks/boundary-check.sh`); running pgvector vs own-type benchmarks needs built code that does not exist yet.

**Consequences:** the blueprint states the parity test plan; the numbers come in IMPLEMENT.

## Research Questions

7 questions across the four corners (techniques = 3 — ≥2 per frontier profile R4; ≤3 per corner). Each citation path verified to exist. Both investigation phases (Fase A broad ast-grep map → Fase B deep Read) declared per question.

| # | Question | Corner | Reference project(s) | Fase A (broad — ast-grep/grep map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | What is the EXACT in-memory/binary layout of pgvector's `vector` (varlena header, `dim` int16, `unused` int16, `float4[]` payload) — the format any parity-preserving Rust type must read/write byte-for-byte? | techniques | pgvector | grep `struct Vector` in `src/vector.h`; grep `vector_recv`/`vector_send` in `src/vector.c` | Read `src/vector.h` (`struct Vector`, ~L11-17) + `src/vector.c` `vector_recv`/`vector_send` (~L370-415) | A struct field table + the binary send/recv wire order |
| Q2 | What are the EXACT distance formulas + accumulation order for `<->` (L2), `<#>` (neg inner product), `<=>` (cosine), including zero-norm / NaN / inf handling for cosine, AND the accumulator float-width per op (CK-3)? | techniques | pgvector | grep `VectorL2SquaredDistance`/`VectorInnerProduct`/`VectorCosineSimilarity` in `src/vector.c` | Read `src/vector.c` `VectorL2SquaredDistance`/`l2_distance` (~L555-580), `VectorInnerProduct`/`vector_negative_inner_product` (~L602-640), `VectorCosineSimilarity`/`cosine_distance` (~L644-680) | Per-op: loop body, accumulator float type (f32 vs `double`/f64), normalization + special-case branches |
| Q3 | How do the two pgrx peers REPRESENT a vector AND DECLARE the `<=>`/`<->`/`<#>` operators+opclasses — competing type or FFI-wrap of pgvector's `#[repr(C)]` layout (CK-2: core `VectorInput` pattern, NOT the `sphere_vector` composite)? | techniques | pgvectorscale, vectorchord | grep `repr(C)`/`PgVectorInternal`/`VectorHeader`/`VectorInput`; grep `CREATE OPERATOR`/`pg_operator`/opclass in install SQL | Read `pgvectorscale/.../pg_vector.rs` (`PgVectorInternal`, ~L11-30) + `vectorchord/src/datatype/memory_vector.rs` (`VectorHeader`/`VectorInput`, ~L24-49) + `vectorchord/sql/install/vchord--1.1.1.sql` (`CREATE OPERATOR` ~L760-780) + `pgvectorscale/.../sql/vectorscale--0.8.0--0.9.0.sql` (`CREATE OPERATOR CLASS` ~L120-145) | FFI-wrap (binary-compatible) vs new type comparison + the operator/opclass wiring idiom |
| Q4 | What is the numeric-parity ORACLE — the regression test fixture that asserts exact distance outputs, reusable to prove our Rust ops match pgvector? | tests | pgvector | grep `l2_distance`/`cosine_distance`/`inner_product` in `test/sql/vector_type.sql` | Read `test/sql/vector_type.sql` + `test/expected/vector_type.out` (distance rows) | The input vectors + expected `<->`/`<#>`/`<=>` outputs we can replay |
| Q5 | How do the pgrx peers TEST their distance functions (unit/SLT/integration), to mirror their test shape for our parity suite? | tests | vectorchord, pgvectorscale | grep test entrypoints under `vectorchord/tests/general/` + `pgvectorscale/tests/` | Read `vectorchord/tests/general/distance.slt` + `vectorchord/tests/general/vector.slt`; `pgvectorscale/tests/test_basic_operations.py` | The test harness shape (SLT vs Python-integration) + what they assert |
| Q6 | What pgrx version + deps does each peer pin, vs theodb_rs's pgrx 0.16.1 — does the FFI-wrap pattern require any crate beyond pgrx, and is it license-clean (D1 permissive)? | deps | pgvectorscale, vectorchord | grep `pgrx` in each `Cargo.toml` | Read `pgvectorscale/pgvectorscale/Cargo.toml` (pgrx `=0.16.1`, ~L31) + `vectorchord/Cargo.toml` (pgrx `=0.17.0`, ~L43) | A dep delta table + the license note (feeds `/deps-audit` at PLAN) |
| Q7 | What build/test TOOLING do the pgrx peers use (cargo pgrx test? SLT runner? Python pytest against a container?) that we can adopt for the M20 parity gate? | tools | vectorchord, pgvectorscale | grep for test-runner config / CI entrypoints under each `tests/` | Read the test entrypoints + any runner config under `vectorchord/tests/` and `pgvectorscale/tests/` | The test command + harness we can reuse (ties to theodb_rs's pytest-against-container + cargo pgrx test) |

## Coverage Matrix

| Corner | Questions | ≥1? |
|---|---|---|
| Integration tests | Q4, Q5 | ✅ |
| Dependencies | Q6 | ✅ |
| Tools | Q7 | ✅ |
| Techniques | Q1, Q2, Q3 | ✅ (≥2, frontier R4; ≤3/corner) |

Every question maps to a Fase A + Fase B method + reference path (all verified to exist). No corner empty → no ADR-deferral needed. Total 7 questions (budget 5-10, ≤3/corner). ✅

## Halt-loop checkpoints (for /discover-execute)

A question is `done` only when: (a) the cited file was Read at the named symbol/line; (b) the answer states the concrete finding (struct fields / formula+order / operator wiring / test shape) with a line-exact citation; (c) for techniques (Q1-Q4) the answer names the SOTA/peer approach and the parity-bearing detail (accumulation order, binary layout). A question with no match after D1's 3-variant retry is `blocked` with reason — never fabricated.

**Absorbed from edge-case review (m20-own-vector-type-edge-cases-2026-06-30.md), plan v1.1:**

- **CK-1 (EC-1, version skew):** before answering Q1/Q2, record the exact pgvector version read (clone is **0.8.3**, `references/pgvector/META.json`) in the blueprint, and add a note that IMPLEMENT MUST cross-check the `theo-db` image's installed pgvector version and confirm the `vector` send/recv format + distance formulas are unchanged (the parity guarantee).
- **CK-2 (EC-2, vectorchord composite):** Q3 MUST extract vectorchord's **core** pgvector-FFI representation (`VectorInput`/`VectorHeader`, `src/datatype/memory_vector.rs`) as the parity/coexistence pattern, and explicitly mark `sphere_vector` + `_vchord_vector_sphere_*` (`src/datatype/operators_vector.rs`, `sql/install/vchord--1.1.1.sql:730-780`) as a DISTINCT range-query feature OUT of M20 scope.
- **CK-3 (EC-4, accumulator width — precision):** the Q2 answer MUST state, PER operator, the accumulator float-width (pgvector stores f32 but accumulates distance in `double`/f64); this is THE bit-exact-parity determinant a Rust port must match.
- **CK-4 (EC-3, scope guard):** `/discover-execute` reads only `vector.c`/`vector.h`/`vector.sql` for the `vector` (float4) type + 3 ops; it does NOT read `halfvec`/`sparsevec`/`bit` sibling types.

## Acceptance Criteria

- [ ] Q1-Q7 each `done` (with `.claude/knowledge-base/references/` citation) or honestly `blocked`.
- [ ] The four coverage corners populated in the blueprint (Integration Tests / Dependencies / Tools / Techniques).
- [ ] Distance formulas (Q2) stated with exact accumulation order + ≥2 references (pgvector + ≥1 pgrx peer) per R2.
- [ ] Coexistence-vs-substitution recommendation (Q3+Q4) backed by the binary-compat + index-impact evidence.
- [ ] ≥1 ADR in the blueprint; every citation resolves on disk; no fabricated path.

## Global Definition of Done

Per `cycle-discover.md` + `discover-blueprint-golden-rule.md`: `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS (no empty coverage corner, no fabricated citation), and the frontier rigor bar (`discover-phd-rigor.md` R1-R6) honored for the techniques corner.
