# Discover Edge Case Review — unified-vector-relational

Date: 2026-06-29
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/unified-vector-relational-plan.md
Research questions analyzed: 7
Edge cases found: 3 (MUST FIX: 1, SHOULD TEST: 1, DOCUMENT: 1)

Paths verified: pgvector `README.md` (5 hits for Iterative/EXPLAIN), `src/hnswscan.c`, `test/sql/{hnsw,ivfflat}_vector.sql`
(both contain `iterative_scan`); pgvectorscale `src/access_method/scan.rs` + label opclass sql. Pinecone path was
imprecise — see EC-1.

## MUST FIX

### EC-1: Q4/Q6 + In-Scope cite imprecise Pinecone paths (`pinecone/data/` is empty)
- **Affected question:** Q4, Q6 (techniques/deps)
- **Family:** Reference path
- **Scenario:** the plan cites `pinecone/db_data/` and `pinecone/data/` for the Vector/fetch model. Verified:
  `pinecone/data/` has only `__init__.py` (empty); the real data model lives in
  `pinecone/db_data/dataclasses/vector.py` (+ `fetch_response.py`, `upsert_response.py`,
  `fetch_by_metadata_response.py`) and the fetch/upsert interface in `pinecone/index/__init__.py`. Fase A
  on `pinecone/data/` would exhaust (empty) → Q4 risks BLOCKED on a fixable path error.
- **Impact:** the migration-mapping question (the load-bearing M16 deliverable) could stall.
- **Suggested fix:** repoint Q4/Q6/In-Scope to `pinecone/db_data/dataclasses/` (Vector model + fetch/upsert
  responses) + `pinecone/index/__init__.py` (fetch/upsert interface). Drop `pinecone/data/`.

## SHOULD TEST

### EC-2: filtered-search recall must be asserted with a NEGATIVE/edge case, not just happy path
- **Affected question:** Q5
- **Suggested halt-loop checkpoint:** the blueprint's test design (from Q5) MUST include the **over-filtering
  edge** — a selective `WHERE` (e.g., 1% match) under HNSW default `ef_search` returning *fewer than k* rows,
  and the assertion that `hnsw.iterative_scan` (or label-filtering / partial index) fixes it (recall
  preserved). Happy-path-only (filter matches most rows) would hide the exact failure M16 must prevent.

## DOCUMENT

### EC-3: Pinecone export at scale (parquet/bulk) vs single fetch is out of M16 scope
- **Accepted risk:** the migration mapping targets the **vector + metadata data model** (id/values/metadata),
  not a bulk parquet export pipeline. A large-scale Pinecone export tool is a follow-up; M16 proves the
  mapping + import shape with a fixture. Documented, deferred.

## Summary

| Question | Edges | MUST FIX | SHOULD TEST | DOCUMENT |
|---|---|---|---|---|
| Q4/Q6 | 1 | 1 | 0 | 1 |
| Q5 | 1 | 0 | 1 | 0 |

**Verdict:** DISCOVERY PLAN NEEDS ADJUSTMENT — 1 MUST FIX absorbed into v1.1 (Pinecone paths); 1 SHOULD-TEST
added as a halt-loop checkpoint (over-filtering edge); 1 DOCUMENT (bulk export deferred).
