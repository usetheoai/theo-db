---
slug: m7-hybrid-search-rrf
created_at: 2026-06-28
goal: Ship hybrid search (FTS + vector fused by RRF) on permissive OSS with measured recall evidence
---

# Plan: Hybrid Search (FTS + vector) + Reciprocal Rank Fusion — M7-S1

> **Version 1.0** — Ship M7-S1: a permissive-OSS hybrid-search capability that fuses PostgreSQL full-text
> search (FTS) with `pgvector` similarity via Reciprocal Rank Fusion (RRF), exposed as a reusable SQL
> function baked into the image, plus an extension of the M2 recall harness to a BEIR-style hybrid eval so
> the recall is **measured, not asserted** (CLAUDE.md TheoDB rule 5). Builds directly on M2; no AGPL
> dependency (the AGPL `pg_search`/BM25 path is deferred to M7-S2 per ROADMAP). Decisions inherited from
> the SHIPPABLE_WITH_CAVEATS blueprint `.claude/knowledge-base/discoveries/blueprints/m7-hybrid-search-rrf-blueprint.md`.

## Goal

> Enable TheoDB users to run hybrid search (full-text + vector) fused by Reciprocal Rank Fusion over
> permissive OSS, measured by the `ai.hybrid_search_rrf` contract integration tests passing AND the
> extended recall harness reporting nDCG@10 + Recall@100 for three retrievers (pure-vector, pure-FTS,
> RRF-hybrid) on a BEIR-style labelled dataset.

## Context

ROADMAP `### M7` (IA avançada) DoD-2 requires "Hybrid search (texto + semântico) + reranking (RRF) com
recall medido (ex.: BEIR)". This plan is **M7-S1**, the first slice — the pure-OSS immediate win on the M2
`pgvector` base. The SOTA anchor (ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md`) is
AlloyDB's `ai.hybrid_search()`, which fuses lexical + vector results with RRF; TheoDB matches the capability
with permissive PostgreSQL primitives and wins on model-agnosticism (CLAUDE.md TheoDB rule 1). The target API
is specified at `docs/features/06-busca-hibrida.md`. The blueprint
`.claude/knowledge-base/discoveries/blueprints/m7-hybrid-search-rrf-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89)
fixed the design: GIN+`ts_rank_cd` FTS leg, RRF `1/(k+rank)` with k=60 (exposed as a parameter),
`FULL OUTER JOIN` + `COALESCE` for empty-leg handling, manual-SQL-first surface. The blueprint also surfaced
the **harness gap (E4)**: the M2 recall harness is pure-vector with no FTS/qrels leg — so the recall win
cannot be claimed until the harness is extended. This plan closes that gap (no unbenchmarked claim —
`public-copy.md` §4-§5). The BM25 permissive alternative (AGPL `pg_search` is barred by D1) is **out of
scope** — deferred to its own discovery (M7-S2).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/30-theodb-embed.sql` | 89 | `ba98af3` (2026-06-27) | M2 embedding SQL fn (`theodb.embed`) baked via initdb.d | Must keep `theodb.embed` + `theodb` schema intact; the new file is additive |
| `sql/40-theodb-hybrid.sql` (NEW) | 0 | — | (to be created) RRF hybrid-search SQL fn | — |
| `Dockerfile` | 67 | `3c85baa` (2026-06-27) | Builds `theo-db` image; copies `sql/*.sql` to `/docker-entrypoint-initdb.d/` | The existing `COPY sql/30-...` line must stay; add a `COPY sql/40-...` line |
| `smoke.sh` | 21 | `ef532c2` (2026-06-26) | Brings up engine, proves `pgvector` live via `pg_isready`+`psql` | Existing vector smoke must keep passing; hybrid smoke is additive |
| `benchmarks/theodb_bench/db.py` | 134 | `651bf65` (2026-06-27) | `VectorDB` adapter (connect/ping/create index/query) for the harness | `VectorDB` public methods stay backward-compatible; FTS methods are additive |
| `benchmarks/theodb_bench/metrics.py` | 33 | `84aead5` (2026-06-27) | Latency percentiles + best-of-N QPS | `latency_percentiles`/`qps_best_of_n` signatures unchanged; add nDCG/recall@100 alongside |
| `benchmarks/theodb_bench/recall.py` | 82 | `651bf65` (2026-06-27) | Distance-thresholded pure-vector recall@k + brute-force ground truth | `recall_at_k` signature unchanged (pure-vector path stays valid) |
| `benchmarks/theodb_bench/hybrid.py` (NEW) | 0 | — | (to be created) RRF fusion + 3-retriever eval driver | — |
| `benchmarks/theodb_bench/beir.py` (NEW) | 0 | — | (to be created) BEIR-style labelled-dataset loader (corpus/queries/qrels) | — |
| `benchmarks/tests/test_hybrid.py` (NEW) | 0 | — | (to be created) unit tests for RRF fusion + nDCG/Recall@100 math | — |
| `benchmarks/tests/test_integration.py` | (exists) | `c421550` (2026-06-27) | Integration tests vs real container (`integration` marker) | Existing tests stay green; hybrid integration tests appended |
| `docs/features/06-busca-hibrida.md` | (exists) | — | Target API spec for hybrid search | Keep as spec; implementation aligns to its manual-SQL §41 shape |
| `docs/benchmarks/m7-hybrid-recall.md` (NEW) | 0 | — | (to be created) measured recall report (the win evidence) | — |
| `.github/workflows/ci.yml` | (exists) | — | CI pipeline | Existing jobs stay; add `hybrid-search` job |
| `CHANGELOG.md` | (exists) | — | Public contract | `[Unreleased]` gets the M7-S1 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `theodb.embed(content, model)` in `sql/30-theodb-embed.sql`
  - **Callers (production):** loaded at initdb; invoked ad-hoc in SQL + by `tools/embedding_server.py` flows.
  - **Callers (tests):** `benchmarks/tests/test_embed_sql.py`.
  - **External (public API consumed by other repos):** no — internal SQL surface. The new `ai.hybrid_search_rrf` is additive and does not change `theodb.embed`.
- **Symbol:** `VectorDB` in `benchmarks/theodb_bench/db.py`
  - **Callers (production):** `benchmarks/theodb_bench/harness.py`, `benchmarks/theodb_bench/__main__.py`.
  - **Callers (tests):** `benchmarks/tests/test_db.py`, `benchmarks/tests/test_integration.py`.
  - **External:** no — the harness is dev-only tooling. New FTS/hybrid methods are additive (no signature change to existing methods).
- **Symbol:** `recall_at_k` in `benchmarks/theodb_bench/recall.py`
  - **Callers (production):** `benchmarks/theodb_bench/harness.py`.
  - **Callers (tests):** `benchmarks/tests/test_recall.py`.
  - **External:** no. Unchanged — the hybrid eval adds new metric functions (nDCG@10, Recall@100) rather than altering this one.

Enumerated via `grep -rln 'recall_at_k\|VectorDB\|theodb.embed' --include='*.py' --include='*.sql' benchmarks/ sql/`.

### Domain glossary

- **RRF (Reciprocal Rank Fusion)** — rank-only fusion: `score(d) = Σ_legs 1/(k + rank_leg(d))`; combines two ranked lists without comparing raw scores (Cormack et al. 2009).
- **k (RRF constant)** — smoothing constant in the RRF denominator; empirical default 60; dampens the influence of a single top-ranked outlier.
- **FTS leg** — the PostgreSQL full-text-search ranked list: `to_tsvector`/`to_tsquery` matched via `@@`, ranked by `ts_rank_cd`, indexed by GIN.
- **Vector leg** — the `pgvector` similarity ranked list: ordered by `<=>` (cosine) / `<->` (L2) distance, indexed by HNSW.
- **Empty-leg** — a document matched by only one retriever; handled by `FULL OUTER JOIN` + `COALESCE(1/(k+rank), 0)`.
- **nDCG@10** — normalized Discounted Cumulative Gain at rank 10; BEIR's primary graded-relevance metric.
- **Recall@100** — fraction of relevant docs (per qrels) retrieved in the top 100; BEIR's secondary metric.
- **qrels** — query-relevance judgements: graded (query_id, doc_id, relevance) labels in a BEIR-style dataset.

### Architecture boundaries affected

Per `rules/architecture.md`: the SQL function is an **infrastructure/adapter** surface inside the database
image (same layer as `theodb.embed`); it implements a capability, not domain orchestration — no inner layer
imports it. The benchmark harness is **dev-only tooling** outside the product layering (it is a client of the
DB boundary via `psycopg`). The plan crosses the **DB ↔ benchmark-client** boundary only through the existing
`VectorDB` adapter (DIP: the harness depends on the adapter, not on raw SQL strings scattered across modules).
No new cross-layer import is introduced into the product code.

## Prior Art & Related Work

- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/m7-hybrid-search-rrf-blueprint.md` —
  the full design source. This plan implements its ADRs D1 (GIN+`ts_rank_cd` default), D2 (RRF k=60 exposed
  as param; manual-SQL MVP), D3 (borrow RRF technique, never AGPL code), and Recommendations 1-8.
- **Reference projects (technique witnesses):**
  `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs:99-118` (RRF SQL shape: `RANK() OVER`,
  `FULL OUTER JOIN`, `1.0/(60+rank)`, `COALESCE` — AGPL, read for technique only);
  `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql:102-105`
  (generated stored `tsvector` + GIN index);
  `.claude/knowledge-base/references/pgvector/README.md:628-629` (`plainto_tsquery` + `ts_rank_cd` hybrid).
- **External literature:** Cormack, Clarke & Büttcher, SIGIR'09, "Reciprocal Rank Fusion…"
  (`https://dl.acm.org/doi/10.1145/1571941.1572114`) — RRF formula + k=60 justification; Thakur et al. 2021,
  "BEIR" (`https://arxiv.org/abs/2104.08663`) — nDCG@10 + Recall@100 methodology.
- **Patterns skills:** none present in `skills/*-patterns/` matching this topic (verified — no hybrid/FTS/RRF
  patterns skill exists).

## Objective

- [ ] A reusable SQL function `ai.hybrid_search_rrf(...)` fuses an FTS leg + a vector leg via RRF, baked into the image (initdb.d), with k exposed as a parameter (default 60).
- [ ] Empty-leg queries (matched by only one retriever) surface correctly via `FULL OUTER JOIN` + `COALESCE` — proven by a contract test.
- [ ] The recall harness is extended with a BEIR-style loader (corpus/queries/qrels) + nDCG@10 + Recall@100 metrics + a 3-retriever driver (pure-vector, pure-FTS, RRF-hybrid).
- [ ] The hybrid eval runs end-to-end against the real container and reports the three retrievers' nDCG@10 + Recall@100 (measured numbers, no fabrication).
- [ ] `smoke.sh` exercises a hybrid query with a golden top-k assertion; CI runs the hybrid contract + a capped eval.
- [ ] A benchmark report `docs/benchmarks/m7-hybrid-recall.md` records the measured numbers + reproduction commands.

## ADRs

### D1 — FTS leg: GIN + `ts_rank_cd` over a generated stored `tsvector`, language explicit

**Decision:** The FTS leg uses a `GENERATED ALWAYS AS (to_tsvector('english', content)) STORED` column with a
**GIN** index, ranked by `ts_rank_cd`. Language is set explicitly in the generated-column DDL (never the
cluster default).

**Rationale:** GIN is built-in, zero-dependency, 100% permissive, and is exactly the index the canonical
pgvector hybrid example assumes (`pgvector/README.md:628-629`;
`supabase-postgres/.../docs-full-text-search.sql:102-105`). Keeps M7-S1 free of new hard dependencies.

**Alternatives considered:** RUM (`rum_tsvector_ops` + `<=>`) — rejected as default: external extension,
"excluded from oriole-17" portability caveat (`supabase-postgres/.../z_15_rum.sql:1-3`); kept as a documented
opt-in escape hatch. BM25 via `pg_search` — rejected: AGPL-3.0, barred by PRD D1 (`paradedb/LICENSE:1`);
deferred to M7-S2.

**Consequences:** GIN stores no rank payload, so `ts_rank_cd` is a post-match compute — acceptable because
each leg is `LIMIT`-bounded before fusion. RUM remains available behind a documented flag.

### D2 — RRF contract: k=60 default exposed as a parameter; one fusion source of truth in SQL

**Decision:** Implement RRF as the manual CTE (`RANK() OVER` per leg → `FULL OUTER JOIN` → summed
`COALESCE(1.0/(k+rank), 0)`), encapsulated in a single SQL function `ai.hybrid_search_rrf(...)` with `k`
defaulting to 60 and overridable per call. The harness's Python RRF (for offline scoring) uses the identical
formula so the two never diverge.

**Rationale:** k=60 is the empirically-justified Cormack 2009 default, cross-validated by the paradedb field
witness (`paradedb/tests/tests/hybrid.rs:112-116`) and the TheoDB spec (`docs/features/06-busca-hibrida.md`
§37, §41). Encapsulating the CTE in one function = one source of truth (parsimony ladder rungs 5-6 — the
minimal thing that works); exposing `k` honors the paper's caveat that the optimum is corpus-specific.

**Alternatives considered:** Hardcode k=60 with no param — rejected: forecloses per-corpus tuning the paper
says matters. Ship the elaborate native `ai.hybrid_search()` JSON-array API first — rejected (over-engineering
per parsimony ladder; the JSON surface is a later thin wrapper over this same function). Weighted RRF from day
one — deferred: unweighted is the MVP contract; weighting is a follow-up.

**Consequences:** Empty-leg handled inside the contract (`FULL OUTER JOIN` + `COALESCE`). A future
`ai.hybrid_search()` JSON wrapper calls this function — no second fusion engine.

### D3 — Recall measured BEIR-style; borrow RRF technique, never AGPL code

**Decision:** Extend the M2 harness with a BEIR-style labelled-dataset path (corpus/queries/qrels), nDCG@10 +
Recall@100, and a 3-retriever driver. The RRF math is the public Cormack 2009 technique; paradedb is read as a
field witness only — no AGPL code/schema/test is copied.

**Rationale:** The blueprint's E4 gap is explicit: the M2 harness is pure-vector with no qrels/FTS leg
(`benchmarks/theodb_bench/recall.py:1-4`), so no recall win may be claimed until the harness is extended
(CLAUDE.md TheoDB rule 5; `public-copy.md` §4-§5). BEIR (nDCG@10 + Recall@100) is the field-standard
methodology (`https://arxiv.org/abs/2104.08663`). Algorithms are not the licensed artifact — the RRF formula
is independently citable from the primary paper (`paradedb/LICENSE:1` is AGPL → code barred, math is not).

**Alternatives considered:** Reuse the pure-vector recall@k as-is — rejected: it has no keyword leg, cannot
score a hybrid retriever. Claim the win from the literature without measuring on TheoDB — rejected: violates
rule 5 / `public-copy.md`. Vendor `pg_search` for a BM25 leg — rejected: AGPL (M7-S2 scope).

### D4 — Embeddings via the existing configurable endpoint (OpenAI-compatible), deterministic test fixtures

**Decision:** The eval's document/query embeddings are produced via the same configurable OpenAI-compatible
endpoint used by M2 (`THEODB_EMBEDDING_ENDPOINT`/`THEODB_EMBEDDING_MODEL`); contract unit tests use small
deterministic synthetic vectors (no network), and the integration eval uses the real endpoint over a small
capped corpus.

**Rationale:** Reuses the M2 embedding path (no new dependency — parsimony ladder rung 4); keeps unit tests
deterministic and offline (no network in unit layer per `rules/testing.md` §6), while the integration layer
exercises the real external boundary.

**Alternatives considered:** Ship a model in the image — rejected (D1/M2 decision: TheoDB ships no model).
Embed in unit tests via live calls — rejected: non-deterministic, violates testing §6.

**Consequences:** The integration eval requires `THEODB_EMBEDDING_*` configured (or it skips with a clear
message — fail-fast, no silent pass). Unit tests never touch the network.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| GIN does not serve ranking; `ts_rank_cd` recomputes per match → FTS leg latency grows with match-set size | Medium | Each leg is `LIMIT`-bounded before fusion; RUM escape hatch documented for a measured ranking-latency need (D1) | DB |
| BEIR full datasets are large; running the full eval in CI is slow/expensive (embedding cost) | Medium | CI runs a **capped** corpus (small N, few queries); the decision-grade full eval runs out-of-CI and is committed under `docs/benchmarks/` | Bench |
| RRF default k=60 may be suboptimal for a given corpus → recall lower than achievable | Low | `k` exposed as a parameter (D2); the report documents the chosen k + that tuning is corpus-specific | Bench |
| The "recall win" may not materialize on a given dataset (hybrid not always > pure-vector) | Medium | Goal metric is **measured numbers reported**, not a hardcoded superiority claim; the report states results honestly (rule 3 / `public-copy.md`) | Bench |
| Embedding endpoint dependency (network) makes the integration eval flaky/cost-bearing | Low | Unit layer is offline+deterministic (D4); integration eval is capped + skips cleanly when the endpoint is unconfigured (fail-fast, no silent green) | Bench |

## Unresolved Questions

- Q1 — Which BEIR-style dataset is the canonical CI fixture (a tiny bundled synthetic corpus vs a small slice of a real BEIR set like `scifact`)? Resolved at implementation: a tiny **bundled synthetic labelled corpus** for CI determinism + an optional real-BEIR slice for the out-of-CI report.
- Q2 — Should `ai.hybrid_search_rrf` accept a precomputed query vector, a query text (embedding it internally via `theodb.embed`), or both? Resolved: accept **both** (text → internal `theodb.embed`; or a precomputed `vector`), defaulting to text for the spec's ergonomics, vector for determinism in tests.
- Q3 — Is per-leg `LIMIT` a function parameter or fixed? Resolved at plan time: a parameter (`per_leg_limit`, default 20, matching the paradedb witness `hybrid.rs:99-104`).

## Dependencies

M7-S1 adds **no new runtime dependency** (Unbreakable Rule 9 — compose over what exists). Every piece is
already shipped or built-in:

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| PostgreSQL FTS (`to_tsvector`/`@@`/`ts_rank_cd`/GIN) | 17.10 (engine) | FTS leg + index | PostgreSQL License (built-in) | shipped (M0/M1); no new dep |
| `pgvector` (`vector`, `<=>`, HNSW) | 0.8.3 | vector leg | PostgreSQL License | shipped in M2; no version change |
| RRF | n/a (plain SQL) | fusion | n/a (algorithm, Cormack 2009) | no extension — `1/(k+rank)` in SQL |
| `psycopg` (benchmark harness, dev-only) | as in `benchmarks/requirements.txt` | DB client for the eval | LGPL (dev tooling, not in distribution) | already a dev dep; no change |
| `numpy` (harness metrics, dev-only) | as in `benchmarks/requirements.txt` | nDCG/recall math | BSD | already a dev dep; no change |

**AGPL note (D1/D3):** the BM25 path (`pg_search`, AGPL-3.0) is explicitly **NOT** a dependency — barred from
the distribution and deferred to M7-S2. No CVE audit delta: zero new declared dependencies.

## Dependency Graph

```
Phase 1 (ai.hybrid_search_rrf SQL fn) ──▶ Phase 3 (smoke + docs + CI)
                                  │              ▲
Phase 2 (recall harness extension)┴──────────────┘
   │  (Phase 1 and Phase 2 are independent — can run in parallel;
   │   Phase 3 depends on BOTH)
```

Phase 1 and Phase 2 are independent (SQL fn vs Python harness) and may proceed in parallel. Phase 3
(integration smoke + benchmark report + CI) depends on both. Final Phase (Integration Validation) is last.

---

## Phase 1: `ai.hybrid_search_rrf` SQL function

**Objective:** Ship a reusable, parametrized RRF hybrid-search SQL function baked into the image, with empty-leg handling proven by a contract test.

### T1.1 — Create `ai.hybrid_search_rrf` SQL function + bake into image

#### Objective
Add an idempotent SQL file defining the `ai` schema and `ai.hybrid_search_rrf(...)` function implementing the RRF CTE over a documents-shaped table, and copy it into the image via initdb.d.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — creates `sql/40-theodb-hybrid.sql` defining `CREATE SCHEMA IF NOT EXISTS ai` and a `plpgsql` function `ai.hybrid_search_rrf` that runs an FTS leg + a vector leg, fuses them via `FULL OUTER JOIN` + summed `COALESCE(1.0/(k+rank),0)`, and returns `(id, score)` ordered by score; adds the `COPY sql/40-...` line to the `Dockerfile`.

2. **Why it is necessary now** — it is the core capability of M7-S1 and the one fusion source of truth (ADR D2). It must exist before the smoke (Phase 3) can exercise a hybrid query. The RRF shape is the proven paradedb witness over `pgvector` (`paradedb/tests/tests/hybrid.rs:99-118`), authority Cormack 2009.

#### Evidence
- RRF SQL shape: `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs:99-118` (`RANK() OVER`, `FULL OUTER JOIN`, `1.0/(60+rank)`, `COALESCE`).
- Generated stored `tsvector` + GIN: `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql:102-105`.
- initdb.d bake pattern: `sql/30-theodb-embed.sql:1-15` + `Dockerfile:64` (`COPY sql/30-theodb-embed.sql /docker-entrypoint-initdb.d/...`).
- Target contract: `docs/features/06-busca-hibrida.md` §37, §41 (manual RRF CTE).

#### Files to edit
```
sql/40-theodb-hybrid.sql — (NEW) ai schema + ai.hybrid_search_rrf function (idempotent)
Dockerfile — add COPY sql/40-theodb-hybrid.sql /docker-entrypoint-initdb.d/40-theodb-hybrid.sql
benchmarks/tests/test_integration.py — RED hybrid contract tests appended (fused order, empty-leg, k param)
```

#### Deep file dependency analysis
- `sql/40-theodb-hybrid.sql` (NEW): mirrors the idempotent header + `CREATE OR REPLACE FUNCTION` pattern of `sql/30-theodb-embed.sql` (Baseline row). Depends on `vector` (already `CREATE EXTENSION IF NOT EXISTS vector` in 30). No downstream SQL depends on it yet; the smoke (Phase 3) and integration tests will call it.
- `Dockerfile` (Baseline row, invariant: keep the `COPY sql/30-...` line): one additive `COPY` line; ordering after 30 is fine (both idempotent at initdb).
- `benchmarks/tests/test_integration.py` (Baseline row, invariant: existing tests stay green): appends hybrid contract tests using the existing `_dsn()` + `integration` marker pattern (`test_integration.py:1-30`).

#### Deep Dives
- **Function signature:** `ai.hybrid_search_rrf(query_text text, query_vector vector, tbl regclass, id_col text, content_tsv_col text, vector_col text, k int DEFAULT 60, per_leg_limit int DEFAULT 20, result_limit int DEFAULT 5) RETURNS TABLE(id text, score real)`. Uses dynamic SQL (`format()` with `%I` identifier quoting — SQL-injection-safe; never `%s` for identifiers).
- **Invariant (D2):** the fusion is rank-only `Σ 1/(k+rank)`; both legs ordered, `RANK() OVER`, `FULL OUTER JOIN`, `COALESCE(...,0)` for the empty leg.
- **Edge cases:** empty FTS leg (no `@@` match) → vector-only rows still returned with FTS contribution 0; empty vector leg likewise; both empty → zero rows (not an error). `k` must be `> 0` (else `1/(k+rank)` degenerate) → validate `k > 0`, raise typed error `ERRCODE 22023` (matches the `theodb.embed` fail-fast pattern `30-theodb-embed.sql`).
- **Query-text vs query-vector (Q2):** if `query_vector IS NULL` and `query_text` given, embed internally via `theodb.embed(query_text)`; if both NULL → typed error.

#### Pseudo-code / Signatures
```pseudocode
function ai.hybrid_search_rrf(query_text, query_vector, tbl, id_col, content_tsv_col, vector_col,
                              k=60, per_leg_limit=20, result_limit=5) returns table(id, score)
  -- precondition: k > 0 (else raise 22023); at least one of query_text/query_vector non-null
  if query_vector is null and query_text is not null: query_vector := theodb.embed(query_text)
  if query_vector is null and query_text is null: raise 22023 'need query_text or query_vector'
  return EXECUTE format($q$
    WITH vec AS (
      SELECT %I AS id, RANK() OVER (ORDER BY %I <=> $1) AS rank
      FROM %s WHERE %I IS NOT NULL ORDER BY %I <=> $1 LIMIT %s),
    fts AS (
      SELECT %I AS id, RANK() OVER (ORDER BY ts_rank_cd(%I, plainto_tsquery($2)) DESC) AS rank
      FROM %s WHERE %I @@ plainto_tsquery($2) LIMIT %s)
    SELECT COALESCE(vec.id, fts.id)::text AS id,
           (COALESCE(1.0/(k+vec.rank),0) + COALESCE(1.0/(k+fts.rank),0))::real AS score
    FROM vec FULL OUTER JOIN fts ON vec.id = fts.id
    ORDER BY score DESC LIMIT %s$q$, id_col, vector_col, tbl, vector_col, vector_col, per_leg_limit,
                                       id_col, content_tsv_col, tbl, content_tsv_col, per_leg_limit, result_limit)
    USING query_vector, query_text

# Example (doc D1 matches both legs, D2 only FTS, D3 only vector):
# input:  query_text='database', query_vector=<v>, per_leg_limit=20, k=60
# output: rows [(D1, 0.0328…), (D2, 0.0163…), (D3, 0.0163…)]  -- empty legs contribute 0, all surface
```

#### Tasks
1. Write `sql/40-theodb-hybrid.sql` idempotent header + `CREATE SCHEMA IF NOT EXISTS ai` + `CREATE OR REPLACE FUNCTION ai.hybrid_search_rrf(...)` with `%I`-quoted dynamic SQL and `k>0` validation.
2. Add `COPY sql/40-theodb-hybrid.sql /docker-entrypoint-initdb.d/40-theodb-hybrid.sql` to `Dockerfile`.
3. Append RED hybrid contract tests to `benchmarks/tests/test_integration.py`.

#### TDD
```
RED:     test_hybrid_fuses_both_legs() — seeds a documents table (3 rows), runs ai.hybrid_search_rrf with a query matching all legs; asserts top result is the doc matched by BOTH legs (highest fused score). MUST fail before the fn exists.
RED:     test_hybrid_empty_fts_leg() — query_text matches NO row via @@; asserts vector-only docs still returned (FULL OUTER JOIN + COALESCE), no error, FTS contribution 0.
RED:     test_hybrid_empty_vector_leg() — query_vector orthogonal/over a column with NULLs; asserts FTS-only docs still returned.
RED:     test_hybrid_invalid_k_raises() — k=0 raises SQLSTATE 22023 (typed error, fail-fast).
RED:     test_hybrid_k_param_changes_score() — same query, k=1 vs k=60 yields different fused scores (param wired, not hardcoded).
GREEN:   Implement sql/40-theodb-hybrid.sql so all RED tests pass against the container.
REFACTOR: Extract the leg-CTE format string only if it improves clarity; else "None expected".
VERIFY:  cd benchmarks && pytest -m integration tests/test_integration.py -k hybrid -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — read-only query within a single statement; no shared mutable state, no transaction-spanning mutation, no locks/async/atomics.

(The function is a read-only query within a single statement; no shared mutable state, no transaction-spanning mutation.)

#### Acceptance Criteria
- [ ] `ai.hybrid_search_rrf` exists after a fresh container init (loaded from initdb.d) — `psql -c "\df ai.hybrid_search_rrf"` lists it.
- [ ] All 5 RED tests pass: `cd benchmarks && pytest -m integration tests/test_integration.py -k hybrid -q` green.
- [ ] Empty-leg: a doc matched by only one retriever is returned — `pytest -m integration tests/test_integration.py -k empty` exits `0` (asserts membership).
- [ ] `k=0` raises SQLSTATE `22023` (typed, fail-fast — `rules/architecture.md` boundary validation).
- [ ] Pass: lint — `cd benchmarks && ruff check tests` zero warnings on changed test file.
- [ ] Pass: size — `sql/40-theodb-hybrid.sql` ≤ 500 lines.

#### DoD (Definition of Done)
- [ ] All tasks completed and validated
- [ ] All tests passing — `cd benchmarks && pytest -m integration -q` green
- [ ] Zero lint warnings — `cd benchmarks && ruff check tests`
- [ ] File-size budget respected (per `architecture.md`)
- [ ] CHANGELOG `[Unreleased]` updated

---

## Phase 2: Recall harness extension (BEIR-style hybrid eval)

**Objective:** Extend the M2 harness so recall is measured for three retrievers (pure-vector, pure-FTS, RRF-hybrid) with nDCG@10 + Recall@100 — closing the blueprint's E4 gap so the win is measured, not asserted.

### T2.1 — Add nDCG@10 + Recall@100 metrics + RRF fusion (pure functions, offline-tested)

#### Objective
Add graded-relevance metrics and the Python RRF fusion to `metrics.py`/`hybrid.py`, unit-tested offline with deterministic fixtures.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — adds `ndcg_at_k(ranked_ids, qrels, k)` and `recall_at_n(ranked_ids, qrels, n)` to `benchmarks/theodb_bench/metrics.py`, and `rrf_fuse(leg_rankings, k=60)` to a new `benchmarks/theodb_bench/hybrid.py`; covers them with offline unit tests.

2. **Why it is necessary now** — these are pure functions (no I/O) and the load-bearing correctness of the eval; per `rules/testing.md` the math must be unit-tested deterministically before any integration run. The Python `rrf_fuse` MUST use the identical `1/(k+rank)` formula as the SQL fn (ADR D2 — one fusion definition) so offline scoring matches the DB.

#### Evidence
- Existing metric module to extend: `benchmarks/theodb_bench/metrics.py:1-33` (latency/QPS; same file gets nDCG/recall@n).
- RRF formula authority: Cormack 2009 (`https://dl.acm.org/doi/10.1145/1571941.1572114`); witness `.claude/knowledge-base/references/paradedb/tests/tests/hybrid.rs:112-116`.
- BEIR metric definition: Thakur et al. 2021 (`https://arxiv.org/abs/2104.08663`) — nDCG@10 + Recall@100.

#### Files to edit
```
benchmarks/theodb_bench/metrics.py — add ndcg_at_k() and recall_at_n() (pure)
benchmarks/theodb_bench/hybrid.py — (NEW) rrf_fuse() pure function (mirrors SQL fn formula)
benchmarks/tests/test_hybrid.py — (NEW) RED unit tests for ndcg/recall@n/rrf_fuse with deterministic fixtures
```

#### Deep file dependency analysis
- `metrics.py` (Baseline row, invariant: existing signatures unchanged): additive functions only; `latency_percentiles`/`qps_best_of_n` untouched. Callers (`harness.py`, `__main__.py`) unaffected.
- `hybrid.py` (NEW): pure module, imported by the eval driver in T2.2. No downstream yet.
- `test_hybrid.py` (NEW): unit-only (no `integration` marker), runs in the fast CI lane (`pytest -m "not integration"`).

#### Deep Dives
- **`rrf_fuse(leg_rankings: list[list[id]], k=60) -> list[(id, score)]`:** each leg is a ranked id list; rank is 1-based position; score = Σ `1/(k+rank)`; ids absent from a leg contribute 0 (mirrors `COALESCE`). Returns descending by score. Identical formula to the SQL fn (D2).
- **`ndcg_at_k(ranked_ids, qrels: dict[id,int], k) -> float`:** DCG = Σ_{i<k} rel_i/log2(i+2); IDCG from the ideal ordering of qrels; nDCG = DCG/IDCG; IDCG==0 → 0.0 (no relevant docs).
- **`recall_at_n(ranked_ids, qrels, n) -> float`:** |{relevant in top-n}| / |{relevant total}|; no relevant → 0.0.
- **Edge cases:** empty ranking → 0.0; k larger than list → use available; tie handling deterministic (stable sort by score then id).

#### Pseudo-code / Signatures
```pseudocode
function rrf_fuse(leg_rankings, k=60):
  scores = defaultdict(float)
  for leg in leg_rankings:
    for rank, id in enumerate(leg, start=1): scores[id] += 1.0/(k+rank)
  return sorted(scores.items(), key=lambda kv: (-kv[1], kv[0]))

# Example: legs=[[A,B,C],[B,D]], k=60
# A: 1/61; B: 1/61+1/61; C: 1/63; D: 1/62  -> order: B, A, D, C
```

#### Tasks
1. Add `ndcg_at_k` + `recall_at_n` to `metrics.py`.
2. Create `hybrid.py` with `rrf_fuse`.
3. Write RED unit tests in `test_hybrid.py` with hand-computed expected values.

#### TDD
```
RED:     test_rrf_fuse_matches_handcalc() — legs=[[A,B,C],[B,D]], k=60 → order [B,A,D,C] with exact scores.
RED:     test_rrf_fuse_empty_leg() — one empty leg → other leg's order preserved, no crash.
RED:     test_ndcg_at_k_perfect_is_1() — ranking == ideal qrels order → nDCG@10 == 1.0.
RED:     test_ndcg_at_k_no_relevant_is_0() — qrels empty → 0.0 (no div-by-zero).
RED:     test_recall_at_n_counts_relevant() — 2 of 3 relevant in top-n → 2/3.
GREEN:   Implement metrics + rrf_fuse minimally to pass.
REFACTOR: dedupe any shared sort helper; else "None expected".
VERIFY:  cd benchmarks && pytest -m "not integration" tests/test_hybrid.py -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — read-only query within a single statement; no shared mutable state, no transaction-spanning mutation, no locks/async/atomics.


#### Acceptance Criteria
- [ ] `cd benchmarks && pytest -m "not integration" tests/test_hybrid.py -q` green (5 tests).
- [ ] `rrf_fuse` formula is byte-identical to the SQL fn (`1/(k+rank)`, COALESCE-equivalent) — asserted by a test comparing against the documented expected scores.
- [ ] Pass: lint — `cd benchmarks && ruff check theodb_bench tests` zero warnings.
- [ ] Pass: dead-code — `cd benchmarks && vulture theodb_bench --min-confidence 80` clean.
- [ ] Pass: size — `metrics.py`, `hybrid.py` ≤ 500 lines.

#### DoD
- [ ] All tasks completed and validated
- [ ] Unit tests passing — `cd benchmarks && pytest -m "not integration" -q` green
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench tests`
- [ ] File-size budget respected
- [ ] CHANGELOG `[Unreleased]` updated

### T2.2 — BEIR-style loader + 3-retriever eval driver (integration)

#### Objective
Add a BEIR-style dataset loader (corpus/queries/qrels) and a driver that runs pure-vector, pure-FTS, and RRF-hybrid retrievers against the real container and reports nDCG@10 + Recall@100 per retriever.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — creates `benchmarks/theodb_bench/beir.py` (loads a labelled corpus/queries/qrels — a tiny bundled synthetic set for CI, optional real-BEIR slice for the report) and adds a driver that, for each retriever, queries the DB (vector via `<=>`, FTS via `@@`+`ts_rank_cd`, hybrid via `ai.hybrid_search_rrf`), then scores with the T2.1 metrics.

2. **Why it is necessary now** — this is the measurement that turns the blueprint's UNBENCHMARKED gap (E4) into real numbers; without it, the recall win cannot be claimed (rule 5). It depends on T2.1 metrics and on Phase 1's SQL fn for the hybrid retriever.

#### Evidence
- Integration test + `VectorDB` adapter pattern: `benchmarks/tests/test_integration.py:1-30`, `benchmarks/theodb_bench/db.py` (Baseline rows).
- FTS DDL (generated tsvector + GIN): `.claude/knowledge-base/references/supabase-postgres/nix/tests/sql/docs-full-text-search.sql:102-105`.
- BEIR methodology: `https://arxiv.org/abs/2104.08663`.
- Embedding endpoint reuse (D4): `sql/30-theodb-embed.sql:1-15`; `.env` `THEODB_EMBEDDING_*`.

#### Files to edit
```
benchmarks/theodb_bench/beir.py — (NEW) labelled-dataset loader (corpus/queries/qrels) + tiny bundled synthetic fixture
benchmarks/theodb_bench/db.py — add FTS helpers (create generated tsvector+GIN, fts query) — additive, no signature change to existing methods
benchmarks/theodb_bench/hybrid.py — add run_three_retrievers(db, dataset) driver (uses ai.hybrid_search_rrf for the hybrid leg)
benchmarks/tests/test_integration.py — RED integration test: 3 retrievers run, all report finite nDCG@10 + Recall@100
```

#### Deep file dependency analysis
- `beir.py` (NEW): pure loader; the bundled synthetic fixture is deterministic (no network). Imported by the driver + tests.
- `db.py` (Baseline row, invariant: existing `VectorDB` methods backward-compatible): adds `create_fts_index()` + `fts_query()` methods; existing connect/ping/vector methods untouched (callers `harness.py`/`__main__.py` unaffected).
- `hybrid.py`: gains the driver (depends on T2.1 `rrf_fuse` + Phase 1 SQL fn for the DB hybrid path).
- `test_integration.py` (invariant: existing tests green): appends one integration test using a tiny synthetic labelled corpus.

#### Deep Dives
- **Dataset shape:** `Dataset(corpus: dict[id,text], queries: dict[qid,text], qrels: dict[qid,dict[id,int]])`. CI fixture: ~12 docs, ~4 queries, hand-labelled qrels → deterministic, fast, no network for the loader itself.
- **Retrievers:** pure-vector (`ORDER BY <=> LIMIT 100`); pure-FTS (`@@` + `ts_rank_cd LIMIT 100`); hybrid (`ai.hybrid_search_rrf(..., result_limit=100)`). Each returns ranked ids → scored by T2.1.
- **Embeddings (D4):** doc/query vectors via `theodb.embed` (real endpoint) for the integration path; the unit/CI synthetic path may use fixed pseudo-embeddings to stay offline+deterministic where embedding the corpus is not the thing under test.
- **Failure / skip:** if `THEODB_EMBEDDING_*` unset for the real-endpoint path, skip with a clear reason (no silent green); the synthetic-vector path always runs.

#### Pseudo-code / Signatures
```pseudocode
function run_three_retrievers(db, dataset, k_rrf=60, top=100) -> dict[name, {ndcg10, recall100}]:
  out = {}
  for name, retrieve in [("vector", db.vector_query), ("fts", db.fts_query),
                         ("hybrid", lambda q: db.hybrid_rrf(q, k_rrf))]:
    per_query = []
    for qid, qtext in dataset.queries.items():
      ranked = retrieve(qtext, top)             # ranked doc ids
      per_query.append((ndcg_at_k(ranked, dataset.qrels[qid], 10),
                        recall_at_n(ranked, dataset.qrels[qid], 100)))
    out[name] = {"ndcg10": mean(n for n,_ in per_query), "recall100": mean(r for _,r in per_query)}
  return out
```

#### Tasks
1. Create `beir.py` loader + bundled synthetic labelled fixture.
2. Add `create_fts_index()` + `fts_query()` to `db.py`; add `hybrid_rrf()` calling the SQL fn.
3. Add `run_three_retrievers` to `hybrid.py`.
4. Write RED integration test asserting all three retrievers report finite nDCG@10 + Recall@100.

#### TDD
```
RED:     test_three_retrievers_report_metrics() [integration] — seed the synthetic labelled corpus, run the driver; assert each of vector/fts/hybrid returns finite nDCG@10 in [0,1] and Recall@100 in [0,1]. MUST fail before the driver/loader exist.
RED:     test_beir_loader_roundtrip() [unit] — loads the bundled fixture; assert corpus/queries/qrels counts + a known qrel label.
GREEN:   Implement beir.py + db.py FTS helpers + driver to pass.
REFACTOR: fold duplicated query SQL into db.py helpers; else "None expected".
VERIFY:  cd benchmarks && pytest tests/test_hybrid.py -q && pytest -m integration tests/test_integration.py -k retrievers -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — read-only query within a single statement; no shared mutable state, no transaction-spanning mutation, no locks/async/atomics.


#### Acceptance Criteria
- [ ] `test_beir_loader_roundtrip` passes (unit): `cd benchmarks && pytest -m "not integration" tests/test_hybrid.py -k loader -q`.
- [ ] `test_three_retrievers_report_metrics` passes (integration) against the container: `cd benchmarks && pytest -m integration tests/test_integration.py -k retrievers -q`.
- [ ] The driver reports a finite nDCG@10 + Recall@100 for all three retrievers — `pytest -m integration -k retrievers` prints values in `[0,1]` and exits `0` (numbers observed, not fabricated).
- [ ] Endpoint-unconfigured path: `ai.hybrid_search_rrf(query_text=>…)` with no query_vector and `theodb.embedding_endpoint` unset raises a typed error (SQLSTATE `22023`, no silent green) — `pytest -m integration -k endpoint` exits `0`.
- [ ] Pass: lint — `cd benchmarks && ruff check theodb_bench tests`; dead-code `vulture theodb_bench --min-confidence 80` clean.
- [ ] Pass: size — `wc -l` on every changed/new file returns `< 500`.

#### DoD
- [ ] All tasks completed and validated
- [ ] Unit + integration tests passing — `cd benchmarks && pytest -q` (with container up) green
- [ ] Zero lint warnings
- [ ] File-size budget respected
- [ ] CHANGELOG `[Unreleased]` updated

---

## Phase 3: Smoke + docs + CI + measured benchmark report

**Objective:** Exercise hybrid search end-to-end in the smoke, record the measured recall numbers, and gate it in CI.

### T3.1 — Hybrid smoke + CI job + measured benchmark report

#### Objective
Extend `smoke.sh` with a hybrid query + golden top-k assertion, add a `hybrid-search` CI job (contract + capped eval), and write `docs/benchmarks/m7-hybrid-recall.md` with the measured numbers + reproduction.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — adds a hybrid block to `smoke.sh` that seeds a tiny table and asserts the top result via `ai.hybrid_search_rrf`; adds a CI job building the image + running the hybrid contract tests + a capped 3-retriever eval; writes the benchmark report with the real measured nDCG@10/Recall@100.

2. **Why it is necessary now** — it is the wiring triad's caller + integration + observable evidence: the smoke is the runtime caller, CI is the integration gate, and the report is the measured evidence that satisfies the M7 DoD ("recall medido"). Per `public-copy.md` the report only states measured numbers.

#### Evidence
- Smoke pattern: `smoke.sh:1-21` (`pg_isready` loop + `psql -v ON_ERROR_STOP=1`).
- CI job pattern: `.github/workflows/ci.yml` (existing `image-and-bench`/`migration-smoke`/`ha-smoke` jobs).
- Report convention: `docs/benchmarks/` (M2 committed decision-grade sweeps there).
- Public-copy rule: `rules/public-copy.md` §4-§5 (no unbenchmarked claims).

#### Files to edit
```
smoke.sh — append a hybrid block: seed tiny table + ai.hybrid_search_rrf golden top-k assertion
.github/workflows/ci.yml — add a hybrid-search job (build image + pytest hybrid contract + capped eval) with timeout-minutes
docs/benchmarks/m7-hybrid-recall.md — (NEW) measured nDCG@10/Recall@100 per retriever + reproduction commands
docs/features/06-busca-hibrida.md — add a short "implemented surface" note linking ai.hybrid_search_rrf (the manual-SQL MVP)
CHANGELOG.md — [Unreleased] M7-S1 entry
```

#### Deep file dependency analysis
- `smoke.sh` (Baseline row, invariant: existing vector smoke green): additive hybrid block after the vector check; same `psql` harness.
- `.github/workflows/ci.yml` (invariant: existing jobs stay): additive `hybrid-search` job; reuses the buildx cache from `image-and-bench` (same pattern as `migration-smoke`/`ha-smoke`); `timeout-minutes` per the M1 review LOW fix convention.
- `docs/benchmarks/m7-hybrid-recall.md` (NEW): records the numbers produced by T2.2's driver.
- `docs/features/06-busca-hibrida.md` (invariant: stays the spec): one additive note pointing at the shipped function (no rewrite).

#### Deep Dives
- **Smoke golden assertion:** seed 3 docs where one matches both legs; assert the top `id` from `ai.hybrid_search_rrf` equals the both-legs doc; `exit 1` on mismatch (fail-loud, like the existing smoke).
- **CI capped eval:** run the synthetic labelled fixture (no external embedding cost) so CI is deterministic + fast; the full real-BEIR slice is run out-of-CI and committed in the report.
- **Report honesty:** if hybrid does not beat pure-vector on the chosen dataset, the report says so plainly (rule 3); the DoD is *measured numbers reported*, not a hardcoded superiority.

#### Tasks
1. Append the hybrid block + golden assertion to `smoke.sh`.
2. Add the `hybrid-search` CI job (build + contract tests + capped eval) with `timeout-minutes`.
3. Run the eval (synthetic for CI + real slice out-of-CI) and write `docs/benchmarks/m7-hybrid-recall.md` with the measured numbers + reproduction.
4. Add the implemented-surface note to `docs/features/06-busca-hibrida.md`; add the CHANGELOG entry.

#### TDD
```
RED:     smoke hybrid assertion fails before sql/40 is loaded (run smoke against an image WITHOUT 40 → top-k mismatch / fn missing).
GREEN:   With sql/40 baked, `bash smoke.sh` prints SMOKE PASSED including the hybrid line.
REFACTOR: factor the seed SQL into a heredoc; else "None expected".
VERIFY:  docker build -t theo-db:dev . && PGPORT=5432 bash smoke.sh   (hybrid line asserts top-k)
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — read-only query within a single statement; no shared mutable state, no transaction-spanning mutation, no locks/async/atomics.


#### Acceptance Criteria
- [ ] `bash smoke.sh` against a freshly-built `theo-db:dev` prints the hybrid assertion line + `SMOKE PASSED`; mismatch exits non-zero.
- [ ] CI `hybrid-search` job is valid YAML and runs the contract tests + capped eval — `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` parses; job has `timeout-minutes`.
- [ ] `docs/benchmarks/m7-hybrid-recall.md` exists with measured nDCG@10 + Recall@100 for the three retrievers + the exact reproduction command (no UNBENCHMARKED in the final report).
- [ ] `public-copy` lint clean on the report — `bash hooks/public-copy-lint.sh docs/benchmarks/m7-hybrid-recall.md` (if present) no banned framings.
- [ ] Pass: size — `wc -l` on every changed file returns `< 500`.

#### DoD
- [ ] All tasks completed and validated
- [ ] Smoke green; CI job parses and runs locally-validated steps
- [ ] Benchmark report committed with measured numbers
- [ ] CHANGELOG `[Unreleased]` updated
- [ ] File-size budget respected

---

## Coverage Matrix

| # | Gap / Requirement (from ROADMAP M7 DoD-2 + blueprint) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Hybrid search (FTS + vector) capability on permissive OSS | T1.1 | `ai.hybrid_search_rrf` SQL fn (GIN+`ts_rank_cd` + `pgvector` + RRF) baked into image |
| 2 | RRF reranking with k=60, exposed as a parameter | T1.1, T2.1 | SQL fn k-param + Python `rrf_fuse` (identical formula, D2) |
| 3 | Empty-leg handling (FULL OUTER JOIN + COALESCE) — E6 | T1.1 | contract tests `test_hybrid_empty_*` |
| 4 | FTS index choice (GIN default, RUM escape hatch) — E2/D1 | T1.1, T3.1 | GIN generated-tsvector default; RUM documented in report/spec note |
| 5 | Recall measured (BEIR — nDCG@10/Recall@100) — closes E4 | T2.1, T2.2, T3.1 | metrics + 3-retriever driver + measured report |
| 6 | No AGPL dependency (pg_search barred) — E3/D3 | T1.1, (M7-S2 deferred) | PG-native FTS only; paradedb technique-witness only |
| 7 | End-to-end runtime evidence (smoke + CI) | T3.1 | hybrid smoke golden assertion + `hybrid-search` CI job |
| 8 | Honest perf claim (no unbenchmarked) — rule 5 | T2.2, T3.1 | report states only measured numbers |

**Coverage: 8/8 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `cd benchmarks && pytest -q` (container up) green
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench tests`
- [ ] Dead-code clean — `cd benchmarks && vulture theodb_bench --min-confidence 80`
- [ ] File-size budget respected (per `rules/architecture.md`)
- [ ] CHANGELOG.md updated under `[Unreleased]` (Unbreakable Rule 6)
- [ ] Backward compatibility preserved — `theodb.embed`, `VectorDB`, `recall_at_k` unchanged
- [ ] Plan-specific: `ai.hybrid_search_rrf` loads from initdb.d; 3-retriever eval reports measured numbers; smoke hybrid assertion green
- [ ] Runtime-metric proof — the 3-retriever eval is observed reporting finite nDCG@10/Recall@100 in the integration run (not just compiling)
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge

## Failure scenarios (external I/O)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| PostgreSQL (`psycopg`, the container) | container not ready / connection refused | run integration test before `pg_isready` passes | harness `VectorDB.connect/ping` raises a clear error; smoke `pg_isready` loop waits then fails loud (no silent pass) |
| PostgreSQL (planner) | FTS index not used / no `@@` match | query with a term absent from the corpus | empty FTS leg → `FULL OUTER JOIN`+`COALESCE` returns vector-only rows, no crash (`test_hybrid_empty_fts_leg`) |
| Embedding endpoint (`THEODB_EMBEDDING_*`, OpenAI-compatible, HTTP) | endpoint unset OR call fails (timeout/5xx) | unset the GUC/env in the integration eval | the SQL `ai.hybrid_search_rrf` vector leg calls `theodb.embed`, which **raises a typed error** (SQLSTATE `22023`/`38000`, no silent green); the Python real-endpoint full eval is deferred out-of-CI (report `docs/benchmarks/m7-hybrid-recall.md`), the CI eval uses the deterministic offline embedder |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate the implemented hybrid search + eval in a real workload.

### Execution
```
docker build -t theo-db:dev .                                   # image with sql/40 baked
docker run -d --name m7-it -e POSTGRES_PASSWORD=postgres -p 5432:5432 theo-db:dev
PGPORT=5432 bash smoke.sh                                       # vector + hybrid golden assertion
cd benchmarks && pip install -r requirements.txt
pytest -m "not integration" -q                                 # unit (metrics, rrf_fuse, loader)
PGHOST=localhost PGPORT=5432 PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration -q                                      # hybrid contract + 3-retriever eval
ruff check theodb_bench tests && vulture theodb_bench --min-confidence 80
```

### Acceptance Criteria
- [ ] All test suites green — `cd benchmarks && pytest -q` exits `0` (unit + integration)
- [ ] Coverage ≥ 90% on changed files (critical paths: the RRF fusion + nDCG/recall math at 100%)
- [ ] Zero lint warnings — `ruff check theodb_bench tests` exits `0`; dead-code clean — `vulture theodb_bench --min-confidence 80` exits `0`
- [ ] Runtime-metric proof — `pytest -m integration -k retrievers` shows the 3-retriever eval reporting finite nDCG@10/Recall@100 (asserted non-NaN in `[0,1]`)
- [ ] Failure scenarios green — `pytest -m integration -k 'empty or endpoint'` exits `0` (empty-leg + endpoint-unconfigured typed-error behaviors observed)
