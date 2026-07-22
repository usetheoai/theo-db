-- TheoDB umbrella extension — upgrade 1.4 -> 1.5.
-- M142 (pg_duckdb tier-out): adds the fail-closed guard to the HTAP codegen functions theodb.htap_refresh_sql
-- and theodb.olap_sql. After M142 the default TheoDB image ships WITHOUT pg_duckdb (it moved to the optional
-- theodb-htap image), so the two functions that produce pg_duckdb statements now RAISE a typed error
-- (feature_not_supported / 0A000) with the next step ("pull the theodb-htap image") when pg_duckdb is absent,
-- instead of handing the client a statement that fails obscurely. See sql/85-theodb-htap.sql (ADR-0056).
--
-- On a GREENFIELD install the regenerated base (theodb--1.0.sql) already concatenates the guarded
-- sql/85-theodb-htap.sql, so this delta is the IN-PLACE upgrade path (`ALTER EXTENSION theodb UPDATE TO '1.5'`)
-- for an already-installed 1.4. CREATE OR REPLACE makes it a no-op where the greenfield base already applied the
-- guarded bodies. Keep this body byte-identical in intent to sql/85-theodb-htap.sql (the objects are defined
-- there; this delta re-applies them so a non-greenfield extension gains the guard without a reinstall).
-- The guard keys on the INSTALLED EXTENSION (pg_extension — a catalog, always safe to query; immune to a future
-- duckdb.query overload), so this delta is safe to apply on an image WITHOUT pg_duckdb. Idempotent +
-- injection-safe (regclass + %L), fail-fast typed errors (Rule 8).

CREATE OR REPLACE FUNCTION theodb.htap_refresh_sql(p_rel regclass)
RETURNS text
LANGUAGE plpgsql
STABLE
AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_duckdb') THEN
        RAISE EXCEPTION 'theodb.htap_refresh_sql: pg_duckdb not installed'
            USING HINT = 'The HTAP/lakehouse surface (Parquet snapshots) requires pg_duckdb — pull the theodb-htap image.',
                  ERRCODE = '0A000';   -- feature_not_supported (typed, fail-closed)
    END IF;
    RETURN format('COPY (SELECT * FROM %s) TO %L (FORMAT parquet)', p_rel::regclass, theodb._htap_path(p_rel));
END;
$$;

CREATE OR REPLACE FUNCTION theodb.olap_sql(p_rel regclass)
RETURNS text
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    snap_path text;
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'pg_duckdb') THEN
        RAISE EXCEPTION 'theodb.olap_sql: pg_duckdb not installed'
            USING HINT = 'The HTAP/lakehouse surface (analytical query over Parquet) requires pg_duckdb — pull the theodb-htap image.',
                  ERRCODE = '0A000';   -- feature_not_supported (typed, fail-closed)
    END IF;

    SELECT s.parquet_path INTO snap_path
        FROM theodb._htap_snapshots s WHERE s.rel = p_rel;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'theodb.olap_sql: no snapshot for %; call theodb.htap_refresh_sql(%) + run it + '
            'theodb.htap_register(%, path) first', p_rel, p_rel, p_rel USING ERRCODE = 'P0002';  -- no_data_found
    END IF;

    RETURN format(
        'SELECT * FROM duckdb.query($DUCK$ '
        'SELECT category, count(*) AS c, round(avg(amount), 4) AS a '
        'FROM read_parquet(%L) GROUP BY category ORDER BY category $DUCK$)',
        snap_path);
END;
$$;
