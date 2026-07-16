---
slug: m102-ai-operators-plan-nodes
milestone_id: M102
created_at: 2026-07-16
goal: Ship AI.IF as a set-oriented plan operator (batched inference + dependency-safe filter push-down) whose result equals the per-row ai.generate path, proven by a deterministic-model result-equivalence pg_test + an EXPLAIN-visible push-down + a measured composition benchmark (round-trips + latency) with an honest statistical-accuracy note.
---

# M102 — AI operators as optimizable plan nodes (`AI.IF` / `sem_filter` pushable)

## Goal

Ship `AI.IF(condition, rows)` as a SET-oriented plan operator (not per-row plpgsql) that (a) BATCHES the inference in
one round-trip (reusing `ai.generate_batch`) and (b) lets the planner push a cheap relational `WHERE` BELOW the
expensive AI op when dependency-safe — whose result EQUALS the per-row `ai.generate` path (proven with a deterministic
test model to avoid real-AI flakiness), with the push-down EXPLAIN-visible and a measured composition benchmark
(round-trips + latency vs the per-row path) reporting the statistical-accuracy methodology (never "AI.IF is fast"
without the quality point). Metric: **the M102 result-equivalence + push-down pg_tests GREEN on pg17 + a measured
`docs/benchmarks/m102-ai-operators.{md,json}` showing the batched/pushed-down composition's round-trip + latency win.**

## Context

M100 (v0.88.0) shipped the DataFusion CustomScan; M101 (v0.89.0) the HTAP cache. M102 closes the gap the user raised:
`ai.generate`/`ai.nl_to_sql` are FUNCTIONS today — black boxes the planner cannot cost, reorder, or batch. M102 makes
`AI.IF` a plan operator so the planner pushes the cheap relational filter before the expensive AI and batches the
inference. **The batching half is already done:** `chat.rs::ai_generate_batch` answers N prompts in ONE round-trip
(the ADR-0007 revisit). M102's new value is the PLAN-NODE surface + the dependency-safe push-down + the 3-axis cost
hook. **Honest boundary (Rule 5):** this is orthogonal to vector recall — a composability/cost win with STATISTICAL
accuracy that MUST be reported with the recall target + sample methodology.

## Baseline Context

### Files that will be touched

| File | LoC today | Role | Change |
|---|---|---|---|
| `theodb_rs/src/chat.rs` | ~250 | `ai_generate`/`ai_generate_batch` (N prompts, 1 round-trip) | REUSE `ai_generate_batch` as the batched inference of the operator |
| `theodb_rs/src/ai_op.rs` | (NEW) | the `AI.IF` set-oriented operator (SQL surface + the batched evaluator) | NEW — the milestone core |
| `theodb_rs/src/am/customscan.rs` / `columnar_agg.rs` | ~960 / ~370 | the CustomScan machinery (M100 planner hook) | REUSE the pattern for the AI-op node + the dependency-safe push-down |
| `theodb_rs/src/am/cost.rs` | ~40 | the honest cost surface (M48) | ADD a 3-axis (cost/time/quality + selectivity) cost estimate for the AI op (Palimpzest model) |
| `docs/adr/00NN-batched-ai-inference.md` | (NEW) | revisit ADR-0007 (per-row HTTP) for batching | NEW ADR |
| `docs/benchmarks/m102-ai-operators.{md,json}` | (NEW) | composition benchmark | NEW |

### Current callers / prior art in this repo (reuse, not greenfield)

- `chat.rs::ai_generate_batch` (`:107`) — the batched inference (N prompts → 1 round-trip); the AI-op evaluates its
  condition over a row-set by building N prompts and calling this ONCE.
- `chat.rs::ai_generate` — the per-row path M102's result-equivalence compares against.
- `am/columnar_agg.rs` — the M100 `create_upper_paths_hook` + CustomScan idiom the AI-op node reuses; the
  dependency-safe push-down is a planner rewrite in the same spirit.
- `am/cost.rs` — the honest cost surface; the 3-axis AI-op cost extends it.
- `nl.rs` — the NL→SQL surface (SSRF/injection guards) the council-security review covers.

### Glossary

- **AI.IF** — a set-oriented boolean AI predicate: over N rows, build N prompts, one batched inference, N booleans.
- **Dependency-safe push-down** — move a cheap relational predicate BELOW the AI op iff `depends_on ∩ generated_fields
  = ∅` (LOTUS `PushDownFilter`); the cheap filter shrinks the row-set the AI sees.
- **3-axis cost** — cost / time / quality + selectivity, the Palimpzest cost model for an AI op.
- **Deterministic test model** — a local, injectable "inference" (`theodb.llm_test_model`) that answers a prompt
  deterministically (e.g., echoes a rule), so result-equivalence is provable WITHOUT a real, flaky, paid API call.

### Architecture boundaries

Per `rules/architecture.md`: the AI-op node is the interface layer; `ai_op.rs` is the application layer; `chat.rs`
(HTTP) is the infrastructure adapter. No panic across C (Rule 8). The AI surface is untrusted input → prompt-injection
+ SSRF guards (council-security) at the boundary (`error-handling.md`).

## Prior Art & Related Work

- **Pillar blueprint (SHIPPABLE 98.8):** `knowledge-base/discoveries/blueprints/single-planner-columnar-ai-blueprint.md`
  Q5 (δ rung) — LOTUS `sem_filter` proxy/oracle cascade + Palimpzest 3-axis cost + the dependency-safe `PushDownFilter`.
- **Apache-2.0 / MIT study:** `lotus` (semantic operators) + `palimpzest` (Cascades cost model) — design study, own code.
- **TheoDB own prior art:** `theodb_rs/src/{chat.rs,nl.rs,am/customscan.rs,am/cost.rs}` + ADR-0007 (per-row HTTP) +
  ADR-0033 (positioning). The batched inference (`ai_generate_batch`) already exists — NOT greenfield.

## ADRs

### D1 — AI.IF as a set-oriented operator over `ai.generate_batch`, not per-row plpgsql

**Decision:** `AI.IF` builds N prompts over a row-set and calls `ai_generate_batch` ONCE; the planner sees a costed
operator it can reorder/push-down.
**Alternatives:** keep per-row `ai.generate` in a `WHERE` (today) — REJECTED (one HTTP round-trip PER ROW throws away
the columnar/batch win; the planner cannot reorder a black-box function). **Rationale:** blueprint Q5; the batching
adapter already exists (`ai_generate_batch`) — this is the plan-node surface over it.

### D2 — Dependency-safe push-down (run the cheap WHERE before the AI)

**Decision:** the planner moves a cheap relational predicate below the AI op iff `depends_on ∩ generated_fields = ∅`
(the AI op does not consume the filtered column's AI-generated output).
**Alternatives:** always run the AI first — REJECTED (evaluates the expensive AI on rows a cheap filter would drop).
**Rationale:** LOTUS `PushDownFilter`; the load-bearing rewrite of the δ rung.

### D3 — Result-equivalence proven with a deterministic test model; real-AI is the benchmark, not the correctness gate

**Decision:** the batched-operator == per-row equivalence is proven with an injectable DETERMINISTIC model
(`theodb.llm_test_model`) so the pg_test is not flaky/paid; the real-AI path is exercised by the composition benchmark
(round-trips + latency), reported with the statistical-accuracy methodology.
**Alternatives:** assert result-equivalence against a live LLM — REJECTED (non-deterministic, paid, flaky — a bad
correctness gate; `testing.md` forbids flaky tests). **Rationale:** correctness of the MECHANISM (batching + push-down)
is deterministic and must be tested deterministically; the AI answer quality is a statistical benchmark, not a unit
assertion.

### D4 — Honest ceiling: composability/cost win with STATISTICAL accuracy, orthogonal to vector recall

**Decision:** the benchmark reports the round-trip/latency win of the pushed-down batched composition + (optional) the
LOTUS cascade's recall target + sample methodology; never "AI.IF is fast" without the quality point.
**Alternatives:** claim an accuracy or speed number without the sample methodology — REJECTED (Rule 5 / public-copy.md).
**Rationale:** M73/M97 discipline applied to the AI surface.

## Dependency Graph

```
Phase A (AI.IF set-oriented operator + deterministic model + result-equivalence vs per-row) ── gates ──▶ Phase B
Phase B (dependency-safe push-down: planner runs the cheap WHERE before the AI; EXPLAIN)     ── gates ──▶ Phase C
Phase C (3-axis cost hook + the composition benchmark: round-trips + latency, honest accuracy note) + ADR-0007 revisit
```

## Phase A — `AI.IF` set-oriented operator + result-equivalence (deterministic)

### Task A1 — `AI.IF(condition, col)` evaluates a batched boolean over a row-set == the per-row path

#### Why this step
The operator core: over N rows, build N prompts, ONE batched inference, N booleans — vs today's per-row round-trip.
Proven with a deterministic model so the test is not flaky/paid (D3), de-risking the mechanism before the planner
push-down.

#### Files to edit
- `theodb_rs/src/ai_op.rs` (NEW) — `ai_if(condition: &str, values: &[Option<&str>], model) -> Vec<Option<bool>>`
  (build N prompts `"{condition}: {value}? answer yes/no"`, call `ai_generate_batch` ONCE, parse yes/no → bool); a
  `theodb.llm_test_model` GUC path returning a deterministic answer (e.g., a local rule) so tests are hermetic.
- `theodb_rs/src/chat.rs` — a `test_model` hook in `chat()` that, when `theodb.llm_test_model` is set, answers
  deterministically without an HTTP call.

#### TDD
- RED: `test_ai_if_batched_equals_per_row` — with the deterministic model, `ai_if("is even", [1,2,3,4])` == the per-row
  `ai.generate` evaluated for each, AND uses ONE inference call (a call-counter asserts 1, not N). Fails before the op.
- GREEN: build prompts + `ai_generate_batch` + parse; the deterministic model hook.
- REFACTOR: a `Yes/No` parser shared with any future `sem_filter`.

#### Concurrency tests
`#### Concurrency tests` — (none — single-threaded) — the operator is single-backend; batching is in-process.

#### Failure scenarios
`## Failure scenarios` — the model returns a non-yes/no / wrong-length array → typed error (the `ai_generate_batch`
N-in/N-out guard); a NULL value in the set → NULL bool (SQL semantics), never a panic.

#### Acceptance criteria
- `AI.IF` over N rows == per-row for the deterministic model; exactly ONE inference call for N rows.

#### DoD
- `cargo pgrx test pg17 ai_if_batched` GREEN on the droplet.

## Phase B — Dependency-safe filter push-down

### Task B1 — the planner runs a cheap relational `WHERE` BEFORE the AI op (dependency-safe); EXPLAIN shows it

#### Why this step
The δ-rung rewrite: `WHERE cheap AND AI.IF(...)` must evaluate `cheap` first, shrinking the row-set the expensive AI
sees — but ONLY when `depends_on ∩ generated_fields = ∅` (D2).

#### Files to edit
- `theodb_rs/src/ai_op.rs` / a planner hook — represent `AI.IF` so the planner orders the cheap qual first (a
  set-function with a high cost, or a CustomScan; the simplest correct: the AI op is a set-returning function the
  planner already orders after cheap quals when its cost is declared high — validate via EXPLAIN).

#### TDD
- RED: `test_pushdown_runs_cheap_filter_first` — `SELECT ... WHERE id < 10 AND AI.IF('...', txt)` calls the AI op with
  ≤ 10 prompts (a call-arg counter), not the full table; EXPLAIN shows the cheap filter below the AI op. Fails before
  the cost/ordering is declared.
- GREEN: declare the AI op's cost high so the planner orders cheap quals first; verify dependency-safety.
- REFACTOR: the `depends_on ∩ generated_fields` check as a helper.

#### Concurrency tests
`#### Concurrency tests` — (none — single-threaded).

#### Failure scenarios
`## Failure scenarios` — a filter that DEPENDS on the AI op's output (not dependency-safe) → NOT pushed down (correct,
just not accelerated); tested by a dependent filter asserting the AI runs on the full set.

#### Acceptance criteria
- A cheap filter runs before the AI op (fewer AI prompts); a dependent filter does not (correct); EXPLAIN reflects it.

#### DoD
- `cargo pgrx test pg17 ai_pushdown` GREEN.

## Phase C — 3-axis cost hook + composition benchmark + ADR-0007 revisit

### Task C1 — 3-axis cost estimate + the measured composition benchmark

#### Why this step
The Palimpzest cost model (cost/time/quality + selectivity) lets the planner pick; the benchmark is the honest
measured artifact (round-trips + latency of the pushed-down batched composition vs per-row), with the accuracy
methodology.

#### Files to edit
- `theodb_rs/src/am/cost.rs` — a 3-axis AI-op cost estimate (calibrated from a sample; honest about naive-without-sample).
- `docs/benchmarks/m102-ai-operators.{md,json}` + a harness — round-trips (1 vs N) + latency of `WHERE cheap AND AI.IF`
  pushed-down vs the per-row path; if a real model is configured, report the recall target + sample methodology.
- `docs/adr/00NN-batched-ai-inference.md` — revisit ADR-0007 (per-row HTTP → batched).

#### TDD
- RED: the benchmark harness runs (deterministic model → round-trip counts; optional real model → latency); a
  result-equivalence cross-check gates it.
- GREEN: the cost hook + benchmark emit the artifact.
- REFACTOR: reproducibility (fixed seed, ≥ 3 runs where timing applies).

#### Failure scenarios
`## Failure scenarios` — no real model configured → the benchmark reports round-trip counts (deterministic) + notes
latency needs a live model; never fabricate an AI latency number.

#### Acceptance criteria
- The benchmark shows the round-trip win (1 vs N) + the push-down row-reduction; if a real model is configured, latency
  + the recall-target/sample methodology; honest ceiling (statistical accuracy, orthogonal to vector recall).

#### DoD
- `bash docs/benchmarks/... ` (or a pg harness) produces `docs/benchmarks/m102-ai-operators.{md,json}`; ADR written.

## Coverage Matrix

| Requirement (ROADMAP M102 DoD) | Task(s) |
|---|---|
| (1) AI.IF/ai.generate as a plan node (EXPLAIN shows + reorders) | A1, B1 |
| (2) cost hook 3-axis + dependency-safe push-down | B1, C1 |
| (3) result-equivalence vs the per-row function | A1 |
| (4) benchmark (push-down + optional cascade with recall-target) | C1 |
| (5) ADR revisiting ADR-0007 (batched inference) | C1 |
| (6) sign-off council-ai-in-db + council-security | Review phase |
| honest boundary (statistical accuracy, orthogonal to vector recall) | D4 (ADR) enforced in the benchmark note |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Cost model calibration needs sample telemetry (else naive estimate) | MEDIUM | honest: report the estimate as naive-without-sample; the sample-learned calibration is the ambitious tail | impl |
| Prompt-injection on the AI-operator surface | HIGH | council-security review mandatory; the prompt template quotes/escapes the value; the NL→SQL guards (nl.rs) are the precedent | impl |
| Real-AI testing is flaky/paid → a bad correctness gate | HIGH | D3: result-equivalence uses a deterministic test model; the real AI is the benchmark, not the unit assertion | impl |
| Batching changes ADR-0007 | MEDIUM | a new ADR revisits it (C1) | plan |
| Over-claiming "AI.IF is fast" without the quality point | MEDIUM | D4 + the benchmark reports the recall target + sample methodology (public-copy.md) | impl |

## Unresolved Questions

- **LOTUS proxy/oracle cascade (optional):** the sample-learned proxy→oracle cascade with a recall guarantee is the
  ambitious tail; slice-1 ships the batched operator + push-down + a naive cost, and the cascade is a follow-up
  (resolved at C1 by whether a sample-telemetry harness lands).
- **Real-model benchmark in CI:** the droplet has an `.env` key but CI does not run `cargo pgrx test`; the latency
  benchmark is a manual/droplet run with the key set (the deterministic round-trip count is CI-safe).

## Failure scenarios

- **Model returns a malformed batch** (A1) — the `ai_generate_batch` N-in/N-out guard → typed error, never a panic.
- **Non-dependency-safe filter** (B1) — NOT pushed down (correct, not accelerated).
- **No real model configured** (C1) — the benchmark reports deterministic round-trip counts; latency needs a live model
  (disclosed, never fabricated).
- **Prompt-injection** (review) — the value is quoted/escaped in the template; council-security signs off the surface.

## Global DoD

- All Phase A–C tasks' `cargo pgrx test pg17` GREEN on the droplet (result-equivalence + push-down, deterministic model).
- `docs/benchmarks/m102-ai-operators.{md,json}` present with measured numbers (round-trips + push-down; latency when a
  real model is configured), methodology, honest statistical-accuracy note.
- No callback panics across C; the AI surface has prompt-injection + SSRF guards.
- CHANGELOG `[Unreleased]` updated; no commits to main; no Co-Authored-By trailer; ADR-0007 revisit written.
- Files respect the ~500 LoC budget.
- Sign-off: council-ai-in-db + council-security (review phase).

## Final Phase — Integration Validation

- Full `cargo pgrx test pg17` suite GREEN (no regression on M99–M101 + the new M102 tests).
- The batched-operator result == the per-row path (deterministic model); the push-down EXPLAIN-visible.
- Benchmark artifact reproducible; honest ceiling stated (statistical accuracy, orthogonal to vector recall).
- council-ai-in-db + council-security review = READY_TO_MERGE before `/release`.
