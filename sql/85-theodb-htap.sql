-- TheoDB M62 — HTAP unified surface (lakehouse-materialized row↔columnar flow).
-- Ships as own-code SQL/plpgsql (ADR-0021 D2 — plpgsql over Rust: these functions only orchestrate dynamic
-- SQL against pg_duckdb, no low-level hot path — parsimony ladder rung 5/6, `.claude/rules/parsimony-ladder.md`).
-- Composes 100% over the already-embedded pg_duckdb (M61/ADR-0020, MIT — Rule 9, zero new piece):
--   theodb.htap_refresh(regclass)   → materializes a row table to a dated Parquet snapshot (COPY … TO parquet)
--   theodb.olap(regclass)           → routes the analytical aggregate to the columnar snapshot via read_parquet
--   theodb.htap_freshness(regclass) → returns the snapshot lag (now() - refreshed_at) — staleness is EXPLICIT
--
-- The M61 finding (docs/adr/0020-m61-embed-pgduckdb.md:36-38, docs/benchmarks/m61-columnar-adoption.md) makes
-- the design honest: pg_duckdb does NOT speed up analytics over the row heap (force_execution = 0.63–0.89×,
-- honest-negative) and WINS ~9× at 5M only over already-COLUMNAR data (Parquet). So the surface is a row↔columnar
-- materialized flow with DATED freshness (a snapshot is a point-in-time, never magic HTAP transparency).
--
-- Safe dynamic SQL: regclass-validated relation + %I (identifier) / %L (literal) quoting — identifiers never
-- interpolated raw, path bound as a literal (injection-safe; same discipline as sql/80-theodb-migrate.sql:60).
-- Error handling (`.claude/rules/error-handling.md` §2 — fail fast, typed): a missing snapshot RAISEs a typed
-- error with a clear next step, never a silent NULL; a failed COPY propagates (not swallowed — Unbreakable Rule 8).
-- Idempotent: safe to re-run / load from the extension install script (CREATE OR REPLACE / IF NOT EXISTS).

CREATE SCHEMA IF NOT EXISTS theodb;

-- Snapshot catalog — one row per materialized relation. The refreshed_at is the freshness clock the whole
-- surface reads: theodb.olap exposes it (so the caller sees the point-in-time), theodb.htap_freshness derives
-- the lag from it. rel is a regclass PK (upsert on refresh — the latest snapshot wins).
CREATE TABLE IF NOT EXISTS theodb._htap_snapshots (
    rel          regclass PRIMARY KEY,
    parquet_path text        NOT NULL,
    refreshed_at timestamptz NOT NULL
);

COMMENT ON TABLE theodb._htap_snapshots IS
    'M62 HTAP snapshot catalog: (rel, parquet_path, refreshed_at) — one row per materialized relation. '
    'refreshed_at is the freshness clock (theodb.olap exposes it; theodb.htap_freshness derives the lag). '
    'Upserted by theodb.htap_refresh (latest snapshot wins).';

-- theodb._htap_path — the snapshot path for a relation, derived from its oid (unique per table). A dedicated
-- IMMUTABLE-shaped helper so htap_refresh and any future GC follow-up (Q3) build the path in ONE place (DRY).
-- The base dir /var/lib/postgresql/htap is created lazily by the COPY via pg_duckdb's file writer.
CREATE OR REPLACE FUNCTION theodb._htap_path(rel regclass)
RETURNS text
LANGUAGE sql
STABLE
AS $$
    SELECT '/var/lib/postgresql/htap/theodb_htap_' || rel::oid || '.parquet';
$$;

-- theodb.htap_refresh — materialize the row table to a dated Parquet snapshot and register it.
-- VOLATILE: writes a file + upserts the catalog (side effects); the planner must never fold/hoist it.
-- The COPY … TO … (FORMAT parquet) runs via pg_duckdb's writer (confirmed at connection level in
-- benchmarks/run_m61_columnar_adoption.py:93; the plpgsql-context uncertainty is Q2 of the plan, resolved
-- empirically by T1.1's roundtrip test — if the direct COPY is restricted inside a function, the documented
-- fallback is to route the COPY through duckdb.query, § Failure scenarios).
CREATE OR REPLACE FUNCTION theodb.htap_refresh(p_rel regclass)
RETURNS timestamptz
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    snap_path text := theodb._htap_path(p_rel);
    snap_at   timestamptz;
BEGIN
    -- Confirm the DuckDB engine is live before writing anything (fail-fast, typed — Rule 8). If pg_duckdb is
    -- not preloaded the probe raises and no partial snapshot is attempted.
    BEGIN
        PERFORM * FROM duckdb.query('SELECT 1 AS ok'); -- no-op probe: confirms the DuckDB engine is live
    EXCEPTION WHEN others THEN
        RAISE EXCEPTION 'theodb.htap_refresh: pg_duckdb engine unavailable (is shared_preload_libraries set?): %',
            SQLERRM USING ERRCODE = '58000';
    END;

    -- Materialize the row heap to a columnar Parquet snapshot. %I quotes the relation identifier, %L the path
    -- literal (injection-safe). A failing COPY raises here and is NOT swallowed (Unbreakable Rule 8): a partial
    -- snapshot must never be registered as fresh.
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

COMMENT ON FUNCTION theodb.htap_refresh(regclass) IS
    'M62 HTAP: materialize a row table to a dated columnar Parquet snapshot (COPY … TO parquet via pg_duckdb) '
    'and register (rel, path, now()) in theodb._htap_snapshots. Returns the snapshot timestamp. Storage is 2× '
    '(heap + Parquet); freshness is DATED (the snapshot lags the heap between refreshes). A failed COPY raises '
    '(no partial snapshot registered). VOLATILE (file write + catalog upsert).';

-- theodb.olap — route the analytical aggregate to the columnar snapshot and return result + freshness.
-- Reads the snapshot path from the catalog (typed error if absent — fail-closed, never a silent NULL), runs the
-- canonical GROUP BY over read_parquet via duckdb.query (the confirmed M61 syntax:
-- benchmarks/run_m61_columnar_adoption.py:94-96), and wraps the rows with the snapshot's refreshed_at so the
-- caller ALWAYS sees the point-in-time (the honesty contract of ADR-0021 D1 — the user must see the timestamp).
-- The aggregate mirrors benchmarks/theodb_bench/columnar.py:11 (_AGG) so the checksum-matched round-trip against
-- the fresh heap holds. VOLATILE: reads an external file whose content changes across refreshes.
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
            p_rel, p_rel USING ERRCODE = 'P0002';   -- no_data_found (typed, fail-closed)
    END IF;

    -- Aggregate over the columnar snapshot. read_parquet lives INSIDE duckdb.query (the M61-confirmed syntax:
    -- read_parquet OUTSIDE duckdb.query would require r['col'] projection). category/count/avg mirror _AGG.
    -- The path is %L-quoted (injection-safe). If the Parquet is missing/corrupt the DuckDB scan raises here,
    -- surfaced as a clear error (not a silent empty result).
    BEGIN
        EXECUTE format(
            'SELECT jsonb_agg(t) FROM duckdb.query($DUCK$ '
            'SELECT category, count(*) AS c, round(avg(amount), 4) AS a '
            'FROM read_parquet(%L) GROUP BY category ORDER BY category $DUCK$) t',
            snap_path)
        INTO result;
    EXCEPTION WHEN others THEN
        RAISE EXCEPTION 'theodb.olap: cannot read snapshot % for % (missing/corrupt Parquet?); re-run '
            'theodb.htap_refresh(%): %', snap_path, p_rel, p_rel, SQLERRM USING ERRCODE = '58030';  -- io_error
    END;

    RETURN jsonb_build_object(
        'snapshot_at', snap_at,
        'rows', COALESCE(result, '[]'::jsonb));
END;
$$;

COMMENT ON FUNCTION theodb.olap(regclass) IS
    'M62 HTAP: route the analytical aggregate to the columnar Parquet snapshot (read_parquet via duckdb.query, '
    '~9× at 5M per M61). Returns {"snapshot_at": ts, "rows": [...]} — rows reflect the SNAPSHOT state (stale by '
    'design between refreshes; freshness is dated). Raises a typed error if no snapshot exists (call '
    'theodb.htap_refresh first) or the Parquet is missing/corrupt. For 100%-fresh ad-hoc queries use the '
    'force_execution fallback (SET duckdb.force_execution=true; SELECT … FROM rel) — slower, but no refresh.';

-- theodb.htap_freshness — the snapshot lag (now() - refreshed_at). Freshness is a DATED contract, not a bug:
-- the OLAP path reads a point-in-time and this is how stale it is. Typed error if there is no snapshot (never a
-- silent NULL that could be mistaken for "fresh"). STABLE: reads the catalog + now() within the statement.
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

COMMENT ON FUNCTION theodb.htap_freshness(regclass) IS
    'M62 HTAP: the snapshot lag (now() - refreshed_at) for a relation — how far the columnar snapshot trails '
    'the live heap. Freshness is a dated contract (grows between refreshes, resets on theodb.htap_refresh), not '
    'a bug. Raises a typed error if no snapshot exists.';

-- Least-privilege: htap_refresh writes a server-side file and reads the whole table; olap runs the DuckDB
-- engine over a file. Not granted to PUBLIC (same posture as theodb.embed / ai.* — outbound/file side effects).
REVOKE ALL ON FUNCTION theodb.htap_refresh(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.olap(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.htap_freshness(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb._htap_path(regclass) FROM PUBLIC;
