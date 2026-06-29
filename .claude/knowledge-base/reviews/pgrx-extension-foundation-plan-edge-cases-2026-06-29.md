# Edge Case Review — pgrx-extension-foundation (implementation plan)

Date: 2026-06-29
Plan: .claude/knowledge-base/plans/pgrx-extension-foundation-plan.md (v1.0)
Tasks analyzed: 6 (T0.1, T1.1, T2.1, T3.1, T4.1, T5.1)
Cases found: 4 (EDGE: 2, NEGATIVE: 2 | MUST FIX: 1, SHOULD TEST: 2, DOCUMENT: 1)

> The plan already covers the bulk of the embed failure surface exhaustively in `## Failure scenarios`
> (unset GUC → 22023, non-http(s) → 22023, redirect → 22023, connect/timeout → 22023, malformed body →
> 22023) and the least-privilege REVOKE parity (T2.1) + coexistence/install-order (T2.1 deep dive). This
> review flags only what is NOT already foreseen.

## MUST FIX

### EC-1: `RETURNS vector` in pgrx — the `Vec<f32>` → `vector` binding is not pinned
- **Affected task:** T0.1, T1.1
- **Kind:** NEGATIVE (the build/contract can silently produce the wrong SQL type)
- **Family:** Format / Integration
- **Scenario:** the plan signature is `theodb.embed(...) RETURNS vector`, but a pgrx `#[pg_extern] fn embed(...) -> Vec<f32>` maps to SQL `real[]` / `float4[]`, **not** pgvector's `vector`. The reference (pgvectorscale) only returns scalars, so there is no in-repo example of returning a `vector`. T0.1 flags this honestly ("fall back to a cast if no binding exists") but does not pin the mechanism — leaving it to implement-time guesswork risks the oracle's `vector`-typed assertions failing, or an implicit-cast surprise.
- **Impact:** `test_embed_sql.py` expects a `vector` (384-dim). If `embed` returns `float4[]`, the function signature diverges from the public contract (`sql/30` returns `vector`) → parity FAILS or requires an undocumented cast.
- **Suggested fix (≤1 sentence):** pin the mechanism in T0.1/T1.1 — declare the function SQL return as `vector` and return the embedding as `Vec<f32>` with an explicit pgrx SQL wrapper casting `float4[]::vector` (no new dep), **OR** add the `pgvector` Rust crate (MIT, has a pgrx `Vector` type — runs through `/deps-audit` in T5.1); choose at T0.1 and record which (affects the deps-audit scope in T5.1).

## SHOULD TEST

### EC-2: empty-but-valid content — `theodb.embed('')`
- **Affected task:** T1.1, T3.1
- **Kind:** EDGE (smallest valid input)
- **Suggested test:** `test_embed_empty_content_matches_plpython` — assert the Rust impl does the SAME thing the plpython3u baseline does for `theodb.embed('')` (either both return a vector for empty input, or both raise 22023). Parity is the oracle: capture the plpython3u behavior on `''` BEFORE removing it (Phase 2), then assert the Rust impl matches. Do not invent new behavior for empty input — match the baseline.

### EC-3: oversized content / endpoint 4xx (token-limit) — typed-error passthrough
- **Affected task:** T1.1
- **Kind:** NEGATIVE (invalid-for-the-endpoint input → upstream 4xx)
- **Suggested test:** `test_embed_endpoint_4xx_maps_to_22023` — stub returns 400/413 for an over-long input; assert the Rust impl raises SQLSTATE 22023 with a clear message (the generic "non-2xx → 22023" path in T1.1 must cover 4xx, not only 5xx/connect errors). Asserts the *specific typed error + message*, not just "it throws".

## DOCUMENT

### EC-4: non-384-dim response passthrough
- **Kind:** EDGE (a valid response whose dimension differs from the stub's 384)
- **Accepted risk:** pgvector's `vector` is dimension-flexible, so `embed` returns whatever dimension the endpoint produces; the oracle asserts 384 only because `tools/embedding_server.py` is a 384-dim model. A real endpoint with a different model returns a different dim — this is correct behavior (the function does not hard-code 384), not a bug. Document in the benchmark/embed notes that the dimension follows the endpoint's model; the parity test pins 384 solely because the stub does.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T0.1 | 0 | 1 | 1 | 0 | 0 |
| T1.1 | 1 | 2 | (EC-1) | 2 | 1 |
| T2.1 | 0 | 0 | 0 | 0 | 0 |
| T3.1 | 1 | 0 | 0 | (EC-2) | 0 |
| T4.1 | 0 | 0 | 0 | 0 | 0 |
| T5.1 | 0 | 0 | (EC-1 dep) | 0 | 0 |

**Coverage check:** every task touching the HTTP boundary (T1.1) has both EDGE (empty content, non-384 dim) and NEGATIVE (4xx, plus the 5 rows already in `## Failure scenarios`) cases. T2.1 (sql/Docker) and T4.1 (benchmark) have no new input boundary beyond what the plan covers.

**Verdict:** PLAN NEEDS ADJUSTMENT — 1 MUST FIX (pin the `vector` return-type binding in T0.1/T1.1, which also scopes T5.1's deps-audit) to absorb into v1.1; 2 SHOULD TEST (empty-content parity, 4xx→22023) to add to T1.1/T3.1 TDD; 1 DOCUMENT (non-384 dim passthrough).
