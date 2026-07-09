-- TheoDB umbrella extension — upgrade 1.3 -> 1.4.
-- M62 (ROADMAP-v3): adds the HTAP unified surface (theodb.htap_refresh / theodb.olap / theodb.htap_freshness)
-- + the snapshot catalog theodb._htap_snapshots + the path helper theodb._htap_path. Own-code SQL/plpgsql
-- composed over the already-embedded pg_duckdb (M61/ADR-0020) — see sql/85-theodb-htap.sql (ADR-0021).
--
-- On a GREENFIELD install the regenerated base (theodb--1.0.sql) already concatenates sql/85-theodb-htap.sql,
-- so this delta is the IN-PLACE upgrade path (`ALTER EXTENSION theodb UPDATE TO '1.4'`) for an already-installed
-- 1.3. CREATE OR REPLACE / IF NOT EXISTS make it a no-op where the greenfield base already created the objects.
-- Keep this body byte-identical in intent to sql/85-theodb-htap.sql (the objects are defined there; this delta
-- re-applies them so a non-greenfield extension gains them without a reinstall). Idempotent + injection-safe
-- (regclass + %I/%L), fail-fast typed errors (Rule 8) — the rationale lives in sql/85-theodb-htap.sql.

CREATE TABLE IF NOT EXISTS theodb._htap_snapshots (
    rel          regclass PRIMARY KEY,
    parquet_path text        NOT NULL,
    refreshed_at timestamptz NOT NULL
);

CREATE OR REPLACE FUNCTION theodb._htap_path(rel regclass)
RETURNS text
LANGUAGE sql
STABLE
AS $$
    SELECT '/var/lib/postgresql/htap/theodb_htap_' || rel::oid || '.parquet';
$$;

CREATE OR REPLACE FUNCTION theodb.htap_refresh(p_rel regclass)
RETURNS timestamptz
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    snap_path text := theodb._htap_path(p_rel);
    snap_at   timestamptz;
BEGIN
    BEGIN
        PERFORM * FROM duckdb.query('SELECT 1 AS ok');
    EXCEPTION WHEN others THEN
        RAISE EXCEPTION 'theodb.htap_refresh: pg_duckdb engine unavailable (is shared_preload_libraries set?): %',
            SQLERRM USING ERRCODE = '58000';
    END;

    snap_at := clock_timestamp();
    EXECUTE format('COPY (SELECT * FROM %s) TO %L (FORMAT parquet)', p_rel::regclass, snap_path);

    INSERT INTO theodb._htap_snapshots (rel, parquet_path, refreshed_at)
        VALUES (p_rel, snap_path, snap_at)
        ON CONFLICT (rel) DO UPDATE
            SET parquet_path = EXCLUDED.parquet_path,
                refreshed_at = EXCLUDED.refreshed_at;

    RETURN snap_at;
END;
$$;

CREATE OR REPLACE FUNCTION theodb.olap(p_rel regclass)
RETURNS jsonb
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    snap_path text;
    snap_at   timestamptz;
    result    jsonb;
BEGIN
    SELECT s.parquet_path, s.refreshed_at INTO snap_path, snap_at
        FROM theodb._htap_snapshots s WHERE s.rel = p_rel;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'theodb.olap: no snapshot for %; call theodb.htap_refresh(%) first',
            p_rel, p_rel USING ERRCODE = 'P0002';
    END IF;

    BEGIN
        EXECUTE format(
            'SELECT jsonb_agg(t) FROM duckdb.query($DUCK$ '
            'SELECT category, count(*) AS c, round(avg(amount), 4) AS a '
            'FROM read_parquet(%L) GROUP BY category ORDER BY category $DUCK$) t',
            snap_path)
        INTO result;
    EXCEPTION WHEN others THEN
        RAISE EXCEPTION 'theodb.olap: cannot read snapshot % for % (missing/corrupt Parquet?); re-run '
            'theodb.htap_refresh(%): %', snap_path, p_rel, p_rel, SQLERRM USING ERRCODE = '58030';
    END;

    RETURN jsonb_build_object(
        'snapshot_at', snap_at,
        'rows', COALESCE(result, '[]'::jsonb));
END;
$$;

CREATE OR REPLACE FUNCTION theodb.htap_freshness(p_rel regclass)
RETURNS interval
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    snap_at timestamptz;
BEGIN
    SELECT s.refreshed_at INTO snap_at FROM theodb._htap_snapshots s WHERE s.rel = p_rel;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'theodb.htap_freshness: no snapshot for %; call theodb.htap_refresh(%) first',
            p_rel, p_rel USING ERRCODE = 'P0002';
    END IF;

    RETURN now() - snap_at;
END;
$$;

REVOKE ALL ON FUNCTION theodb.htap_refresh(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.olap(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.htap_freshness(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb._htap_path(regclass) FROM PUBLIC;
