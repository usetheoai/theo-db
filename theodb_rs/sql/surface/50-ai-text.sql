-- TheoDB M7-S3 / M18 — Generative-AI SQL surface over a CONFIGURABLE chat-completions endpoint.
-- Mirrors AlloyDB's ai.generate / google_ml_integration, but model-agnostic: the database calls a
-- configurable OpenAI-compatible chat endpoint (it does NOT ship a model). Same GUC mechanism as embed:
--   SET theodb.llm_endpoint = 'https://api.openai.com/v1/chat/completions';  -- required
--   SET theodb.llm_model    = 'gpt-4o-mini';                                 -- optional default model
--   SET theodb.llm_api_key  = '...';                                         -- optional bearer token
--
-- M18 (ROADMAP-v2): the single HTTP source of truth `ai._chat` and the task wrappers `ai.if` /
-- `ai.analyze_sentiment` / `ai.rank` / `ai.generate_batch` are now implemented in Rust by the `theodb_rs`
-- extension (theodb_rs/src/chat.rs) — NOT here (no more plpython3u in this file). This file keeps the thin
-- SQL keepers (`ai.generate`, `ai.summarize`) and the `ai.agg_summarize` aggregate, which call `ai._chat`
-- by name. B-030: ESTE ARQUIVO PASSOU A SER PARTE DO theodb_rs (extension_sql_file! em src/surface.rs,
-- com `requires = ["theodb_ai_wrappers"]`), então a ordem contra `ai._chat` é garantida e os corpos
-- voltaram a ser LANGUAGE sql — validados em tempo de CREATE, não no primeiro uso.
-- Idempotent: safe to re-run / load from docker-entrypoint-initdb.d.

-- M18: ai._chat + the generative wrappers are Rust (theodb_rs); plpython3u dropped from requires in M19. Not created here.

-- ai.generate — raw text completion. LANGUAGE sql: o corpo É validado contra ai._chat no CREATE (B-030).
-- VOLATILE: uma chamada de LLM é não-determinística e tem efeito colateral (rede/custo) — STABLE deixaria
-- o planejador dobrar ou içar uma chamada sobre N linhas.
CREATE FUNCTION ai.generate(prompt text, model text DEFAULT NULL)
RETURNS text
LANGUAGE sql
VOLATILE
AS $$ SELECT ai._chat(prompt, NULL, model) $$;

-- ai.summarize — content -> concise summary text. LANGUAGE sql, validada no CREATE; VOLATILE (chamada LLM).
CREATE FUNCTION ai.summarize(content text, model text DEFAULT NULL)
RETURNS text
LANGUAGE sql
VOLATILE
AS $$ SELECT ai._chat(content, 'Summarize the following text concisely in 1-2 sentences.', model) $$;

-- ai.agg_summarize — AGGREGATE: collapse many rows into a single summary (feature 11, aggregate path).
-- Composed from ai._chat (Rule 9 — no reinvention). sfunc is a pure-SQL newline-join (NULL/empty-skipping);
-- finalfunc makes ONE ai._chat call on the accumulation, bounded to 12000 chars for cost/token safety
-- (map-reduce for larger groups is deferred — YAGNI). Empty / all-NULL / all-empty group -> NULL (no LLM call).
-- ORDER DEPENDENCE: a plain aggregate has indeterminate input order, so the concatenation (and thus the
-- summary) is not reproducible across runs unless the caller pins it: `ai.agg_summarize(x ORDER BY <key>)`.
-- VOLATILITY: like EVERY PostgreSQL aggregate, ai.agg_summarize's own pg_proc row is provolatile='i'. That is
-- NOT a footgun: the paid, non-deterministic LLM call lives in the VOLATILE finalfunc (ai._agg_summ_final),
-- which the executor re-runs per query — aggregates are never constant-folded. The transition fn is IMMUTABLE.
CREATE FUNCTION ai._agg_summ_accum(state text, item text)
RETURNS text
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE
        WHEN item IS NULL OR item = '' THEN state
        WHEN state IS NULL THEN item
        ELSE state || E'\n' || item
    END;
$$;

-- finalfunc: LANGUAGE sql (o IF virou CASE, mesmo comportamento) + VOLATILE (a chamada LLM).
-- Grupo vazio / todo-NULL -> NULL, sem chamada.
CREATE FUNCTION ai._agg_summ_final(state text)
RETURNS text
LANGUAGE sql
VOLATILE
AS $$
    SELECT CASE WHEN state IS NULL THEN NULL ELSE ai._chat(
        left(state, 12000),
        'Summarize the following collected texts into a single concise summary (1-3 sentences).',
        NULL) END
$$;

CREATE AGGREGATE ai.agg_summarize(text) (
    sfunc = ai._agg_summ_accum,
    stype = text,
    finalfunc = ai._agg_summ_final
);

COMMENT ON AGGREGATE ai.agg_summarize(text) IS
  'Collapse many rows into one summary via ai._chat. ONE synchronous LLM call per group; cost scales with '
  'the number of groups (not row-capped) — caller controls grouping. Per-group input capped at 12000 chars '
  '(map-reduce deferred). Empty/all-NULL/all-empty group -> NULL (no call). Input order is indeterminate '
  'unless pinned: ai.agg_summarize(x ORDER BY <key>). Like all PG aggregates its pg_proc is provolatile=i; '
  'the non-deterministic LLM call lives in the VOLATILE finalfunc (re-run per query). REVOKE FROM PUBLIC '
  '(needs ai._chat EXECUTE).';

-- Least-privilege: these functions make server-side outbound HTTP (via ai._chat), so they are NOT granted to
-- PUBLIC (same posture as theodb.embed). The Rust ai._chat / ai.if / ai.analyze_sentiment / ai.rank /
-- ai.generate_batch carry their own REVOKE in theodb_rs (lib.rs). IMPORTANT (SECURITY INVOKER): the public
-- wrappers run as the CALLER and each calls ai._chat as the caller too — a role needs EXECUTE on ai._chat
-- *in addition to* the wrapper it uses:
--   GRANT EXECUTE ON FUNCTION ai._chat(text,text,text), ai.generate(text,text), ai.if(text,text),
--     ai.analyze_sentiment(text,text), ai.summarize(text,text), ai.rank(text,text),
--     ai.generate_batch(text[],text), ai.agg_summarize(text) TO <role>;
-- Do NOT grant ai._chat to PUBLIC to "fix" a permission error — that re-opens outbound HTTP to every role.
REVOKE ALL ON FUNCTION ai.generate(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.summarize(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai._agg_summ_accum(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai._agg_summ_final(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.agg_summarize(text) FROM PUBLIC;

COMMENT ON FUNCTION ai.generate(text, text) IS
  'Generate text from a prompt via the configurable model endpoint (theodb.llm_*). SYNCHRONOUS per row.';
COMMENT ON FUNCTION ai.summarize(text, text) IS
  'Summarize content concisely via the configurable model. SYNCHRONOUS per row.';
