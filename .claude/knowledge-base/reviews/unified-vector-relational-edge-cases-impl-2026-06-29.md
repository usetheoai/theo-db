# Edge Case Review — M16 unified-vector-relational (implementation plan)

Date: 2026-06-29
Plan: .claude/knowledge-base/plans/unified-vector-relational-plan.md
Tasks analyzed: 5 (T1.1, T1.2, T2.1, T2.2, T3.1)
Cases found: 4 (EDGE: 2, NEGATIVE: 2 | MUST FIX: 1, SHOULD TEST: 1, DOCUMENT: 2)

## MUST FIX

### EC-1: the over-filtering test can be a FALSE GREEN if the edge isn't actually reproduced
- **Affected task:** T1.2
- **Kind:** EDGE (the test's own validity)
- **Family:** State / Test design
- **Scenario:** `test_filtered_search_preserves_recall` asserts `n_with == k AND n_with >= n_without`. But if
  the dataset is too small / the filter not selective enough, the non-iterative path may already return k
  (`n_without == k`), so over-filtering never manifests and the assertion passes **trivially** — proving
  nothing. The whole point (recall preserved under a real over-filtering condition) would be unverified.
- **Impact:** a green test that does not actually exercise the correctness risk M16 exists to prove.
- **Suggested fix:** the test MUST first establish the edge is real — assert `n_without < k` (over-filtering
  reproduced) in the non-iterative state; if it cannot reproduce it (dataset/ef_search), `pytest.xfail` with
  an explicit reason rather than passing. Only then assert `n_with == k`. (Add to T1.2 TDD + AC.)

## SHOULD TEST

### EC-2: import_pinecone with a hostile table/column name (injection) + dimension-mismatch values
- **Affected task:** T2.1
- **Kind:** NEGATIVE
- **Suggested test:** `test_import_pinecone_safe_identifiers` — call with a table created as `"weird;name"`
  (and a column with a quote) → assert it inserts correctly via `%I`/`regclass` (no SQL injection, no error
  from the identifier). And `test_import_pinecone_dim_mismatch` — a `values` array of the wrong length →
  pgvector raises a typed error (the `::vector` cast / column typmod), no partial corrupt insert. (Drawbacks
  already names injection; promote to explicit tests.)

## DOCUMENT

### EC-3: Final-phase rebuild needs disk (host at 99%)
- **Kind:** —
- **Accepted risk:** `docker build -t theo-db:m16 .` reuses the cached heavy stages (M15 proved a cache-reuse
  build is small/fast); only `sql/80` + the cat layer change. If disk is exhausted, surface it honestly and
  validate via a `theo-db:m15` container + copying `sql/80` into the extension dir (the M15 technique), rather
  than a false "validated".

### EC-4: ai.summarize in the unified e2e depends on an LLM endpoint
- **Kind:** NEGATIVE (already in `## Failure scenarios`)
- **Accepted risk:** the unified-query test asserts the JOIN/filter/order legs deterministically; the `ai.*`
  leg is exercised via the deterministic chat stub (tools/chat_server.py) OR asserted structurally (presence),
  never a flaky live LLM call. Documented in Failure scenarios — no plan change.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|---|---|---|---|---|---|
| T1.2 | 1 | 0 | 1 | 0 | 1 |
| T2.1 | 1 | 2 | 0 | 1 | 0 |
| T1.1 | 0 | 1 | 0 | 0 | 1 |

**Verdict:** PLAN NEEDS ADJUSTMENT — 1 MUST FIX absorbed (T1.2 must prove the over-filtering edge is real, else
xfail); 1 SHOULD-TEST added (T2.1 hostile-identifier + dim-mismatch); 2 DOCUMENT (disk rebuild, LLM stub).
