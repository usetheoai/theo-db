-- TheoDB M12 — theodb_ai_nl CONFIG surface (config / templates / value-index) over the M7-S4 NL gate.
-- The security-critical generate+execute gate lives in sql/60 (ai.nl_to_sql / ai.nl_query: L1-L4) and is
-- REUSED UNCHANGED here. This file adds a CONFIGURATION layer: register schema context, prompt templates,
-- and a categorical value-index once, then query by config id. The config only ENRICHES the prompt and
-- supplies the allowed-relation list — it NEVER relaxes the deterministic L2/L4/L3 validation (ai.nl_query_cfg
-- delegates to the unchanged ai.nl_query). Divergence from the literal 58-function AlloyDB theodb_ai_nl
-- extension is intentional (ADR D1): we ship the three core capabilities in schema `ai`.
-- Idempotent: safe to re-run / load from docker-entrypoint-initdb.d AFTER sql/60.

CREATE SCHEMA IF NOT EXISTS ai;

-- nl_config — binds an app config to its allowed_relations, persisted schema context, a template, a model.
CREATE TABLE IF NOT EXISTS ai.nl_config (
    config_id         text PRIMARY KEY,
    allowed_relations text[] NOT NULL,
    schema_context    text,
    template_id       text,
    model             text,
    created_at        timestamptz NOT NULL DEFAULT now()
);

-- nl_templates — named, enable-able blocks of domain instructions folded into the prompt CONTEXT (never the
-- security system prompt — a template cannot weaken the gate).
CREATE TABLE IF NOT EXISTS ai.nl_templates (
    template_id   text PRIMARY KEY,
    system_prompt text NOT NULL,
    enabled       boolean NOT NULL DEFAULT true
);

-- nl_value_index — for a categorical column, its distinct allowed values, surfaced to the model as a hint.
CREATE TABLE IF NOT EXISTS ai.nl_value_index (
    config_id    text NOT NULL,
    relation     text NOT NULL,
    column_name  text NOT NULL,
    values       text[] NOT NULL,
    refreshed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (config_id, relation, column_name)
);

-- ai.nl_add_config — upsert a config.
CREATE OR REPLACE FUNCTION ai.nl_add_config(p_id text, p_rels text[], p_ctx text DEFAULT NULL,
                                            p_template text DEFAULT NULL, p_model text DEFAULT NULL)
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    IF p_id IS NULL OR p_id = '' THEN
        RAISE EXCEPTION 'ai.nl_add_config: config_id must not be empty' USING ERRCODE = '22023';
    END IF;
    IF p_rels IS NULL OR array_length(p_rels, 1) IS NULL THEN
        RAISE EXCEPTION 'ai.nl_add_config: allowed_relations must be a non-empty array' USING ERRCODE = '22023';
    END IF;
    INSERT INTO ai.nl_config (config_id, allowed_relations, schema_context, template_id, model)
        VALUES (p_id, p_rels, p_ctx, p_template, p_model)
        ON CONFLICT (config_id) DO UPDATE
            SET allowed_relations = EXCLUDED.allowed_relations,
                schema_context    = EXCLUDED.schema_context,
                template_id       = EXCLUDED.template_id,
                model             = EXCLUDED.model;
END $$;

-- ai.nl_add_template — upsert a template (enabled by default).
CREATE OR REPLACE FUNCTION ai.nl_add_template(p_id text, p_system_prompt text)
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    IF p_id IS NULL OR p_id = '' OR p_system_prompt IS NULL OR p_system_prompt = '' THEN
        RAISE EXCEPTION 'ai.nl_add_template: template_id and system_prompt must not be empty' USING ERRCODE = '22023';
    END IF;
    INSERT INTO ai.nl_templates (template_id, system_prompt, enabled)
        VALUES (p_id, p_system_prompt, true)
        ON CONFLICT (template_id) DO UPDATE SET system_prompt = EXCLUDED.system_prompt;
END $$;

-- ai.nl_set_template_enabled — enable/disable a template without deleting it.
CREATE OR REPLACE FUNCTION ai.nl_set_template_enabled(p_id text, p_enabled boolean)
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    UPDATE ai.nl_templates SET enabled = p_enabled WHERE template_id = p_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'ai.nl_set_template_enabled: template % not found', p_id USING ERRCODE = '22023';
    END IF;
END $$;

-- ai.nl_set_value_index — operator supplies the values explicitly (zero dynamic SQL — no injection surface).
CREATE OR REPLACE FUNCTION ai.nl_set_value_index(p_config text, p_relation text, p_column text, p_values text[])
RETURNS void
LANGUAGE plpgsql
AS $$
BEGIN
    IF p_values IS NULL THEN
        RAISE EXCEPTION 'ai.nl_set_value_index: values must not be NULL' USING ERRCODE = '22023';
    END IF;
    INSERT INTO ai.nl_value_index (config_id, relation, column_name, values, refreshed_at)
        VALUES (p_config, p_relation, p_column, p_values, now())
        ON CONFLICT (config_id, relation, column_name) DO UPDATE
            SET values = EXCLUDED.values, refreshed_at = now();
END $$;

-- ai.nl_refresh_value_index — auto-populate the value-index from data. GUARDED (ADR D3): the relation MUST be
-- in the config's allowed_relations (operator-vetted), the column MUST be a plain identifier, and the relation
-- MUST resolve. The dynamic query is a fixed-shape read (SELECT DISTINCT <col> FROM <rel>) with the column
-- quoted via %I and the relation rendered via ::regclass (auto-qualified+quoted) — no user SQL is executed.
CREATE OR REPLACE FUNCTION ai.nl_refresh_value_index(p_config text, p_relation text, p_column text,
                                                     p_max int DEFAULT 50)
RETURNS int
LANGUAGE plpgsql
AS $$
DECLARE
    v_allowed text[];
    v_vals    text[];
BEGIN
    IF p_max IS NULL OR p_max <= 0 THEN
        RAISE EXCEPTION 'ai.nl_refresh_value_index: max_values must be > 0 (got %)', p_max USING ERRCODE = '22023';
    END IF;
    SELECT allowed_relations INTO v_allowed FROM ai.nl_config WHERE config_id = p_config;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'ai.nl_refresh_value_index: config % not found', p_config USING ERRCODE = '22023';
    END IF;
    IF NOT (p_relation = ANY (v_allowed)) THEN
        RAISE EXCEPTION 'ai.nl_refresh_value_index: relation % is not in config allowed_relations', p_relation
            USING ERRCODE = '22023';
    END IF;
    IF p_column !~ '^[A-Za-z_][A-Za-z0-9_]*$' THEN
        RAISE EXCEPTION 'ai.nl_refresh_value_index: invalid column identifier %', p_column USING ERRCODE = '22023';
    END IF;
    IF to_regclass(p_relation) IS NULL THEN
        RAISE EXCEPTION 'ai.nl_refresh_value_index: relation % does not resolve', p_relation USING ERRCODE = '22023';
    END IF;
    -- bound the scan; fixed-shape read over validated identifiers (no user SQL).
    PERFORM set_config('statement_timeout', '5000', true);
    EXECUTE format(
        'SELECT array_agg(v ORDER BY v) FROM (SELECT DISTINCT %I::text AS v FROM %s WHERE %I IS NOT NULL LIMIT %s) z',
        p_column, p_relation::regclass, p_column, p_max
    ) INTO v_vals;
    v_vals := coalesce(v_vals, ARRAY[]::text[]);
    INSERT INTO ai.nl_value_index (config_id, relation, column_name, values, refreshed_at)
        VALUES (p_config, p_relation, p_column, v_vals, now())
        ON CONFLICT (config_id, relation, column_name) DO UPDATE
            SET values = EXCLUDED.values, refreshed_at = now();
    RETURN coalesce(array_length(v_vals, 1), 0);
END $$;

-- ai.nl_query_cfg — config-aware NL query. Loads the config, builds a prompt ENRICHMENT block (schema_context
-- + the enabled template + value-index hints) and PREPENDS it to the question, then delegates to the UNCHANGED
-- ai.nl_query with the config's allowed_relations. The deterministic gate (L2/L4/L3) runs on every query — the
-- enrichment is prompt text only and cannot bypass it (ADR D2). Returns jsonb rows.
CREATE OR REPLACE FUNCTION ai.nl_query_cfg(question text, config_id text, max_rows int DEFAULT 100)
RETURNS jsonb
LANGUAGE plpgsql
AS $$
DECLARE
    cfg      ai.nl_config;
    v_tmpl   text := NULL;
    v_hints  text := NULL;
    enriched text;
BEGIN
    IF question IS NULL OR btrim(question) = '' THEN
        RAISE EXCEPTION 'ai.nl_query_cfg: question must not be empty' USING ERRCODE = '22023';
    END IF;
    SELECT * INTO cfg FROM ai.nl_config WHERE ai.nl_config.config_id = nl_query_cfg.config_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'ai.nl_query_cfg: config % not found', config_id USING ERRCODE = '22023';
    END IF;

    IF cfg.template_id IS NOT NULL THEN
        SELECT system_prompt INTO v_tmpl FROM ai.nl_templates
            WHERE template_id = cfg.template_id AND enabled;
    END IF;

    SELECT string_agg(
               format('Column %s.%s allowed values: %s', relation, column_name, array_to_string(values, ', ')),
               E'\n')
        INTO v_hints
        FROM ai.nl_value_index WHERE ai.nl_value_index.config_id = nl_query_cfg.config_id;

    -- concat_ws drops NULL parts; the gate's own L1 system prompt still applies inside ai.nl_query.
    enriched := concat_ws(E'\n', cfg.schema_context, v_tmpl, v_hints, '', 'Question: ' || question);

    RETURN ai.nl_query(enriched, cfg.allowed_relations, cfg.model, max_rows);  -- UNCHANGED M7-S4 gate
END $$;

-- Least-privilege: the config functions touch config tables and ai.nl_query_cfg makes an outbound LLM call
-- (via the gate) + executes dynamic SQL — NOT granted to PUBLIC (same posture as the gate). A caller of
-- ai.nl_query_cfg needs EXECUTE on ai.nl_query, ai.nl_to_sql AND ai._chat too (SECURITY INVOKER chain).
REVOKE ALL ON FUNCTION ai.nl_add_config(text, text[], text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.nl_add_template(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.nl_set_template_enabled(text, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.nl_set_value_index(text, text, text, text[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.nl_refresh_value_index(text, text, text, int) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.nl_query_cfg(text, text, int) FROM PUBLIC;

COMMENT ON FUNCTION ai.nl_query_cfg(text, text, int) IS
  'Config-aware NL->SQL: loads ai.nl_config, enriches the prompt with schema_context + enabled template + '
  'value-index hints, then delegates to the UNCHANGED ai.nl_query (L1-L4 gate) with the config allowed_relations. '
  'The config enriches the prompt only — it never relaxes the deterministic anti-injection gate. Not granted to PUBLIC.';
COMMENT ON FUNCTION ai.nl_refresh_value_index(text, text, text, int) IS
  'Auto-populate ai.nl_value_index from data. GUARDED: relation must be in the config allowed_relations, column '
  'must be a plain identifier, relation must resolve; fixed-shape read (quote_ident + ::regclass). Not granted to PUBLIC.';
