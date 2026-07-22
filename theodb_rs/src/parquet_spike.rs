//! Parquet reader SPIKE (Fase 4 — theodb-evolution, falsifiable spike).
//!
//! Pergunta que o spike responde, MEDINDO: o `theodb_rs` lê um Parquet externo **own-code** (via DataFusion +
//! Arrow — Apache-2.0, JÁ no binário; NÃO DuckDB) e produz o MESMO agregado que a superfície M62 (`olap_sql`)
//! gera hoje via `pg_duckdb.read_parquet`? Se sim, é o caminho para remover o bundle DuckDB de 118 MB.
//!
//! Reusa (Regra 9) o padrão de execução DataFusion in-extension já provado em `am/df_executor.rs`
//! (`tokio` current-thread + `block_on`), trocando `ctx.read_batch(batch)` (memória) por `ctx.register_parquet`
//! (arquivo). Retorna o agregado canônico do M62 — `category, count(*), round(avg(amount),4) GROUP BY category
//! ORDER BY category` — para comparação linha-a-linha com o `SELECT * FROM duckdb.query($$…read_parquet…$$)`.
//!
//! Atrás de `--features spike-parquet` — off no build default. Se o veredito for GO, promove para superfície
//! de produção num milestone de remoção do pg_duckdb; se NO-GO, o crate shipado nunca pagou o custo.

use datafusion::arrow::array::{Float64Array, Int64Array, StringArray};
use datafusion::prelude::{ParquetReadOptions, SessionContext};
use pgrx::prelude::*;

/// Lê um Parquet externo own-code e roda o agregado canônico do M62. Retorna `(category, c, a)` — o mesmo shape
/// que o `olap_sql` do pg_duckdb produz — para provar paridade sem DuckDB. Erros do DataFusion viram erro tipado.
#[pg_extern]
fn read_parquet_agg_spike(
    path: String,
) -> TableIterator<'static, (name!(category, String), name!(c, i64), name!(a, f64))> {
    let rows = run_parquet_agg(&path)
        .unwrap_or_else(|e| crate::pg::err_input(&format!("read_parquet_agg_spike: {e}")));
    TableIterator::new(rows.into_iter())
}

/// O núcleo: registra o Parquet no DataFusion e coleta o agregado. `enable_all()` liga o driver de I/O que o
/// `object_store` (LocalFileSystem) usa para ler o arquivo — o `df_executor` in-memory não precisava disso.
fn run_parquet_agg(path: &str) -> Result<Vec<(String, i64, f64)>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    let path = path.to_string();
    rt.block_on(async move {
        let ctx = SessionContext::new();
        ctx.register_parquet("parq", &path, ParquetReadOptions::default())
            .await
            .map_err(|e| format!("register_parquet('{path}'): {e}"))?;
        let df = ctx
            // CAST(category AS VARCHAR) força Utf8 (StringArray) — o reader Parquet do DataFusion 54 pode
            // emitir Utf8View, que quebraria o downcast; o cast torna o spike robusto ao tipo de string do reader.
            .sql(
                "SELECT CAST(category AS VARCHAR) AS category, count(*) AS c, round(avg(amount), 4) AS a \
                 FROM parq GROUP BY category ORDER BY category",
            )
            .await
            .map_err(|e| format!("sql: {e}"))?;
        let batches = df.collect().await.map_err(|e| format!("collect: {e}"))?;

        let mut out: Vec<(String, i64, f64)> = Vec::new();
        for b in &batches {
            let cat = b
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .ok_or("col0 (category) não é Utf8")?;
            let c = b
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("col1 (count) não é Int64")?;
            let a = b
                .column(2)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or("col2 (avg) não é Float64")?;
            for r in 0..b.num_rows() {
                out.push((cat.value(r).to_string(), c.value(r), a.value(r)));
            }
        }
        Ok::<Vec<(String, i64, f64)>, String>(out)
    })
}
