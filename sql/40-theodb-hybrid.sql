-- TheoDB M7-S1 — Hybrid search (FTS + vector) fused by Reciprocal Rank Fusion (RRF).
-- Mirrors AlloyDB's ai.hybrid_search() capability with permissive PostgreSQL primitives:
--   FTS leg  = PostgreSQL native full-text search (to_tsvector/@@/ts_rank_cd, GIN index)
--   vector leg = pgvector similarity (<=> cosine), HNSW index
--   fusion   = RRF, score(d) = sum over legs of 1/(k + rank_leg(d))   [Cormack et al., SIGIR'09]
-- The RRF math is the public Cormack 2009 technique (k=60 empirical default), NOT borrowed from any
-- AGPL implementation (D3 — ParadeDB pg_search is AGPL and barred by D1; read for technique only).
-- Empty-leg handling: a doc matched by only one retriever still surfaces (FULL OUTER JOIN + COALESCE).
-- Idempotent: safe to re-run / load from docker-entrypoint-initdb.d.

CREATE EXTENSION IF NOT EXISTS vector;
CREATE SCHEMA IF NOT EXISTS ai;

-- ai.hybrid_search_rrf — fuse an FTS leg and a vector leg over a documents-shaped table via RRF.
--
--   tbl              : the source table (regclass)
--   id_col           : the id/key column name (returned, cast to text)
--   content_tsv_col  : a tsvector column (e.g. a GENERATED STORED to_tsvector('english', content)) — FTS leg
--   vector_col       : a pgvector column — vector leg
--   query_text       : text query for the FTS leg AND (when query_vector is NULL) embedded via theodb.embed
--   query_vector     : precomputed query vector for the vector leg (overrides embedding query_text)
--   k                : RRF smoothing constant (default 60, Cormack 2009); MUST be > 0
--   per_leg_limit    : rows fetched per leg before fusion (default 20)
--   result_limit     : fused rows returned (default 5)
--
-- Returns (id text, score real) ordered by fused score DESC.
CREATE OR REPLACE FUNCTION ai.hybrid_search_rrf(
    tbl              regclass,
    id_col           text,
    content_tsv_col  text,
    vector_col       text,
    query_text       text    DEFAULT NULL,
    query_vector     vector  DEFAULT NULL,
    k                int     DEFAULT 60,
    per_leg_limit    int     DEFAULT 20,
    result_limit     int     DEFAULT 5
)
RETURNS TABLE(id text, score real)
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    qvec vector;
BEGIN
    -- Fail-fast, typed (Rule 8). RRF denominator (k + rank) must stay positive.
    IF k IS NULL OR k <= 0 THEN
        RAISE EXCEPTION 'ai.hybrid_search_rrf: k must be > 0 (got %)', k USING ERRCODE = '22023';
    END IF;
    IF per_leg_limit IS NULL OR per_leg_limit <= 0 THEN
        RAISE EXCEPTION 'ai.hybrid_search_rrf: per_leg_limit must be > 0 (got %)', per_leg_limit USING ERRCODE = '22023';
    END IF;
    IF result_limit IS NULL OR result_limit <= 0 THEN
        RAISE EXCEPTION 'ai.hybrid_search_rrf: result_limit must be > 0 (got %)', result_limit USING ERRCODE = '22023';
    END IF;
    IF query_text IS NULL AND query_vector IS NULL THEN
        RAISE EXCEPTION 'ai.hybrid_search_rrf: provide query_text and/or query_vector' USING ERRCODE = '22023';
    END IF;

    -- Resolve the vector leg's query vector: explicit query_vector wins; else embed query_text (D4).
    qvec := query_vector;
    IF qvec IS NULL AND query_text IS NOT NULL THEN
        qvec := theodb.embed(query_text);
    END IF;

    -- One fusion source of truth (D2): RANK() per leg -> FULL OUTER JOIN -> summed COALESCE(1/(k+rank),0).
    -- Identifier args are quoted with %I (injection-safe); query values are passed as parameters ($1..$4).
    RETURN QUERY EXECUTE format($q$
        WITH vec AS (
            SELECT %1$I AS _id,
                   RANK() OVER (ORDER BY %2$I <=> $1) AS rank
            FROM %3$s
            WHERE %2$I IS NOT NULL AND $1 IS NOT NULL
            ORDER BY %2$I <=> $1
            LIMIT $3
        ),
        fts AS (
            -- Pin 'english' (matches the GENERATED to_tsvector('english',…) column; never rely on the
            -- cluster default_text_search_config — ADR D1). The explicit ORDER BY before LIMIT keeps the
            -- TOP per_leg_limit by relevance: with a GIN/@@ scan rows come back in heap order, so without
            -- it LIMIT would keep an arbitrary subset and silently drop the most-relevant FTS docs.
            SELECT %1$I AS _id,
                   RANK() OVER (ORDER BY ts_rank_cd(%4$I, plainto_tsquery('english', $2)) DESC) AS rank
            FROM %3$s
            WHERE $2 IS NOT NULL AND %4$I @@ plainto_tsquery('english', $2)
            ORDER BY ts_rank_cd(%4$I, plainto_tsquery('english', $2)) DESC
            LIMIT $3
        )
        SELECT COALESCE(vec._id, fts._id)::text AS id,
               (COALESCE(1.0 / ($4 + vec.rank), 0.0)
              + COALESCE(1.0 / ($4 + fts.rank), 0.0))::real AS score
        FROM vec
        FULL OUTER JOIN fts ON vec._id = fts._id
        -- Deterministic tie-break (id ASC) so equal fused scores order identically to the offline
        -- Python twin rrf_fuse (ADR D2 — one fusion definition, stable ordering).
        ORDER BY score DESC, id ASC
        LIMIT $5
    $q$, id_col, vector_col, tbl::text, content_tsv_col)
    USING qvec, query_text, per_leg_limit, k, result_limit;
END;
$$;

-- Least-privilege, consistent with theodb.embed: when query_vector is NULL this function calls
-- theodb.embed (server-side outbound HTTP). It is SECURITY INVOKER (default), so theodb.embed's own
-- REVOKE-from-PUBLIC is honored per-caller — but we REVOKE here too so the privilege model is explicit
-- and does not silently become an SSRF surface if someone ever switches this to SECURITY DEFINER.
REVOKE ALL ON FUNCTION ai.hybrid_search_rrf(regclass, text, text, text, text, vector, int, int, int) FROM PUBLIC;

COMMENT ON FUNCTION ai.hybrid_search_rrf(regclass, text, text, text, text, vector, int, int, int) IS
  'Hybrid search: fuse a PostgreSQL FTS leg (ts_rank_cd over a tsvector column) and a pgvector leg (<=>) '
  'via Reciprocal Rank Fusion (score = sum 1/(k+rank), k default 60 — Cormack et al. 2009). Empty legs are '
  'handled by FULL OUTER JOIN + COALESCE so a doc matched by only one retriever still surfaces. query_text '
  'feeds the FTS leg and, when query_vector is NULL, is embedded via theodb.embed for the vector leg. '
  'Identifier args are %I-quoted (injection-safe). The native ai.hybrid_search() JSON-array surface is a '
  'future thin wrapper over this one function (one fusion source of truth).';
