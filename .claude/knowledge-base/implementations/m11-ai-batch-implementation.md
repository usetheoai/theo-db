# Implementation — M11 `ai.generate_batch` (accelerated batch AI, feature 08)

**Slug:** m11-ai-batch · **Milestone:** M11 · **Date:** 2026-06-28
**Plan:** `.claude/knowledge-base/plans/m11-ai-batch-plan.md` (plan-confidence SHIPPABLE 90.4)

## What shipped

`ai.generate_batch(prompts text[], model text DEFAULT NULL) -> text[]` — answers N prompts in **ONE**
`ai._chat` round-trip (Rule 9 — composed from the shipped helper, no new dependency):

- Packs N numbered prompts into a single request; system prompt asks for ONLY a JSON array of exactly N
  strings; parses + **validates `len == N`** (strips an optional ```json fence); typed `22023` on invalid
  JSON / wrong length / NULL element / NULL array.
- **Empty array → empty array, NO LLM call** (cost safety).
- `VOLATILE`; `REVOKE ALL ... FROM PUBLIC` (parity with scalar `ai.*`); baked via initdb.d.

Stub (`tools/chat_server.py`): thread-safe request counter + `GET /count` + a JSON-array reply branch
(sizes the array from the numbered items; `__wronglen__` seam returns N-1 to exercise the fail-fast).

Files: `sql/50-theodb-ai.sql`, `tools/chat_server.py`, `benchmarks/tests/test_ai_sql.py`,
`docs/sql-ai-functions.md`, `smoke.sh`, `CHANGELOG.md`.

## Evidence (real, no mock)

### The acceleration is MEASURED (round-trip count, not an unbenchmarked latency claim)
- `test_generate_batch_one_roundtrip` — a batch of 3 prompts bumps the stub `/count` by **exactly 1** and
  returns 3 answers in order. PASS.
- `test_scalar_generate_is_n_roundtrips` — 3 scalar `ai.generate` calls bump `/count` by **3**. PASS.
  → **N round-trips → 1** is the measured win (Rule 5: claim ⇒ measurement).

### Negatives / edge (typed, fail-fast)
- empty array → `[]`, `/count` delta `0` (no call); NULL element → `22023`; wrong-length reply
  (`__wronglen__`) → `22023`. PASS.

### Concurrency (the counter underpinning the measurement)
- `test_stub_counter_is_threadsafe` — 50 parallel chat requests bump the counter by **exactly 50**
  (Lock prevents lost updates). PASS. Atomic-counter invariant: every accepted request increments once.

### Real OpenAI (opt-in, gpt-4o-mini, key from gitignored `.env`)
- `test_real_openai_generate_batch_shape` — **1 passed**.
- Captured real output for `ai.generate_batch(ARRAY['Capital of France? one word','2+2? a number only','Opposite of hot? one word'])`:

  > **→ `['Paris', '4', 'cold']`** — 3 correct answers, in order, in ONE request to the real model.

### No regression
- Full offline `ai.*` suite: **29 passed**; idempotent re-apply of `sql/50` (twice, exit 0); ruff clean.

## Gates

- REVOKE FROM PUBLIC verified: `has_function_privilege('public','ai.generate_batch(text[],text)','execute')` → `f`.
- No new dependency (Rule 9); scalar `ai.*` + `ai._chat` + `ai.agg_summarize` unchanged (backward compatible).
- No unbenchmarked perf claim (Rule 5) — acceleration stated as the measured round-trip reduction only.

## Known limits (honest)

- Best-effort: a model returning invalid JSON or the wrong length fails fast (`22023`); for a guaranteed
  per-item result use the scalar `ai.generate`.
- One large request can hit the token limit — caller chunks (auto-chunk deferred, YAGNI).
- Only `ai.generate` is batched this slice (if/rank/sentiment batching deferred — YAGNI).
