# Discovery Plan: M21 — Own ANN index (HNSW + IVFFlat) in Rust

> **Version 1.1** (edge-cases absorbed: EC-1 Q4 vectorchord path fixed; EC-2/EC-3 checkpoints added; EC-4/EC-5 → ADR D4) — Investigate how to build a custom PostgreSQL **index access method** (HNSW + IVFFlat) in
> Rust/pgrx that can substitute pgvector's index, but **only** when it reaches recall@k parity on TheoDB's
> existing harness (`benchmarks/theodb_bench`). In scope: `pgvector` (the C reference AM we must match),
> `pgvectorscale` (the canonical Rust/pgrx index-AM template), `vectorchord` (a second Rust/pgrx AM datapoint).
> Output: a blueprint that decides the **coexistence-vs-substitution** migration path and the recall-parity gate
> design, so `/to-plan` for M21 can scope the implementation honestly (measurement-first, anti-sunk-cost).

**Slug:** `m21-own-ann-index`
**Owner:** paulohenriquevn
**Created:** 2026-06-30
**Time budget:** 6h (per-project breakdown in ADR D1)

## Context

M21 (`ROADMAP-v2.md:116`) requires an **own ANN index access method (HNSW + IVFFlat) in Rust**, substituting the
pgvector index **only** when recall@k parity is proven on the M2/M9 harness — else an honest ADR keeps pgvector
(anti-sunk-cost). The milestone is tagged **risk ALTO (PhD-level)**; measurement-first is the explicit guard-rail
(`ROADMAP-v2.md:124,126`). M20 just shipped own f32-parity distance **functions** in coexistence with pgvector's
type (`.claude/knowledge-base/reviews/m20-own-vector-type-review-2026-06-30.md`), and deferred the index AM +
opclass to M21 — this discovery is the prior-art investigation that must precede any code (Unbreakable Rule 9;
TheoDB rule 1 "anchor on SOTA"; TheoDB rule 5 "performance is a claim, not an opinion").

The PhD-rigor profile applies: M21 touches a performance- and algorithm-bearing pillar (P2 vector/AI), so per
`.claude/rules/discover-phd-rigor.md` the techniques corner must anchor on the pgvector SOTA, cite ≥2 primary
sources per technique, and surface benchmark/recall methodology rather than asserting conclusions.

The harness already exists (`benchmarks/theodb_bench/recall.py:61` `recall_at_k`, `:41` `brute_force_ground_truth`;
`harness.py:29` `run_benchmark`) — this discovery must **reuse** it, not rebuild it (Rule 9; the DoD says "harness
M2/M9").

## Objective

Decide whether TheoDB can build an own HNSW + IVFFlat index AM in Rust that reaches pgvector recall@k parity, and
**how** (coexistence vs substitution), so the M21 implementation plan is evidence-backed and the parity gate is
defined before any code is written.

- [ ] All research questions in this plan answered with citations to `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison table populated for every in-scope reference project (pgvector / pgvectorscale / vectorchord)
- [ ] Recommendations section provides at least one concrete decision proposal per in-scope research question
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvector/` | `src/` (`hnsw*.c/.h`, `ivf*.c`, `ivfflat.h`) | The C SOTA AM we must match for recall@k — algorithm + GUCs + scan ordering |
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/src/access_method/`, `pgvectorscale/src/util/`, `pgvectorscale/Cargo.toml` | Canonical Rust/pgrx index-AM template (`IndexAmRoutine` wiring, own pages, build/scan) |
| `.claude/knowledge-base/references/vectorchord/` | `src/index/`, `tests/`, `Cargo.toml`, `crates/simd/` | Second Rust/pgrx AM datapoint (opclass, scanners, SIMD distance, deps) |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/pgvector/test/`, `doc/` | Test SQL + docs are not the AM internals (oracle rows already reused in M20) |
| `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/`, `.../labels/` | Scalar-bit-quantization + label filtering are **M22** scope (anti-scope-creep) |
| `.claude/knowledge-base/references/vectorchord/src/index/vchordg/` | Graph-quant variant beyond HNSW/IVF — M22 datapoint, not M21 |
| Any project NOT cloned into `.claude/knowledge-base/references/` | Cross-Project Rule: never claim a project feature without reading its source |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvector: 3h (the algorithm + GUCs we must match — deepest dive); pgvectorscale: 2h (the Rust AM
template); vectorchord: 1h (corroborating datapoint for deps/tests/opclass).

**Rationale:** pgvector is the parity target (recall@k must match IT), so its HNSW/IVF algorithm + scan knobs earn
the deepest read. pgvectorscale is the closest Rust/pgrx index-AM analog (it registers a real `CREATE ACCESS
METHOD`), so it gets the template-extraction budget. vectorchord is a second datapoint to avoid single-source bias
(PhD-rigor R2) but does not need a full read.

**Alternatives considered:** equal 2h split (rejected — pgvector parity is the crux, deserves more); pgvector-only
deep-dive (rejected — violates PhD-rigor R2 ≥2 primary sources per technique for the Rust-AM-wiring technique).

**Stop condition — per question (mandatory):** When a question's Fase A returns empty matches after 3 consecutive
retries with different query variants, mark the question BLOCKED with reason "Fase A exhausted — no hotspots found"
and continue. Do NOT pad with unrelated hotspots from another question's scope.

**Stop condition — per project (mandatory):** When a project's time budget is exhausted with N questions still
pending, mark all remaining questions for that project BLOCKED with reason "budget exhausted" and continue with the
next project. If every remaining project is in the same state, emit `<promise>BLUEPRINT_BLOCKED</promise>` (NOT
`BLUEPRINT_COMPLETE`) with the honest blocked-questions report.

**Anti-pattern:** NEVER fabricate Fase B answers to close a question whose Fase A was exhausted (Unbreakable Rule 3).

**Consequences:** the halt-loop stops iterating a project when its budget is exhausted; blocked questions surface
explicitly and become next-discovery seed.

### D2 — Investigation depth

**Decision:** Read the pgvector AM entry files end-to-end (`hnswbuild.c`, `hnswscan.c`, `ivfbuild.c`, `ivfscan.c`);
for the Rust references, Grep the `IndexAmRoutine` hooks + storage utilities first (Fase A), then Read each hotspot
(Fase B). Do NOT attempt to read every file in pgvectorscale/vectorchord (they are large) — only the AM-wiring,
build, scan, storage, options, and Cargo manifests.

**Rationale:** the algorithm parity demands full reads of pgvector's scan/build; the Rust-AM wiring is a structural
pattern best located by symbol grep then read at the hotspot (KISS — `.claude/rules/parsimony-ladder.md`). Reading
everything would blow the time budget (YAGNI).

**Alternatives considered:** full read of all three (rejected — budget); grep-only with no deep read (rejected —
loses intent/edge-cases that the blueprint needs).

**Consequences:** the blueprint cites line-exact hotspots for the Rust AM wiring, and end-to-end behavior for the
pgvector algorithm — the asymmetry is deliberate and budget-driven.

### D3 — Migration decision is a blueprint deliverable, not a single question

**Decision:** The coexistence-vs-substitution migration decision (M21 DoD) is synthesized in the blueprint's
Cross-cutting Comparison + ADR sections from the mechanics questions (Q3 storage/pages, Q4 AM registration/opclass)
+ the parity question (Q5) + the test/measurement question (Q7) — it is NOT a standalone research question, because
it is a *decision we make*, not a *fact we read from a reference*.

**Rationale:** per `/discover-plan` rule "A discovery plan ASKS questions; it doesn't answer them." The migration
choice is an answer; the questions feed it. Folding it into Q3 (can the own AM use its own pages without touching
pgvector's index/data?) keeps each question answerable from a reference.

**Alternatives considered:** a dedicated "should we coexist or substitute?" question (rejected — unanswerable from a
reference, it is a TheoDB decision); deferring the decision to `/to-plan` (rejected — the M21 DoD demands the
migration decision be documented, and the blueprint is where evidence converges).

**Consequences:** the blueprint MUST contain an explicit ADR on coexistence vs substitution, citing Q3/Q4/Q5/Q7
evidence; `/to-plan` consumes it as a locked decision.

### D4 — The Rust references are AM-scaffolding sources, NOT HNSW/IVFFlat algorithm sources (EC-4/EC-5)

**Decision:** pgvector (C) is the **sole** source for the HNSW/IVFFlat **algorithm** (Q1/Q2) and recall knobs (Q5).
pgvectorscale (DiskANN+SBQ) and vectorchord (RaBitQ) are sources **only** for the Rust/pgrx **AM scaffolding** —
`IndexAmRoutine` wiring, own-page storage, registration, deps, test shape (Q3/Q4/Q6/Q7). The blueprint must NOT
borrow DiskANN graph or RaBitQ-quant details as if they were HNSW. Furthermore (EC-5), "retain pgvector index
(coexistence, own AM gated/deferred)" is a **valid** blueprint outcome under measurement-first — the recommendation
must not force a substitution the recall evidence does not support (anti-sunk-cost, `ROADMAP-v2.md:124`).

**Rationale:** the two Rust references implement different ANN families; conflating their storage/algorithm with
HNSW/IVFFlat would fabricate technique claims (Unbreakable Rule 3). The measurement-first guard-rail is the whole
point of M21's risk-ALTO framing.

**Alternatives considered:** treat pgvectorscale's DiskANN as the algorithm to port (rejected — that is M22 scope
and not HNSW/IVF); force "substitute pgvector" as the conclusion (rejected — violates measurement-first).

**Consequences:** Q3/Q4/Q6/Q7 cite the Rust refs for scaffolding; Q1/Q2/Q5 cite pgvector for the algorithm; the
final Recommendation keeps coexistence-retain as a first-class option.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — grep/ast map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How does pgvector implement the **HNSW** algorithm (build graph with `M`/`ef_construction`, layer assignment, entry point, candidate heap; insert; scan with `hnsw_ef_search`)? | techniques | `.claude/knowledge-base/references/pgvector/src/` | `grep -nE "HNSW_DEFAULT_M\|ef_construction\|HnswSearchLayer\|entryPoint\|InsertElement" pgvector/src/hnsw.h pgvector/src/hnswbuild.c pgvector/src/hnswutils.c` | Read `hnswbuild.c`, `hnswutils.c` (`HnswSearchLayer`), `hnswscan.c:55` end-to-end; capture graph params, layer math, candidate/visited sets, scan path | Prose + param table (M, ef_construction, ef_search defaults) + pseudo-code of build/search with `path:line` |
| Q2 | How does pgvector implement **IVFFlat** (k-means list build, list assignment, scan over `ivfflat_probes` closest lists, ordering)? | techniques | `.claude/knowledge-base/references/pgvector/src/` | `grep -nE "probes\|ListInfo\|IvfflatKmeans\|maxProbes\|GetScanItems" pgvector/src/ivfflat.h pgvector/src/ivfbuild.c pgvector/src/ivfkmeans.c pgvector/src/ivfscan.c` | Read `ivfbuild.c`, `ivfkmeans.c`, `ivfscan.c:133` (`probes` loop) end-to-end; capture kmeans method, list layout, probe selection, scan tuple ordering | Prose + param table (lists, probes defaults) + pseudo-code of build/scan with `path:line` |
| Q3 | What `IndexAmRoutine` hooks + **on-disk storage** (own buffer pages via pgrx page/buffer utils vs reusing the heap) does a Rust AM implement, and what `unsafe`/`pg_sys` FFI surface is required — can it own its pages without touching pgvector's index/data (the coexistence pre-condition)? | tools | `.claude/knowledge-base/references/pgvectorscale/`, `.claude/knowledge-base/references/vectorchord/` | `grep -nE "amroutine\.\|fn ambuild\|fn aminsert\|fn amgettuple\|fn ambeginscan\|BuildState" pgvectorscale/pgvectorscale/src/access_method/mod.rs pgvectorscale/pgvectorscale/src/access_method/build.rs pgvectorscale/pgvectorscale/src/access_method/scan.rs` + `grep -nE "Page\|Buffer\|ReadBufferExtended\|BLCKSZ" pgvectorscale/pgvectorscale/src/util/page.rs pgvectorscale/pgvectorscale/src/util/buffer.rs` | Read each hooked fn + `util/page.rs`, `util/buffer.rs`; capture which hooks are mandatory, how pages are allocated/written, what `unsafe` blocks wrap pg_sys | Table: hook → Rust fn → what it does → `path:line`; + storage model (own pages? heap reuse?) + unsafe/FFI inventory |
| Q4 | How does a Rust/pgrx project **register** a custom index AM end-to-end — the `amhandler` (`#[pg_extern]` returning `IndexAmRoutine`), `CREATE ACCESS METHOD … TYPE INDEX HANDLER`, and the **operator class** binding `<=>`/`<->`/`<#>`? | tools | `.claude/knowledge-base/references/pgvectorscale/`, `.claude/knowledge-base/references/vectorchord/` | `grep -nE "amhandler\|index_am_handler\|CREATE ACCESS METHOD\|OPERATOR CLASS\|opclass" pgvectorscale/pgvectorscale/src/access_method/mod.rs vectorchord/src/index/vchordrq/am/mod.rs vectorchord/sql/install/vchord--1.1.0.sql vectorchord/src/index/vchordrq/opclass.rs` (EC-1: vectorchord handler is in `vchordrq/am/mod.rs` + the install SQL, NOT `index/mod.rs`) | Read `pgvectorscale/.../access_method/mod.rs:27-90` (validated handler + `CREATE ACCESS METHOD diskann`), vectorchord `vchordrq/am/mod.rs` + `vchordrq/opclass.rs`; capture exact SQL + pg_extern signature + opclass strategy numbers | SQL snippets (amhandler fn + CREATE ACCESS METHOD + CREATE OPERATOR CLASS) + `path:line` per snippet |
| Q5 | What governs **recall@k** in pgvector (which knobs: `hnsw_ef_search` / `ivfflat_probes`; distance ties; iterative scan) so an own index can be tuned to match it — and what is the parity-measurement contract? | techniques | `.claude/knowledge-base/references/pgvector/src/` | `grep -nE "ef_search\|probes\|DefineCustomInt\|iterative_scan\|tuples" pgvector/src/hnswscan.c pgvector/src/ivfscan.c pgvector/src/hnsw.c pgvector/src/ivfflat.c` | Read the GUC definitions + scan loops; capture default values, valid ranges, how they trade recall vs latency, tie/eps behavior | Table: knob → default → range → recall/latency effect → `path:line`; + statement of what "parity" must hold (recall@k within eps at matched knobs) |
| Q6 | What runtime/build **dependencies** do the Rust AMs pull in beyond `pgrx` (SIMD distance, k-means/RNG, quantization), with versions + **licenses** (Apache/MIT/BSD gate — TheoDB D1)? | deps | `.claude/knowledge-base/references/pgvectorscale/`, `.claude/knowledge-base/references/vectorchord/` | `grep -nE "^[a-z].*=|version|license" pgvectorscale/pgvectorscale/Cargo.toml vectorchord/Cargo.toml` + `ls vectorchord/crates/` + `grep -rn "license" vectorchord/crates/simd/Cargo.toml` | Read both `Cargo.toml` + vectorchord `crates/*/Cargo.toml`; capture each non-pgrx dep + version + license; flag any AGPL/GPL (forbidden, D1) | Table: dep → version → license → purpose → AGPL-clean? (yes/no) |
| Q7 | How do pgvectorscale / vectorchord **test** their index AM against a real Postgres (build + scan + recall correctness), so M21 can mirror the test shape and reuse `benchmarks/theodb_bench` for the recall-parity gate? | tests | `.claude/knowledge-base/references/pgvectorscale/`, `.claude/knowledge-base/references/vectorchord/` | `grep -rnE "#\[pg_test\]\|fn test_\|recall\|CREATE INDEX.*USING" pgvectorscale/pgvectorscale/src/access_method/plain/tests.rs vectorchord/tests/` + `ls vectorchord/tests/vchordrq/` | Read representative test files; capture how they build an index, run a `<=>` scan, and assert correctness/recall; map onto `theodb_bench/recall.py:61` `recall_at_k` reuse | Table: test → what it builds → what it asserts → `path:line`; + a sketch of the M21 parity gate reusing `theodb_bench` |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q7 | Covered |
| Dependencies | Q6 | Covered |
| Tools | Q3, Q4 | Covered |
| Techniques | Q1, Q2, Q5 | Covered |

**Coverage: 4/4 corners covered (100%)**

Question budget: 7 total (within the 5–10 default window); techniques carries 3 (≥2 required by PhD-rigor R4,
within the deterministic ≤3-per-corner budget); tools carries 2; every other corner ≥1. Q3 (the Rust AM
scaffolding: hooks + own-page storage + FFI) is classed **tools** — it is the pgrx index-AM *integration
mechanism*, not an ANN algorithm (the algorithms are Q1/Q2, pgvector-sourced).

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | every `.claude/knowledge-base/references/{project}/{path}` declared in its Fase A exists | Mark Qx BLOCKED "path not found", continue |
| Per-question Fase A budget | Fase A returned ≥1 hotspot OR 3 query-variant retries attempted | After 3 retries empty, mark Qx BLOCKED "Fase A exhausted"; continue |
| After answering Qx | blueprint section under Qx has ≥1 citation | Re-iterate Qx (1 retry max) |
| Techniques depth (PhD R2) | each technique question (Q1/Q2/Q5) cites ≥2 primary sources (≥2 distinct ref files or a paper) | Add the second source or mark the gap honestly |
| Q1/Q2 scope guard (EC-2) | Q1/Q2 answers cover the **in-memory algorithm + scan path**; WAL/parallel-build/vacuum explicitly deferred to "implementation concern, noted not detailed" | Trim durability machinery from the answer; note it as deferred |
| Q5 parity definition (EC-3) | Q5 states parity as a **tolerance band** — recall@k within eps at matched `hnsw_ef_search`/`ivfflat_probes` (reusing `theodb_bench/recall.py` eps), NOT bit-exact neighbor-set identity | Re-state the parity contract as a band before DONE |
| Per-project time budget | project budget not exhausted | When exhausted, mark remaining Qx for that project BLOCKED "budget exhausted"; advance |
| Before promising complete | all 4 coverage corners have populated sections AND a coexistence-vs-substitution ADR exists (which keeps "retain pgvector index" a valid outcome, EC-5) | Refuse promise, continue iterating |

## Acceptance Criteria

- [ ] All research questions answered OR explicitly marked BLOCKED with reason
- [ ] All four coverage corners have populated sections in the blueprint
- [ ] Every citation in the blueprint points to a real `.claude/knowledge-base/references/{...}` path
- [ ] At least one ADR in the blueprint synthesizes the **coexistence-vs-substitution** migration decision (D3)
- [ ] Recall-parity gate design reuses `benchmarks/theodb_bench` (Q7) — no harness rebuild
- [ ] Time budget respected per project
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint saved at `.claude/knowledge-base/discoveries/blueprints/m21-own-ann-index-blueprint.md`

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed → re-score)
- [ ] Final `/discover-confidence` verdict recorded in the blueprint header
- [ ] No fabricated citations
- [ ] Coverage Matrix 100% covered
- [ ] ADRs reference at least one principle from project rules (Rule 9 Don't-Reinvent; KISS; `.claude/rules/discover-phd-rigor.md`; `architecture.md` DIP for the AM↔distance boundary)
