# Implementation Summary — M7-S3 Generative-AI SQL functions

**Slug:** m7-ai-generative-functions
**Date:** 2026-06-28
**Plan:** `.claude/knowledge-base/plans/m7-ai-generative-functions-plan.md` (SHIPPABLE_WITH_CAVEATS 87.6)

## Delivered

Five scalar generative-AI SQL functions in the `ai` schema over a configurable OpenAI-compatible
chat-completions endpoint, with one private `ai._chat` HTTP helper (DRY) — `sql/50-theodb-ai.sql`:

| Function | Returns | Wiring triad |
|---|---|---|
| `ai.generate` | text | caller: smoke + tests; integration test; observable: contract result |
| `ai.if` | boolean | first-token parse (yes/no), fail-fast 22023 |
| `ai.analyze_sentiment` | text (pos/neg/neutral) | first-token label, fail-fast 22023 |
| `ai.summarize` | text | summarize system prompt routed |
| `ai.rank` | real | first-float parse, fail-fast 22023 |
| `ai._chat` (private) | text | single HTTP source of truth; REVOKE FROM PUBLIC |

All VOLATILE (LLM call). GUCs `theodb.llm_endpoint`/`llm_model`/`llm_api_key`. SSRF guard (http(s)-only,
no-redirect) + typed fail-fast (22023 / 38000) inherited from the M2 `theodb.embed` pattern. All six
REVOKE'd from PUBLIC.

## Test evidence (real, no mock — M2 precedent)

- **Offline contract suite (deterministic stub, CI):** 18 tests green — per-function happy + routing
  (generate vs summarize system prompt) + negatives (malformed→22023 ×3, endpoint-unset→22023, SSRF→22023,
  conn-refused→38000, empty-completion→38000, bad-shape→38000, NULL-prompt→22023, neutral label, explicit-no).
  `pytest -m integration tests/test_ai_sql.py -k 'not real'` → **18 passed**.
- **Real OpenAI end-to-end (opt-in, key from gitignored `.env`):** `test_real_openai_sentiment_polarity`
  ran against `https://api.openai.com/v1/chat/completions` (model `gpt-4o-mini`) on 2026-06-28 →
  **1 passed**. `ai.analyze_sentiment('I absolutely loved this, it was wonderful')` → `positive`;
  `ai.analyze_sentiment('This was awful, I hated every minute')` → `negative`. Polarity + label-set shape
  asserted (never exact text — LLM non-determinism). This is the recorded evidence for the CHANGELOG
  "polaridade verificada" claim (review HIGH cross-val).
- No regression: full suite 69 unit + 44 integration green; ruff + vulture clean.

## Review fixes applied (cycle-review NEEDS_FIXES → resolved)

- HIGH: `ai.generate`/`ai.summarize` STABLE → VOLATILE (verified `provolatile='v'` on all six).
- HIGH: real-OpenAI evidence recorded here (this file).
- MEDIUM: `ai.if`/`ai.analyze_sentiment` first-token parse (fixes "not …"→False + "positively awful"→positive misparse; D4 fail-fast preserved).
- MEDIUM: GRANT comment corrected (SECURITY INVOKER → grant `ai._chat` + wrapper together; never grant `_chat` to PUBLIC).
- MEDIUM: added tests for empty-completion (38000), bad-shape (38000), NULL-prompt (22023), neutral label, explicit-no, message-substring assertions.
- LOW: stale "não implementadas" banner in `docs/features/07` updated.

## Known follow-ups (honest, non-blocking)

- DRY: `ai._chat` duplicates `theodb.embed`'s SSRF/transport knowledge. Tracked: extract a shared
  `theodb._http_post_json` helper before the 3rd HTTP-calling function (touches `sql/30`, so a separate slice).
- `ai.hybrid_search_rrf` (M7-S1, released) is STABLE-over-`theodb.embed` — same volatility concern; note for a
  future hardening slice (released code, lower acuity since not called per-row).
- Array/cursor "accelerated" modes + the packaged `theodb_ml` extension surface — deferred per the plan (YAGNI).
