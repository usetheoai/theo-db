//! Parquet reader SPIKE (Fase 4 — theodb-evolution, falsifiable spike).
//!
//! Pergunta que o spike responde, MEDINDO: o `theodb_rs` lê um Parquet externo **own-code** (via DataFusion +
//! Arrow — Apache-2.0, JÁ no binário; NÃO DuckDB) e produz o MESMO agregado que a superfície M62 (`olap_sql`)
//! gera hoje via `pg_duckdb.read_parquet`? Se sim, é o caminho para remover o bundle DuckDB de 118 MB.
//!
//! Reusa (Regra 9) exatamente a **DataFrame API** que o `am/df_executor.rs` (M100) já usa no build default —
//! `read_parquet(...).aggregate(group, [count, avg])` + `functions_aggregate::expr_fn` — trocando a fonte de
//! `read_batch` (memória) por `read_parquet` (arquivo). NÃO usa `ctx.sql()` (evita o feature `sql` + o parser,
//! mantendo o custo de tamanho honesto). Ordena e arredonda em Rust (paridade com `round(...,4)` do M62).
//!
//! Atrás de `--features spike-parquet` — off no build default. Se GO, promove num milestone de remoção do
//! pg_duckdb; se NO-GO, o crate shipado nunca pagou o custo.

use datafusion::arrow::array::{Array, Float64Array, Int64Array, StringArray, StringViewArray};
use datafusion::functions_aggregate::expr_fn::{avg, count};
use datafusion::prelude::{ParquetReadOptions, SessionContext, col, lit};
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

/// O núcleo: registra o Parquet no DataFusion e coleta `count(*)` + `avg(amount)` por `category`. `enable_all()`
/// liga o driver de I/O que o `object_store` (LocalFileSystem) usa para ler o arquivo.
fn run_parquet_agg(path: &str) -> Result<Vec<(String, i64, f64)>, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    let path = path.to_string();
    rt.block_on(async move {
        let ctx = SessionContext::new();
        let df = ctx
            .read_parquet(&path, ParquetReadOptions::default())
            .await
            .map_err(|e| format!("read_parquet('{path}'): {e}"))?
            .aggregate(
                vec![col("category")],
                vec![
                    count(lit(1i64)).alias("c"),
                    avg(col("amount")).alias("a"),
                ],
            )
            .map_err(|e| format!("aggregate: {e}"))?;
        let batches = df.collect().await.map_err(|e| format!("collect: {e}"))?;

        let mut out: Vec<(String, i64, f64)> = Vec::new();
        for b in &batches {
            let cats = extract_strings(b.column(0).as_ref())?;
            let c = b
                .column(1)
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("col c (count) não é Int64")?;
            let a = b
                .column(2)
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or("col a (avg) não é Float64")?;
            for r in 0..b.num_rows() {
                // round(...,4) em Rust — paridade com o round(avg(amount),4) do M62/pg_duckdb.
                let av = (a.value(r) * 10000.0).round() / 10000.0;
                out.push((cats[r].clone(), c.value(r), av));
            }
        }
        // ORDER BY category em Rust (evita a incerteza da API de sort do DataFrame no DF54).
        out.sort_by(|x, y| x.0.cmp(&y.0));
        Ok::<Vec<(String, i64, f64)>, String>(out)
    })
}

/// O reader Parquet do DataFusion 54 pode emitir Utf8 (`StringArray`) OU Utf8View (`StringViewArray`) — trata os
/// dois para o spike ser robusto ao tipo de string do reader.
fn extract_strings(arr: &dyn Array) -> Result<Vec<String>, String> {
    if let Some(a) = arr.as_any().downcast_ref::<StringArray>() {
        Ok((0..a.len()).map(|i| a.value(i).to_string()).collect())
    } else if let Some(a) = arr.as_any().downcast_ref::<StringViewArray>() {
        Ok((0..a.len()).map(|i| a.value(i).to_string()).collect())
    } else {
        Err(format!(
            "coluna category é {:?}, não uma string array",
            arr.data_type()
        ))
    }
}
