-- TheoDB M16 — migration helper: import a Pinecone export into a TheoDB table (the unification moat).
-- Maps the Pinecone Vector model {id: str, values: list[float], metadata: dict} → a relational table with
-- (id, embedding vector, metadata jsonb). Native jsonb parsing (no plpython3u, no stdlib json, no pinecone
-- client dependency — ADR D3 / parsimony ladder rung 3). Safe dynamic SQL via regclass + %I (same discipline
-- as ai.hybrid_search_rrf, sql/40) — identifiers never interpolated raw; values bound as parameters.
-- deps (vector) declared in theodb.control `requires` (M15).

CREATE SCHEMA IF NOT EXISTS theodb;

-- theodb.import_pinecone(target, export, [id_col], [embedding_col], [metadata_col]) -> rows inserted.
-- `export` is a JSON array of Pinecone records: [{"id":"a","values":[...],"metadata":{...}}, ...].
-- The caller passes the export as jsonb (Postgres parses it natively). Fails fast (SQLSTATE 22023) on a
-- non-array export or a record missing id/values — no partial/corrupt insert beyond the failing element.
CREATE OR REPLACE FUNCTION theodb.import_pinecone(
    target        regclass,
    export        jsonb,
    id_col        text DEFAULT 'id',
    embedding_col text DEFAULT 'embedding',
    metadata_col  text DEFAULT 'metadata'
) RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    rec jsonb;
    n   integer := 0;
BEGIN
    IF jsonb_typeof(export) <> 'array' THEN
        RAISE EXCEPTION 'theodb.import_pinecone: export must be a JSON array of records'
            USING ERRCODE = '22023';
    END IF;

    FOR rec IN SELECT value FROM jsonb_array_elements(export) AS value LOOP
        IF NOT (rec ? 'id' AND rec ? 'values') THEN
            RAISE EXCEPTION 'theodb.import_pinecone: each record must have "id" and "values" (got: %)', rec
                USING ERRCODE = '22023';
        END IF;
        -- %I quotes identifiers; target is a validated regclass; values bound as params (injection-safe).
        EXECUTE format(
            'INSERT INTO %s (%I, %I, %I) VALUES ($1, $2::vector, $3)',
            target, id_col, embedding_col, metadata_col
        )
        USING rec->>'id',
              (rec->'values')::text,
              COALESCE(rec->'metadata', '{}'::jsonb);
        n := n + 1;
    END LOOP;

    RETURN n;
END;
$$;

COMMENT ON FUNCTION theodb.import_pinecone(regclass, jsonb, text, text, text) IS
    'Import a Pinecone export (JSON array of {id,values,metadata}) into a TheoDB table (id, embedding vector, metadata jsonb). Native jsonb; safe dynamic SQL. M16.';

-- Least privilege: a migration helper that writes to caller-owned tables — not for PUBLIC (parity with ai.*).
REVOKE ALL ON FUNCTION theodb.import_pinecone(regclass, jsonb, text, text, text) FROM PUBLIC;
