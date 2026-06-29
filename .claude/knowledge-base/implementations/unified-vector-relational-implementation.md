# Implementation Summary — M16 unified-vector-relational

**Slug:** unified-vector-relational · **Milestone:** M16 · **Date:** 2026-06-29
**Plan:** `.claude/knowledge-base/plans/unified-vector-relational-plan.md` (v1.1, plan-confidence SHIPPABLE_WITH_CAVEATS 86.0)
**Completion promise:** IMPLEMENTATION_COMPLETE
**Commit:** `9a100d2`

## What shipped

The unification moat (ADR 0005) is now **demonstrable**: vector + relational + AI in one transactional SQL,
filtered search that **preserves recall**, and a dependency-free **Pinecone import** — the step from
"installable" to "product of fact". No engine code, no new dependency, no performance claim.

## Tasks (plan) → result

| Task | Result | Evidence |
|---|---|---|
| T1.1 canonical unified query + e2e test | done | `docs/quickstart.md` § Unified query; `test_unified_query_returns_correct_joined_rows` (vector `JOIN` relational `WHERE` → known nearest in-stock+category row) |
| T1.2 filtered search recall + EXPLAIN | done | `test_filtered_search_preserves_recall` PROVES the over-filtering edge (far cat cluster outside `ef_search`, `enable_seqscan=off` forces the index → `n_without<k`; `iterative_scan=strict_order` restores k); `test_filtered_search_uses_index` (EXPLAIN Index Scan) |
| T2.1 `theodb.import_pinecone` (native jsonb) | done | `sql/80-theodb-migrate.sql`; maps/malformed(22023)/hostile-identifier/dim-mismatch tests; wired into Makefile PARTS + Dockerfile |
| T2.2 migration guide | done | `docs/migrate-from-pinecone.md` (mapping + runnable import) |
| T3.1 honest 1-vs-2 demo | done | `docs/unification-1-vs-2-systems.md` (simplicity/consistency; `test_demo_doc_has_no_perf_claim` enforces no speed claim) |

## Wiring triad

1. **Caller:** the documented unified query + `theodb.import_pinecone` are the production surfaces exercised by
   the migration guide and the quickstart.
2. **Integration test:** `benchmarks/tests/test_unified.py` (10 tests) against a real `theo-db:m16` container.
3. **Runtime observability:** `EXPLAIN (ANALYZE, BUFFERS)` proves the index is used; `pg_proc` shows
   `theodb.import_pinecone` installed via the extension.

## Integration validation (vs theo-db:m16, rebuilt)

- `docker build -t theo-db:m16 .` — OK (cache-reused; `sql/80` in the build).
- `test_unified.py` → **10 passed** (incl. the over-filtering proof, not an xfail).
- `test_extension_install.py` → **9 passed** (sql/80 additive — no regression).
- `smoke.sh` → SMOKE PASSED. `ruff` → clean.

## Honest notes

- **Over-filtering is genuinely reproduced** (not skipped): the far-cluster + `enable_seqscan=off` setup makes
  the approximate index under-return without iterative scan, then `strict_order` restores k — real evidence of
  the recall fix (EC-1).
- **No performance claim** (ADR 0005 / public-copy) — the demo measures simplicity/consistency.
- **Native jsonb import** (no plpython3u / no stdlib json / no pinecone client) — improves on the blueprint's
  "stdlib json" (ADR D3): Postgres parses jsonb natively (parsimony).
- **Scope-honest:** sparse vectors + bulk/parquet export deferred (documented in the migration guide).
- plan-confidence `concurrency_tests_missing` soft-floor is a false positive (single-threaded; signal from
  "transactional" prose) — does not affect the score.

## Next

`/code-quality` → `/review` → `/release` (M17 / next).
