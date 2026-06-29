# Blueprint: Unified vector+relational+AI — filtered search, the unified query, Pinecone migration

> **Version 1.0** — Synthesizes how to deliver TheoDB's unification moat (ADR 0005): **correct filtered
> vector search** (pgvector post-filter + `hnsw.iterative_scan`; pgvectorscale label-filtering), the
> **canonical unified query** (vector over a relational table + `ai.*`, one transaction — extending our
> `ai.hybrid_search_rrf` pattern), and **Pinecone migration** (the `Vector{id,values,metadata}` data model →
> a TheoDB table). Honesty (ADR 0005): the goal is **correctness (recall preserved under filter)** +
> demonstrable unification + a migration path — NOT a speed number.

**Slug:** `unified-vector-relational`
**Source plan:** `.claude/knowledge-base/discoveries/plans/unified-vector-relational-plan.md`
**Owner:** TheoDB maintainers
**Generated:** 2026-06-29 via `/discover-execute` (executed inline, citations verified on disk)
**Confidence verdict:** SHIPPABLE_WITH_CAVEATS (89.0 — 0 fabricated citations, 4/4 corners; sole soft cap `soft_floor_citation_density_low`, heuristic, accepted — prose-dense + repo/README citations)

## Context

ADR `0005-unification-as-differentiator` (LOCKED) makes unification the moat. M16 must make it real. The
load-bearing correctness risk is **filtered vector search**: an approximate index post-filtered can return
*fewer than k* rows (over-filtering). This blueprint establishes the correct pattern + how to prove it, the
unified-query skeleton, and the Pinecone→TheoDB mapping — reusing pgvector/pgvectorscale (Unbreakable Rule 9),
honoring `.claude/rules/public-copy.md` (no perf claim) and `.claude/rules/testing.md` (over-filtering is the edge).

## Objective

Let M16 implement, with evidence: a correct filtered vector search, the canonical unified query, and a
Pinecone import.

---

## Coverage Corner 1 — Integration Tests

### pgvector — how filtered / ANN correctness is tested
- **Pattern:** `pg_regress` SQL that builds an HNSW index then runs `ORDER BY val <=> '[...]'` and `COUNT(*)`
  over the ordered set — asserting the result set under the index
  (`.claude/knowledge-base/references/pgvector/test/sql/hnsw_vector.sql:7,41-43`: `CREATE INDEX … USING hnsw
  (val vector_cosine_ops)` then `SELECT … ORDER BY val <=> '[3,3,3]'` + `COUNT(*)` checks, incl. the NULL
  edge `<=> (SELECT NULL::vector)`).
- **Transfer to M16:** our e2e test mirrors this against a real container, **plus** the over-filtering edge
  (a selective `WHERE` returning < k rows under default `ef_search`, then asserting `hnsw.iterative_scan`
  restores the full k — recall preserved). This is the NEGATIVE/edge case (`testing.md §4.1`).

---

## Coverage Corner 2 — Dependencies

### Pinecone import — minimal deps (stdlib only)
- The Pinecone `Vector` is a plain dataclass/struct with `from_dict(dict)` deserialization
  (`.claude/knowledge-base/references/pinecone-python-client/pinecone/models/vectors/vector.py:13,25-28,36`).
  Its on-the-wire shape is JSON (`id`, `values`, `metadata`). → A TheoDB importer needs only **stdlib `json`**
  to parse an exported `{id, values, metadata}` record — **no new dependency** (Rule 9 / parsimony ladder).
- `db_data/dataclasses/vector.py` is a re-export shim of `pinecone/models/vectors/vector.py:Vector`
  (`.../db_data/dataclasses/vector.py:12`) — confirms the canonical model location.

| Dep for import | Needed? | Citation |
|---|---|---|
| `json` (stdlib) | yes (parse export record) | model is JSON-serializable (`models/vectors/vector.py:36` `from_dict`) |
| `pinecone` client | **no** | we read an export file, not call the API — avoids a runtime dep |
| any new package | **no** | parsimony — stdlib covers it |

---

## Coverage Corner 3 — Tools

### Proving the index is actually used under a filter (honest demo)
- `EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM items ORDER BY embedding <-> '[3,1,2]' LIMIT 5;`
  (`.claude/knowledge-base/references/pgvector/README.md:702-705`) — the recipe to assert an **Index Scan**
  (not Seq Scan) under a filter, used in M16 tests.
- Knobs: `SET hnsw.iterative_scan = strict_order;` (`README.md:453,477`), `relaxed_order` (`:483`),
  `SET hnsw.max_scan_tuples = 20000;` (`:517`), `SET hnsw.ef_search = 100;` (`:275`), and
  `SET LOCAL enable_seqscan = off;` (`:898`) to force index use in a deterministic test.

---

## Coverage Corner 4 — Techniques

### T1 — Filtered vector search: pgvector vs pgvectorscale (Q1)

**The over-filtering failure mode (pgvector README, verbatim):** *"With approximate indexes, filtering is
applied **after** the index is scanned. If a condition matches 10% of rows, with HNSW and the default
`hnsw.ef_search` of 40, only 4 rows will match on average."*
(`.claude/knowledge-base/references/pgvector/README.md:450`). This is exactly why a pure post-filter returns
< k rows.

**pgvector's three fixes (README § Filtering, :432-470):**
| Approach | When | DDL | Citation |
|---|---|---|---|
| B-tree on the filter column | low-selectivity filter (few matching rows) → exact NN | `CREATE INDEX ON items (category_id)` | `README.md:432-436` |
| **Iterative index scan** | high-selectivity filter → scan more of the index until k found | `SET hnsw.iterative_scan = strict_order` (or `relaxed_order`); bounded by `hnsw.max_scan_tuples` | `README.md:450-453`, `:483`, `:517` |
| Partial index | few distinct filter values | `CREATE INDEX … USING hnsw (embedding …) WHERE (category_id = 123)` | `README.md:458-461` |

**pgvectorscale's alternative — in-index label-filtering:** a `smallint[]` label column with the `&&`
(overlap) operator and the `vector_smallint_label_ops` opclass, so the filter is applied **inside** the
DiskANN scan (not post-hoc) (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/sql/vectorscale--0.8.0--0.9.0.sql:60-67,111-115,147-175`).

**Recommendation:** default to **pgvector `hnsw.iterative_scan = strict_order`** (no schema change, exact
order, works with any relational `WHERE`) — the general unification answer. Offer **pgvectorscale
label-filtering** as the optimization for categorical low-cardinality filters (labels as `smallint[]`).
Partial index for a handful of fixed values. The over-filtering edge MUST be in the test (recall preserved).

### T2 — The canonical unified query (Q2)

Our `ai.hybrid_search_rrf(tbl regclass, id_col text, …, vector_col text, …)` already runs a **vector leg over
a relational table** via safe dynamic SQL (`.claude/knowledge-base/references/` n/a — repo `sql/40-theodb-hybrid.sql:16-31`),
proving the vector-over-relational pattern. The unified query generalizes it to one transactional SQL:

```sql
-- canonical unified query: vector search + relational JOIN + filter + AI, one transaction
SELECT p.id, p.name, p.price,
       ai.summarize(p.description) AS gist           -- AI leg (same instance)
FROM products p
JOIN inventory i ON i.product_id = p.id              -- relational JOIN (operational data)
WHERE i.in_stock AND p.category_id = 3               -- relational filter
ORDER BY p.embedding <=> $1                           -- vector leg (pgvector)
LIMIT 5;                                              -- with SET hnsw.iterative_scan = strict_order
```

No ETL, no second system, transactionally consistent (the vector and the business row are the same row). This
is the diff vs Pinecone (no relational JOIN) and the OSS diff vs AlloyDB.

### T3 — Pinecone → TheoDB mapping (Q3)

Pinecone `Vector` (`.claude/knowledge-base/references/pinecone-python-client/pinecone/models/vectors/vector.py:13,25-28`):
`id: str`, `values: list[float]`, `sparse_values: SparseValues | None`, `metadata: dict[str, Any] | None`.

| Pinecone field | TheoDB column | Note |
|---|---|---|
| `id` | `id text PRIMARY KEY` | |
| `values` | `embedding vector(N)` | `'[...]'::vector` from the float list |
| `metadata` | `metadata jsonb` (or promoted relational columns) | the unification win: metadata becomes queryable SQL columns / `jsonb` for `WHERE` + JOIN |
| `sparse_values` | (deferred) | sparse vectors out of M16 scope (honest) |

**Import shape:** read the exported JSON record (`from_dict`, `models/vectors/vector.py:36`) → `INSERT … (id,
embedding, metadata) VALUES ($1, $2::vector, $3::jsonb)`. Stdlib `json` only.

---

## Cross-cutting Comparison

| Dimension | pgvector (A) | pgvectorscale (B) | pinecone-client (C) |
|---|---|---|---|
| Filtered search | post-filter + `iterative_scan` + partial index | in-index label-filter (`smallint[]` `&&`) | metadata filter (separate system) |
| Filter setup cost | none (GUC) / B-tree | label column + opclass | n/a |
| Order guarantee | strict or relaxed | DiskANN order | n/a |
| Role for M16 | **default** filtered-search + unified query + test/EXPLAIN | optimization for categorical filters | **migration source** (data model) |

## ADRs

### D1 — Default filtered search = pgvector `hnsw.iterative_scan = strict_order`; label-filter as optimization
**Decision:** M16's unified query uses post-filter + `hnsw.iterative_scan = strict_order` as the default;
pgvectorscale label-filtering is documented as the optimization for low-cardinality categorical filters.
**Rationale:** iterative_scan needs no schema change, preserves exact order, and works with any relational
`WHERE` — the general unification answer (`README.md:450-453`). Label-filtering requires a `smallint[]` label
column + opclass (`vectorscale--0.8.0--0.9.0.sql:147-175`) — worth it only for categorical filters.
**Alternatives considered:** post-filter only (rejected — over-filtering returns < k); label-filtering as
default (rejected — forces a label schema on every table).
**Consequences:** the over-filtering edge is the load-bearing test; both paths documented honestly.

### D2 — Unified query extends the existing `ai.hybrid_search_rrf` vector-over-regclass pattern
**Decision:** ship the canonical unified query (T2) as a first-class example + e2e test, reusing the
vector-over-relational pattern we already have (`sql/40-theodb-hybrid.sql`).
**Rationale:** Rule 9 — we already run a vector leg over a `regclass` with safe dynamic SQL; M16 demonstrates
the full JOIN+filter+ai.* composition, no new engine code.
**Alternatives considered:** a new bespoke "unified_search" function (rejected — YAGNI; plain SQL is the
product surface and is more honest about "it's just SQL").
**Consequences:** the diff vs Pinecone (relational JOIN) is demonstrated in plain SQL anyone can read.

### D3 — Pinecone import via stdlib json, metadata → jsonb; no client/runtime dep
**Decision:** import reads exported `{id, values, metadata}` JSON → `INSERT … vector/jsonb`; stdlib `json`
only; metadata to `jsonb` (promotable to columns).
**Rationale:** the model is JSON-serializable (`models/vectors/vector.py:36`); avoiding the `pinecone` runtime
dep keeps the importer permissive + dependency-free (parsimony).
**Alternatives considered:** use the `pinecone` client to fetch live (rejected — adds a runtime dep + network
coupling; an export file is portable). sparse_values import (deferred — out of M16 scope, honest).
**Consequences:** simple, dependency-free importer; sparse vectors are a documented follow-up.

### D4 — Honest demo: simplicity/consistency, not speed
**Decision:** the 1-vs-2-systems demo measures lines-of-code / number-of-systems / staleness (no ETL), never
latency/throughput.
**Rationale:** ADR 0005 + `public-copy.md` — performance is competitive, not a claim. The unification win is
consistency (vector and business row in one transaction) + operational simplicity.
**Alternatives considered:** a speed benchmark vs Pinecone (rejected — would be an unbenchmarked/again-perf
claim; not our moat).
**Consequences:** the demo is defensible and on-message.

## Recommendations for the project

| # | Recommendation | Linked to | Priority |
|---|---|---|---|
| 1 | Ship the canonical unified query (vector + JOIN + WHERE + `ai.*`) as a first-class example in `docs/quickstart.md` + an e2e test against the container | Q2, D2, `architecture.md` | HIGH |
| 2 | Filtered search: document + test `hnsw.iterative_scan = strict_order` with the **over-filtering edge** (selective WHERE returns < k → iterative restores k, recall preserved); `EXPLAIN (ANALYZE,BUFFERS)` asserts index use | Q1, Q4, Q6, D1, `testing.md §4.1` | HIGH |
| 3 | `import-from-pinecone`: a function/script mapping `{id,values,metadata}` → `(id, embedding vector, metadata jsonb)`, stdlib json, with a fixture test + `docs/migrate-from-pinecone.md` | Q3, Q5, D3 | HIGH |
| 4 | Honest demo `docs/` (+ script): same RAG in TheoDB (1 SQL, transactional) vs Pinecone+Postgres (2 systems, ETL) — measures simplicity/consistency, not speed | D4, `public-copy.md` | MEDIUM |
| 5 | Document pgvectorscale label-filtering as the categorical-filter optimization (not default) | Q1, D1 | LOW |

## Blocked questions (if any)

(none — all 6 questions answered with verified citations; the Pinecone path correction from edge-case review
(EC-1) was applied and the model confirmed at `models/vectors/vector.py`.)

## Halt-loop progress (audit trail)

- Execution mode: inline (operator-driven) for citation precision.
- Questions answered: 6 / 6 (Q1, Q2, Q3, Q4, Q5, Q6) · Blocked: 0 · Coverage corners: 4/4.
- Citations to `.claude/knowledge-base/references/` verified on disk during collection.

## Related

- Discovery plan: `.claude/knowledge-base/discoveries/plans/unified-vector-relational-plan.md`
- Edge-case review: `.claude/knowledge-base/reviews/unified-vector-relational-edge-cases-2026-06-29.md`
- Decision anchor: `docs/adr/0005-unification-as-differentiator.md`
- Project rules: `.claude/rules/architecture.md`, `.claude/rules/testing.md`, `.claude/rules/public-copy.md`, `.claude/rules/parsimony-ladder.md`
