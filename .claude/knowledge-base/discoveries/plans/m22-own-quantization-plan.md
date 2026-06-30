# Discovery Plan: M22 — Own scale/quantization (SBQ) in Rust

> **Version 1.1** (edge-cases absorbed: EC-1 Q6 workspace-inherited license; EC-2 Q3 rerank honesty checkpoint; EC-3 bytes/vector formula not pg_relation_size) — Investigate how to build TheoDB's own **scalar quantization** (SBQ-quality) + quantized ANN
> search in Rust, to substitute pgvectorscale **only** when it reaches **recall@k parity AND a measured memory
> profile** vs pgvectorscale (else an honest ADR keeps pgvectorscale — anti-sunk-cost, M22 DoD). In scope:
> `pgvectorscale` (the SBQ + StreamingDiskANN reference we must match) and `vectorchord` (RaBitQ — a second
> quantization datapoint). Output: a blueprint that decides the migration path (coexistence vs substitution) and
> the recall+memory gate design, so `/to-plan` for M22 can scope the implementation honestly (measurement-first,
> the highest-risk milestone of v2).

**Slug:** `m22-own-quantization`
**Owner:** paulohenriquevn
**Created:** 2026-06-30
**Time budget:** 6h (per-project breakdown in ADR D1)

## Context

M22 (`ROADMAP-v2.md:128`) requires an **own scale + quantization index in Rust** (target DiskANN/SBQ-quality),
substituting pgvectorscale **only** with recall parity **and** measured memory profile (`ROADMAP-v2.md:135`); else
an honest ADR keeps pgvectorscale (anti-sunk-cost). It is tagged **risk MÁXIMO — the most expensive of v2**;
measurement-first is the rigorous guard-rail. M21 just shipped own HNSW + IVFFlat ANN search in Rust
(`.claude/knowledge-base/reviews/m21-own-ann-index-review-2026-06-30.md`, coexistence, SQL-callable) reusing the
M20 f32 distance kernel. M22 adds **quantization** — a compressed vector representation that trades a controlled
recall loss for a large memory reduction (the pgvectorscale SBQ value proposition). This discovery is the
prior-art investigation that must precede any code (Unbreakable Rule 9; TheoDB rule 1 "anchor on SOTA"; rule 5
"performance/memory is a claim, not an opinion").

The PhD-rigor profile applies (`.claude/rules/discover-phd-rigor.md`): M22 touches the performance/memory-bearing
P2 pillar, so the techniques corner must anchor on the pgvectorscale SBQ SOTA, cite ≥2 primary sources per
technique, and surface the recall+memory methodology rather than asserting conclusions.

The recall harness exists and is reused (`benchmarks/theodb_bench/recall.py:61` `recall_at_k`; `:41`
`brute_force_ground_truth`); memory is measured as **bytes/vector** (`benchmarks/theodb_bench/db.py:131`
`index_size_bytes` exists for real indexes; the quantized-size formula gives bytes/vector for the SQL-callable
form). M21's own HNSW/IVF (`theodb_rs/src/ann/`) is the search substrate the quantizer plugs into.

## Objective

Decide whether TheoDB can build an own SBQ-quality quantizer + quantized ANN search in Rust that reaches
pgvectorscale recall@k parity at a comparable memory profile, and **how** (coexistence vs substitution), so the
M22 implementation plan is evidence-backed and the recall+memory gate is defined before any code is written.

- [ ] All research questions answered with citations to `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison table populated for every in-scope reference project (pgvectorscale / vectorchord)
- [ ] Recommendations section provides at least one concrete decision proposal per in-scope research question
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/src/access_method/sbq/` (`quantize.rs`, `storage.rs`, `node.rs`, `cache.rs`, `tests.rs`), `meta_page.rs` | The SBQ quantization + memory-layout SOTA we must match |
| `.claude/knowledge-base/references/vectorchord/` | `crates/rabitq/src/` (`bit.rs`, `bits.rs`, `lib.rs`, `packing.rs`), `crates/rabitq/Cargo.toml` | RaBitQ — a second quantization datapoint (asymmetric LUT, residual) |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/{build,scan,vacuum}.rs` (the full StreamingDiskANN AM) | The planner-integrated on-disk AM is M22b — M22 is measurement-first SQL-callable (mirrors the M21 scope decision) |
| `.claude/knowledge-base/references/vectorchord/src/index/` (the vchordrq AM wiring) | Already covered in M21 discovery; M22 needs only the quantization crate |
| `.claude/knowledge-base/references/pgvector/` | pgvector has no quantization (it is the f32 baseline M21 already covered) |
| Any project NOT cloned into `.claude/knowledge-base/references/` | Cross-Project Rule: never claim a project feature without reading its source |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvectorscale: 4h (the SBQ quantizer + memory layout we must match — deepest dive); vectorchord: 2h
(RaBitQ as a corroborating datapoint for the asymmetric-distance + packing techniques).

**Rationale:** pgvectorscale SBQ is the parity target (recall + memory must match IT), so its quantizer earns the
deepest read. vectorchord RaBitQ is a second source to avoid single-source bias (PhD-rigor R2) but does not need
a full read.

**Alternatives considered:** equal 3h split (rejected — SBQ parity is the crux); pgvectorscale-only (rejected —
violates R2 ≥2 sources per technique).

**Stop condition — per question (mandatory):** When a question's Fase A returns empty matches after 3 consecutive
retries with different query variants, mark BLOCKED "Fase A exhausted" and continue; never pad from another
question's scope.

**Stop condition — per project (mandatory):** When a project's budget is exhausted with questions pending, mark
them BLOCKED "budget exhausted" and continue; if every remaining project is in the same state, emit
`<promise>BLUEPRINT_BLOCKED</promise>` with the honest report.

**Anti-pattern:** NEVER fabricate Fase B answers to close a Fase-A-exhausted question (Unbreakable Rule 3).

**Consequences:** the halt-loop stops on budget exhaustion; blocked questions surface as next-discovery seed.

### D2 — Investigation depth

**Decision:** Read the pgvectorscale SBQ files end-to-end (`quantize.rs`, `node.rs`, `storage.rs`); for
vectorchord RaBitQ, Grep the quantization entry points (Fase A) then Read each hotspot (Fase B). Do NOT read the
full StreamingDiskANN AM (build/scan/vacuum) — that is M22b scope.

**Rationale:** the quantization parity demands a full read of the SBQ quantizer; the RaBitQ comparison is a
structural pattern best located by symbol grep then read at the hotspot (KISS). Reading the full AM would blow
the budget (YAGNI).

**Alternatives considered:** full read of both AMs (rejected — budget + M22b scope); grep-only (rejected — loses
the quantization math).

**Consequences:** the blueprint cites line-exact SBQ quantizer behavior + a RaBitQ comparison; the AM wiring is
explicitly deferred.

### D3 — The migration decision is a blueprint deliverable, not a single question

**Decision:** the coexistence-vs-substitution decision (M22 DoD) is synthesized in the blueprint's Cross-cutting
Comparison + ADRs from the technique questions (Q1 SBQ, Q2 RaBitQ, Q3 recall/memory tradeoff) + the tooling
questions (Q4 memory layout, Q5 search integration) + the test question (Q7) — it is a *decision we make*, not a
*fact we read*.

**Rationale:** per `/discover-plan` rule "a discovery plan ASKS questions; it doesn't answer them." The migration
choice is fed by the questions, mirroring M21 ADR D3.

**Alternatives considered:** a dedicated "coexist or substitute?" question (rejected — unanswerable from a
reference); deferring to `/to-plan` (rejected — the DoD demands the decision be documented in the blueprint).

**Consequences:** the blueprint MUST contain an explicit ADR on coexistence vs substitution + the "retain
pgvectorscale" anti-sunk-cost option, citing Q1–Q7 evidence; `/to-plan` consumes it as a locked decision.

### D4 — The reference is the SBQ-scaffolding source, NOT a drop-in; "retain pgvectorscale" is valid

**Decision:** pgvectorscale SBQ (`quantize.rs`) is the **algorithm** source for the quantizer; its DiskANN AM
wiring is out of scope (M22b). vectorchord RaBitQ is a corroborating quantization datapoint only. "Retain
pgvectorscale (own quantizer gated/deferred)" is a **valid** blueprint outcome under measurement-first — the
recommendation must not force a substitution the recall+memory evidence does not support (anti-sunk-cost,
`ROADMAP-v2.md:135`).

**Rationale:** M22 is risk-MÁXIMO; the measurement-first guard-rail is the entire point. Conflating the quantizer
with the full DiskANN AM would fabricate technique claims (Rule 3).

**Alternatives considered:** port the whole StreamingDiskANN (rejected — M22b scope, multi-month); force
"substitute pgvectorscale" (rejected — violates measurement-first).

**Consequences:** Q1/Q4/Q5 cite the SBQ quantizer + memory layout; the final Recommendation keeps coexistence-
retain a first-class option.

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — grep map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How does pgvectorscale **SBQ** quantize a vector — the per-dimension threshold (mean-based training), `num_bits_per_dimension`, the bit packing, and the `quantize(full_vector) -> Vec<SbqVectorElement>` output? | techniques | `.claude/knowledge-base/references/pgvectorscale/` | `grep -nE "fn quantize\|num_bits_per_dimension\|start_training\|finish_training\|threshold\|mean\|count" pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs` | Read `quantize.rs` end-to-end (`SbqQuantizer`, `quantize:52`, `start_training:104`, `finish_training:150`); capture the training (mean per dim → threshold), the 1-bit/n-bit encode, the packing into the store type | Prose + the quantize algorithm (train → threshold → bit-encode → pack) with `path:line` + the SbqVectorElement layout |
| Q2 | How does vectorchord **RaBitQ** quantize — `code(vector)`, `code_metadata` (the 4 correction factors), `code_elements`, and the asymmetric query LUT (`preprocess`)? | techniques | `.claude/knowledge-base/references/vectorchord/` | `grep -nE "pub fn code\|code_metadata\|code_elements\|preprocess\|pack_code\|CodeMetadata" vectorchord/crates/rabitq/src/bit.rs` | Read `bit.rs` (`code:97`, `code_metadata:68`, `code_elements:88`, `preprocess:126`, `pack_code:135`); capture the binary code + the 4-factor metadata + the asymmetric distance estimator | Prose + the RaBitQ code+metadata structure + asymmetric-distance sketch with `path:line` |
| Q3 | What is the **recall-vs-memory tradeoff** of SBQ — how the quantized distance approximates the true distance, and whether a full-precision **re-ranking** pass recovers recall (the SBQ→rerank pattern)? | techniques | `.claude/knowledge-base/references/pgvectorscale/` | `grep -nE "rerank\|resort\|num_neighbors\|full_distance\|exact\|distance\|SbqSearch\|recheck" pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs pgvectorscale/pgvectorscale/src/access_method/sbq/storage.rs` | Read the distance + (if present) rerank path in `quantize.rs`/`storage.rs`; capture how the quantized estimate is used and whether full vectors are re-checked for the top candidates | Statement of the tradeoff: bits/dim → memory (bytes/vector) vs recall; whether rerank is needed; the parity contract (recall within eps at matched bits/dim, memory ≤ pgvectorscale) |
| Q4 | What is the **memory layout** — `quantized_size_bytes(num_dimensions, num_bits_per_dimension)`, the packing into the store word, and how M22 computes bytes/vector to measure the memory profile? | tools | `.claude/knowledge-base/references/pgvectorscale/` | `grep -nE "quantized_size\|quantized_size_bytes\|BITS_STORE_TYPE_SIZE\|SbqVectorElement\|num_dimensions" pgvectorscale/pgvectorscale/src/access_method/sbq/quantize.rs pgvectorscale/pgvectorscale/src/access_method/sbq/node.rs` | Read `quantized_size_bytes:47` + the store-type packing + `ClassicSbqNode` (`node.rs`/`storage.rs:27`); capture the exact bytes/vector formula | The bytes/vector formula (f32 = 4·dim vs SBQ = ceil(dim·bits/8)) + the measurement method M22 uses |
| Q5 | How does the quantizer **integrate with the ANN search** — does SBQ replace the stored vector in the graph/list, and how does the quantized distance drive candidate selection (so M22 can plug it into the M21 HNSW/IVF)? | tools | `.claude/knowledge-base/references/pgvectorscale/` | `grep -nE "SbqSearchDistanceMeasure\|distance\|get_distance\|quantized\|fn search\|Storage" pgvectorscale/pgvectorscale/src/access_method/sbq/storage.rs pgvectorscale/pgvectorscale/src/access_method/sbq/mod.rs` | Read the storage/search glue in `sbq/storage.rs` + `sbq/mod.rs`; capture how the quantized node is read during search + the distance measure | Sketch: how M22 stores quantized vectors + uses quantized distance in the M21 HNSW/IVF search (with optional rerank) with `path:line` |
| Q6 | What runtime/build **dependencies** does the quantization pull in (SIMD, zerocopy/rkyv serialization, half), with versions + **licenses** (Apache/MIT/BSD gate — TheoDB D1; AGPL forbidden)? | deps | `.claude/knowledge-base/references/pgvectorscale/`, `.claude/knowledge-base/references/vectorchord/` | `grep -nE "simdeez\|rkyv\|zerocopy\|half\|version\|license\|workspace" pgvectorscale/pgvectorscale/Cargo.toml vectorchord/crates/rabitq/Cargo.toml vectorchord/Cargo.toml` (EC-1: rabitq uses `license.workspace = true` → also read `[workspace.package] license` in the workspace-root `vectorchord/Cargo.toml`) | Read both crate manifests + the WORKSPACE-root manifest for inherited `license`; capture each non-pgrx/std dep + version + resolved license; flag any AGPL/GPL | Table: dep → version → resolved license (incl. workspace-inherited) → purpose → AGPL-clean? + the minimal set M22 actually needs (likely std-only bit ops) |
| Q7 | How does pgvectorscale **test** SBQ (recall + memory: low-memory index creation), so M22 mirrors the test shape and reuses `benchmarks/theodb_bench` for the recall gate + a bytes/vector memory gate? | tests | `.claude/knowledge-base/references/pgvectorscale/` | `grep -rnE "#\[pg_test\]\|num_bits\|low_memory\|recall\|compressed\|storage_layer" pgvectorscale/pgvectorscale/src/access_method/sbq/tests.rs` | Read representative SBQ tests; capture how they build a compressed index + assert correctness; map onto `theodb_bench/recall.py:61` reuse + a bytes/vector assertion | Table: test → what it builds/asserts → `path:line` + a sketch of the M22 recall+memory gate reusing `theodb_bench` |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q7 | Covered |
| Dependencies | Q6 | Covered |
| Tools | Q4, Q5 | Covered |
| Techniques | Q1, Q2, Q3 | Covered |

**Coverage: 4/4 corners covered (100%)**

Question budget: 7 total (within the 5–10 default window); techniques carries 3 (≥2 per PhD-rigor R4, within the
deterministic ≤3-per-corner budget); tools 2; deps + tests 1 each. Each question maps to exactly one corner.

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | every `.claude/knowledge-base/references/{project}/{path}` declared in its Fase A exists | Mark Qx BLOCKED "path not found", continue |
| Per-question Fase A budget | Fase A returned ≥1 hotspot OR 3 query-variant retries attempted | After 3 retries empty, mark Qx BLOCKED "Fase A exhausted"; continue |
| After answering Qx | blueprint section under Qx has ≥1 citation | Re-iterate Qx (1 retry max) |
| Techniques depth (PhD R2) | each technique question (Q1/Q2/Q3) cites ≥2 primary sources (≥2 distinct ref files or a paper) | Add the second source or mark the gap honestly |
| Memory metric (Q4) | the bytes/vector formula is stated concretely (f32 4·dim vs SBQ ceil(dim·bits/8)), not hand-waved; gate compares COMPUTED bytes/vector, NOT `pg_relation_size` (EC-3 — that is M22b on-disk) | Re-state the formula + measurement method before DONE |
| Rerank honesty (Q3, EC-2) | the blueprint states HONESTLY whether pgvectorscale SBQ does a full-precision rerank (Fase A found `num_neighbors` but no `rerank` symbol) — if absent, "own quantizer + optional rerank" is a TheoDB design choice, not a borrowed fact | Re-state honestly; never fabricate a rerank path absent from source |
| Per-project time budget | project budget not exhausted | When exhausted, mark remaining Qx for that project BLOCKED "budget exhausted"; advance |
| Before promising complete | all 4 coverage corners populated AND a coexistence-vs-substitution ADR exists (keeping "retain pgvectorscale" valid, D4) | Refuse promise, continue iterating |

## Acceptance Criteria

- [ ] All research questions answered OR explicitly marked BLOCKED with reason
- [ ] All four coverage corners have populated sections in the blueprint
- [ ] Every citation in the blueprint points to a real `.claude/knowledge-base/references/{...}` path
- [ ] At least one ADR synthesizes the **coexistence-vs-substitution** migration decision (D3/D4)
- [ ] The recall+memory gate design reuses `benchmarks/theodb_bench` (Q7) + a concrete bytes/vector formula (Q4)
- [ ] Time budget respected per project
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint saved at `.claude/knowledge-base/discoveries/blueprints/m22-own-quantization-blueprint.md`

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed → re-score)
- [ ] Final `/discover-confidence` verdict recorded in the blueprint header
- [ ] No fabricated citations
- [ ] Coverage Matrix 100% covered
- [ ] ADRs reference at least one principle from project rules (Rule 9 Don't-Reinvent; KISS; `.claude/rules/discover-phd-rigor.md`; `architecture.md` DIP for the quantizer↔search boundary)
