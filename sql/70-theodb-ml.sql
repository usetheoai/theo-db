-- TheoDB M13 — theodb_ml model registry (literal spec-07 packaged surface) over the unchanged ai._chat.
-- Register named (endpoint, model_name) configs and select one per session with theodb_ml.apply_model, which
-- SETs the session GUCs that ai._chat already reads (theodb.llm_endpoint / theodb.llm_model). ai._chat is
-- REUSED UNCHANGED — this file never modifies it.
--
-- SECURITY DIVERGENCE (ADR D2): the registry stores endpoint + model_name ONLY. There is NO api_key column.
-- Persisting API keys in a table would be a security regression (pg_dump, logical replication, base backups,
-- SELECT-by-grantee). Keys remain SESSION GUCs (theodb.llm_api_key), set out of band per session — the same
-- posture M7-S3 documents on ai._chat. apply_model bridges endpoint+model to ai._chat; the key is set
-- separately by the caller. This deliberately diverges from the literal AlloyDB create_model (which stores
-- credentials).
-- Idempotent: safe to re-run / load from docker-entrypoint-initdb.d AFTER sql/50.

CREATE SCHEMA IF NOT EXISTS theodb_ml;

-- models — named endpoint+model configs. NO api_key column (keys stay session GUCs — see SECURITY above).
CREATE TABLE IF NOT EXISTS theodb_ml.models (
    model_id   text PRIMARY KEY,
    endpoint   text NOT NULL,
    model_name text,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- theodb_ml.create_model — register/upsert a named model. The endpoint must be http(s) (SSRF-consistent with
-- ai._chat, which re-validates at call time). No credentials are accepted or stored.
CREATE OR REPLACE FUNCTION theodb_ml.create_model(p_id text, p_endpoint text, p_model_name text DEFAULT NULL)
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    IF p_id IS NULL OR p_id = '' THEN
        RAISE EXCEPTION 'theodb_ml.create_model: model_id must not be empty' USING ERRCODE = '22023';
    END IF;
    IF p_endpoint IS NULL OR p_endpoint !~ '^https?://' THEN
        RAISE EXCEPTION 'theodb_ml.create_model: endpoint must be http(s):// (got %)', p_endpoint
            USING ERRCODE = '22023';
    END IF;
    INSERT INTO theodb_ml.models (model_id, endpoint, model_name)
        VALUES (p_id, p_endpoint, p_model_name)
        ON CONFLICT (model_id) DO UPDATE
            SET endpoint = EXCLUDED.endpoint, model_name = EXCLUDED.model_name;
END $$;

-- theodb_ml.drop_model — remove a registered model.
CREATE OR REPLACE FUNCTION theodb_ml.drop_model(p_id text)
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    DELETE FROM theodb_ml.models WHERE model_id = p_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'theodb_ml.drop_model: model % not found', p_id USING ERRCODE = '22023';
    END IF;
END $$;

-- theodb_ml.list_models — enumerate registered models (no secrets to hide; endpoint+model only).
CREATE OR REPLACE FUNCTION theodb_ml.list_models()
RETURNS TABLE(model_id text, endpoint text, model_name text)
LANGUAGE sql
STABLE
AS $$
    SELECT model_id, endpoint, model_name FROM theodb_ml.models ORDER BY model_id;
$$;

-- theodb_ml.apply_model — select a registered model for THIS session by SETting the GUCs ai._chat reads
-- (theodb.llm_endpoint / theodb.llm_model). Does NOT touch ai._chat. The API key is NOT set here — the caller
-- supplies it separately (SET theodb.llm_api_key = ...), per session (keys are never persisted — ADR D2).
CREATE OR REPLACE FUNCTION theodb_ml.apply_model(p_id text)
RETURNS void
LANGUAGE plpgsql
AS $$
DECLARE
    m theodb_ml.models;
BEGIN
    SELECT * INTO m FROM theodb_ml.models WHERE model_id = p_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'theodb_ml.apply_model: model % not found', p_id USING ERRCODE = '22023';
    END IF;
    PERFORM set_config('theodb.llm_endpoint', m.endpoint, false);  -- session-level
    IF m.model_name IS NOT NULL THEN
        PERFORM set_config('theodb.llm_model', m.model_name, false);
    END IF;
END $$;

-- Least-privilege: these functions manage model configs + SET session GUCs that gate outbound HTTP via
-- ai._chat — NOT granted to PUBLIC (same posture as ai.*). After apply_model, the caller still needs EXECUTE
-- on the ai.* function + ai._chat (SECURITY INVOKER chain) and must SET theodb.llm_api_key itself.
REVOKE ALL ON FUNCTION theodb_ml.create_model(text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_ml.drop_model(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_ml.list_models() FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_ml.apply_model(text) FROM PUBLIC;

COMMENT ON FUNCTION theodb_ml.apply_model(text) IS
  'Select a registered model for this session: SETs theodb.llm_endpoint/llm_model (the GUCs ai._chat reads) '
  'from theodb_ml.models. Does NOT modify ai._chat and does NOT set the API key (keys are session GUCs, never '
  'persisted — ADR D2). Not granted to PUBLIC.';
COMMENT ON FUNCTION theodb_ml.create_model(text, text, text) IS
  'Register/upsert a named (endpoint, model_name) config. Endpoint must be http(s) (SSRF-consistent). NO '
  'api_key is accepted or stored — keys remain session GUCs (ADR D2). Not granted to PUBLIC.';
