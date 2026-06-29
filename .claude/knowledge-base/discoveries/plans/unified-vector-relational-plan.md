# Discovery Plan: Unified vector+relational+AI — filtered search, the unified query, and Pinecone migration

> **Version 1.1** (edge-case MUST-FIX absorbed: Pinecone paths repointed to `pinecone/db_data/dataclasses/` + `pinecone/index/__init__.py`; over-filtering edge added as a checkpoint) — Investigates how to deliver TheoDB's all-in-one **unification** moat (ADR 0005): correct
> **filtered vector search** (relational `WHERE` + vector `ORDER BY` without losing valid candidates), the
> **canonical unified query** (vector `JOIN` relational + `ai.*` in one transactional SQL), and a **Pinecone
> migration** path (export/fetch format → TheoDB table). References: pgvector (iterative scan / filtering),
> pgvectorscale (label filtering), pinecone-python-client (fetch/upsert format). Output: a blueprint that
> unblocks **M16**. Honesty (ADR 0005): performance is competitive, not leader — this discovery targets
> **correctness (recall preserved under filter)** + demonstrable unification + migration, NOT a speed number.

**Slug:** `unified-vector-relational`
**Owner:** TheoDB maintainers
**Created:** 2026-06-29
**Time budget:** 7h (pgvector 3h, pgvectorscale 2h, pinecone-python-client 2h)

## Context

ADR `0005-unification-as-differentiator` (LOCKED, sign-off CTO) sets the moat: vector + relational + AI in
one instance, single transactional SQL, no ETL/2nd system — the alternative OSS to AlloyDB/Pinecone. M16
must make this **real and demonstrable**. The hardest correctness risk is **filtered vector search**: an
approximate index (HNSW/IVFFlat) scanned then post-filtered can return *fewer* rows than asked (over-filtering)
— the exact pain that makes pure vector DBs struggle and that we must get right to claim "vector+relational
together is better in one system". pgvector's README documents this (post-filter + `hnsw.iterative_scan`);
pgvectorscale offers label-filtering. We must learn the correct pattern + how to prove it (recall preserved,
index used via `EXPLAIN`). Honors `.claude/rules/architecture.md` (extension boundaries), `.claude/rules/testing.md`
(edge vs negative — over-filtering is the edge), `.claude/rules/public-copy.md` (no perf claim), Unbreakable
Rule 9 (reuse pgvector/pgvectorscale filtering, don't reinvent).

## Objective

Produce a blueprint that lets us implement, with evidence: (a) a **correct filtered vector search** pattern
(recall preserved under a relational filter, index-used), (b) the **canonical unified query** (vector `JOIN`
relational + `ai.*`, one transaction), and (c) a **Pinecone import** mapping (vectors+metadata → TheoDB).

- [ ] All research questions answered with citations to `.claude/knowledge-base/references/`
- [ ] Cross-cutting comparison populated (pgvector vs pgvectorscale filtering; Pinecone data model vs TheoDB)
- [ ] A concrete recommendation for the filtered-search pattern (iterative scan vs label filtering vs partial index) with the recall trade-off
- [ ] A concrete Pinecone→TheoDB mapping (vector + metadata columns) + import shape
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

| Project | In-scope subdirectories | Reason |
|---|---|---|
| `.claude/knowledge-base/references/pgvector/` | `README.md` (§ Filtering / Iterative index scans), `src/hnswscan.c`, `src/ivfscan.c`, `test/sql/hnsw_vector.sql`, `test/sql/ivfflat_vector.sql` | Canonical Postgres filtered-search semantics (post-filter + iterative scan) + how it's tested |
| `.claude/knowledge-base/references/pgvectorscale/` | `pgvectorscale/src/access_method/scan.rs`, `pgvectorscale/sql/vectorscale--0.8.0--0.9.0.sql` (label opclass), `tests/` | Label-filtering approach to in-index filtering (the alternative to post-filter) |
| `.claude/knowledge-base/references/pinecone-python-client/` | `pinecone/db_data/dataclasses/` (`vector.py`, `fetch_response.py`, `upsert_response.py`), `pinecone/index/__init__.py` (fetch/upsert interface), `tests/` | Pinecone vector+metadata data model + fetch/upsert shape → the migration mapping (EC-1 fix: `pinecone/data/` is empty) |

### Out-of-Scope (explicit)

| Project / Subdir | Why excluded |
|---|---|
| `.claude/knowledge-base/references/pgvector/src/*.h` build internals beyond scan | We need scan/filter semantics, not the full C API |
| `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/sbq/` (quantization) | Quantization is the perf axis — out of scope per ADR 0005 (competitive, not leader) |
| `.claude/knowledge-base/references/pinecone-python-client/pinecone/grpc/` | Transport detail; the data model (not gRPC) is what migration needs |
| Any standalone vector DB (Qdrant/Milvus) | Not Postgres extensions; out of the unification scope |
| Any `*/{tests/perf,build,dist}/` | Perf benchmarks (ADR 0005: not a speed number) + build artifacts |

## ADRs

### D1 — Time budget + stop conditions

**Decision:** pgvector 3h (canonical filtering + iterative scan + how tested — the core correctness), pgvectorscale 2h (label-filtering alternative), pinecone-python-client 2h (data model for migration).

**Rationale:** filtered-search correctness is the highest-risk deliverable → pgvector (the README documents the exact pre/post-filter + iterative-scan semantics) earns the deepest dive. pgvectorscale is the alternative pattern. Pinecone is a bounded data-model read.

**Stop condition — per question:** Fase A empty after 3 query-variant retries → mark BLOCKED ("Fase A exhausted"), continue. Never fabricate Fase B.

**Stop condition — per project:** budget exhausted with questions pending → mark them BLOCKED ("budget exhausted"), continue; if all remaining projects are in that state → emit `<promise>BLUEPRINT_BLOCKED</promise>`.

**Anti-pattern:** never fabricate a Fase B answer to close an exhausted question (Unbreakable Rule 3).

**Consequences:** the halt-loop stops per budget; blocked questions surface as next-discovery seed.

### D2 — Investigation depth

**Decision:** Read the pgvector README § Filtering + the scan entry points end-to-end (load-bearing semantics); Grep + targeted Read for pgvectorscale label scan and the Pinecone data model.

**Rationale:** the filtering semantics (post-filter vs iterative vs label) are the decision; reading them partially loses the exact rule. The Pinecone model only needs the fetch/upsert shape.

**Consequences:** deep, line-exact on filtering; entrypoint-level on Pinecone (flagged honestly).

### D3 — Coverage corners (all four covered)

**Decision:** all four corners covered (see matrix). No deferral.

**Rationale:** filtering touches techniques (the algorithm), tests (how recall-under-filter is asserted), deps (what the unified query/import needs), tools (`EXPLAIN`/`SET` to prove index use).

**Consequences:** techniques corner carries 3 (filtered-search, unified-query, pinecone-mapping).

## Research Questions

| # | Question | Corner | Reference project(s) | Fase A (broad — map) | Fase B (deep — Read) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | **Filtered vector search — comparison:** how does pgvector handle a relational `WHERE` + vector `ORDER BY` (post-filter vs `hnsw.iterative_scan` vs partial index, and the over-filtering failure mode) VS pgvectorscale's in-index **label-filtering** (which opclass/operator enables it)? | techniques | `.claude/knowledge-base/references/pgvector/README.md`, `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/scan.rs`, `.../sql/vectorscale--0.8.0--0.9.0.sql` | Grep `## Filtering`/`iterative_scan`/`max_scan_tuples` in pgvector README AND `label`/`vector_smallint_label_ops`/`&&` in pgvectorscale sql + scan.rs | Read pgvector § Filtering + § Iterative index scans, then the pgvectorscale label opclass DDL + scan-side filter | A side-by-side: post-filter+iterative (pgvector) vs label-filter (pgvectorscale), over-filtering failure mode + the recall trade-off + a recommended default, with citations |
| Q2 | What is the canonical **unified query** shape already enabled by our pieces (vector `JOIN` relational + `WHERE` + `ai.*`)? | techniques | `.claude/knowledge-base/references/pgvector/README.md`, repo `sql/40-theodb-hybrid.sql` (our dynamic SQL over a relational table) | Grep `ORDER BY` + `<=>` patterns in README; Read our `ai.hybrid_search_rrf` SQL shape | Read how a vector order-by composes with a relational table + filter | A canonical SQL skeleton (vector + JOIN + WHERE + ai.*) the implementation will turn into a first-class example |
| Q3 | What is the Pinecone **data model** (vector id + values + metadata) and the fetch/upsert shape to map into a TheoDB table? | techniques | `.claude/knowledge-base/references/pinecone-python-client/pinecone/db_data/dataclasses/vector.py`, `.../dataclasses/fetch_response.py`, `.../pinecone/index/__init__.py` | Grep `class Vector`, `metadata`, `def fetch`, `def upsert` in `db_data/dataclasses` + `index/__init__.py` | Read the Vector dataclass + fetch_response to capture id+values+metadata shape | A field mapping: Pinecone {id, values, metadata} → TheoDB {id, embedding vector, metadata cols/jsonb} + import shape |
| Q4 | How does pgvector **test** filtered / iterative-scan correctness (so we mirror the recall-under-filter assertion)? | tests | `.claude/knowledge-base/references/pgvector/test/sql/hnsw_vector.sql`, `.../test/sql/ivfflat_vector.sql` | Grep `iterative_scan`, `WHERE`, `ORDER BY` in the test sql | Read the relevant test cases | The assertion pattern for filtered search → our e2e test design |
| Q5 | What runtime/dev deps does a Pinecone import need (parse the fetch/export format), and can we do it with stdlib only? | deps | `.claude/knowledge-base/references/pinecone-python-client/pinecone/db_data/dataclasses/`, `pinecone-python-client/pyproject.toml` | Grep `import` in `db_data/dataclasses`; check `pyproject.toml` for the serialization format (json) | Read the dataclass serialization to see the on-disk/fetch JSON shape | Dep list for import (ideally stdlib/json only — Rule 9/parsimony) + citations |
| Q6 | What tools prove the index is actually used under a filter (so the demo is honest)? | tools | `.claude/knowledge-base/references/pgvector/README.md` | Grep `EXPLAIN`, `SET hnsw`, `enable_seqscan` in README | Read the query-tuning section | The `EXPLAIN (ANALYZE)` + `SET hnsw.iterative_scan` recipe to assert index use in tests |

## Coverage Matrix

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q4 | Covered |
| Dependencies | Q5 | Covered |
| Tools | Q6 | Covered |
| Techniques | Q1, Q2, Q3 | Covered |

**Coverage: 4/4 corners covered (100%)** — techniques carries 3 (≤ 3/corner budget); total 6 questions. Q1 fuses the pgvector vs pgvectorscale filtered-search comparison into one question (was Q1+Q2).

## Halt-loop Checkpoints

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | every cited `.claude/knowledge-base/references/{...}` path exists | mark Qx BLOCKED "path not found", continue |
| Per-question Fase A budget | ≥ 1 hotspot OR 3 retries | after 3 empty retries, mark BLOCKED "Fase A exhausted" |
| Filtered-search correctness (Q1) | blueprint states the over-filtering failure mode AND the fix (iterative scan / label / partial index) | re-iterate (1 retry) |
| Over-filtering test design (Q4, EC-2) | blueprint's test design includes the NEGATIVE/edge case — a selective `WHERE` (~1% match) returning fewer than k rows under default `ef_search`, fixed by iterative scan / label / partial index (recall preserved) — not happy-path only | re-iterate Q4 (1 retry) |
| Honesty (ADR 0005) | no speed/throughput claim appears as a conclusion; only correctness/recall + unification | strip any perf claim; mark UNBENCHMARKED |
| Web-source discipline | WebFetch host ∈ allowlist (postgresql.org/github.com) | drop off-allowlist source |
| Before promising complete | 4 corners populated AND Q1 gives a concrete filtered-search recommendation AND Q3 gives a concrete Pinecone→TheoDB mapping | refuse promise, continue |

## Acceptance Criteria

- [ ] All research questions answered OR BLOCKED with reason
- [ ] All four coverage corners populated
- [ ] Every citation resolves to a real `.claude/knowledge-base/references/{...}` path
- [ ] Concrete filtered-search recommendation (pattern + recall trade-off) — Q1
- [ ] Concrete unified-query skeleton — Q2
- [ ] Concrete Pinecone→TheoDB mapping + import dep list — Q3/Q5
- [ ] ≥ 1 ADR synthesizing decisions
- [ ] `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint at `.claude/knowledge-base/discoveries/blueprints/unified-vector-relational-blueprint.md`

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed)
- [ ] Final `/discover-confidence` verdict in the blueprint header
- [ ] No fabricated citations
- [ ] Coverage Matrix 100%
- [ ] ADRs cite ≥ 1 project rule/principle (architecture.md / testing.md / public-copy.md / Rule 9 / ADR 0005)
