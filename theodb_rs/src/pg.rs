//! pg-glue layer (blueprint ADR-1): typed-error `ereport` helpers + session-GUC reads shared across modules.
//! The portable modules (`embed`, `chat`) route ALL their ABI access through here; the SPI-orchestration
//! adapters (`nl`, `hybrid`, `migrate`) additionally call `Spi`/pgrx directly (the accepted ADR-C boundary).
//! Nothing here is embedding-specific.
use pgrx::prelude::*;

/// Raise SQLSTATE XX002 (index_corrupted) for a persisted index page that fails structural validation.
/// Diverges.
///
/// M146 T2.1 — antes disto os erros de desserialização do AM saíam por `pg_sys::error!`, que rende **XX000
/// (internal_error)**: indistinguível de um bug nosso, e portanto inútil para o operador decidir se deve
/// REINDEXAR ou abrir um issue. O precedente é o próprio PostgreSQL: o `contrib/amcheck` usa
/// `ERRCODE_INDEX_CORRUPTED` em dezenas de sites justamente para dar à corrupção um código próprio.
pub(crate) fn err_corrupt(msg: &str) -> ! {
    pgrx::pg_sys::panic::ErrorReport::new(
        PgSqlErrorCode::ERRCODE_INDEX_CORRUPTED,
        msg.to_string(),
        "theodb",
    )
    .report(PgLogLevel::ERROR);
    unreachable!()
}

/// Raise SQLSTATE 22023 (invalid_parameter_value) for input/config errors. Diverges.
pub(crate) fn err_input(msg: &str) -> ! {
    pgrx::pg_sys::panic::ErrorReport::new(
        PgSqlErrorCode::ERRCODE_INVALID_PARAMETER_VALUE,
        msg.to_string(),
        "theodb",
    )
    .report(PgLogLevel::ERROR);
    unreachable!()
}

/// Raise SQLSTATE 38000 (external_routine_exception) for HTTP/response failures. Diverges.
pub(crate) fn err_external(msg: &str) -> ! {
    pgrx::pg_sys::panic::ErrorReport::new(
        PgSqlErrorCode::ERRCODE_EXTERNAL_ROUTINE_EXCEPTION,
        msg.to_string(),
        "theodb",
    )
    .report(PgLogLevel::ERROR);
    unreachable!()
}

/// Raise SQLSTATE 0A000 (feature_not_supported) for an unavailable cross-extension seam — e.g. the
/// hybrid-search fail-fast guard when `theodb.embed` is missing (theodb_rs dropped). Diverges.
pub(crate) fn err_unsupported(msg: &str) -> ! {
    pgrx::pg_sys::panic::ErrorReport::new(
        PgSqlErrorCode::ERRCODE_FEATURE_NOT_SUPPORTED,
        msg.to_string(),
        "theodb",
    )
    .report(PgLogLevel::ERROR);
    unreachable!()
}

/// Emit a WARNING-level server log for a transient, retried failure. Observability for the retry path
/// (wiring-triad pillar c): without it, a flapping endpoint is invisible until the final hard failure.
/// Does not diverge — the caller continues to the next retry.
pub(crate) fn warn(msg: &str) {
    pgrx::warning!("{}", msg);
}

/// Read a session GUC by its (trusted, literal) name. Mirrors the plpython3u
/// `current_setting(name, true)` call — returns None when unset/empty.
pub(crate) fn guc(name: &str) -> Option<String> {
    // `name` is a hardcoded literal (no user input), exactly as the plpython3u version formatted it.
    Spi::get_one::<String>(&format!("SELECT current_setting('{name}', true)"))
        .ok()
        .flatten()
        .filter(|s| !s.is_empty())
}
