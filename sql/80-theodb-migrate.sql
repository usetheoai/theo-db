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

-- theodb.import_pinecone_chunked(...) — M-audit-remediation (audit #6 unbounded_collection / #7 memory_inefficiency).
-- The FUNCTION above ingests the WHOLE export in ONE transaction (unbounded memory/WAL for a large export).
-- This PROCEDURE ingests in `chunk_size` batches with a COMMIT per batch, bounding the in-flight footprint of
-- a large migration. A plpgsql FUNCTION CANNOT COMMIT (it runs in the caller's transaction) — only a PROCEDURE
-- (CALLed) can; that is the whole reason this is a PROCEDURE and the FUNCTION is kept for small/atomic imports.
--
-- It indexes into the jsonb array (export -> j) rather than a `FOR ... IN SELECT jsonb_array_elements` loop:
-- PostgreSQL forbids COMMIT while a query portal/cursor is held open, but procedure parameters + variables
-- survive COMMIT, so indexing is the COMMIT-safe pattern. Same %I-quoted, regclass-validated, parameter-bound
-- dynamic SQL as the FUNCTION (injection-safe). MUST be CALLed in autocommit (not inside an explicit
-- transaction block — COMMIT would raise "invalid transaction termination").
--
-- Semantics (documented, NOT all-or-nothing): a mid-run failure leaves already-COMMITted batches persisted
-- (that is the point of chunking — bounded footprint + partial progress survives an abort). Use the atomic
-- FUNCTION when you need all-or-nothing.
CREATE PROCEDURE theodb.import_pinecone_chunked(
    target        regclass,
    export        jsonb,
    chunk_size    int  DEFAULT 1000,
    id_col        text DEFAULT 'id',
    embedding_col text DEFAULT 'embedding',
    metadata_col  text DEFAULT 'metadata'
)
LANGUAGE plpgsql
AS $$
DECLARE
    total integer;
    i     integer := 0;
    j     integer;
    rec   jsonb;
    ins   text;
BEGIN
    IF chunk_size IS NULL OR chunk_size <= 0 THEN
        RAISE EXCEPTION 'theodb.import_pinecone_chunked: chunk_size must be > 0 (got %)', chunk_size
            USING ERRCODE = '22023';
    END IF;
    IF jsonb_typeof(export) <> 'array' THEN
        RAISE EXCEPTION 'theodb.import_pinecone_chunked: export must be a JSON array of records'
            USING ERRCODE = '22023';
    END IF;

    total := jsonb_array_length(export);
    -- Build the injection-safe INSERT once (identifiers %I-quoted; target is a validated regclass;
    -- values bound as parameters) — identical discipline to the FUNCTION.
    ins := format('INSERT INTO %s (%I, %I, %I) VALUES ($1, $2::vector, $3)',
                  target, id_col, embedding_col, metadata_col);

    WHILE i < total LOOP
        j := i;
        WHILE j < LEAST(i + chunk_size, total) LOOP
            rec := export -> j;   -- index into the materialized jsonb (no portal held across COMMIT)
            IF NOT (rec ? 'id' AND rec ? 'values') THEN
                RAISE EXCEPTION 'theodb.import_pinecone_chunked: each record must have "id" and "values" (got: %)', rec
                    USING ERRCODE = '22023';
            END IF;
            EXECUTE ins USING rec->>'id', (rec->'values')::text, COALESCE(rec->'metadata', '{}'::jsonb);
            j := j + 1;
        END LOOP;
        COMMIT;                   -- bound the footprint: each chunk is durable before the next starts
        i := i + chunk_size;
    END LOOP;

    RAISE NOTICE 'theodb.import_pinecone_chunked: imported % records in chunks of %', total, chunk_size;
END;
$$;

COMMENT ON PROCEDURE theodb.import_pinecone_chunked(regclass, jsonb, int, text, text, text) IS
    'Chunked Pinecone import: ingest a JSON array of {id,values,metadata} into a TheoDB table in chunk_size '
    'batches with a COMMIT per batch (bounds memory/WAL for large migrations). CALL in autocommit. NOT '
    'all-or-nothing (committed batches survive a mid-run abort) — use theodb.import_pinecone (FUNCTION) for '
    'small/atomic imports. Same %I-quoted, parameter-bound dynamic SQL (injection-safe). audit-remediation.';

REVOKE ALL ON PROCEDURE theodb.import_pinecone_chunked(regclass, jsonb, int, text, text, text) FROM PUBLIC;
