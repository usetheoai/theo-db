//! TheoDB's own PostgreSQL extension in Rust (pgrx) — M17 (ROADMAP-v2 / ADR 0006).
//!
//! `theodb.embed` (formerly plpython3u + urllib, `sql/30-theodb-embed.sql`) rewritten in Rust at
//! **proven parity**: same signature, same vector output, same typed SQLSTATEs as the baseline.
//! Mirrors AlloyDB's embedding() pattern — the DB calls a configurable model endpoint (it does NOT
//! ship a model). Configuration via the same session GUCs:
//!   SET theodb.embedding_endpoint = 'http://host:8088/v1/embeddings';   -- required
//!   SET theodb.embedding_model    = 'BAAI/bge-small-en-v1.5';           -- optional
//!   SET theodb.embedding_api_key  = '...';                              -- optional bearer token
//!
//! Layering (blueprint ADR-1 — 3 boundaries): `pg` (Postgres/pgrx glue: typed errors + GUCs) ·
//! `embed` (portable domain logic: the HTTP call + parse) · this file (api-surface: the `#[pg_extern]`
//! entrypoint + the `theodb.embed` SQL wrapper). `lib.rs` is the composition/api root + module map.
use pgrx::prelude::*;

::pgrx::pg_module_magic!();

mod ann;
mod ann_query;
mod chat;
mod sbq;
mod embed;
mod http;
mod hybrid;
mod migrate;
mod nl;
mod pg;
mod vec;

// theodb_rs owns its OWN schema `theodb_rs` (so it never tries to CREATE the `theodb` schema, which
// is owned by the umbrella `theodb` extension — PG forbids a second extension from CREATE-IF-NOT-EXISTS
// on a schema it does not own). The public `theodb.embed` wrapper is created INTO the existing `theodb`
// schema via extension_sql (creating an object in another extension's schema is allowed; only CREATE
// SCHEMA conflicts). theodb_rs `requires = 'theodb'` so the `theodb` schema exists first.
#[pg_schema]
mod theodb_rs {
    use pgrx::prelude::*;

    /// api-surface: the `theodb_rs._embed_text(content, model)` entrypoint. A thin delegate to the
    /// domain logic in `crate::embed` — the SQL wrapper `theodb.embed` casts its text output to `vector`.
    /// Internal: REVOKEd from PUBLIC alongside the wrapper.
    #[pg_extern]
    fn _embed_text(content: Option<&str>, model: Option<&str>) -> String {
        crate::embed::run(content, model)
    }

    /// api-surface: the `theodb_rs._embed_batch_text(content[], model)` entrypoint — N inputs → N
    /// pgvector text literals in ONE HTTP round-trip (the N→1 fix for the audit's CRITICAL embed N+1).
    /// A thin delegate to `crate::embed::run_batch`; the SQL wrapper `theodb.embed_batch` casts each
    /// element `::vector`. Internal: REVOKEd from PUBLIC alongside the wrapper.
    #[pg_extern]
    fn _embed_batch_text(content: Vec<Option<String>>, model: Option<&str>) -> Vec<String> {
        // text[] maps to Vec<Option<String>> (nullable elements); borrow each as &str for the domain
        // layer (which detects NULL elements -> 22023). `content` lives through the call.
        let refs: Vec<Option<&str>> = content.iter().map(|o| o.as_deref()).collect();
        crate::embed::run_batch(&refs, model)
    }

    // ── M18: the generative ai.* surface (was plpython3u in sql/50) ──────────────────────────────────
    // Thin delegates to `crate::chat`; the public `ai.*` SQL wrappers (below) carry the documented names,
    // return types, VOLATILE, and REVOKE. `ai._chat` stays SQL-callable so ai.generate/summarize/the
    // aggregate finalfunc / M19 nl_to_sql keep working.

    /// `theodb_rs._ai_chat` — one chat-completions round-trip (the SQL `ai._chat`).
    #[pg_extern]
    fn _ai_chat(prompt: Option<&str>, system: Option<&str>, model: Option<&str>) -> String {
        crate::chat::chat(prompt, system, model)
    }

    /// `theodb_rs._ai_if` — natural-language condition -> boolean (the SQL `ai.if`).
    #[pg_extern]
    fn _ai_if(prompt: Option<&str>, model: Option<&str>) -> bool {
        crate::chat::ai_if(prompt, model)
    }

    /// `theodb_rs._ai_sentiment` — content -> {positive,negative,neutral} (the SQL `ai.analyze_sentiment`).
    #[pg_extern]
    fn _ai_sentiment(content: Option<&str>, model: Option<&str>) -> String {
        crate::chat::ai_sentiment(content, model)
    }

    /// `theodb_rs._ai_rank` — natural-language scoring -> real (the SQL `ai.rank`).
    #[pg_extern]
    fn _ai_rank(prompt: Option<&str>, model: Option<&str>) -> f32 {
        crate::chat::ai_rank(prompt, model)
    }

    /// `theodb_rs._ai_generate_batch` — N prompts -> N answers in ONE round-trip (the SQL `ai.generate_batch`).
    /// NULL array -> 22023; NULL element -> 22023; empty -> empty (no call); JSON null -> SQL NULL element.
    #[pg_extern]
    fn _ai_generate_batch(
        prompts: Option<Vec<Option<String>>>,
        model: Option<&str>,
    ) -> Vec<Option<String>> {
        let prompts = match prompts {
            Some(p) => p,
            None => crate::pg::err_input("ai.generate_batch: prompts must not be NULL"),
        };
        let refs: Vec<Option<&str>> = prompts.iter().map(|o| o.as_deref()).collect();
        crate::chat::ai_generate_batch(&refs, model)
    }

    // ── M19: NL→SQL (the last plpython3u) — anti-injection L1/L2/L4 + L3 sandbox, now Rust ───────────────
    /// `theodb_rs._nl_to_sql` — validate a question into ONE read-only SELECT over the allowlist (the SQL
    /// `ai.nl_to_sql`). Returns the validated SQL; raises 22023 on any violation. Does NOT execute.
    #[pg_extern]
    fn _nl_to_sql(question: Option<&str>, allowed_relations: Vec<Option<String>>, model: Option<&str>) -> String {
        let refs: Vec<Option<&str>> = allowed_relations.iter().map(|o| o.as_deref()).collect();
        crate::nl::nl_to_sql(question, &refs, model)
    }

    // ── M19: hybrid search (RRF) — Rust entrypoints orchestrating the fusion SQL via SPI (crate::hybrid) ──
    /// `theodb_rs._hybrid_search_rrf` — the RRF hybrid-search entrypoint (the SQL `ai.hybrid_search_rrf`).
    /// The public wrapper passes `tbl::text` (regclass→quoted name) and `query_vector::text`.
    #[pg_extern]
    #[allow(clippy::too_many_arguments)]
    fn _hybrid_search_rrf(
        tbl_text: &str,
        id_col: &str,
        content_tsv_col: &str,
        vector_col: &str,
        query_text: Option<&str>,
        query_vector_text: Option<&str>,
        k: i32,
        per_leg_limit: i32,
        result_limit: i32,
    ) -> TableIterator<'static, (name!(id, String), name!(score, f32))> {
        TableIterator::new(crate::hybrid::run_rrf(
            tbl_text, id_col, content_tsv_col, vector_col, query_text, query_vector_text, k,
            per_leg_limit, result_limit,
        ))
    }

    /// `theodb_rs._hybrid_search_json` — the literal spec-06 JSON surface (the SQL `ai.hybrid_search(jsonb)`).
    /// Delegates to the SAME fusion as `_hybrid_search_rrf` (one fusion source of truth). Missing keys → 22023.
    #[pg_extern]
    fn _hybrid_search_json(
        config: pgrx::JsonB,
    ) -> TableIterator<'static, (name!(id, String), name!(score, f32))> {
        TableIterator::new(crate::hybrid::run_rrf_json(config.0))
    }

    // ── M19: Pinecone import — Rust loop + %I-quoted INSERT via SPI (crate::migrate) ─────────────────────
    /// `theodb_rs._import_pinecone` — the Pinecone import entrypoint (the SQL `theodb.import_pinecone`).
    /// The public wrapper passes `target::text` (regclass→quoted name). Returns the count of inserted records.
    #[pg_extern]
    fn _import_pinecone(
        target_text: &str,
        export: pgrx::JsonB,
        id_col: &str,
        embedding_col: &str,
        metadata_col: &str,
    ) -> i32 {
        crate::migrate::import(target_text, export.0, id_col, embedding_col, metadata_col)
    }

    // ── M20: own f32-parity distance ops over pgvector's values (crate::vec) ─────────────────────────────
    // The public `theodb.*` wrappers cast `vector::real[]` (pgvector's lossless cast) so these receive the
    // exact f32 payload as a pgrx-native Vec<f32> (no unsafe FFI; pgrx handles detoast). Coexistence (ADR D1).
    /// `theodb_rs._vec_l2` — L2 distance `<->` (the SQL `theodb.l2_distance`).
    #[pg_extern(immutable, parallel_safe, strict)]
    fn _vec_l2(a: Vec<f32>, b: Vec<f32>) -> f64 {
        crate::vec::l2_distance(&a, &b)
    }

    /// `theodb_rs._vec_ip` — inner product (the SQL `theodb.inner_product`, byte-for-byte with pgvector's
    /// `inner_product`). The `<#>` operator distance is `-theodb.inner_product` (pgvector's
    /// `vector_negative_inner_product`); exposed positive for a clean 1:1 parity comparison.
    #[pg_extern(immutable, parallel_safe, strict)]
    fn _vec_ip(a: Vec<f32>, b: Vec<f32>) -> f64 {
        crate::vec::inner_product(&a, &b)
    }

    /// `theodb_rs._vec_cosine` — cosine distance `<=>` (the SQL `theodb.cosine_distance`).
    #[pg_extern(immutable, parallel_safe, strict)]
    fn _vec_cosine(a: Vec<f32>, b: Vec<f32>) -> f64 {
        crate::vec::cosine_distance(&a, &b)
    }

    // ── M21: own ANN index (HNSW + IVFFlat) search over a table column ───────────────────────────────────
    // The public `theodb.hnsw_knn` / `theodb.ivfflat_knn` wrappers (regclass arg) cast the table `::text` and
    // call these. Build-once-answer-batch: `queries` is the flattened concatenation of query vectors in
    // `qdim`-sized chunks (one build per call, ADR D1). VOLATILE (reads a table) — NOT immutable. Coexistence:
    // reads `embed_col::real[]` only; never touches pgvector's index/storage (ADR D1). REVOKEd from PUBLIC.
    /// `theodb_rs._hnsw_knn` — own HNSW top-k over `src_table.embed_col`.
    #[allow(clippy::too_many_arguments)]
    #[pg_extern(volatile)] // reads a table via Spi — explicitly VOLATILE (never IMMUTABLE)
    fn _hnsw_knn(
        src_table: &str,
        embed_col: &str,
        id_col: &str,
        metric: &str,
        queries: Vec<f32>,
        qdim: i32,
        k: i32,
        m: i32,
        ef_construction: i32,
        ef_search: i32,
        seed: i64,
    ) -> TableIterator<'static, (name!(query_idx, i32), name!(id, i64), name!(distance, f64))> {
        let rows = crate::ann_query::knn(
            crate::ann_query::Algo::Hnsw,
            src_table,
            embed_col,
            id_col,
            metric,
            &queries,
            qdim,
            crate::ann_query::Params { k, m, ef_construction, ef_search, lists: 1, probes: 1, seed },
        );
        TableIterator::new(rows)
    }

    /// `theodb_rs._ivfflat_knn` — own IVFFlat top-k over `src_table.embed_col`.
    #[allow(clippy::too_many_arguments)]
    #[pg_extern(volatile)] // reads a table via Spi — explicitly VOLATILE (never IMMUTABLE)
    fn _ivfflat_knn(
        src_table: &str,
        embed_col: &str,
        id_col: &str,
        metric: &str,
        queries: Vec<f32>,
        qdim: i32,
        k: i32,
        lists: i32,
        probes: i32,
        seed: i64,
    ) -> TableIterator<'static, (name!(query_idx, i32), name!(id, i64), name!(distance, f64))> {
        let rows = crate::ann_query::knn(
            crate::ann_query::Algo::Ivfflat,
            src_table,
            embed_col,
            id_col,
            metric,
            &queries,
            qdim,
            crate::ann_query::Params { k, m: 16, ef_construction: 64, ef_search: 64, lists, probes, seed },
        );
        TableIterator::new(rows)
    }

    // ── M22: own SBQ scalar quantization + quantized ANN search (crate::sbq) ──────────────────────────────
    // The public `theodb.sbq_knn` wrapper (regclass arg) casts the table `::text` and flattens `queries
    // vector[]` to `real[]` + `qdim`. Build-once-answer-batch: quantize the corpus (own SBQ), candidate-gen via
    // the M21 IVFFlat carrier, Hamming rank, full-precision f32 rerank. VOLATILE (reads a table). Coexistence:
    // reads `embed_col::real[]`; never touches pgvector/pgvectorscale. REVOKEd from PUBLIC.
    /// `theodb_rs._sbq_knn` — own SBQ quantized top-k over `src_table.embed_col`.
    #[allow(clippy::too_many_arguments)]
    #[pg_extern(volatile)] // reads a table via Spi — explicitly VOLATILE (never IMMUTABLE)
    fn _sbq_knn(
        src_table: &str,
        embed_col: &str,
        id_col: &str,
        metric: &str,
        queries: Vec<f32>,
        qdim: i32,
        k: i32,
        bits: i32,
        lists: i32,
        probes: i32,
        over_fetch: i32,
        seed: i64,
    ) -> TableIterator<'static, (name!(query_idx, i32), name!(id, i64), name!(distance, f64))> {
        let rows = crate::sbq::knn(
            src_table,
            embed_col,
            id_col,
            metric,
            &queries,
            crate::sbq::SbqParams { qdim, k, bits, lists, probes, over_fetch, seed },
        );
        TableIterator::new(rows)
    }

    /// `theodb_rs._sbq_bytes_per_vector` — the own SBQ storage footprint (bytes/vector) at `dim` × `bits`
    /// (`ceil(dim·bits/64)·8`). The public `theodb.sbq_bytes_per_vector` exposes it for the memory gate; f32
    /// baseline is `4·dim`. IMMUTABLE/STRICT (a pure formula).
    #[pg_extern(immutable, parallel_safe, strict)]
    fn _sbq_bytes_per_vector(dim: i32, bits: i32) -> i64 {
        // Upper-bound dim too (defensive: keeps dim*bits well within usize/i64, no overflow on any arch).
        crate::ann_query::require((1..=1_000_000).contains(&dim), "theodb sbq: dim must be in [1, 1000000]");
        crate::ann_query::require((1..=8).contains(&bits), "theodb sbq: bits must be in [1, 8]");
        crate::sbq::SbqQuantizer::bytes_per_vector(dim as usize, bits as u8) as i64
    }
}

// SQL wrapper: the public `theodb.embed(content, model DEFAULT NULL) RETURNS vector`. Casts the Rust
// function's text output to `vector` via pgvector's input function (plan ADR D5 — no extra crate).
// Both functions are REVOKEd from PUBLIC (least-privilege parity with sql/30:80).
extension_sql!(
    r#"
CREATE FUNCTION theodb.embed(content text, model text DEFAULT NULL)
RETURNS vector
LANGUAGE sql
AS $$ SELECT theodb_rs._embed_text(content, model)::vector $$;

COMMENT ON FUNCTION theodb.embed(text, text) IS
  'Generate an embedding for content via the configurable model endpoint (theodb.embedding_endpoint). '
  'Returns a pgvector value. Implemented in Rust (theodb_rs extension, M17). '
  'SECURITY: server-side outbound HTTP to the configured endpoint (http(s) only, no redirects); '
  'not granted to PUBLIC. theodb.embedding_api_key is a session GUC (visible to SHOW / captured by '
  'log_statement) — set it per-session out of band, not in logged DDL. '
  'CALL IS SYNCHRONOUS: one blocking HTTP round-trip per row.';

REVOKE ALL ON FUNCTION theodb.embed(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._embed_text(text, text) FROM PUBLIC;
"#,
    name = "theodb_embed_wrapper",
    // Reference the pgrx function as a bare-ident PositioningRef::FullPath (matches the extern by
    // `unaliased_name`, like pgvectorscale's `requires = [smallint_array_overlap]`) so the wrapper's
    // SQL-language body, which validates `theodb_rs._embed_text` at CREATE time, is emitted AFTER it.
    requires = [_embed_text],
);

// SQL wrapper: the public `theodb.embed_batch(content text[], model DEFAULT NULL) RETURNS vector[]`.
// Casts each Rust text-literal output `::vector` and preserves the input order via WITH ORDINALITY.
// COALESCE makes an empty input array return an empty `vector[]` (NOT NULL) — the array_agg over zero
// rows is NULL otherwise (edge-case EC-1). REVOKEd from PUBLIC (least-privilege parity with embed).
extension_sql!(
    r#"
CREATE FUNCTION theodb.embed_batch(content text[], model text DEFAULT NULL)
RETURNS vector[]
LANGUAGE sql
AS $$
  SELECT COALESCE(array_agg(t::vector ORDER BY ord), ARRAY[]::vector[])
  FROM unnest(theodb_rs._embed_batch_text(content, model)) WITH ORDINALITY AS u(t, ord)
$$;

COMMENT ON FUNCTION theodb.embed_batch(text[], text) IS
  'Generate embeddings for an array of inputs in ONE HTTP round-trip (collapses the per-row embed N+1). '
  'Returns vector[] aligned to the input order. Implemented in Rust (theodb_rs, audit-remediation). '
  'Mirrors ai.generate_batch N-in/N-out: a size mismatch is a typed error, NULL elements are rejected, '
  'an empty array returns an empty vector[] with no HTTP call. Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION theodb.embed_batch(text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._embed_batch_text(text[], text) FROM PUBLIC;
"#,
    name = "theodb_embed_batch_wrapper",
    requires = [_embed_batch_text],
);

// SQL wrappers: the public generative `ai.*` surface (M18 — was plpython3u in sql/50, now Rust). Created
// INTO the existing `ai` schema (owned by the `theodb` umbrella; theodb_rs `requires = theodb` so it exists
// first). Exact public signatures / RETURNS / VOLATILE preserved; every function REVOKEd from PUBLIC (it
// makes server-side outbound HTTP). `ai._chat` stays SQL-callable (ai.generate/summarize/the aggregate
// finalfunc + M19 nl_to_sql depend on it by name).
extension_sql!(
    r#"
CREATE FUNCTION ai._chat(prompt text, system text DEFAULT NULL, model text DEFAULT NULL)
RETURNS text LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_chat(prompt, system, model) $$;

CREATE FUNCTION ai."if"(prompt text, model text DEFAULT NULL)
RETURNS boolean LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_if(prompt, model) $$;

CREATE FUNCTION ai.analyze_sentiment(content text, model text DEFAULT NULL)
RETURNS text LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_sentiment(content, model) $$;

CREATE FUNCTION ai.rank(prompt text, model text DEFAULT NULL)
RETURNS real LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_rank(prompt, model) $$;

CREATE FUNCTION ai.generate_batch(prompts text[], model text DEFAULT NULL)
RETURNS text[] LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._ai_generate_batch(prompts, model) $$;

COMMENT ON FUNCTION ai._chat(text, text, text) IS
  'PRIVATE: one configurable chat-completions round-trip (theodb.llm_endpoint, http(s)-only, no redirects) '
  '+ parse of choices[0].message.content. Single HTTP source of truth for the public ai.* functions. '
  'Implemented in Rust (theodb_rs, M18). Not granted to PUBLIC. theodb.llm_api_key is a session GUC.';

REVOKE ALL ON FUNCTION ai._chat(text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai."if"(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.analyze_sentiment(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.rank(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.generate_batch(text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_chat(text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_if(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_sentiment(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_rank(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ai_generate_batch(text[], text) FROM PUBLIC;
"#,
    name = "theodb_ai_wrappers",
    requires = [_ai_chat, _ai_if, _ai_sentiment, _ai_rank, _ai_generate_batch],
);

// SQL wrappers: the public NL→SQL surface (M19 — `ai.nl_to_sql` was the last plpython3u, now Rust). Created
// INTO the existing `ai` schema; exact public signatures preserved; REVOKEd from PUBLIC (they make an
// outbound LLM call via ai._chat and ai.nl_query executes dynamic SQL). The M12 config layer (sql/61) and
// `ai.nl_query` resolve `ai.nl_to_sql` by name.
extension_sql!(
    r#"
CREATE FUNCTION ai.nl_to_sql(question text, allowed_relations text[], model text DEFAULT NULL)
RETURNS text LANGUAGE sql VOLATILE
AS $$ SELECT theodb_rs._nl_to_sql(question, allowed_relations, model) $$;

COMMENT ON FUNCTION ai.nl_to_sql(text, text[], text) IS
  'Translate a natural-language question into ONE validated read-only SELECT over allowed_relations (via the '
  'configurable model). Defense: L2 static validation (single statement, SELECT/WITH-only, banned-function '
  'denylist) + L4 parser-grade relation allowlist (EXPLAIN enumerates every planned relation). Fail-fast '
  '22023. Does NOT execute. Implemented in Rust (theodb_rs, M19). Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION ai.nl_to_sql(text, text[], text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._nl_to_sql(text, text[], text) FROM PUBLIC;
"#,
    name = "theodb_nl_wrappers",
    requires = [_nl_to_sql],
);

// SQL wrappers: the public hybrid-search surface (M19 — was plpgsql in sql/40, now Rust). Created INTO the
// existing `ai` schema. The public ai.hybrid_search_rrf keeps its exact named parameters + DEFAULTs (callers
// use tbl => …, query_vector => … named notation) and the `regclass`/`vector` types pgrx cannot express
// natively — the thin SQL wrapper bridges them to the text-typed Rust entrypoint (tbl::text, query_vector::text).
// RETURNS TABLE(id text, score real) preserved. Both REVOKEd from PUBLIC (the embed leg makes outbound HTTP).
// NOTE (M19 scope decision): hybrid now co-resides with theodb.embed in theodb_rs, so the former cross-extension
// seam guard scenario (drop theodb_rs, hybrid survives → 0A000) no longer applies — dropping theodb_rs removes
// both (the function itself → 42883). The defensive 0A000 guard remains for an individually-dropped embed.
extension_sql!(
    r#"
CREATE FUNCTION ai.hybrid_search_rrf(
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
LANGUAGE sql STABLE
AS $$
  SELECT id, score FROM theodb_rs._hybrid_search_rrf(
    tbl::text, id_col, content_tsv_col, vector_col, query_text, query_vector::text,
    k, per_leg_limit, result_limit)
$$;

CREATE FUNCTION ai.hybrid_search(config jsonb)
RETURNS TABLE(id text, score real)
LANGUAGE sql STABLE
AS $$ SELECT id, score FROM theodb_rs._hybrid_search_json(config) $$;

COMMENT ON FUNCTION ai.hybrid_search_rrf(regclass, text, text, text, text, vector, int, int, int) IS
  'Hybrid search: fuse a PostgreSQL FTS leg (ts_rank_cd over a tsvector column) and a pgvector leg (<=>) via '
  'Reciprocal Rank Fusion (score = sum 1/(k+rank), k default 60 — Cormack et al. 2009). Empty legs handled by '
  'FULL OUTER JOIN + COALESCE. query_text feeds FTS and, when query_vector is NULL, is embedded via '
  'theodb.embed. Implemented in Rust (theodb_rs, M19) — orchestrates ONE fusion SQL via SPI (one fusion '
  'source of truth). Identifier args are %I-quoted (injection-safe). Not granted to PUBLIC.';

COMMENT ON FUNCTION ai.hybrid_search(jsonb) IS
  'Literal spec-06 JSON surface over ai.hybrid_search_rrf (one fusion definition). Implemented in Rust '
  '(theodb_rs, M19). Fail-fast 22023 on missing required keys (table, id_col, content_tsv_col, vector_col). '
  'Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION ai.hybrid_search_rrf(regclass, text, text, text, text, vector, int, int, int) FROM PUBLIC;
REVOKE ALL ON FUNCTION ai.hybrid_search(jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._hybrid_search_rrf(text, text, text, text, text, text, int, int, int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._hybrid_search_json(jsonb) FROM PUBLIC;
"#,
    name = "theodb_hybrid_wrappers",
    requires = [_hybrid_search_rrf, _hybrid_search_json],
);

// SQL wrapper: the public migration helper `theodb.import_pinecone` (M19 — was plpgsql in sql/80, now Rust).
// The chunked PROCEDURE theodb.import_pinecone_chunked STAYS plpgsql (ADR-D — only a plpgsql PROCEDURE can
// COMMIT per batch). Created INTO the existing `theodb` schema; exact signature + DEFAULTs preserved; REVOKEd
// from PUBLIC (writes to caller-owned tables). The thin wrapper bridges `regclass` to the text-typed Rust fn.
extension_sql!(
    r#"
CREATE FUNCTION theodb.import_pinecone(
    target        regclass,
    export        jsonb,
    id_col        text DEFAULT 'id',
    embedding_col text DEFAULT 'embedding',
    metadata_col  text DEFAULT 'metadata'
) RETURNS integer
LANGUAGE sql
AS $$ SELECT theodb_rs._import_pinecone(target::text, export, id_col, embedding_col, metadata_col) $$;

COMMENT ON FUNCTION theodb.import_pinecone(regclass, jsonb, text, text, text) IS
  'Import a Pinecone export (JSON array of {id,values,metadata}) into a TheoDB table (id, embedding vector, '
  'metadata jsonb). Native jsonb (serde); safe dynamic SQL (%I-quoted, regclass-validated, parameter-bound). '
  'Implemented in Rust (theodb_rs, M19). Fail-fast 22023 on a non-array export or a record missing id/values. '
  'For large/atomic-vs-chunked imports see theodb.import_pinecone_chunked (PROCEDURE). Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION theodb.import_pinecone(regclass, jsonb, text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._import_pinecone(text, jsonb, text, text, text) FROM PUBLIC;
"#,
    name = "theodb_import_wrapper",
    requires = [_import_pinecone],
);

// SQL wrappers: TheoDB's own distance functions (M20 — own f32-parity ops over pgvector's values). Created
// INTO the existing `theodb` schema; cast `vector::real[]` (pgvector's lossless IMPLICIT cast) so the Rust
// fns receive the exact f32 payload. COEXISTENCE (ADR D1): these are NEW functions — they do NOT redefine
// pgvector's `<->`/`<#>`/`<=>` operators on the shared `vector` type (no conflict), and pgvector's
// type/indexes are untouched. STRICT (NULL in → NULL out, parity with pgvector) + IMMUTABLE (pure). REVOKE
// parity. `theodb.inner_product` mirrors pgvector's positive `inner_product`; the `<#>` distance is its
// negation (pgvector `vector_negative_inner_product`). `theodb_rs requires theodb requires vector`, so the
// `vector` type + its `::real[]` cast exist at CREATE time.
extension_sql!(
    r#"
CREATE FUNCTION theodb.l2_distance(a vector, b vector) RETURNS float8
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
AS $$ SELECT theodb_rs._vec_l2(a::real[], b::real[]) $$;

CREATE FUNCTION theodb.inner_product(a vector, b vector) RETURNS float8
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
AS $$ SELECT theodb_rs._vec_ip(a::real[], b::real[]) $$;

CREATE FUNCTION theodb.cosine_distance(a vector, b vector) RETURNS float8
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
AS $$ SELECT theodb_rs._vec_cosine(a::real[], b::real[]) $$;

COMMENT ON FUNCTION theodb.l2_distance(vector, vector) IS
  'TheoDB own L2 distance (<->) over pgvector values, at f32 numeric parity with pgvector.l2_distance. '
  'Implemented in Rust (theodb_rs, M20). Coexists with pgvector (reads vector::real[]; no competing type).';
COMMENT ON FUNCTION theodb.inner_product(vector, vector) IS
  'TheoDB own inner product over pgvector values, at f32 parity with pgvector.inner_product. The <#> distance '
  'is -theodb.inner_product. Implemented in Rust (theodb_rs, M20).';
COMMENT ON FUNCTION theodb.cosine_distance(vector, vector) IS
  'TheoDB own cosine distance (<=>) over pgvector values, at f32 parity with pgvector.cosine_distance. '
  'Implemented in Rust (theodb_rs, M20).';

REVOKE ALL ON FUNCTION theodb.l2_distance(vector, vector) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.inner_product(vector, vector) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.cosine_distance(vector, vector) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._vec_l2(real[], real[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._vec_ip(real[], real[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._vec_cosine(real[], real[]) FROM PUBLIC;
"#,
    name = "theodb_vector_ops_wrapper",
    requires = [_vec_l2, _vec_ip, _vec_cosine],
);

// SQL wrappers: TheoDB's own ANN index search (M21 — own HNSW + IVFFlat in Rust, recall-gated). Created INTO
// the existing `theodb` schema. The public surface takes `queries vector[]`; the wrapper flattens it (query-
// major) into a `real[]` + derives `qdim` from the first query, bridging to the text/real[]-typed Rust extern
// (pgrx cannot express `regclass`/`vector[]` natively). RETURNS TABLE(query_idx int, id bigint, distance
// float8). VOLATILE (reads a table). COEXISTENCE (ADR D1): reads `embed_col::real[]` only — pgvector's type,
// operators, and HNSW/IVFFlat indexes are untouched; this is an ADDITIONAL own-algorithm path, not a
// replacement. Empty `queries` → empty real[] → 0 rows (EC-5). REVOKEd from PUBLIC (reads caller-owned tables).
extension_sql!(
    r#"
CREATE FUNCTION theodb.hnsw_knn(
    src_table       regclass,
    embed_col       text,
    queries         vector[],
    k               int    DEFAULT 10,
    m               int    DEFAULT 16,
    ef_construction int    DEFAULT 64,
    ef_search       int    DEFAULT 40,
    metric          text   DEFAULT 'l2',
    id_col          text   DEFAULT 'id',
    seed            bigint DEFAULT 42
) RETURNS TABLE(query_idx int, id bigint, distance float8)
LANGUAGE sql VOLATILE
AS $$
  SELECT query_idx, id, distance FROM theodb_rs._hnsw_knn(
    src_table::text, embed_col, id_col, metric,
    (SELECT COALESCE(array_agg(x ORDER BY qi, ci), ARRAY[]::real[])
       FROM unnest(queries) WITH ORDINALITY AS u(qv, qi),
            unnest(qv::real[]) WITH ORDINALITY AS e(x, ci)),
    COALESCE(array_length((queries[1])::real[], 1), 1),
    k, m, ef_construction, ef_search, seed)
$$;

CREATE FUNCTION theodb.ivfflat_knn(
    src_table       regclass,
    embed_col       text,
    queries         vector[],
    k               int    DEFAULT 10,
    lists           int    DEFAULT 100,
    probes          int    DEFAULT 1,
    metric          text   DEFAULT 'l2',
    id_col          text   DEFAULT 'id',
    seed            bigint DEFAULT 42
) RETURNS TABLE(query_idx int, id bigint, distance float8)
LANGUAGE sql VOLATILE
AS $$
  SELECT query_idx, id, distance FROM theodb_rs._ivfflat_knn(
    src_table::text, embed_col, id_col, metric,
    (SELECT COALESCE(array_agg(x ORDER BY qi, ci), ARRAY[]::real[])
       FROM unnest(queries) WITH ORDINALITY AS u(qv, qi),
            unnest(qv::real[]) WITH ORDINALITY AS e(x, ci)),
    COALESCE(array_length((queries[1])::real[], 1), 1),
    k, lists, probes, seed)
$$;

COMMENT ON FUNCTION theodb.hnsw_knn(regclass, text, vector[], int, int, int, int, text, text, bigint) IS
  'TheoDB own HNSW ANN search (M21): build a Hierarchical Navigable Small World graph over src_table.embed_col '
  'in Rust and answer a batch of top-k queries (<->/<#>/<=>). Recall@k is gated vs pgvector by '
  'benchmarks/bench_ann_index.py. Coexists with pgvector (reads vector::real[]; no competing index). '
  'Measurement-first SQL-callable form (ADR D1); not granted to PUBLIC.';
COMMENT ON FUNCTION theodb.ivfflat_knn(regclass, text, vector[], int, int, int, text, text, bigint) IS
  'TheoDB own IVFFlat ANN search (M21): k-means++ inverted lists over src_table.embed_col in Rust, scan the '
  'probes nearest lists. Recall@k gated vs pgvector. Coexists with pgvector. Not granted to PUBLIC.';

REVOKE ALL ON FUNCTION theodb.hnsw_knn(regclass, text, vector[], int, int, int, int, text, text, bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.ivfflat_knn(regclass, text, vector[], int, int, int, text, text, bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._hnsw_knn(text, text, text, text, real[], int, int, int, int, int, bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._ivfflat_knn(text, text, text, text, real[], int, int, int, int, bigint) FROM PUBLIC;
"#,
    name = "theodb_ann_wrappers",
    requires = [_hnsw_knn, _ivfflat_knn],
);

// SQL wrappers: TheoDB's own SBQ quantized search (M22 — own scalar quantization, recall + memory gated).
// Created INTO the existing `theodb` schema. `theodb.sbq_knn` takes `queries vector[]`, flattens it query-major
// into `real[]` + derives `qdim` from the first query (same bridge as M21). `theodb.sbq_bytes_per_vector` is the
// memory metric (bytes/vector, parity with pgvectorscale's formula + ~Nx vs f32). RETURNS TABLE(query_idx int,
// id bigint, distance float8). VOLATILE (sbq_knn reads a table). COEXISTENCE (ADR D3): reads `embed_col::real[]`
// only — pgvectorscale/pgvector untouched. REVOKEd from PUBLIC.
extension_sql!(
    r#"
CREATE FUNCTION theodb.sbq_knn(
    src_table  regclass,
    embed_col  text,
    queries    vector[],
    k          int    DEFAULT 10,
    bits       int    DEFAULT 1,
    lists      int    DEFAULT 100,
    probes     int    DEFAULT 1,
    over_fetch int    DEFAULT 4,
    metric     text   DEFAULT 'l2',
    id_col     text   DEFAULT 'id',
    seed       bigint DEFAULT 42
) RETURNS TABLE(query_idx int, id bigint, distance float8)
LANGUAGE sql VOLATILE
AS $$
  SELECT query_idx, id, distance FROM theodb_rs._sbq_knn(
    src_table::text, embed_col, id_col, metric,
    (SELECT COALESCE(array_agg(x ORDER BY qi, ci), ARRAY[]::real[])
       FROM unnest(queries) WITH ORDINALITY AS u(qv, qi),
            unnest(qv::real[]) WITH ORDINALITY AS e(x, ci)),
    COALESCE(array_length((queries[1])::real[], 1), 1),
    k, bits, lists, probes, over_fetch, seed)
$$;

CREATE FUNCTION theodb.sbq_bytes_per_vector(dim int, bits int DEFAULT 1)
RETURNS bigint LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
AS $$ SELECT theodb_rs._sbq_bytes_per_vector(dim, bits) $$;

COMMENT ON FUNCTION theodb.sbq_knn(regclass, text, vector[], int, int, int, int, int, text, text, bigint) IS
  'TheoDB own SBQ quantized ANN search (M22): per-dimension mean-threshold scalar bit quantization (own Rust, '
  'permissive — NOT the AGPL RaBitQ), candidate-gen via the M21 IVFFlat carrier, Hamming rank + full-precision '
  'f32 rerank. recall@k gated vs pgvectorscale; memory = bytes/vector (theodb.sbq_bytes_per_vector). Coexists '
  'with pgvectorscale/pgvector. Measurement-first SQL-callable (planner AM = M22b); not granted to PUBLIC.';
COMMENT ON FUNCTION theodb.sbq_bytes_per_vector(int, int) IS
  'Own SBQ storage footprint bytes/vector = ceil(dim*bits/64)*8 (parity with pgvectorscale at matched bits; '
  '~Nx reduction vs f32 4*dim). The memory metric for the M22 gate. Implemented in Rust (theodb_rs, M22).';

REVOKE ALL ON FUNCTION theodb.sbq_knn(regclass, text, vector[], int, int, int, int, int, text, text, bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._sbq_knn(text, text, text, text, real[], int, int, int, int, int, int, bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.sbq_bytes_per_vector(int, int) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb_rs._sbq_bytes_per_vector(int, int) FROM PUBLIC;
"#,
    name = "theodb_sbq_wrappers",
    requires = [_sbq_knn, _sbq_bytes_per_vector],
);

// Rust-side unit tests for the input-validation guards (no network needed). The cross-language
// parity + HTTP behaviors are proven by the Python oracle (benchmarks/tests/test_embed_sql.py)
// against the rebuilt image.
#[cfg(any(test, feature = "pg_test"))]
#[pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test(error = "must not be NULL")]
    fn embed_null_content_rejected() {
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text(NULL, NULL)");
    }

    #[pg_test(error = "embedding_endpoint is not set")]
    fn embed_unset_endpoint_rejected() {
        // Ensure the GUC is unset for this test (ignore error if it was never set).
        Spi::run("RESET theodb.embedding_endpoint").ok();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text('x', NULL)");
    }

    #[pg_test(error = "http(s)")]
    fn embed_non_http_scheme_rejected() {
        Spi::run("SET theodb.embedding_endpoint = 'file:///etc/passwd'").unwrap();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text('x', NULL)");
    }

    #[pg_test(error = "call failed")]
    fn embed_unreachable_endpoint_fails_typed() {
        // Port 1 is unreachable -> connect error -> "call failed" (38000).
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text('x', NULL)");
    }

    #[pg_test(error = "must not be NULL")]
    fn embed_batch_rejects_null_element() {
        // A NULL element breaks N-in/N-out alignment -> 22023, BEFORE any GUC/HTTP (endpoint set but unused).
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        Spi::run("SELECT theodb_rs._embed_batch_text(ARRAY['x', NULL]::text[], NULL)").unwrap();
    }

    #[pg_test]
    fn embed_batch_empty_makes_no_call() {
        // Empty input -> empty result with NO HTTP call (endpoint is unreachable; if it were called this
        // would error). Proves the no-HTTP short-circuit.
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        let n = Spi::get_one::<i64>(
            "SELECT cardinality(theodb_rs._embed_batch_text(ARRAY[]::text[], NULL))",
        )
        .unwrap();
        assert_eq!(n, Some(0));
    }
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec![]
    }
}
