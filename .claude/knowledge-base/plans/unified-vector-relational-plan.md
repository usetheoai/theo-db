---
slug: unified-vector-relational
milestone_id: M16
created_at: 2026-06-29
goal: Make the all-in-one unification demonstrable — unified query + recall-preserving filtered search + Pinecone import
---

# Plan: M16 — Unification made real: the unified query, filtered search, and Pinecone migration

> **Version 1.1** (edge-case MUST-FIX absorbed: T1.2 over-filtering test must PROVE the edge is real — assert `n_without < k` first, else xfail; T2.1 adds hostile-identifier + dim-mismatch negative tests) — TheoDB's moat is unification (ADR 0005): vector + relational + AI in one instance, one
> transactional SQL, no ETL/2nd system. M15 made it installable; M16 makes the unification **demonstrable** —
> a canonical unified query (vector `JOIN` relational + `WHERE` + `ai.*`), **recall-preserving filtered
> search** (`hnsw.iterative_scan` against the over-filtering failure mode), and a **Pinecone import**
> (`{id,values,metadata}` → `(id, embedding vector, metadata jsonb)`). Anchored on the
> `unified-vector-relational` blueprint (SHIPPABLE_WITH_CAVEATS). Honesty (ADR 0005 / Rule 5): performance is
> competitive, not leader — **no speed claim**; the proof is correctness (recall under filter) + unification.

## Goal

> Enable TheoDB users to run vector search joined with relational data and AI in one transactional SQL **and** to import a Pinecone export, so that the unification differentiator is demonstrable, measured by `benchmarks/tests/test_unified.py` passing (unified query + filtered-search-recall-preserved + Pinecone import, all green against the container).

## Context

ADR `0005-unification-as-differentiator` (LOCKED, sign-off CTO) sets unification as the moat and performance
as competitive (not leader). The discovery `unified-vector-relational` (blueprint SHIPPABLE_WITH_CAVEATS)
established: (a) the over-filtering failure mode of approximate indexes and the fix
(`hnsw.iterative_scan = strict_order`), (b) the unified-query skeleton (extends our `ai.hybrid_search_rrf`
vector-over-`regclass` pattern), (c) the Pinecone `Vector{id,values,metadata}` data model → a TheoDB table.
M16 turns the blueprint into demonstrable, tested capability — the step from "installable" to "product of
fact". No engine code; reuse pgvector/pgvectorscale (Rule 9), no new dependency (parsimony ladder).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `sql/40-theodb-hybrid.sql` | 151 | `6c1dddb` (2026-06-28) | `ai.hybrid_search_rrf` — the vector-over-`regclass` safe-dynamic-SQL pattern M16 reuses | unchanged (read-only reference for the pattern) |
| `sql/30-theodb-embed.sql` | 88 | `6c1dddb` (2026-06-28) | `theodb.embed` (plpython3u + `import json`) — confirms the `theodb` schema + plpython3u baseline | `theodb` schema + `theodb.embed` unchanged |
| `sql/80-theodb-migrate.sql` (NEW) | 0 | — | (the `theodb.import_pinecone` function) | — |
| `Makefile` | 36 | `6c1dddb` (2026-06-28) | PGXS build — concatenates `sql/NN-*.sql` into `theodb--1.0.sql` | add `sql/80` to `PARTS` |
| `Dockerfile` | ~88 | `164a1c8`/`380a201` (2026-06-28) | builds the image + concatenates the bodies + `CREATE EXTENSION theodb` | add `sql/80` to the `cat` list |
| `docs/quickstart.md` | 132 | `164a1c8` (2026-06-28) | 12-feature quickstart (M15) | add a "unified query" section; keep existing |
| `docs/migrate-from-pinecone.md` (NEW) | 0 | — | (Pinecone → TheoDB migration guide) | — |
| `docs/unification-1-vs-2-systems.md` (NEW) | 0 | — | (honest 1-vs-2 demo: simplicity/consistency, not speed) | — |
| `benchmarks/tests/test_unified.py` (NEW) | 0 | — | (M16 e2e tests: unified query, filtered-recall, import) | — |
| `CHANGELOG.md` | (exists) | — | public contract | `[Unreleased]` entries |

### Current callers / dependents

- **`ai.hybrid_search_rrf`** (`sql/40-theodb-hybrid.sql:27`) — the vector-over-`regclass` pattern M16 mirrors;
  M16 does NOT modify it (read-only reference). Callers: `smoke.sh`, `benchmarks/tests/test_hybrid.py`.
- **`theodb` schema** (created in `sql/30-theodb-embed.sql`) — M16 adds `theodb.import_pinecone` into it.
  Callers of the schema: `theodb.embed` (existing). New function has no existing caller (it is new product
  surface) — its caller is the migration doc + the test (wiring triad).
- **The extension build** (`Makefile` `PARTS`, `Dockerfile` cat) — adding `sql/80` extends the install script;
  `test_extension_install.py` asserts surfaces by name (won't break; import_pinecone is additive).
- **External public API consumed by other repos:** none.

### Domain glossary

- **over-filtering** — an approximate index (HNSW/IVFFlat) scanned then post-filtered returns *fewer than k*
  rows because the filter is applied after the (bounded) index scan.
- **iterative index scan** — pgvector 0.8+ feature (`hnsw.iterative_scan`) that scans more of the index until
  k results are found (bounded by `hnsw.max_scan_tuples`); `strict_order` keeps exact distance order.
- **unified query** — a single SQL that combines a vector `ORDER BY <=>`, a relational `JOIN`+`WHERE`, and an
  `ai.*` call, in one transaction (no ETL, no second system).
- **Pinecone Vector** — `{id: str, values: list[float], metadata: dict}` — the record shape of a Pinecone
  export.

### Architecture boundaries affected

Per `.claude/rules/architecture.md`: `theodb.import_pinecone` lives in the `theodb` schema (the existing
data/utility namespace), uses safe dynamic SQL (`quote_ident`/`format`/`::regclass`) like `ai.hybrid_search_rrf`
— no cross-layer leak. The unified query + filtered search are **user-facing SQL patterns** (docs + tests),
not engine code — the composition-root/usage layer. No DIP boundary crossed.

## Prior Art & Related Work

- **Internal blueprint:** `.claude/knowledge-base/discoveries/blueprints/unified-vector-relational-blueprint.md`
  — the source (5 recommendations + 4 ADRs); M16 implements Recommendations 1-5.
- **Decision anchor:** `docs/adr/0005-unification-as-differentiator.md` (the moat).
- **Reference (filtered search):** `.claude/knowledge-base/references/pgvector/README.md:450-461` (post-filter
  + `hnsw.iterative_scan` + partial index), `:702-705` (`EXPLAIN (ANALYZE, BUFFERS)`).
- **Reference (label-filter, the optimization):** `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/sql/vectorscale--0.8.0--0.9.0.sql:147-175`.
- **Reference (Pinecone model):** `.claude/knowledge-base/references/pinecone-python-client/pinecone/models/vectors/vector.py:13,25-28`.
- **Repo pattern reused:** `sql/40-theodb-hybrid.sql:27-31` (vector-over-`regclass` safe dynamic SQL).

## Dependencies

**(none — M16 adds no new package dependency.)** Per Rule 9 / parsimony ladder, M16 reuses only what is present:

| Component | Already present? | Note |
|---|---|---|
| pgvector `hnsw.iterative_scan` | yes (pgvector 0.8+ in the image) | filtered search — a GUC, no code |
| jsonb (native Postgres) | yes (engine) | Pinecone import parses jsonb natively — no `plpython3u`/stdlib json needed (improves on blueprint D3; see ADR D3) |
| `ai.*` (rerank/summarize) | yes (M7/M10-13) | the AI leg of the unified query |
| pytest | yes (`benchmarks/`) | e2e tests |

No npm/pip/cargo/go dependency added. No CVE surface change.

## Objective

- [ ] SG1 — Canonical unified query (vector `JOIN` relational + `WHERE` + `ai.*`) documented + e2e-tested.
- [ ] SG2 — Filtered search: `hnsw.iterative_scan` recipe + over-filtering edge test (recall preserved) + `EXPLAIN` proves index use.
- [ ] SG3 — `theodb.import_pinecone(target, export jsonb)` maps `{id,values,metadata}` → `(id, embedding, metadata jsonb)`, tested with a fixture.
- [ ] SG4 — `docs/migrate-from-pinecone.md` (guide).
- [ ] SG5 — Honest 1-vs-2-systems demo (simplicity/consistency, not speed).
- [ ] SG6 — No new dependency; no performance claim.

## ADRs

### D1 — Filtered search default = `hnsw.iterative_scan = strict_order`
**Decision:** the unified query + docs default to post-filter + `SET hnsw.iterative_scan = strict_order`;
pgvectorscale label-filtering documented as the categorical-filter optimization (not default).
**Rationale:** iterative_scan needs no schema change, preserves exact order, works with any relational `WHERE`
(blueprint D1; pgvector README:450-453). Label-filtering needs a `smallint[]` label column + opclass — worth
it only for categorical low-cardinality filters.
**Alternatives considered:** post-filter only (rejected — over-filtering returns < k); label-filter default
(rejected — forces a label schema on every table).
**Consequences:** the over-filtering edge is the load-bearing test.

### D2 — Unified query is plain SQL (example + test), not a new engine function
**Decision:** ship the unified query as a first-class documented SQL example + e2e test, reusing the
vector-over-`regclass` pattern we already have — no new function.
**Rationale:** Rule 9 / YAGNI — "it's just SQL" is the honest, demonstrable product surface; a bespoke
`unified_search()` would hide that the composition is native SQL.
**Alternatives considered:** a new `theodb.unified_search()` wrapper (rejected — YAGNI; the value is showing
plain SQL does it).
**Consequences:** the diff vs Pinecone (relational JOIN in one SQL) is readable by anyone.

### D3 — Pinecone import via native jsonb (plpgsql), not stdlib json / not the pinecone client
**Decision:** `theodb.import_pinecone(target regclass, export jsonb, …)` parses the export with **native
Postgres jsonb** (`jsonb_array_elements`) in plpgsql + safe dynamic SQL; no `plpython3u`, no `json` module, no
`pinecone` client dependency.
**Rationale:** the blueprint (D3) said "stdlib json" — but Postgres parses jsonb **natively** (parsimony
ladder rung 3: native platform feature beats a hand-rolled parser). The client passes the export as `jsonb`;
the engine does the rest. Zero dependency, and it runs in-DB (no external process).
**Alternatives considered:** plpython3u + `json` (rejected — unnecessary; jsonb is native + plpython3u is
superuser-only); the `pinecone` client to fetch live (rejected — runtime dep + network coupling; an export
file is portable).
**Consequences:** dependency-free, in-DB importer; sparse_values deferred (documented).

### D4 — Honest demo measures simplicity/consistency, never speed
**Decision:** the 1-vs-2-systems demo measures lines-of-SQL / number-of-systems / staleness (no ETL), not
latency/throughput.
**Rationale:** ADR 0005 + `public-copy.md` — performance is competitive, not a claim; the unification win is
transactional consistency + operational simplicity.
**Alternatives considered:** a speed benchmark vs Pinecone (rejected — unbenchmarked perf claim; not our moat).
**Consequences:** the demo is defensible and on-message.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Over-filtering could silently return < k rows if iterative_scan isn't set → wrong "unification" demo | High | the load-bearing test asserts the edge: selective WHERE returns < k without iterative, == k with it (recall preserved) | maintainers |
| `EXPLAIN` plan shape may vary across pgvector versions → brittle "Index Scan" assertion | Medium | assert on `Index Scan`/index name substring + `SET LOCAL enable_seqscan=off` to force; tolerate plan-text variance | maintainers |
| Pinecone export format may vary (json array vs ndjson; nested metadata) | Medium | support the documented `{id,values,metadata}` array shape + fixture; fail-fast typed error on unknown shape (negative test) | maintainers |
| `import_pinecone` dynamic SQL could be an injection surface | High | `quote_ident`/`format('%I')`/`::regclass` exactly like `ai.hybrid_search_rrf`; never interpolate raw identifiers; test with a hostile table/column name | maintainers |
| Adding `sql/80` changes the generated `theodb--1.0.sql` (pre-1.0, unreleased publicly) | Low | acceptable pre-1.0; `test_extension_install` asserts surfaces by name (additive, no break) | maintainers |

## Unresolved Questions

- Q1 — Does the bundled pgvector build expose `hnsw.iterative_scan` (0.8+)? Expected yes (M14 shipped 0.8.x) —
  verified in T1.2 against the container.
- Q2 — Pinecone export: single json array vs ndjson? M16 targets the `{id,values,metadata}` **array** shape +
  a fixture; ndjson/parquet bulk is a documented follow-up (not blocking).
- Q3 — sparse_values import — deferred (dense vectors only in M16), documented in the migration guide.

## Dependency Graph

```
Phase 1 (unified query + filtered search) ──▶ Phase 3 (1-vs-2 demo, uses the unified query)
Phase 2 (import-from-pinecone) ─────────────▶ Phase 3 (demo can import the dataset)
        │                                              │
        └──────────────▶ Final Phase (integration validation) ◀──────────────┘
```

Phase 1 and Phase 2 are independent (parallelizable). Phase 3 (demo) uses both. Final validation needs all.

---

## Phase 1: Unified query + recall-preserving filtered search

**Objective:** prove the canonical unified query works and filtered search preserves recall (no over-filtering).

### T1.1 — Canonical unified query (vector JOIN relational + WHERE + ai.*) + e2e test

#### Objective
Document + e2e-test a single transactional SQL combining vector search, a relational JOIN+filter, and `ai.*`.

#### Why this step (action + reasoning)
1. **What:** add a "Unified query" section to `docs/quickstart.md` with the canonical SQL (vector `ORDER BY
   <=>` + `JOIN` + `WHERE` + an `ai.*` call) and an e2e test asserting a correct result against the container.
2. **Why now:** this is the literal proof of the moat (ADR 0005 / blueprint D2). Without a tested, documented
   example, "unification" is a claim, not a demonstrable product surface.

#### Evidence
- Pattern reused: `sql/40-theodb-hybrid.sql:27-31` (vector-over-`regclass`). Skeleton: blueprint § T2.
- Table model already in `docs/quickstart.md:25-32` (products with `embedding` + `category_id`).

#### Files to edit
```
docs/quickstart.md — add "## Unified query (the differentiator)" section with the canonical SQL
benchmarks/tests/test_unified.py (NEW) — RED: test_unified_query_returns_correct_joined_rows
```

#### Deep file dependency analysis
- `docs/quickstart.md` (Baseline row) gains a section; existing content unchanged. The test seeds a `products`
  + `inventory` table, runs the unified SQL, asserts the top row matches the expected (vector-nearest AND
  in-stock AND category filter). No production code changes (D2 — plain SQL).

#### Deep Dives
- **Canonical SQL** (the example):
  `SELECT p.id, ai.summarize(p.description) FROM products p JOIN inventory i ON i.product_id=p.id WHERE i.in_stock AND p.category_id=$2 ORDER BY p.embedding <=> $1 LIMIT $3;`
- The test uses a **deterministic** assertion: seed vectors so the nearest in-stock+category row is known;
  `ai.summarize` is exercised via the deterministic chat stub (tools/chat_server.py) OR asserted structurally
  if no stub — the unified JOIN+filter+order correctness is the core assertion (AI leg presence, not content).

#### Pseudo-code / Signatures
```python
def test_unified_query_returns_correct_joined_rows(conn):
    # seed products(embedding, category_id) + inventory(in_stock); known nearest in-stock cat=3 row = P
    rows = run(unified_sql, query_vec, 3, 5)
    assert rows[0].id == P          # vector-nearest AND in_stock AND category=3
    assert all(r.in_category_and_stock for r in rows)
```

#### Tasks
1. Add the "Unified query" section to `docs/quickstart.md`.
2. RED: `test_unified_query_returns_correct_joined_rows` (seed + assert).

#### TDD
```
RED:     test_unified_query_returns_correct_joined_rows() — seed products+inventory; unified SQL returns the known nearest in-stock+category row (FAILS until tables/SQL exist)
GREEN:   the SQL is plain (pgvector + JOIN + ai.*) — already supported by the image; the test sets it up
REFACTOR: None expected
VERIFY:  PGHOST=localhost PGPORT=<port> python3 -m pytest benchmarks/tests/test_unified.py::test_unified_query_returns_correct_joined_rows -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `python3 -m pytest benchmarks/tests/test_unified.py::test_unified_query_returns_correct_joined_rows -q` exits 0 against the container.
- [ ] `grep -q "Unified query" docs/quickstart.md` exits 0 AND the section contains a `JOIN` + `<=>` + `ai.` in one statement.
- [ ] No new dependency; no performance number in the doc (`grep -iE "faster|x speedup|ms|qps" docs/quickstart.md` finds none in the new section).

#### DoD
- [ ] Test green; CHANGELOG `[Unreleased]` updated; doc section present.

### T1.2 — Recall-preserving filtered search (over-filtering edge) + EXPLAIN

#### Objective
Prove a selective relational `WHERE` + vector `ORDER BY` returns the full k via `hnsw.iterative_scan` (no over-filtering), and the index is used.

#### Why this step (action + reasoning)
1. **What:** an e2e test that reproduces over-filtering (selective WHERE → < k rows under default ef_search)
   and asserts `SET hnsw.iterative_scan = strict_order` restores k (recall preserved); plus an `EXPLAIN
   (ANALYZE, BUFFERS)` assertion that an Index Scan is used.
2. **Why now:** over-filtering is THE correctness risk of "vector+relational together" (blueprint D1/T1). If we
   demo unification without proving recall under filter, the demo is dishonest.

#### Evidence
- pgvector README:450 (over-filtering: "only 4 rows will match on average"), :453 (`SET hnsw.iterative_scan =
  strict_order`), :517 (`max_scan_tuples`), :705 (`EXPLAIN (ANALYZE, BUFFERS)`).

#### Files to edit
```
docs/quickstart.md — add filtered-search note (SET hnsw.iterative_scan = strict_order) to the unified section
benchmarks/tests/test_unified.py — RED: test_filtered_search_preserves_recall + test_filtered_search_uses_index
```

#### Deep file dependency analysis
- Test-only + a doc note. Seeds N rows where a selective `WHERE` (~1% match) would under-return with default
  HNSW ef_search; asserts iterative_scan restores the expected count. No production code.

#### Deep Dives
- **Over-filtering reproduction:** insert e.g. 2000 rows, 1% with `category_id=99`; HNSW index; `WHERE
  category_id=99 ORDER BY embedding <=> $1 LIMIT 10`. Without iterative scan → fewer than 10. With `SET
  hnsw.iterative_scan = strict_order` → 10 (the true nearest matching rows). Assert both states.
- **Index-use:** `EXPLAIN (ANALYZE, BUFFERS)` of the filtered order-by; assert plan text contains `Index
  Scan` (or the hnsw index name); use `SET LOCAL enable_seqscan = off` to make it deterministic.

#### Pseudo-code / Signatures
```python
def test_filtered_search_preserves_recall(conn):
    # 2000 rows, 1% category=99; HNSW index
    n_without = count(filtered_orderby, iterative='off')   # may be < 10 (over-filtering)
    n_with    = count(filtered_orderby, iterative='strict_order')
    assert n_with == 10 and n_with >= n_without            # recall restored
```

#### Tasks
1. RED: `test_filtered_search_preserves_recall` (over-filtering edge).
2. RED: `test_filtered_search_uses_index` (EXPLAIN Index Scan).
3. Add the `hnsw.iterative_scan` note to the doc.

#### TDD
```
RED:     test_filtered_search_preserves_recall() — FIRST assert the edge is real (n_without < k → over-filtering reproduced; else pytest.xfail with reason), THEN assert n_with == k with iterative_scan=strict_order (recall restored). Never a trivial pass (EC-1).
RED:     test_filtered_search_uses_index() — EXPLAIN (ANALYZE,BUFFERS) of the filtered order-by shows an Index Scan (enable_seqscan=off)
GREEN:   feature exists in pgvector 0.8 (image) — tests configure it
REFACTOR: None expected
VERIFY:  PGHOST=localhost PGPORT=<port> python3 -m pytest benchmarks/tests/test_unified.py -k filtered -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `python3 -m pytest benchmarks/tests/test_unified.py -k filtered -q` exits 0.
- [ ] The recall test FIRST proves the edge (`n_without < k`, else xfail) THEN asserts `n_with == k` (no trivial pass — EC-1).
- [ ] The index test asserts the plan contains `Index Scan` (not only `Seq Scan`).

#### DoD
- [ ] Both filtered tests green; doc note present; CHANGELOG updated.

---

## Phase 2: Import from Pinecone

### T2.1 — `theodb.import_pinecone(target, export jsonb)` (native jsonb) + tests

#### Objective
A dependency-free in-DB function mapping a Pinecone export `{id,values,metadata}` array → a TheoDB table.

#### Why this step (action + reasoning)
1. **What:** add `sql/80-theodb-migrate.sql` with `theodb.import_pinecone(target regclass, export jsonb,
   id_col text DEFAULT 'id', embedding_col text DEFAULT 'embedding', metadata_col text DEFAULT 'metadata')
   RETURNS int` (count inserted), using `jsonb_array_elements` + safe dynamic SQL; wire into the build.
2. **Why now:** the migration path is the north-star-metric enabler (migrations Pinecone → TheoDB). Native
   jsonb (D3) makes it zero-dependency and in-DB.

#### Evidence
- Pinecone model: `.claude/knowledge-base/references/pinecone-python-client/pinecone/models/vectors/vector.py:25-28`
  (`id: str`, `values: list[float]`, `metadata: dict`).
- Safe dynamic SQL pattern: `sql/40-theodb-hybrid.sql:27-31` (`format`/`%I`/`::regclass`).

#### Files to edit
```
sql/80-theodb-migrate.sql (NEW) — theodb.import_pinecone(...) plpgsql + REVOKE ... FROM PUBLIC
Makefile — add sql/80-theodb-migrate.sql to PARTS
Dockerfile — add sql/80-theodb-migrate.sql to the cat list (install script build)
benchmarks/tests/test_unified.py — RED: test_import_pinecone_maps_records + test_import_pinecone_rejects_malformed
```

#### Deep file dependency analysis
- `sql/80` (NEW) adds `theodb.import_pinecone` into the `theodb` schema (created in `sql/30`). The build
  (`Makefile` PARTS + `Dockerfile` cat) concatenates it into `theodb--1.0.sql`. `test_extension_install.py`
  asserts surfaces by name (additive — no break). Downstream caller: the migration doc + the test.

#### Deep Dives
- **Function body (native jsonb + safe dynamic SQL):**
  - validate `jsonb_typeof(export) = 'array'` → else `RAISE EXCEPTION` (typed, SQLSTATE 22023).
  - `FOR rec IN SELECT jsonb_array_elements(export)`: require `rec ? 'id'` and `rec ? 'values'` → else raise.
  - `EXECUTE format('INSERT INTO %s (%I, %I, %I) VALUES ($1, $2::vector, $3)', target, id_col, embedding_col,
    metadata_col) USING rec->>'id', (rec->'values')::text, COALESCE(rec->'metadata','{}'::jsonb)`.
  - return the inserted count.
- **Security:** `target` is `regclass` (validated); columns via `%I` (`quote_ident`); values via `USING`
  params (no interpolation) — injection-safe (mirrors `ai.hybrid_search_rrf`). `REVOKE ALL ... FROM PUBLIC`.
- **Edge/negative:** non-array export → 22023; element missing `id`/`values` → 22023; empty array → returns 0.

#### Pseudo-code / Signatures
```sql
CREATE OR REPLACE FUNCTION theodb.import_pinecone(
  target regclass, export jsonb,
  id_col text DEFAULT 'id', embedding_col text DEFAULT 'embedding', metadata_col text DEFAULT 'metadata'
) RETURNS int LANGUAGE plpgsql AS $$
DECLARE rec jsonb; n int := 0;
BEGIN
  IF jsonb_typeof(export) <> 'array' THEN
    RAISE EXCEPTION 'theodb.import_pinecone: export must be a JSON array' USING ERRCODE='22023';
  END IF;
  FOR rec IN SELECT * FROM jsonb_array_elements(export) LOOP
    IF NOT (rec ? 'id' AND rec ? 'values') THEN
      RAISE EXCEPTION 'theodb.import_pinecone: each record needs id and values' USING ERRCODE='22023';
    END IF;
    EXECUTE format('INSERT INTO %s (%I,%I,%I) VALUES ($1,$2::vector,$3)', target, id_col, embedding_col, metadata_col)
      USING rec->>'id', (rec->'values')::text, COALESCE(rec->'metadata','{}'::jsonb);
    n := n + 1;
  END LOOP;
  RETURN n;
END $$;
-- Example: theodb.import_pinecone('items'::regclass,
--   '[{"id":"a","values":[1,0,0],"metadata":{"cat":3}}]') -> 1 row in items
```

#### Tasks
1. Write `sql/80-theodb-migrate.sql` (function + REVOKE).
2. Add `sql/80` to `Makefile` PARTS + `Dockerfile` cat list.
3. RED: `test_import_pinecone_maps_records` + `test_import_pinecone_rejects_malformed`.

#### TDD
```
RED:     test_import_pinecone_maps_records() — import a 2-record jsonb export into a table; assert 2 rows with id/embedding/metadata mapped (returns 2)
RED:     test_import_pinecone_rejects_malformed() — non-array export AND a record missing 'values' each raise SQLSTATE 22023 (typed error, no partial corruption)
RED:     test_import_pinecone_safe_identifiers() — target table/column with hostile names (e.g. "weird;name", quoted col) → inserts correctly via %I/regclass, no injection (EC-2)
RED:     test_import_pinecone_dim_mismatch() — a values array of the wrong length → typed error from the ::vector cast / column typmod, no partial corrupt insert (EC-2)
GREEN:   write sql/80 + wire build
REFACTOR: None expected
VERIFY:  PGHOST=localhost PGPORT=<port> python3 -m pytest benchmarks/tests/test_unified.py -k pinecone -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `python3 -m pytest benchmarks/tests/test_unified.py -k pinecone -q` exits 0.
- [ ] Mapping test: `theodb.import_pinecone(...)` returns the record count AND rows have `embedding::vector` + `metadata::jsonb` populated.
- [ ] Negative test: non-array AND missing-`values` each raise `SQLSTATE 22023` (assert the typed error, not just "raises").
- [ ] `grep -q 'REVOKE' sql/80-theodb-migrate.sql` exits 0 (least privilege).
- [ ] Pass: size — `sql/80-theodb-migrate.sql` ≤ 500 lines.

#### DoD
- [ ] Tests green; build includes sql/80; CHANGELOG updated.

### T2.2 — `docs/migrate-from-pinecone.md`

#### Objective
A guide: export from Pinecone → `CREATE TABLE` (vector + metadata) → `theodb.import_pinecone` → query.

#### Why this step (action + reasoning)
1. **What:** write the migration guide with the field mapping (blueprint § T3) + a runnable example.
2. **Why now:** the doc is the north-star-metric enabler; the function without a guide is invisible.

#### Evidence
- Mapping table: blueprint § T3 (Pinecone `{id,values,metadata}` → TheoDB `(id text, embedding vector, metadata jsonb)`).

#### Files to edit
```
docs/migrate-from-pinecone.md (NEW) — guide + runnable example + sparse_values caveat
```

#### Deep file dependency analysis
- New doc; references `theodb.import_pinecone` (T2.1). No code dependency.

#### Deep Dives
- Honest caveats: dense vectors only (sparse deferred); array export shape; metadata → jsonb (promotable to
  columns). No performance claim (`public-copy.md`).

#### Tasks
1. Write `docs/migrate-from-pinecone.md`.

#### TDD
```
RED:     test_migrate_doc_sql_runs() — extract the fenced SQL from docs/migrate-from-pinecone.md (CREATE TABLE + import + query) and run against the container; assert no error
GREEN:   write the doc with runnable SQL
REFACTOR: None expected
VERIFY:  PGHOST=localhost PGPORT=<port> python3 -m pytest benchmarks/tests/test_unified.py -k migrate_doc -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `python3 -m pytest benchmarks/tests/test_unified.py -k migrate_doc -q` exits 0 (the guide's SQL runs).
- [ ] `test -f docs/migrate-from-pinecone.md` exits 0; mentions `theodb.import_pinecone` + the field mapping.
- [ ] No performance claim in the guide.

#### DoD
- [ ] Doc present + its SQL runs green; CHANGELOG updated.

---

## Phase 3: Honest 1-vs-2-systems demo

### T3.1 — `docs/unification-1-vs-2-systems.md` (simplicity/consistency, not speed)

#### Objective
A reproducible doc showing the same RAG step in TheoDB (1 SQL, transactional) vs Pinecone+Postgres (2 systems, sync).

#### Why this step (action + reasoning)
1. **What:** a doc that puts side by side: TheoDB unified query (one transactional SQL) vs the 2-system flow
   (Pinecone query → fetch ids → Postgres JOIN → app-side merge), measuring **simplicity/consistency**
   (number of systems, lines, staleness window), NOT latency.
2. **Why now:** this is the on-message proof of the moat (ADR 0005 / D4) — and the honest one.

#### Evidence
- D4 (demo measures simplicity/consistency). The unified SQL from T1.1; the 2-system flow uses the Pinecone
  client model (`.../pinecone/models/vectors/vector.py`).

#### Files to edit
```
docs/unification-1-vs-2-systems.md (NEW) — side-by-side: 1 SQL vs 2-system flow; metrics = systems/lines/staleness
```

#### Deep file dependency analysis
- New doc; uses the T1.1 unified query. No code.

#### Deep Dives
- Metrics table: # systems (1 vs 2), # moving parts (no ETL vs sync job), staleness window (0 — same txn vs
  eventual). Explicit "NOT a speed comparison" disclaimer (`public-copy.md`).

#### Tasks
1. Write `docs/unification-1-vs-2-systems.md`.

#### TDD
```
RED:     test_demo_doc_has_no_perf_claim() — grep the doc for banned perf framings (faster/x faster/qps/latency ms) → asserts NONE; and asserts it has the unified SQL block
GREEN:   write the doc (simplicity/consistency framing)
REFACTOR: None expected
VERIFY:  python3 -m pytest benchmarks/tests/test_unified.py -k demo_doc -q
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `python3 -m pytest benchmarks/tests/test_unified.py -k demo_doc -q` exits 0.
- [ ] `test -f docs/unification-1-vs-2-systems.md` exits 0; contains the unified SQL + a "not a speed comparison" disclaimer.
- [ ] No banned perf framing (grep finds none).

#### DoD
- [ ] Doc present; no-perf-claim test green; CHANGELOG updated.

---

## Coverage Matrix

| # | Gap / Requirement (ROADMAP M16 DoD) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Query unificada canônica + teste e2e | T1.1 | doc section + e2e test (JOIN+WHERE+`<=>`+`ai.*`) |
| 2 | Filtered vector search eficiente (over-filtering + EXPLAIN) | T1.2 | iterative_scan recall test + Index-Scan EXPLAIN test |
| 3 | Migração do Pinecone (import + teste) | T2.1 | `theodb.import_pinecone` (native jsonb) + map/negative tests |
| 3b | Guia de migração | T2.2 | `docs/migrate-from-pinecone.md` + runnable-SQL test |
| 4 | Demo honesta 1-vs-2 | T3.1 | `docs/unification-1-vs-2-systems.md` + no-perf-claim test |
| 5 | Sem dep nova / sem claim de performance | T1.1, T2.1, T3.1 | native jsonb (D3); public-copy lint in ACs |

**Coverage: 5/5 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `python3 -m pytest benchmarks/tests/test_unified.py -q` green against the container
- [ ] `test_extension_install.py` still green (additive `sql/80`, no regression)
- [ ] `smoke.sh` still green against the rebuilt image
- [ ] Zero lint — `ruff check benchmarks/tests/test_unified.py`
- [ ] File-size budget respected (each changed file ≤ 500 lines)
- [ ] CHANGELOG.md `[Unreleased]` updated
- [ ] Backward compatibility — `ai.hybrid_search_rrf`/`theodb.embed` unchanged; `sql/80` additive
- [ ] Plan-specific: NO performance claim anywhere (public-copy); over-filtering recall test proves correctness
- [ ] **Plan archived** after `/review` READY_TO_MERGE + PR merged

## Failure scenarios (when I/O external)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `postgres` (DB driver, psycopg2) | DB unavailable when a test connects | point the test at a closed port / stopped container | test fails fast with a clear connection error; conftest gates on readiness; no partial assertion |
| `theodb.import_pinecone` (jsonb input) | malformed export (non-array; record missing `values`) | feed non-array + missing-`values` jsonb fixtures | `RAISE EXCEPTION … SQLSTATE 22023` (typed, fail-fast); no partial/corrupt insert |
| `ai.summarize` in the unified query (HTTP to LLM) | LLM endpoint unset/unreachable during the e2e test | run without `theodb.llm_endpoint` set | typed error from `ai._chat` (existing M7 behavior); the test either sets the deterministic stub OR asserts the JOIN/filter legs structurally (AI leg presence), never a flaky live call |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** prove the unification is demonstrable in a real container — not just unit asserts.

### Execution

```
docker build -t theo-db:m16 .                                   # image with sql/80 (import_pinecone)
# run a fresh container on a free port, wait for healthcheck
PGHOST=localhost PGPORT=<port> python3 -m pytest benchmarks/tests/test_unified.py -q     # M16 e2e
PGHOST=localhost PGPORT=<port> python3 -m pytest benchmarks/tests/test_extension_install.py -q  # no regression
PGHOST=localhost PGPORT=<port> bash smoke.sh                    # product smoke
ruff check benchmarks/tests/test_unified.py                     # lint
```

### Acceptance Criteria

- [ ] `docker build -t theo-db:m16 .` exits 0 (sql/80 in the build)
- [ ] `test_unified.py` exits 0 (unified query + filtered recall + index + pinecone import + docs SQL + no-perf-claim)
- [ ] `test_extension_install.py` 9/9 still green; `smoke.sh` SMOKE PASSED
- [ ] `theodb.import_pinecone` present on a fresh container (`SELECT 1 FROM pg_proc … proname='import_pinecone'`)
- [ ] Over-filtering recall test proves recall preserved (n_with == k ≥ n_without)
- [ ] Zero lint; no performance claim in any new doc

### If Validation Fails

1. Identify plan-caused vs pre-existing failures.
2. Fix all plan-caused failures before declaring complete.
3. Re-run the chain.
4. Log pre-existing issues in the PR description.
