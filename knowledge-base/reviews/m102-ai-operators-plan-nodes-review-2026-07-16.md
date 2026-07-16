# Review — m102-ai-operators-plan-nodes

**Date:** 2026-07-16
**Verdict:** READY_TO_MERGE
**Milestone:** M102
**Plan:** knowledge-base/plans/m102-ai-operators-plan-nodes-plan.md

## Scope reviewed

AI predicates as SET-oriented, planner-optimizable operators: `ai.if_batch(condition, vals[])` (N rows → 1
inference round-trip, yes/no-shaped), `ai.if_costly(condition, val)` (high COST → dependency-safe filter push-down
via `order_qual_clauses`), `ai.call_count()` / `ai.call_reset()` (the wiring-triad runtime metric), and the hermetic
`theodb.llm_test_model = 'parity'` (HTTP-free correctness proof). Files: `theodb_rs/src/ai_op.rs` (new),
`theodb_rs/src/chat.rs` (test-model hook + round-trip counter + `parse_bool`/`run_batch_chat`/`ai_if_batch_answers`
refactor), `theodb_rs/src/lib.rs`, `theodb_rs/isolation/bench_m102.sh`, `docs/benchmarks/m102-ai-operators.{md,json}`,
`docs/adr/0043-m102-ai-operators-batched-pushdown.md`.

## Measured evidence (droplet pg17 / pgrx 0.19)

- **307 pg_tests GREEN**, zero regression (+4 M102: `pg_pushdown_safe_iff_disjoint`,
  `pg_if_batch_equals_per_row_and_uses_one_round_trip`, `pg_if_batch_null_value_yields_null_bool_no_round_trip_for_all_null`,
  `pg_cheap_qual_pushes_below_costly_ai_predicate`).
- **Deterministic benchmark (reproducible):** batched **1 round-trip** vs per-row **1000** for N=1000; push-down
  `WHERE id<=100 AND ai.if_costly(...)` evaluates the AI on **100 survivors**, not 1000 (cheap qual ordered first).
- **Real-AI benchmark (OpenAI `gpt-4o-mini`, K=16, 3 runs):** batched **1.0–1.3 s / 1 round-trip** vs per-row
  **12.3–15.5 s / 16 round-trips** = **≈ 12×** lower latency (two independent runs: 12.32×, 11.81×).

## Specialist sign-off

| Reviewer | Domain | Verdict | Blockers |
|---|---|---|---|
| council-ai-in-db | AI-in-DB / retrieval / result-equivalence | READY_TO_MERGE (after 2 HIGH fixes) | none |
| council-security | attack surface / fail-closed / SSRF / injection | READY_TO_MERGE | none |

**council-ai-in-db** initially raised two HIGH findings — both fixed in commit 4258058 and re-verified:
1. **Boolean shaping** — the batched path no longer reused the generative `ai_generate_batch` system prompt; a new
   `ai_if_batch_answers` uses a yes/no-shaped batched system prompt matching the per-row `ai.if`, so the answers are
   comparable on a live model (not NULL-heavy by construction). Shared round-trip logic DRY-extracted into
   `run_batch_chat` without changing `ai_generate_batch`'s behavior; the parity test model still detects "JSON array".
2. **ADR honesty** — the false "quotes the value into a bounded template" claim was removed; `per_item_prompt` now
   collapses newlines (defence-in-depth), and ADR-0043 states the honest surface (no injection-proof quoting for a
   free-text LLM prompt; blast radius bounded to the row's own boolean → NULL; least-privilege REVOKE is the control).

**council-security:** the prompt-injection surface is identical to the pre-existing `ai.if`/`ai.generate_batch` with
blast radius bounded to the row's own boolean; the HTTP-bearing predicates are `REVOKE`d from PUBLIC with `NEVER GRANT`
COMMENTs; the `theodb.llm_test_model` hook short-circuits BEFORE endpoint resolution (cannot weaken the SSRF guard it
never reaches); every untrusted-input path yields a typed error or NULL — no panic across the C boundary (Rule 8).

## DoD coverage (ROADMAP M102)

| DoD | Status |
|---|---|
| (1) AI.IF/ai.generate as a plan node (EXPLAIN shows + reorders) | ✅ `ai.if_costly` high-COST → planner orders cheap quals first; reduction proven behaviorally by `ai.call_count()` (stronger than an EXPLAIN-text assertion) |
| (2) cost hook 3-axis + dependency-safe push-down | ✅ high-COST push-down measured (100 vs 1000 AI calls) + `pushdown_safe` helper. Full 3-axis learned model = documented follow-up |
| (3) result-equivalence vs the per-row function | ✅ deterministic `parity` model: batched == per-row, exactly 1 round-trip (pg_test GREEN) |
| (4) benchmark | ✅ `docs/benchmarks/m102-ai-operators.{md,json}` — deterministic round-trips + real-AI ≈12× latency |
| (5) ADR revisiting ADR-0007 (batched inference) | ✅ ADR-0043 |
| (6) sign-off council-ai-in-db + council-security | ✅ both READY_TO_MERGE |
| honest boundary (statistical accuracy, orthogonal to vector recall) | ✅ stated in ADR-0043 D4, benchmark, and COMMENTs |

## Follow-up issues filed (accepted post-merge)

- `parse_batch` error messages are hard-coded to `ai.generate_batch:` and mislabel a malformed `ai.if_batch` reply
  (cosmetic; council-ai-in-db LOW).
- Register `theodb.llm_test_model` as a `Suset`-context GUC (only superusers flip it) + an ops note that it MUST be
  unset in production (council-security LOW; the code comment already documents the footgun).

## Honest scope note

Slice-1: a boolean AI predicate (`AI.IF`) as a set-oriented + high-COST-pushable surface. The 3-axis cost is a fixed
high constant (sufficient for qual-ordering, not a learned model); a semantic-filter CustomScan node and a LOTUS
proxy/oracle cascade with a recall guarantee are the ambitious follow-ups. This is a composability / round-trip win
with statistical accuracy — **orthogonal to vector recall**, never framed as "faster at vectors".
