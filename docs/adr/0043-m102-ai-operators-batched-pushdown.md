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
  them in **ONE** batched round-trip via a **yes/no-shaped** batched inference (`ai_if_batch_answers` — a batched
  system prompt that instructs "'yes' or 'no' for each", the SAME framing the per-row `ai.if` uses), parsing each to
  a bool. Because both surfaces now carry the yes/no instruction, their answers are directly comparable on a live
  model — not merely under the deterministic test model.
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
  accuracy, **orthogonal to vector recall**. Never framed as "faster at vectors". Both surfaces now carry the same
  yes/no framing, so the correctness of the mechanism (1 round-trip, NULL alignment, push-down) is proven
  deterministically; the RESIDUAL difference on a live model is context-bleed drift (all N questions share one
  batched message) — a genuinely statistical effect whose bounded-recall version (a LOTUS proxy/oracle cascade) is
  the follow-up. Answers are NOT asserted byte-identical on a live model; the deterministic `parity` model is the
  correctness gate.
- **Security (council-security: READY_TO_MERGE):** the AI-operator surface takes untrusted values that become LLM
  prompt input — an inherent prompt-injection surface identical to the pre-existing `ai.if` / `ai.generate_batch`,
  with the blast radius bounded to the row's OWN boolean (an unparseable/poisoned answer becomes `NULL`, never an
  escalation). There is **no injection-proof quoting for a free-text LLM prompt** (unlike SQL `%I`); the honest
  control is least-privilege — the functions are `REVOKE`d from PUBLIC (parity with the other `ai.*`) and carry a
  `NEVER GRANT to an isolated role` COMMENT. As defence-in-depth, `per_item_prompt` collapses newlines in the value
  so it cannot forge a new numbered line in the batched protocol. The SSRF `http(s)://` guard in
  `chat::resolve_chat_cfg` is unchanged; the `theodb.llm_test_model` hook short-circuits BEFORE endpoint resolution
  (cannot weaken the guard it never reaches) and MUST be left unset in production (silent-stub footgun otherwise).
- **Deferred (tracked):** a sampled-telemetry-calibrated 3-axis cost model; a semantic-filter CustomScan node; the
  LOTUS proxy/oracle cascade with a recall guarantee.

## Cross-references

- Plan: `knowledge-base/plans/m102-ai-operators-plan-nodes-plan.md`
- Benchmark: `docs/benchmarks/m102-ai-operators.{md,json}`
- Code: `theodb_rs/src/ai_op.rs`, `theodb_rs/src/chat.rs` (test model + round-trip counter + `parse_bool`)
- Prior ADR: `docs/adr/0007-synchronous-per-row-model-http.md` (the deferred-batch decision this closes for predicates)
