//! pg-glue layer (blueprint ADR-1): the boundary between TheoDB's portable domain logic and
//! PostgreSQL/pgrx internals — typed-error `ereport` + session-GUC reads. Nothing here is
//! embedding-specific; it is the only module that talks to the Postgres ABI.
use pgrx::prelude::*;

/// Raise SQLSTATE 22023 (invalid_parameter_value) for input/config errors. Diverges.
pub(crate) fn err_input(msg: &str) -> ! {
    pgrx::pg_sys::panic::ErrorReport::new(
        PgSqlErrorCode::ERRCODE_INVALID_PARAMETER_VALUE,
        msg.to_string(),
        "theodb.embed",
    )
    .report(PgLogLevel::ERROR);
    unreachable!()
}

/// Raise SQLSTATE 38000 (external_routine_exception) for HTTP/response failures. Diverges.
pub(crate) fn err_external(msg: &str) -> ! {
    pgrx::pg_sys::panic::ErrorReport::new(
        PgSqlErrorCode::ERRCODE_EXTERNAL_ROUTINE_EXCEPTION,
        msg.to_string(),
        "theodb.embed",
    )
    .report(PgLogLevel::ERROR);
    unreachable!()
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
