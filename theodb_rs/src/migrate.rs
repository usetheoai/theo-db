//! Domain layer (blueprint M19, ADR-C): the Pinecone import migration helper, ported from the plpgsql
//! `theodb.import_pinecone` FUNCTION (sql/80). Maps a Pinecone export {id, values, metadata} → a relational
//! table (id, embedding vector, metadata jsonb). The Rust function owns the loop + native jsonb parsing
//! (serde — no pinecone client, no stdlib json dependency); the INSERT is built with Postgres-native `%I`
//! quoting via `format()` over SPI (NOT hand-rolled), so the injection-safety of the plpgsql version is
//! preserved byte-for-byte (sql/80:38-44). The chunked PROCEDURE stays plpgsql (ADR-D): a plpgsql PROCEDURE
//! can COMMIT per batch, which a Rust #[pg_extern] function (running in the caller's transaction) cannot.
//!
//! Fails fast (22023) on a non-array export or a record missing id/values — no partial/corrupt insert beyond
//! the failing element (the whole statement aborts). A wrong-length `values` array surfaces the pgvector
//! typmod error (DataException, 22xxx) from the `$2::vector` cast — propagated faithfully, not reclassified.
use pgrx::prelude::*;
use serde_json::Value;

use crate::pg::err_input;

/// The INSERT template. `%1$s`=target (regclass::text, already quoted), `%2$I`=id_col, `%3$I`=embedding_col,
/// `%4$I`=metadata_col — substituted by Postgres `format()` (injection-safe). `$1..$3` are LITERAL here and
/// bind per record: $1=id (text), $2=values (text → ::vector), $3=metadata (jsonb). Byte-faithful to sql/80:39.
const INSERT_TEMPLATE: &str = "INSERT INTO %1$s (%2$I, %3$I, %4$I) VALUES ($1, $2::vector, $3)";

/// Import a Pinecone export into `target` and return the number of records inserted. `target_text` is a
/// `regclass::text` (already correctly quoted by Postgres). Diverges (22023) on a malformed export.
fn import(
    target_text: &str,
    export: Value,
    id_col: &str,
    embedding_col: &str,
    metadata_col: &str,
) -> i32 {
    let records = match export {
        Value::Array(a) => a,
        _ => err_input("theodb.import_pinecone: export must be a JSON array of records"),
    };

    // Build the INSERT once with Postgres-native %I quoting (injection-safe) — one format() call over SPI.
    let build_q = format!("SELECT format($fq${INSERT_TEMPLATE}$fq$, $1, $2, $3, $4)");
    let insert_sql = Spi::get_one_with_args::<String>(
        &build_q,
        &[target_text.into(), id_col.into(), embedding_col.into(), metadata_col.into()],
    )
    .ok()
    .flatten()
    .unwrap_or_else(|| err_input("theodb.import_pinecone: could not build INSERT"));

    // One mutable SPI session for the whole loop (mirrors the plpgsql FUNCTION running in the caller's txn:
    // a mid-loop error aborts the statement — no partial insert survives). A wrong-dim `$2::vector` cast
    // raises the pgvector typmod ERROR, which longjmps out faithfully (SPI_execute is not PG_TRY-wrapped).
    Spi::connect_mut(|client| {
        let mut n: i32 = 0;
        for rec in &records {
            if rec.get("id").is_none() || rec.get("values").is_none() {
                err_input(&format!(
                    "theodb.import_pinecone: each record must have \"id\" and \"values\" (got: {rec})"
                ));
            }
            // rec->>'id' semantics: string stays unquoted; number/bool → text; null/absent → NULL.
            let id_text: Option<String> = match rec.get("id") {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Null) | None => None,
                Some(v) => Some(v.to_string()),
            };
            // (rec->'values')::text — the JSON array as text, re-parsed by the $2::vector cast.
            let values_text = rec.get("values").map(|v| v.to_string());
            // COALESCE(rec->'metadata', '{}') — absent key defaults to an empty object.
            let meta = rec
                .get("metadata")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            client
                .update(
                    insert_sql.as_str(),
                    None,
                    &[id_text.into(), values_text.into(), pgrx::JsonB(meta).into()],
                )
                .unwrap_or_else(|e| err_input(&format!("theodb.import_pinecone: insert failed: {e:?}")));
            n += 1;
        }
        n
    })
}

#[pg_schema]
mod theodb_rs {
    use super::import;
    use pgrx::prelude::*;

    /// api-surface: the Pinecone import entrypoint (the SQL `theodb.import_pinecone`). The public wrapper
    /// passes `target::text` (regclass→quoted name). Returns the count of inserted records.
    #[pg_extern]
    fn _import_pinecone(
        target_text: &str,
        export: pgrx::JsonB,
        id_col: &str,
        embedding_col: &str,
        metadata_col: &str,
    ) -> i32 {
        import(target_text, export.0, id_col, embedding_col, metadata_col)
    }
}
