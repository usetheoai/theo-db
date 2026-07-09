-- TheoDB umbrella extension — upgrade 1.3 -> 1.4.
-- M62 (ROADMAP-v3): adds the HTAP unified surface (CODEGEN edition) — theodb.htap_refresh_sql /
-- theodb.htap_register / theodb.olap_sql / theodb.htap_freshness + the snapshot catalog theodb._htap_snapshots +
-- the path helper theodb._htap_path. Own-code SQL/plpgsql composed over the already-embedded pg_duckdb
-- (M61/ADR-0020) — see sql/85-theodb-htap.sql (ADR-0021).
--
-- CODEGEN pivot (measured): pg_duckdb PROHIBITS DuckDB execution inside a function
-- (`ERROR: DuckDB execution is not supported inside functions` — no GUC to relax it), so the theodb.* functions
-- BUILD (never run) the COPY / duckdb.query statements as text and the CLIENT runs them at the connection level.
-- The catalog + freshness are pure SQL (no DuckDB) and run fine in a function. See sql/85-theodb-htap.sql for the
-- full rationale (ADR-0021). This is NOT a workaround — it is the architecturally-correct surface given pg_duckdb.
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

CREATE OR REPLACE FUNCTION theodb.htap_refresh_sql(p_rel regclass)
RETURNS text
LANGUAGE sql
STABLE
AS $$
    SELECT format('COPY (SELECT * FROM %s) TO %L (FORMAT parquet)', p_rel::regclass, theodb._htap_path(p_rel));
$$;

CREATE OR REPLACE FUNCTION theodb.htap_register(p_rel regclass, p_parquet_path text)
RETURNS timestamptz
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    snap_at timestamptz := now();
BEGIN
    IF p_parquet_path IS NULL OR btrim(p_parquet_path) = '' THEN
        RAISE EXCEPTION 'theodb.htap_register: parquet_path must be a non-empty path for %', p_rel
            USING ERRCODE = '22023';
    END IF;

    INSERT INTO theodb._htap_snapshots (rel, parquet_path, refreshed_at)
        VALUES (p_rel, p_parquet_path, snap_at)
        ON CONFLICT (rel) DO UPDATE
            SET parquet_path = EXCLUDED.parquet_path,
                refreshed_at = EXCLUDED.refreshed_at;

    RETURN snap_at;
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
    SELECT s.parquet_path INTO snap_path
        FROM theodb._htap_snapshots s WHERE s.rel = p_rel;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'theodb.olap_sql: no snapshot for %; call theodb.htap_refresh_sql(%) + run it + '
            'theodb.htap_register(%, path) first', p_rel, p_rel, p_rel USING ERRCODE = 'P0002';
    END IF;

    RETURN format(
        'SELECT * FROM duckdb.query($DUCK$ '
        'SELECT category, count(*) AS c, round(avg(amount), 4) AS a '
        'FROM read_parquet(%L) GROUP BY category ORDER BY category $DUCK$)',
        snap_path);
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
        RAISE EXCEPTION 'theodb.htap_freshness: no snapshot for %; call theodb.htap_refresh_sql(%) + run it + '
            'theodb.htap_register(%, path) first', p_rel, p_rel, p_rel USING ERRCODE = 'P0002';
    END IF;

    RETURN now() - snap_at;
END;
$$;

-- Old M62 pre-pivot functions (theodb.htap_refresh(regclass), theodb.olap(regclass)) called duckdb.query INSIDE
-- the function body — which pg_duckdb prohibits (they always errored). Drop them so an already-installed 1.3.x
-- that received the broken shapes does not keep a dead, failing surface. No-op on a greenfield 1.4 (never existed).
DROP FUNCTION IF EXISTS theodb.htap_refresh(regclass);
DROP FUNCTION IF EXISTS theodb.olap(regclass);

REVOKE ALL ON FUNCTION theodb.htap_refresh_sql(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.htap_register(regclass, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.olap_sql(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.htap_freshness(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb._htap_path(regclass) FROM PUBLIC;
