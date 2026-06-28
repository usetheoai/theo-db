-- TheoDB M7-S3 — Generative-AI SQL functions over a CONFIGURABLE chat-completions endpoint.
-- Mirrors AlloyDB's ai.generate / google_ml_integration, but model-agnostic: the database calls a
-- configurable OpenAI-compatible chat endpoint (it does NOT ship a model). Extends the M2 theodb.embed
-- pattern (sql/30) from the embeddings endpoint to the chat-completions endpoint — same GUC mechanism,
-- same plpython3u, same SSRF/no-redirect/fail-fast discipline.
--   SET theodb.llm_endpoint = 'https://api.openai.com/v1/chat/completions';  -- required
--   SET theodb.llm_model    = 'gpt-4o-mini';                                 -- optional default model
--   SET theodb.llm_api_key  = '...';                                         -- optional bearer token
-- One private helper (ai._chat) is the single HTTP source of truth; the five public functions are thin
-- task-specific wrappers (D1). Idempotent: safe to re-run / load from docker-entrypoint-initdb.d.

CREATE EXTENSION IF NOT EXISTS plpython3u;
CREATE SCHEMA IF NOT EXISTS ai;

-- ai._chat — PRIVATE: one configurable chat-completions round-trip + parse. Not granted to PUBLIC.
CREATE OR REPLACE FUNCTION ai._chat(prompt text, system text DEFAULT NULL, model text DEFAULT NULL)
RETURNS text
LANGUAGE plpython3u
AS $$
import json
import urllib.request
import urllib.error

def _cfg(name):
    # name is a trusted literal (no user input) — current_setting(..., true) returns NULL if unset.
    return plpy.execute("SELECT current_setting('%s', true) AS v" % name)[0]["v"]

if prompt is None:
    plpy.error("ai._chat: prompt must not be NULL", sqlstate="22023")

endpoint = _cfg("theodb.llm_endpoint")
if not endpoint:
    plpy.error(
        "ai._chat: theodb.llm_endpoint is not set — "
        "SET theodb.llm_endpoint = 'https://host/v1/chat/completions'",
        sqlstate="22023",
    )

# SSRF hardening: only http(s); refuse file://, ftp://, gopher://, etc. (the GUC is session-settable).
if not (endpoint.startswith("http://") or endpoint.startswith("https://")):
    plpy.error("ai._chat: endpoint must be http(s)://", sqlstate="22023")

mdl = model or _cfg("theodb.llm_model") or "default"
api_key = _cfg("theodb.llm_api_key")

messages = []
if system:
    messages.append({"role": "system", "content": system})
messages.append({"role": "user", "content": prompt})

payload = json.dumps({"model": mdl, "messages": messages}).encode()
req = urllib.request.Request(endpoint, data=payload, method="POST")
req.add_header("Content-Type", "application/json")
if api_key:
    req.add_header("Authorization", "Bearer " + api_key)

# Do NOT follow redirects — a 30x to 169.254.169.254 (cloud metadata) or an internal host would be SSRF.
class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None

opener = urllib.request.build_opener(_NoRedirect)
try:
    with opener.open(req, timeout=30) as resp:
        body = json.loads(resp.read())
# URLError = DNS/refused/5xx; OSError = timeout; ValueError = JSONDecodeError on a non-JSON 200.
# All fail loud + typed — never a silent NULL (Rule 8). api_key is NEVER included in the error text.
except (urllib.error.URLError, OSError, ValueError) as e:
    plpy.error("ai._chat: chat endpoint call failed: %s" % e, sqlstate="38000")

try:
    content = body["choices"][0]["message"]["content"]
except (KeyError, IndexError, TypeError) as e:
    plpy.error(
        "ai._chat: unexpected chat response shape (%s): %s" % (e, str(body)[:200]),
        sqlstate="38000",
    )

if content is None or content == "":
    plpy.error("ai._chat: endpoint returned an empty completion", sqlstate="38000")

return content
$$;

-- ai.generate — raw text completion.
-- VOLATILE (default): an LLM call is non-deterministic + has side effects (network/cost) — STABLE would
-- let the planner fold/hoist calls (e.g. one call broadcast over N rows), contradicting "per row".
CREATE OR REPLACE FUNCTION ai.generate(prompt text, model text DEFAULT NULL)
RETURNS text
LANGUAGE sql
VOLATILE
AS $$
    SELECT ai._chat(prompt, NULL, model);
$$;

-- ai.if — natural-language condition -> boolean. Output-constrained system prompt + fail-fast parse (D4).
CREATE OR REPLACE FUNCTION ai.if(prompt text, model text DEFAULT NULL)
RETURNS boolean
LANGUAGE plpython3u
AS $$
out = plpy.execute(
    plpy.prepare("SELECT ai._chat($1, $2, $3) AS v", ["text", "text", "text"]),
    [prompt, "Answer with exactly one word: yes or no.", model],
)[0]["v"]
import re
# Match on the FIRST token, not startswith — else "not sure"/"nope" would wrongly hit the "no" prefix
# and silently return False (D4 says: unparseable → fail-fast, never guess).
first = re.split(r"[^a-z0-9]+", (out or "").strip().lower(), maxsplit=1)[0]
if first in ("yes", "true", "1", "y", "t"):
    return True
if first in ("no", "false", "0", "n", "f"):
    return False
plpy.error("ai.if: unparseable boolean from model: %s" % (out or "")[:50], sqlstate="22023")
$$;

-- ai.analyze_sentiment — content -> one of {positive, negative, neutral}. Fail-fast on out-of-set (D4).
CREATE OR REPLACE FUNCTION ai.analyze_sentiment(content text, model text DEFAULT NULL)
RETURNS text
LANGUAGE plpython3u
AS $$
out = plpy.execute(
    plpy.prepare("SELECT ai._chat($1, $2, $3) AS v", ["text", "text", "text"]),
    [content, "Classify the sentiment of the text. Reply with exactly one of: positive, negative, neutral.", model],
)[0]["v"]
import re
# First-token match (not startswith) so "positively awful" can't be misread as "positive".
first = re.split(r"[^a-z]+", (out or "").strip().lower(), maxsplit=1)[0]
if first in ("positive", "negative", "neutral"):
    return first
plpy.error("ai.analyze_sentiment: model did not return a known label: %s" % (out or "")[:50], sqlstate="22023")
$$;

-- ai.summarize — content -> concise summary text. VOLATILE (LLM call — see ai.generate note).
CREATE OR REPLACE FUNCTION ai.summarize(content text, model text DEFAULT NULL)
RETURNS text
LANGUAGE sql
VOLATILE
AS $$
    SELECT ai._chat(content, 'Summarize the following text concisely in 1-2 sentences.', model);
$$;

-- ai.rank — natural-language scoring -> real. Parses the first float; fail-fast if none (D4).
CREATE OR REPLACE FUNCTION ai.rank(prompt text, model text DEFAULT NULL)
RETURNS real
LANGUAGE plpython3u
AS $$
import re
out = plpy.execute(
    plpy.prepare("SELECT ai._chat($1, $2, $3) AS v", ["text", "text", "text"]),
    [prompt, "Reply with a single number between 0 and 1 and nothing else.", model],
)[0]["v"]
m = re.search(r"-?\d+(?:\.\d+)?", out or "")
if not m:
    plpy.error("ai.rank: model did not return a number: %s" % (out or "")[:50], sqlstate="22023")
return float(m.group(0))
$$;

-- ai.agg_summarize — AGGREGATE: collapse many rows into a single summary (feature 11, aggregate path).
-- Composed from ai._chat (Rule 9 — no reinvention). sfunc is a pure-SQL newline-join (NULL/empty-skipping);
-- finalfunc makes ONE ai._chat call on the accumulation, bounded to 12000 chars for cost/token safety
-- (map-reduce for larger groups is deferred — YAGNI). Empty / all-NULL / all-empty group -> NULL (no LLM call).
-- ORDER DEPENDENCE: a plain aggregate has indeterminate input order, so the concatenation (and thus the
-- summary) is not reproducible across runs unless the caller pins it: `ai.agg_summarize(x ORDER BY <key>)`.
-- SECURITY INVOKER (the caller needs EXECUTE on ai._chat too — see the grant note).
-- VOLATILITY: like EVERY PostgreSQL aggregate (string_agg/array_agg/sum are all IMMUTABLE; none can be
-- VOLATILE — there is no syntax for it and PG does not derive it from member fns), ai.agg_summarize's own
-- pg_proc row is provolatile='i'. That is NOT a footgun: the paid, non-deterministic LLM call lives in the
-- VOLATILE finalfunc (ai._agg_summ_final), which the executor re-runs per query — aggregates are never
-- constant-folded (they require input rows). The transition fn below is honestly IMMUTABLE (pure concat).
CREATE OR REPLACE FUNCTION ai._agg_summ_accum(state text, item text)
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

CREATE OR REPLACE FUNCTION ai._agg_summ_final(state text)
RETURNS text
LANGUAGE sql
VOLATILE
AS $$
    SELECT CASE
        WHEN state IS NULL THEN NULL
        ELSE ai._chat(
            left(state, 12000),
            'Summarize the following collected texts into a single concise summary (1-3 sentences).',
            NULL)
    END;
$$;

-- No CREATE OR REPLACE AGGREGATE exists; DROP-then-CREATE keeps re-applying this file idempotent.
DROP AGGREGATE IF EXISTS ai.agg_summarize(text);
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

-- Least-privilege: these functions make server-side outbound HTTP (the configured endpoint), so they are
-- NOT granted to PUBLIC (same posture as theodb.embed).
-- IMPORTANT (SECURITY INVOKER): the public wrappers run as the CALLER, and each one calls ai._chat as the
-- caller too — so a role needs EXECUTE on ai._chat *in addition to* the wrapper it uses. Grant BOTH:
--   GRANT EXECUTE ON FUNCTION ai._chat(text,text,text), ai.generate(text,text), ai.if(text,text),
--     ai.analyze_sentiment(text,text), ai.summarize(text,text), ai.rank(text,text) TO <role>;
-- Do NOT grant ai._chat to PUBLIC to "fix" a permission error — that re-opens outbound HTTP to every role.
REVOKE ALL ON FUNCTION ai._chat(text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.generate(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.if(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.analyze_sentiment(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.summarize(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.rank(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai._agg_summ_accum(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai._agg_summ_final(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.agg_summarize(text) FROM PUBLIC;

COMMENT ON FUNCTION ai._chat(text, text, text) IS
  'PRIVATE: one configurable chat-completions round-trip (theodb.llm_endpoint, http(s)-only, no redirects) '
  '+ parse of choices[0].message.content. Single HTTP source of truth for the public ai.* functions. '
  'Not granted to PUBLIC. theodb.llm_api_key is a session GUC (visible to SHOW / log_statement) — set it '
  'per-session out of band, not in logged DDL. SYNCHRONOUS: one blocking HTTP round-trip per call.';
COMMENT ON FUNCTION ai.generate(text, text) IS
  'Generate text from a prompt via the configurable model endpoint (theodb.llm_*). SYNCHRONOUS per row.';
COMMENT ON FUNCTION ai.if(text, text) IS
  'Evaluate a natural-language condition -> boolean via the configurable model. Fail-fast (22023) on '
  'unparseable output.';
COMMENT ON FUNCTION ai.analyze_sentiment(text, text) IS
  'Classify sentiment -> positive/negative/neutral via the configurable model. Fail-fast (22023) on '
  'out-of-set output.';
COMMENT ON FUNCTION ai.summarize(text, text) IS
  'Summarize content concisely via the configurable model. SYNCHRONOUS per row.';
COMMENT ON FUNCTION ai.rank(text, text) IS
  'Score a prompt -> real via the configurable model. Fail-fast (22023) if no number is returned.';
