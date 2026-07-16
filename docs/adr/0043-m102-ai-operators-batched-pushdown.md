# ADR 0043 — M102: AI predicates as SET-oriented, planner-optimizable operators (revisits ADR 0007)

- Status: Accepted
- Date: 2026-07-16
- Deciders: TheoDB core (CYCLE M102; sign-off council-ai-in-db + council-security)
- Tags: ai-surface, data-flow, scaling, planner, batched-inference
- Supersedes / relates to: ADR 0007 (synchronous per-row model HTTP — this ADR delivers the deferred batch half),
  ADR 0006 (own-code Rust), ADR 0033 (North-Star positioning: AI-native/HTAP is the moat, not vector speed)

## Context

ADR 0007 recorded that `ai.*` and `theodb.embed` issue **one blocking HTTPS round-trip per row** and marked the
batch/async path as **deferred**. `ai.generate_batch` (N prompts → 1 round-trip) later closed the batch half for
the *generative* surface. But `ai.if` — the predicate that a user actually wants to put in a `WHERE` — was still:

1. **per-row** — `SELECT … WHERE ai.if(prompt)` fans out to N sequential HTTP round-trips; and
2. **planner-opaque** — a `VOLATILE` scalar the optimizer cannot batch, cost, or reorder against cheap quals.

M102 makes the predicate surface **set-oriented and planner-friendly** without a bespoke planner rewrite.

## Decision

Ship two surfaces over the SAME inference (`crate::chat`):

- **`ai.if_batch(condition, vals[]) -> bool[]`** — builds N per-item prompts `"{condition}: {value}"` and answers
  them in **ONE** batched round-trip (`ai_generate_batch`), parsing each yes/no to a bool. Element-for-element equal
  to the per-row `ai.if` on the same per-item prompt.
- **`ai.if_costly(condition, val) -> bool`** — a per-row scalar declared with **`COST 100000`** so Postgres's own
  `order_qual_clauses` evaluates cheaper relational quals FIRST. `WHERE cheap AND ai.if_costly(...)` then
  short-circuits the expensive AI on the rows the cheap qual already dropped — LOTUS's **dependency-safe filter
  push-down**, delegated to the planner (Rule 9 — do not reinvent qual ordering). A pure `pushdown_safe(depends_on,
  generated)` helper encodes the `∩ = ∅` safety condition for the design.
- **`ai.call_count()` / `ai.call_reset()`** — expose the inference round-trip count as the wiring-triad **runtime
  metric**, proving "1 round-trip for N rows" and the push-down reduction at query time.
- **`theodb.llm_test_model = 'parity'`** — a hermetic, HTTP-free deterministic model used ONLY by tests/benchmarks,
  so result-equivalence of the batched operator vs the per-row path is proven WITHOUT a flaky/paid live LLM.

### Alternatives rejected

- **Keep per-row `ai.if` only (ADR 0007 status quo)** — REJECTED: one HTTP round-trip per row (measured 12× slower
  than batched on a live model at K=16) and planner-opaque.
- **A full CustomScan planner rewrite that injects a semantic-filter node** — REJECTED for slice-1: large FFI
  surface, and Postgres's built-in `order_qual_clauses` already delivers the dependency-safe push-down when the AI
  predicate carries a high `COST`. KISS + Rule 9. (A learned 3-axis Palimpzest cost model + LOTUS proxy/oracle
  cascade remain the ambitious follow-up.)
- **Assert result-equivalence against a live LLM** — REJECTED: non-deterministic, paid, flaky — a bad correctness
  gate (`testing.md` forbids flaky tests). The mechanism is deterministic and is tested deterministically; the live
  model is the *benchmark*, not the *unit assertion*.

## Consequences

- **Measured (docs/benchmarks/m102-ai-operators.*):** batched = **1 round-trip** vs **N** for per-row (1 vs 1000
  deterministic); push-down evaluates the AI on **≤ K survivors** not all N; real OpenAI `gpt-4o-mini` latency
  **≈ 12×** lower batched vs per-row at K=16 (two runs: 12.17×, 11.81×).
- **Honest ceiling (public-copy.md / North-Star ADR 0033):** a composability / round-trip win with STATISTICAL
  accuracy, **orthogonal to vector recall**. Never framed as "faster at vectors". The batched and per-row prompts
  differ, so answers are NOT asserted identical on a live model — that is the follow-up (proxy/oracle cascade).
- **Security (council-security):** the AI-operator surface takes untrusted values; the per-item prompt quotes the
  value into a bounded template, the functions are `REVOKE`d from PUBLIC (least-privilege parity with the other
  `ai.*`), and the SSRF `http(s)://` endpoint guard in `chat::resolve_chat_cfg` is unchanged.
- **Deferred (tracked):** a sampled-telemetry-calibrated 3-axis cost model; a semantic-filter CustomScan node; the
  LOTUS proxy/oracle cascade with a recall guarantee.

## Cross-references

- Plan: `knowledge-base/plans/m102-ai-operators-plan-nodes-plan.md`
- Benchmark: `docs/benchmarks/m102-ai-operators.{md,json}`
- Code: `theodb_rs/src/ai_op.rs`, `theodb_rs/src/chat.rs` (test model + round-trip counter + `parse_bool`)
- Prior ADR: `docs/adr/0007-synchronous-per-row-model-http.md` (the deferred-batch decision this closes for predicates)
