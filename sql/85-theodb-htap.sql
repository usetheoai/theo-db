-- TheoDB M62 — HTAP unified surface (lakehouse-materialized row↔columnar flow) — CODEGEN edition.
-- Ships as own-code SQL/plpgsql (ADR-0021 D2 — plpgsql over Rust: these functions only build/validate SQL and
-- touch a small catalog, no low-level hot path — parsimony ladder rung 5/6, `.claude/rules/parsimony-ladder.md`).
--
-- ── ARCHITECTURAL CONSTRAINT (measured, reproducible on theodb:m62) — WHY THIS SURFACE IS STATEMENT-LEVEL ──
-- pg_duckdb PROHIBITS DuckDB execution INSIDE a function. Any `SELECT … FROM duckdb.query(…)`, `COPY … TO …
-- (FORMAT parquet)` (the parquet writer is the DuckDB engine), or `read_parquet(…)` evaluated from within a
-- plpgsql/SQL function body raises: `ERROR: DuckDB execution is not supported inside functions`. There is NO GUC
-- to relax it. This INVALIDATES a "single transparent call" surface where `theodb.htap_refresh`/`theodb.olap`
-- run DuckDB internally (they would always error). At the STATEMENT level (a connection, outside any function)
-- the same `COPY (…) TO '<path>' (FORMAT parquet)` and `SELECT * FROM duckdb.query($$ … read_parquet('<path>')
-- … $$)` WORK — M61 proved it end-to-end (docs/benchmarks/m61-columnar-adoption.md; the confirmed syntax lives
-- at benchmarks/run_m61_columnar_adoption.py). So the honest, architecturally-correct surface here is CODEGEN:
--   • the theodb.* functions BUILD (never run) the DuckDB statements as text, and do the catalog work in pure SQL;
--   • the CLIENT runs the returned statement at the connection level (where DuckDB is allowed).
-- This is NOT a workaround — it is the correct design given the pg_duckdb restriction. The trade-off is honest
-- (ADR-0021, Rule 3 / `public-copy.md`): the surface is a codegen statement-level flow, not a single opaque
-- transparent call. It remains the lakehouse/D2 bet, assisted by a tiny catalog + freshness clock.
--
-- The functions:
--   theodb.htap_refresh_sql(regclass)        → TEXT: the `COPY (SELECT * FROM tbl) TO '<path>' (FORMAT parquet)`
--                                              statement (pure string build; client runs it at connection level)
--   theodb.htap_register(regclass, text)     → timestamptz: upsert the snapshot catalog AFTER the client ran the
--                                              COPY (pure SQL, no DuckDB — runs fine in a function). Returns now().
--   theodb.olap_sql(regclass)                → TEXT: the `SELECT * FROM duckdb.query($$ …read_parquet('<path>')…$$)`
--                                              analytical statement over the registered snapshot (client runs it)
--   theodb.htap_freshness(regclass)          → interval: now() - refreshed_at from the catalog (pure SQL)
--
-- The M61 finding (docs/adr/0020-m61-embed-pgduckdb.md, docs/benchmarks/m61-columnar-adoption.md) makes the
-- design honest: pg_duckdb does NOT speed up analytics over the row heap (force_execution = 0.63–0.89×,
-- honest-negative) and WINS ~9× at 5M only over already-COLUMNAR data (Parquet). So the surface is a row↔columnar
-- materialized flow with DATED freshness (a snapshot is a point-in-time, never magic HTAP transparency).
--
-- Safe SQL codegen: regclass-validated relation + %I (identifier) / %L (literal) quoting via format() — the
-- relation identifier is never interpolated raw and the path is bound as a literal (injection-safe; same
-- discipline as sql/80-theodb-migrate.sql). Error handling (`.claude/rules/error-handling.md` §2 — fail fast,
-- typed): olap_sql / htap_freshness with no snapshot RAISE a typed error with a clear next step, never a silent
-- NULL. NO function calls duckdb.query internally — that is the whole point of this pivot.
-- Idempotent: safe to re-run / load from the extension install script (CREATE OR REPLACE / IF NOT EXISTS).

CREATE SCHEMA IF NOT EXISTS theodb;

-- Snapshot catalog — one row per materialized relation. The refreshed_at is the freshness clock the whole
-- surface reads: theodb.olap_sql resolves the path from it, theodb.htap_freshness derives the lag from it.
-- rel is a regclass PK (upsert on register — the latest snapshot wins).
CREATE TABLE IF NOT EXISTS theodb._htap_snapshots (
    rel          regclass PRIMARY KEY,
    parquet_path text        NOT NULL,
    refreshed_at timestamptz NOT NULL
);

COMMENT ON TABLE theodb._htap_snapshots IS
    'M62 HTAP snapshot catalog: (rel, parquet_path, refreshed_at) — one row per materialized relation. '
    'refreshed_at is the freshness clock (theodb.htap_freshness derives the lag; theodb.olap_sql resolves the '
    'path). Upserted by theodb.htap_register AFTER the client runs the COPY (latest snapshot wins).';

-- theodb._htap_path — the default snapshot path for a relation, derived from its oid (unique per table). A
-- dedicated helper so htap_refresh_sql builds the path in ONE place (DRY). The base dir
-- /var/lib/postgresql/htap is created lazily by the COPY (run by the client) via pg_duckdb's file writer.
CREATE OR REPLACE FUNCTION theodb._htap_path(rel regclass)
RETURNS text
LANGUAGE sql
STABLE
AS $$
    SELECT '/var/lib/postgresql/htap/theodb_htap_' || rel::oid || '.parquet';
$$;

-- theodb.htap_refresh_sql — RETURN (do NOT run) the COPY statement that materializes the row table to a dated
-- Parquet snapshot. Pure string building via format() — NO DuckDB execution here (the parquet writer is the
-- DuckDB engine, prohibited inside a function). The CLIENT executes the returned text at the connection level.
-- %I quotes the relation identifier, %L the path literal (injection-safe). IMMUTABLE-shaped: given the same
-- relation it returns the same statement (it reads only the catalog-independent path helper).
CREATE OR REPLACE FUNCTION theodb.htap_refresh_sql(p_rel regclass)
RETURNS text
LANGUAGE sql
STABLE
AS $$
    SELECT format('COPY (SELECT * FROM %s) TO %L (FORMAT parquet)', p_rel::regclass, theodb._htap_path(p_rel));
$$;

COMMENT ON FUNCTION theodb.htap_refresh_sql(regclass) IS
    'M62 HTAP codegen: RETURN the COPY statement that materializes a row table to a dated Parquet snapshot '
    '(COPY (SELECT * FROM tbl) TO ''<path>'' (FORMAT parquet)). Does NOT execute — pg_duckdb prohibits DuckDB '
    'execution inside functions, so the CLIENT runs the returned text at the connection level, then calls '
    'theodb.htap_register(tbl, path). Injection-safe (regclass + %I/%L). STABLE (no side effects).';

-- theodb.htap_register — upsert the snapshot catalog AFTER the client has run the COPY. Pure SQL/plpgsql, NO
-- DuckDB — this runs fine inside a function. The client passes the parquet_path it wrote (typically the one from
-- theodb.htap_refresh_sql / theodb._htap_path). Returns now() as the registered refreshed_at.
CREATE OR REPLACE FUNCTION theodb.htap_register(p_rel regclass, p_parquet_path text)
RETURNS timestamptz
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    snap_at timestamptz := now();
BEGIN
    -- Validate the client-supplied path is non-empty (fail-fast, typed — Rule 8). A blank path would register a
    -- snapshot theodb.olap_sql could never read; refuse it loudly instead of storing a broken row.
    IF p_parquet_path IS NULL OR btrim(p_parquet_path) = '' THEN
        RAISE EXCEPTION 'theodb.htap_register: parquet_path must be a non-empty path for %', p_rel
            USING ERRCODE = '22023';   -- invalid_parameter_value (typed, fail-closed)
    END IF;

    INSERT INTO theodb._htap_snapshots (rel, parquet_path, refreshed_at)
        VALUES (p_rel, p_parquet_path, snap_at)
        ON CONFLICT (rel) DO UPDATE
            SET parquet_path = EXCLUDED.parquet_path,
                refreshed_at = EXCLUDED.refreshed_at;

    RETURN snap_at;
END;
$$;

COMMENT ON FUNCTION theodb.htap_register(regclass, text) IS
    'M62 HTAP: register (rel, parquet_path, now()) in theodb._htap_snapshots AFTER the client ran the COPY from '
    'theodb.htap_refresh_sql. Pure SQL (no DuckDB) — runs inside a function. Upsert (latest snapshot wins). '
    'Returns the registered refreshed_at. Raises invalid_parameter_value on a blank path. VOLATILE (catalog write).';

-- theodb.olap_sql — RESOLVE the snapshot path from the catalog and RETURN (do NOT run) the analytical statement
-- that aggregates the columnar Parquet snapshot. Typed error if there is no snapshot (fail-closed, never a silent
-- NULL). The returned statement is the canonical _AGG (category/count/avg) over read_parquet, wrapped in
-- duckdb.query — the M61-confirmed syntax. IMPORTANT: `SELECT *` (not named columns) — duckdb.query returns a
-- record whose columns are defined by the inner query; naming them in the outer SELECT breaks (measured on the
-- image). The CLIENT executes the returned text at the connection level (where DuckDB is allowed).
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
            'theodb.htap_register(%, path) first', p_rel, p_rel, p_rel USING ERRCODE = 'P0002';  -- no_data_found
    END IF;

    -- Build the analytical statement. read_parquet lives INSIDE duckdb.query (the M61-confirmed syntax); the
    -- path is %L-quoted (injection-safe). `SELECT *` over duckdb.query(...) — the record columns come from the
    -- inner query (category, c, a); naming them outside would fail (measured). The client runs this text.
    RETURN format(
        'SELECT * FROM duckdb.query($DUCK$ '
        'SELECT category, count(*) AS c, round(avg(amount), 4) AS a '
        'FROM read_parquet(%L) GROUP BY category ORDER BY category $DUCK$)',
        snap_path);
END;
$$;

COMMENT ON FUNCTION theodb.olap_sql(regclass) IS
    'M62 HTAP codegen: RETURN the analytical statement (SELECT * FROM duckdb.query($$ …read_parquet(snapshot)… '
    '$$)) over the registered columnar snapshot (~9× at 5M per M61). Does NOT execute — pg_duckdb prohibits '
    'DuckDB inside functions, so the CLIENT runs the returned text at the connection level. Uses SELECT * (the '
    'duckdb.query record columns come from the inner query; naming them breaks). Raises no_data_found (P0002) if '
    'no snapshot exists — call theodb.htap_refresh_sql, run it, then theodb.htap_register first. STABLE.';

-- theodb.htap_freshness — the snapshot lag (now() - refreshed_at). Freshness is a DATED contract, not a bug: the
-- OLAP path reads a point-in-time and this is how stale it is. Typed error if there is no snapshot (never a
-- silent NULL that could be mistaken for "fresh"). Pure SQL/plpgsql (no DuckDB) — runs fine in a function.
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

COMMENT ON FUNCTION theodb.htap_freshness(regclass) IS
    'M62 HTAP: the snapshot lag (now() - refreshed_at) for a relation — how far the columnar snapshot trails the '
    'live heap. Freshness is a dated contract (grows between refreshes, resets on theodb.htap_register), not a '
    'bug. Pure SQL (no DuckDB). Raises no_data_found (P0002) if no snapshot exists.';

-- Least-privilege: the codegen functions only build text + touch a small catalog, but htap_register writes the
-- catalog and the returned statements run file/DuckDB work. Not granted to PUBLIC (same posture as theodb.embed /
-- ai.* — the surface orchestrates server-side file + engine side effects when the client runs the statements).
REVOKE ALL ON FUNCTION theodb.htap_refresh_sql(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.htap_register(regclass, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.olap_sql(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.htap_freshness(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb._htap_path(regclass) FROM PUBLIC;
