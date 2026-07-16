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
//! `embed` (portable domain logic: the HTTP call + parse) · `api` (api-surface: the `#[pg_extern]`
//! entrypoints in schema `theodb_rs` + the `extension_sql!` DDL wrappers, e.g. `theodb.embed`). `lib.rs`
//! is now the thin composition/module root only — the crate doc, `pg_module_magic!()`, the module map,
//! and the `pg_test` harness (M25 split the api-surface out into `api.rs`; ADR 0009).

::pgrx::pg_module_magic!();

/// Extension load hook (M34) — registers the `theodb_ivfflat` reloption kind (`WITH (lists=N)`) and the
/// `theodb_ivfflat.probes` scan GUC. Runs once in the postmaster before any DDL (pgrx honors a user `_PG_init`).
#[allow(non_snake_case)]
#[::pgrx::pg_guard]
pub unsafe extern "C-unwind" fn _PG_init() { unsafe {
    am::options::init();
    am::guc::init();
    // M92 spike — register the arbitrary-WHERE Custom Scan Provider methods + install the pathlist hook
    // (inert unless `theodb.enable_vecfilter` is on).
    am::customscan::init();
    // M99 Phase C2: register the columnar pre-commit flush callback (persists pending INSERT rows into durable
    // stripes + their MVCC catalog rows before commit).
    am::columnar::init();
    // M54: register the vectorizer background worker (only when preloaded — guarded internally, no-op in a
    // backend CREATE EXTENSION so it stays silent there).
    vectorizer::register_worker();
}}

mod am;
mod ann;
mod ann_query;
mod chat;
mod chunk;
mod dtype;
mod sbq;
mod sq8;
mod embed;
mod http;
mod hybrid;
mod migrate;
mod nl;
mod pg;
mod pq;
mod rerank;
mod vec;
mod vectorizer;

mod api;

// Rust-side unit tests for the input-validation guards (no network needed). The cross-language
// parity + HTTP behaviors are proven by the Python oracle (benchmarks/tests/test_embed_sql.py)
// against the rebuilt image.
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test(error = "theodb.embed: content must not be NULL")]
    fn embed_null_content_rejected() {
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text(NULL, NULL)");
    }

    #[pg_test(error = "theodb.embed: theodb.embedding_endpoint is not set — SET theodb.embedding_endpoint = 'http://host:port/v1/embeddings'")]
    fn embed_unset_endpoint_rejected() {
        // Ensure the GUC is unset for this test (ignore error if it was never set).
        Spi::run("RESET theodb.embedding_endpoint").ok();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text('x', NULL)");
    }

    #[pg_test(error = "theodb.embed: endpoint must be http(s)://")]
    fn embed_non_http_scheme_rejected() {
        Spi::run("SET theodb.embedding_endpoint = 'file:///etc/passwd'").unwrap();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text('x', NULL)");
    }

    #[pg_test(error = "theodb.embed: endpoint call failed: Connection refused (os error 111)")]
    fn embed_unreachable_endpoint_fails_typed() {
        // Port 1 is unreachable -> connect error -> "call failed" (38000). The OS error (111 = ECONNREFUSED
        // on Linux) is part of the exact match pgrx-tests 0.16.1 requires; the suite runs in the Linux builder.
        Spi::run("SET theodb.embedding_endpoint = 'http://127.0.0.1:1/v1/embeddings'").unwrap();
        let _ = Spi::get_one::<String>("SELECT theodb_rs._embed_text('x', NULL)");
    }

    #[pg_test(error = "theodb.embed_batch: array elements must not be NULL")]
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

    // ── M65 — ai.rerank offline guards (input-path + SSRF + no-HTTP short-circuit; no network) ──────────

    #[pg_test(error = "ai.rerank: query must not be NULL")]
    fn rerank_null_query_rejected() {
        Spi::run("SET theodb.rerank_endpoint = 'http://127.0.0.1:1/rerank'").unwrap();
        Spi::run("SELECT * FROM theodb_rs._ai_rerank(NULL, ARRAY['d']::text[], NULL, NULL)").unwrap();
    }

    #[pg_test(error = "ai.rerank: documents must not be NULL")]
    fn rerank_null_documents_rejected() {
        Spi::run("SET theodb.rerank_endpoint = 'http://127.0.0.1:1/rerank'").unwrap();
        Spi::run("SELECT * FROM theodb_rs._ai_rerank('q', NULL, NULL, NULL)").unwrap();
    }

    #[pg_test(error = "ai.rerank: document array elements must not be NULL")]
    fn rerank_null_element_rejected() {
        // A NULL element breaks N-in/N-out alignment -> 22023, BEFORE any GUC/HTTP (endpoint set but unused).
        Spi::run("SET theodb.rerank_endpoint = 'http://127.0.0.1:1/rerank'").unwrap();
        Spi::run("SELECT * FROM theodb_rs._ai_rerank('q', ARRAY['d', NULL]::text[], NULL, NULL)").unwrap();
    }

    #[pg_test]
    fn rerank_empty_documents_makes_no_call() {
        // Empty docs -> zero rows with NO HTTP call (endpoint unreachable; if called this would error).
        Spi::run("SET theodb.rerank_endpoint = 'http://127.0.0.1:1/rerank'").unwrap();
        let n = Spi::get_one::<i64>(
            "SELECT count(*) FROM theodb_rs._ai_rerank('q', ARRAY[]::text[], NULL, NULL)",
        )
        .unwrap();
        assert_eq!(n, Some(0));
    }

    #[pg_test(error = "ai.rerank: theodb.rerank_endpoint is not set — SET theodb.rerank_endpoint = 'http://host:port/rerank'")]
    fn rerank_unset_endpoint_rejected() {
        Spi::run("RESET theodb.rerank_endpoint").ok();
        Spi::run("SELECT * FROM theodb_rs._ai_rerank('q', ARRAY['d']::text[], NULL, NULL)").unwrap();
    }

    #[pg_test(error = "ai.rerank: endpoint must be http(s)://")]
    fn rerank_non_http_scheme_rejected() {
        Spi::run("SET theodb.rerank_endpoint = 'file:///etc/passwd'").unwrap();
        Spi::run("SELECT * FROM theodb_rs._ai_rerank('q', ARRAY['d']::text[], NULL, NULL)").unwrap();
    }

    #[pg_test(error = "ai.rerank: endpoint call failed: Connection refused (os error 111)")]
    fn rerank_unreachable_endpoint_fails_typed() {
        // Port 1 unreachable -> connect error -> "call failed" (38000). The OS error string is part of the
        // exact match pgrx-tests 0.16.1 requires (the suite runs in the Linux builder).
        Spi::run("SET theodb.rerank_endpoint = 'http://127.0.0.1:1/rerank'").unwrap();
        Spi::run("SELECT * FROM theodb_rs._ai_rerank('q', ARRAY['d']::text[], NULL, NULL)").unwrap();
    }
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec![]
    }
}
