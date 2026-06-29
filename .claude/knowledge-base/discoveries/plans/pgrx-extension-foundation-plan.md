# Discovery Plan: pgrx extension foundation — TheoDB's own Rust extension + theodb.embed rewrite (parity + benchmark)

> **Version 1.1** (edge-case MUST-FIX absorbed: HTTP-crate web source repointed `docs.rs` → `github.com` (allowlist); coexistence checkpoint added; Docker-build/disk noted) — Investigates how to build TheoDB's **own** PostgreSQL extension in Rust via **pgrx** (the
> ROADMAP-v2 / ADR 0006 foundation) and rewrite the first function (`theodb.embed`, today plpython3u) in Rust
> **with proven parity + a benchmark** (latency Rust vs plpython3u). Primary reference: **pgvectorscale**
> (a real pgrx extension, already cloned). Output: a blueprint that unblocks **M17**. Honesty (ADR 0006/0002):
> measurement-first — the benchmark is a gate; no perf claim without evidence; the rewrite is "done" only when
> the existing tests prove functional parity.

**Slug:** `pgrx-extension-foundation`
**Owner:** TheoDB maintainers
**Created:** 2026-06-29
**Time budget:** 6h (pgvectorscale 3h, web pgrx/HTTP docs 2h, our baseline 1h)

## Context

ADR `0006` (LOCKED) pivots TheoDB to a Postgres-based DB with **own code in Rust (pgrx)**. M17 is the
foundation: stand up our own pgrx extension and rewrite the simplest surface (`theodb.embed`, today
plpython3u + urllib in `sql/30-theodb-embed.sql`) in Rust, proving the "plpython3u → own Rust extension"
pattern with **parity** (same vectors/typed errors via `benchmarks/tests/test_embed_sql.py`) and a **benchmark**
(latency, the CTO's explicit requirement). The biggest unknowns: (a) the pgrx project shape + how the
`.control`/install SQL are generated; (b) a **minimal, audited** HTTP crate (pgvectorscale does no HTTP — no
in-repo example, so this needs the crate's official docs); (c) how the new Rust extension **coexists** with the
current SQL-only `theodb` extension during the incremental rewrite; (d) a reproducible parity benchmark. Honors
`.claude/rules/architecture.md` (extension boundaries), `.claude/rules/testing.md` (parity = the existing tests),
`.claude/rules/parsimony-ladder.md` + Rule 9 (minimal deps — stdlib/one audited crate, never reinvent HTTP),
`.claude/rules/public-copy.md` (no perf claim without benchmark), and ADR 0006 (own code) + ADR 0002
(measurement-first).

## Objective

Produce a blueprint that lets M17 implement, with evidence: (a) the pgrx project + build wired into the image,
(b) `theodb.embed` in Rust with a minimal audited HTTP dep + SSRF/typed-error parity, (c) coexistence with the
current SQL-only extension, (d) a reproducible latency benchmark + functional-parity proof.

- [ ] All research questions answered with citations to `.claude/knowledge-base/references/`
- [ ] A concrete pgrx project skeleton (Cargo.toml + `#[pg_extern]`/`#[pg_schema]` + control/sql generation)
- [ ] A concrete minimal HTTP crate recommendation (license D1-OK + CVE-clean) with SSRF/typed-error mapping
- [ ] A coexistence plan (Rust ext alongside the current SQL-only `theodb`) for the incremental rewrite
- [ ] A reproducible benchmark design (latency Rust vs plpython3u) + functional-parity via existing tests
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/Cargo.toml`, `pgvectorscale/src/lib.rs`, `pgvectorscale/vectorscale.control`, `Makefile`, `pgvectorscale/src/access_method/mod.rs` (pg_extern/schema usage) | The reference pgrx extension: project shape, `#[pg_extern]`/`#[pg_schema]`, control, build (init/install) |
| `.claude/knowledge-base/references/vectorchord/` | `Cargo.toml` (pgrx layout only) | A second pgrx layout for cross-check (study-only — AGPL; pattern only, never copy code) |
| `.claude/knowledge-base/references/pgvector/` | `Makefile`, `vector.control` | C-extension contrast (why pgrx vs C) |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/` algorithm internals (DiskANN/SBQ) | M17 is the foundation + a simple function; the index algorithm is M21/M22 |
| `.claude/knowledge-base/references/vectorchord/src/` (any code beyond Cargo.toml) | AGPL — pattern/layout only; never read for code reuse |
| Any `*/target/`, build artifacts | not source |
| Rewriting any other surface (ai.*, nl, hybrid) | those are M18/M19; M17 is `theodb.embed` only |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvectorscale 3h (the real pgrx reference — shape/build/test), web 2h (pgrx book + the HTTP crate's
docs, since no in-repo HTTP example exists), our baseline 1h (`sql/30` contract + `test_embed_sql.py`).

**Rationale:** pgrx structure/build is fully exemplified by pgvectorscale; the only gap with no clone is the
HTTP-in-Rust crate → web (allowlisted github/docs.rs).

**Stop condition — per question:** Fase A empty after 3 query-variant retries → BLOCKED ("Fase A exhausted"),
continue. Never fabricate Fase B.

**Stop condition — per project:** budget exhausted with questions pending → BLOCKED ("budget exhausted"); if all
remaining are in that state → emit `<promise>BLUEPRINT_BLOCKED</promise>`.

**Anti-pattern:** never fabricate a Fase B answer (Unbreakable Rule 3).

**Consequences:** the HTTP-crate question is the most web-dependent; if the allowlist lacks the crate's host, it
is marked with the crate's `github.com`/`docs.rs` doc (both allowlisted) or BLOCKED honestly.

### D2 — Investigation depth

**Decision:** Read pgvectorscale `Cargo.toml` + `vectorscale.control` + the `#[pg_extern]`/`#[pg_schema]` usage
end-to-end; Read the Makefile pgrx targets; Grep + targeted Read for the HTTP crate decision (web docs).

**Rationale:** the project shape + control generation are load-bearing (copying the idiom precisely); the HTTP
crate is a bounded decision (minimal + audited).

**Consequences:** deep on pgrx shape; web-cited on the HTTP crate (flagged honestly).

### D3 — Coverage corners (all four covered)

**Decision:** all four covered (see matrix). No deferral.

**Rationale:** the foundation touches techniques (pgrx shape + HTTP + benchmark), tests (pgrx test + Python
parity), deps (pgrx + HTTP crate), tools (cargo pgrx + Docker + disk).

**Consequences:** techniques carries 3 (pgrx-shape+coexistence, HTTP-minimal, benchmark).

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — map) | Fase B (deep — Read) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | What is the pgrx project shape — `Cargo.toml` (pgrx dep + pgNN features), `#[pg_extern]`/`#[pg_schema]` to expose a function in the `theodb` schema, how the `.control` + install SQL are generated (`cargo pgrx schema`) — and how does the new Rust extension **coexist** with the current SQL-only `theodb` (incremental rewrite, no name clash)? | techniques | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml`, `.../src/lib.rs`, `.../vectorscale.control`, `.../src/access_method/mod.rs` | Grep `pg_extern`, `pg_schema`, `#[pg_guard]`, `extension_sql` in `pgvectorscale/src/`; Read `Cargo.toml` + `vectorscale.control` | Read the `#[pg_extern]`/`#[pg_schema]` examples + control fields | A `theodb-rs` skeleton (Cargo.toml + `#[pg_schema] mod theodb` + `#[pg_extern] fn embed`) + control + coexistence note (separate ext name vs migrating `theodb`) + citations |
| Q2 | What is a **minimal, audited** HTTP client crate for the embed POST (vs plpython3u urllib), and how to preserve SSRF (http(s)-only, no redirect) + typed errors (SQLSTATE 22023)? | techniques | web: **`github.com`** (minreq/ureq repos + READMEs — `docs.rs` is NOT allowlisted, EC-1) + repo `sql/30-theodb-embed.sql` (the contract to match) | WebFetch the candidate crate **github repo** (README + Cargo.toml — features, dep-tree, redirect policy); Read `sql/30` SSRF/error contract | Read crate docs for blocking POST + no-redirect + timeout; map plpy.error→`ereport`/`PgSqlErrorCode` | Crate choice + dep-tree size + how to set no-redirect/timeout + SSRF check + typed-error mapping + citations |
| Q3 | How to **benchmark** the Rust embed vs plpython3u reproducibly (latency/throughput) AND prove **functional parity** (same vectors/typed errors)? | techniques | repo `benchmarks/` + `benchmarks/tests/test_embed_sql.py` (the parity oracle) | Glob `benchmarks/`; Read `test_embed_sql.py` to see the parity assertions; check the existing harness shape | Read the test to capture the parity contract; design a latency micro-bench (N calls, mean±std, same stub) | A benchmark design (stub endpoint, N runs, mean±std, Rust vs plpython3u) + the parity-via-existing-tests plan + citations |
| Q4 | How does pgvectorscale **test** a pgrx extension (`#[pg_test]`), and how do we keep the Python parity tests (`test_embed_sql.py`) green against the Rust version? | tests | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/` (pg_test), repo `benchmarks/tests/test_embed_sql.py` | Grep `#[pg_test]`, `pgrx::pg_test`, `pg_schema` in pgvectorscale src; Read a pg_test example + our test_embed_sql | Read one `#[pg_test]` + the Python parity test | Test strategy: pgrx `#[pg_test]` for unit + the existing Python e2e as the parity gate + citations |
| Q5 | What deps does the Rust extension pull (pgrx version to match the image; the HTTP crate) and are they license-D1-OK + CVE-clean? | deps | `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml`, repo `Dockerfile` (PGRX_VERSION) + web **github.com** (crate license/Cargo.toml) | Grep `pgrx =`, version in pgvectorscale Cargo.toml + `PGRX_VERSION` in our Dockerfile; check the HTTP crate license via its github repo | Read the versions; confirm pgrx `=0.16.1` matches our image; check the HTTP crate license/CVE | Dep list (pgrx 0.16.1 + HTTP crate) with license (D1) + CVE status, `/deps-audit` plan + citations |
| Q6 | What are the build/install tools (`cargo pgrx init/install/schema`) and how to wire the Rust extension into the Docker image (scale-builder pattern) — and what is the **disk/build cost** (pgrx init compiles a Postgres)? | tools | `.claude/knowledge-base/references/pgvectorscale/Makefile`, repo `Dockerfile` (scale-builder stage) | Grep `cargo pgrx`, `init`, `install`, `PGRX_HOME` in pgvectorscale Makefile + our Dockerfile | Read the init/install targets + our scale-builder stage | The build recipe (init --pg17 / install --release) + Dockerfile wiring + an honest disk-cost note (pgrx init compiles PG) + citations |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q4 | Covered |
| Dependencies | Q5 | Covered |
| Tools | Q6 | Covered |
| Techniques | Q1, Q2, Q3 | Covered |

**Coverage: 4/4 corners covered (100%)** — techniques carries 3 (pgrx-shape+coexistence, HTTP-minimal, benchmark); total 6 questions.

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | every cited `.claude/knowledge-base/references/{...}` path exists | mark Qx BLOCKED "path not found", continue |
| Per-question Fase A budget | ≥ 1 hotspot OR 3 retries | after 3 empty retries, BLOCKED "Fase A exhausted" |
| HTTP crate (Q2/Q5) | the recommended crate is permissive (D1) + its source host is `github.com` (allowlisted; NOT docs.rs — EC-1) | drop off-allowlist source; if no permissive minimal crate, flag for human |
| Coexistence (Q1, EC-2) | the blueprint states how the Rust `theodb.embed` coexists with the SQL-only `theodb` (separate ext name OR migrate the fn out of `theodb--1.0.sql`) — no duplicate-definition clash on CREATE EXTENSION | re-iterate Q1 (1 retry) |
| Benchmark honesty (Q3) | the design is reproducible (N runs, mean±std, same stub) AND no perf conclusion is asserted in the blueprint (only the method) | strip any premature perf claim |
| Disk-cost honesty (Q6) | the blueprint states pgrx init compiles a Postgres (~GB) and the build's disk footprint | add the honest note |
| Before promising complete | 4 corners populated AND Q1 gives a concrete skeleton AND Q2 gives a concrete crate + SSRF/error mapping AND Q3 gives a concrete benchmark design | refuse promise, continue |

## Acceptance Criteria

- [ ] All research questions answered OR BLOCKED with reason
- [ ] All four coverage corners populated
- [ ] Every citation resolves to a real `.claude/knowledge-base/references/{...}` path (or an allowlisted web doc for the HTTP crate)
- [ ] Concrete pgrx skeleton (Q1) + coexistence plan
- [ ] Concrete minimal HTTP crate + SSRF/typed-error mapping (Q2)
- [ ] Concrete reproducible benchmark design + parity-via-tests (Q3)
- [ ] ≥ 1 ADR synthesizing decisions (incl. the HTTP-crate choice + coexistence)
- [ ] `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint at `.claude/knowledge-base/discoveries/blueprints/pgrx-extension-foundation-blueprint.md`

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed)
- [ ] Final `/discover-confidence` verdict in the blueprint header
- [ ] No fabricated citations
- [ ] Coverage Matrix 100%
- [ ] ADRs cite ≥ 1 project rule/principle (architecture.md / testing.md / parsimony-ladder.md / Rule 9 / ADR 0006 / ADR 0002)
