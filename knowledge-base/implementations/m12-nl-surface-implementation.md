# Implementation — M12 `theodb_ai_nl` config surface (feature 12)

**Slug:** m12-nl-surface · **Milestone:** M12 · **Date:** 2026-06-28
**Plan:** `.claude/knowledge-base/plans/m12-nl-surface-plan.md` (plan-confidence SHIPPABLE 97.6)

## What shipped

The `theodb_ai_nl` **config surface** in schema `ai`, composed over the UNCHANGED M7-S4 NL gate
(`sql/60` not modified — Rule 9). New file `sql/61-theodb-nl-config.sql` (+ Dockerfile COPY after 60):

- **3 tables:** `ai.nl_config` (config_id → allowed_relations, schema_context, template, model),
  `ai.nl_templates` (named enable-able prompt blocks), `ai.nl_value_index` (categorical column → distinct values).
- **Management fns:** `ai.nl_add_config`, `ai.nl_add_template`, `ai.nl_set_template_enabled`,
  `ai.nl_set_value_index` (explicit, zero dynamic SQL), `ai.nl_refresh_value_index` (auto-populate, D3-guarded).
- **`ai.nl_query_cfg(question, config_id, max_rows)`** — loads the config, builds a prompt enrichment block
  (schema_context + enabled template + value-index hints), prepends it to the question, and delegates to the
  **unchanged `ai.nl_query`** with the config's allowed_relations. The deterministic L2/L4/L3 gate runs on
  every query — the config enriches the prompt ONLY, it never relaxes the anti-injection defense (ADR D2).
- All new functions `REVOKE ALL ... FROM PUBLIC`.

Honest divergence (ADR D1): we ship the 3 core capabilities in schema `ai`, NOT the literal 58-function
AlloyDB `theodb_ai_nl` extension (auto-template-from-history / concept-types / etc. are YAGNI, deferred).

## Evidence (real, no mock)

### Security PRESERVED through the config (the headline risk)
- `test_nl_query_cfg_injection_blocked_and_db_intact` — `__NLINJECT_DROP__` through `ai.nl_query_cfg` →
  `22023` (gate rejects) AND `documents` row count unchanged (2). **The config layer did not weaken the gate.** PASS.
- `test_nl_query_cfg_benign_returns_rows` — config drives a benign query → `[{"n": 2}]`. PASS.
- `test_nl_query_cfg_config_not_found_raises` — missing config → `22023`. PASS.

### Value-index (D3 guard)
- `test_nl_refresh_value_index_populates_from_data` — refresh `documents.content` → 2 distinct values stored. PASS.
- `test_nl_refresh_value_index_rejects_non_allowlisted_relation` — refresh over `secret` (not in cfg1
  allowlist) → `22023` (no arbitrary-table read). PASS.
- `test_nl_refresh_value_index_rejects_bad_column_identifier` — `content; DROP` column → `22023`. PASS.

### Least-privilege
- `test_nl_config_functions_revoked_from_public` — all 6 new fns `has_function_privilege('public',…)=f`. PASS.

### Real OpenAI (opt-in, gpt-4o-mini, key from gitignored `.env`)
- `test_real_openai_nl_query_cfg` — **1 passed**.
- Captured: a registered config (`rcfg` over `documents`) driving `ai.nl_query_cfg('how many documents are there?', 'rcfg')`:

  > **→ `[{'count': 3}]`** — the real model generated a `SELECT count(*) FROM documents`, the gate validated
  > it against the config's allowed_relations, and it executed read-only over the 3 seeded rows.

### No regression / gate untouched
- Full offline nl suite: **29 passed**; `sql/60-theodb-nl.sql` **unchanged** (`git diff` count 0); idempotent
  re-apply of `sql/61` (twice, exit 0).

## Gates

- REVOKE FROM PUBLIC verified for all 6 new functions (`f`).
- The M7-S4 gate (`sql/60`) is byte-unchanged — security preserved by construction.
- No new dependency (Rule 9); the value-index refresh runs a fixed-shape read with `quote_ident` + `::regclass`
  over an operator-allowlisted relation (no user SQL executed).

## Known limits (honest)

- The literal AlloyDB `theodb_ai_nl` function names + the auto-template-from-history / concept-types /
  fragments surface (spec steps ~23-58) are deferred (YAGNI — ADR D1/Q1/Q2). The three core capabilities
  (config, templates, value-index) close the M12 DoD.
