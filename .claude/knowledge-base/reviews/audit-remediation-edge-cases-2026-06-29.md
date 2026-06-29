# Edge Case Review — audit-remediation (implementation plan)

Date: 2026-06-29
Plan: .claude/knowledge-base/plans/audit-remediation-plan.md (v1.0)
Tasks analyzed: 6 (T0.1, T1.1, T2.1, T3.1, T4.1, T5.1)
Cases found: 3 (EDGE: 2, NEGATIVE: 1 | MUST FIX: 1, SHOULD TEST: 2)

> The plan already carries thorough Drawbacks & Risks (5), Failure scenarios (5 HTTP rows), and Unresolved
> Questions (Q1 default_version, Q2 minreq status). This review flags only genuinely-new edges.

## MUST FIX

### EC-1: `theodb.embed_batch(ARRAY[]::text[])` — `array_agg` over empty unnest returns NULL, not empty `vector[]`
- **Affected task:** T1.1
- **Kind:** EDGE (empty-but-valid input boundary)
- **Family:** Format / boundary.
- **Scenario:** the SQL wrapper builds `vector[]` via `array_agg(t::vector ORDER BY ord)` over `unnest(...)`. For an EMPTY input array, `_embed_batch_text` returns an empty `text[]`, `unnest` yields zero rows, and `array_agg` returns **NULL** — so `theodb.embed_batch(ARRAY[]::text[])` would return NULL, not the empty `vector[]` the test (`test_embed_batch_empty`) expects.
- **Impact:** NULL-vs-empty mismatch — a caller doing `WHERE … = ANY(embed_batch(...))` or `array_length` gets NULL surprises; the empty-input contract is wrong.
- **Suggested fix (≤1 sentence):** wrap the wrapper in `COALESCE(array_agg(...), ARRAY[]::vector[])` so empty input → empty `vector[]` (and ensure `run_batch` short-circuits empty → `vec![]` with NO HTTP call, as already specified).

## SHOULD TEST

### EC-2: retry sleep vs `statement_timeout` / api_key non-leak under retry (ai._chat plpython3u)
- **Affected task:** T3.1
- **Kind:** NEGATIVE (failure-under-retry).
- **Suggested test:** `test_retry_respects_bounds` — assert (a) total attempts ≤ 3 (stub hit-counter), so a down endpoint can't hang beyond cap×timeout; (b) the api_key never appears in the final error message after retries are exhausted (reuse the existing no-leak assertion). Retry must not turn a bounded failure into an unbounded hang, and must not change the typed-error/secret posture.

### EC-3: `theodb.embed_batch` N-in/N-out with a DUPLICATE input (`['a','a']`) + ordering under a permuted `data[].index`
- **Affected task:** T1.1
- **Kind:** EDGE (extreme of valid input).
- **Suggested test:** `test_embed_batch_order_and_dups` — `embed_batch(ARRAY['a','b','a'])` returns 3 vectors where `[0]==[2]` (same input → same vector, deterministic stub) and `[1]` differs; if the stub returns `data` out of `index` order, the index-aligned mapping still places each correctly. Proves the index-mapping (not array-position) invariant from D1.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST |
|---|---|---|---|---|
| T1.1 | 2 (EC-1, EC-3) | 0 | 1 (EC-1) | 1 (EC-3) |
| T3.1 | 0 | 1 (EC-2) | 0 | 1 (EC-2) |
| T2.1/T4.1/T5.1 | — | — | 0 | 0 (covered by plan's risks/tests) |

**Verdict:** PLAN NEEDS ADJUSTMENT — 1 MUST FIX (EC-1: empty-array COALESCE in the embed_batch wrapper) to absorb into T1.1; 2 SHOULD-TEST (EC-2 retry-bounds+no-leak, EC-3 batch order/dups) added to T3.1/T1.1 TDD.
