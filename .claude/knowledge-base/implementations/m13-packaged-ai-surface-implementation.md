# Implementation — M13 native packaged AI surface (features 06/07)

**Slug:** m13-packaged-ai-surface · **Milestone:** M13 · **Date:** 2026-06-28
**Plan:** `.claude/knowledge-base/plans/m13-packaged-ai-surface-plan.md` (plan-confidence SHIPPABLE 95.2)

## What shipped

Two literal packaged surfaces over existing capabilities (Rule 9 — no reinvention; `ai.hybrid_search_rrf` +
`ai._chat` UNCHANGED):

- **`ai.hybrid_search(config jsonb)`** (`sql/40`, additive) — the literal spec-06 JSON API; a THIN wrapper
  parsing `{table,id_col,content_tsv_col,vector_col,query_text,query_vector,k,per_leg_limit,result_limit}`
  and delegating to `ai.hybrid_search_rrf` (one fusion source of truth). Returns the same `TABLE(id, score)`.
  Honest sugar (a calling convention, not new fusion). Fail-fast 22023 on missing required keys.
- **`theodb_ml` model registry** (`sql/70`, NEW) — `theodb_ml.models(model_id, endpoint, model_name)` +
  `create_model` (http(s)-validated) / `drop_model` / `list_models` / `apply_model`. `apply_model` SETs the
  session GUCs `theodb.llm_endpoint`/`llm_model` that `ai._chat` already reads — bridging the registry to the
  unchanged chat path. The literal spec-07 `create_model` surface + a real capability (named multi-endpoint/
  model configs, per-session selection).
- All new functions `REVOKE ALL ... FROM PUBLIC`.

## SECURITY divergence (ADR D2) — keys are NEVER persisted

The registry stores **endpoint + model_name ONLY — there is NO `api_key` column**. Persisting API keys in a
table is a security regression (pg_dump, logical replication, base backups, SELECT-by-grantee). Keys remain
**session GUCs** (`theodb.llm_api_key`), set out of band per session — the same posture M7-S3 documents on
`ai._chat`. `apply_model` bridges endpoint+model; the key is set separately by the caller. This deliberately
diverges from the literal AlloyDB `create_model` (which stores credentials). Asserted by a test
(`test_theodb_ml_registry_has_no_api_key_column` → 0 `%key%` columns).

## Evidence (real, no mock)

### Literal hybrid JSON surface (sugar — parity, not new fusion)
- `test_hybrid_search_json_matches_rrf` — `ai.hybrid_search(jsonb)` returns **identical rows** to
  `ai.hybrid_search_rrf` for the same config. PASS.
- `test_hybrid_search_json_missing_keys_raises` — missing required keys → 22023. PASS.

### Registry → ai._chat bridge (real capability)
- `test_theodb_ml_create_apply_drives_generate` — `create_model` + `apply_model` SETs `theodb.llm_endpoint`
  (asserted via `current_setting`) and a subsequent `ai.generate` works using the applied endpoint. PASS.
- `test_theodb_ml_drop_and_list`, `_rejects_non_http_endpoint` (SSRF guard), `_apply_unknown_model_raises`
  (22023). PASS.
- `test_theodb_ml_functions_revoked_from_public` — all 4 registry fns + `ai.hybrid_search` non-public. PASS.

### Security
- `test_theodb_ml_registry_has_no_api_key_column` — **0** `%key%` columns in `theodb_ml.models`. PASS.

### Real OpenAI (opt-in, gpt-4o-mini, key from gitignored `.env`)
- `test_real_openai_theodb_ml_apply_model` — **1 passed**.
- Captured: `theodb_ml.create_model('openai', <endpoint>, 'gpt-4o-mini')` → `theodb_ml.apply_model('openai')`
  → `ai.generate('Reply with the single word: ok')`:

  > **→ `'ok'`** — a registered model, applied via the registry, drove a real `ai.generate` against gpt-4o-mini
  > (endpoint resolved from the registry; key set as a session GUC, never persisted).

### No regression / capabilities unchanged
- 103 offline tests pass; `sql/50-theodb-ai.sql` (`ai._chat`) **unchanged**; `sql/40` change is the additive
  wrapper only (`ai.hybrid_search_rrf` untouched); idempotent re-apply of `sql/70` (twice, exit 0); ruff clean.

## Honest framing (sugar vs capability)

- `ai.hybrid_search(jsonb)` is **sugar** — a JSON entry over the existing RRF (ADR D1).
- `theodb_ml` is a **real capability** (named multi-endpoint/model registry + per-session selection), minus
  key persistence (ADR D2). The literal AlloyDB `create_model` credential storage is deliberately diverged.
