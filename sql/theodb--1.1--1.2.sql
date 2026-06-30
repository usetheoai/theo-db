-- TheoDB umbrella extension — upgrade 1.1 -> 1.2.
-- M18 (ROADMAP-v2): the generative ai.* surface (`ai._chat` + `ai.if` / `ai.analyze_sentiment` / `ai.rank`
-- / `ai.generate_batch`) moved from plpython3u (sql/50) to the Rust `theodb_rs` extension. This delta:
--   1. Re-defines the SQL keepers (`ai.generate`, `ai.summarize`, `ai._agg_summ_final`) as plpgsql so their
--      bodies are late-bound to `ai._chat` (which now lives in theodb_rs, created after theodb) AND so they no
--      longer hold a LANGUAGE sql pg_depend on `ai._chat` — that dependency would otherwise block the DROP.
--   2. Conditionally retires the legacy plpython3u `ai.*` so adding the Rust theodb_rs `ai.*` does not clash
--      on `CREATE FUNCTION` — ONLY when the function is genuinely plpython3u AND not a theodb_rs member.
-- On a fresh 1.2 install the keepers are already plpgsql (sql/50 base) and there is no plpython3u ai.* to drop,
-- so step 2 is a no-op. Mirrors the M17 1.0->1.1 embed retirement idiom.

-- 1. Late-bind the SQL keepers (idempotent; matches the 1.2 base) — removes the sql-dependency on ai._chat.
CREATE OR REPLACE FUNCTION ai.generate(prompt text, model text DEFAULT NULL)
RETURNS text LANGUAGE plpgsql VOLATILE
AS $$ BEGIN RETURN ai._chat(prompt, NULL, model); END; $$;

CREATE OR REPLACE FUNCTION ai.summarize(content text, model text DEFAULT NULL)
RETURNS text LANGUAGE plpgsql VOLATILE
AS $$ BEGIN RETURN ai._chat(content, 'Summarize the following text concisely in 1-2 sentences.', model); END; $$;

CREATE OR REPLACE FUNCTION ai._agg_summ_final(state text)
RETURNS text LANGUAGE plpgsql VOLATILE
AS $$
BEGIN
    IF state IS NULL THEN
        RETURN NULL;
    END IF;
    RETURN ai._chat(left(state, 12000),
        'Summarize the following collected texts into a single concise summary (1-3 sentences).', NULL);
END;
$$;

-- 2. Conditionally retire the legacy plpython3u ai.* (guarded: plpython3u AND NOT a theodb_rs member).
DO $$
DECLARE r record;
BEGIN
  FOR r IN
    SELECT p.oid::regprocedure AS sig
    FROM pg_proc p
    JOIN pg_language l ON l.oid = p.prolang
    WHERE p.pronamespace = 'ai'::regnamespace
      AND p.proname IN ('_chat', 'if', 'analyze_sentiment', 'rank', 'generate_batch')
      AND l.lanname = 'plpython3u'
      AND NOT EXISTS (
        SELECT 1 FROM pg_depend d
        JOIN pg_extension e ON e.oid = d.refobjid
        WHERE d.objid = p.oid AND d.deptype = 'e' AND e.extname = 'theodb_rs')
  LOOP
    EXECUTE format('DROP FUNCTION %s', r.sig);
    RAISE NOTICE 'theodb 1.1->1.2: retired the legacy plpython3u %', r.sig;
  END LOOP;
END
$$;
