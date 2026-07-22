//! Lakehouse Parquet own-code (M143) — lê/escreve Parquet externo via DataFusion + Arrow (Apache-2.0, já no
//! binário; **sem DuckDB**). Substitui a superfície M62 que dependia do `pg_duckdb` (read_parquet + duckdb.query).
//! Ligado por default (a feature `spike-parquet` do spike Fase 4 foi promovida a permanente — veredito GO em
//! `docs/benchmarks/parquet-reader-owncode-spike.md`).
//!
//! Superfície:
//! - `theodb.olap(path)` → `(category, c, a)` tipado — o agregado canônico do M62 (paridade byte-a-byte vs
//!   pg_duckdb, provada no spike). É o que o `sql/85` (`theodb.olap`) chama.
//! - `theodb.read_parquet(path)` → `SETOF jsonb` — leitor **geral** (schema arbitrário): cada linha Parquet vira
//!   um `jsonb` via arrow-json, cobrindo TODOS os tipos (escalares → json; nested/list/struct → objeto/array)
//!   sem a complexidade de `SETOF record` dinâmico no pgrx (ADR-M143-D1).
//!
//! Reusa (Regra 9) o padrão de execução DataFusion in-extension de `am/df_executor.rs` (tokio current-thread +
//! `block_on`). Erros do DataFusion viram erro tipado (fail-closed, `error-handling.md` §2), nunca panic
//! atravessando a fronteira C.

use datafusion::arrow::array::{Array, Float64Array, Int64Array, StringArray, StringViewArray};
use datafusion::arrow::json::writer::LineDelimitedWriter;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::functions_aggregate::expr_fn::{avg, count};
use datafusion::prelude::{ParquetReadOptions, SessionContext, col, lit};
use pgrx::prelude::*;

/// Executa `f(ctx)` num runtime tokio current-thread (o padrão do df_executor — um backend por vez). `enable_all`
/// liga o driver de I/O que o `object_store` (LocalFileSystem) usa para ler/escrever o arquivo.
fn with_runtime<T, F>(f: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    rt.block_on(f)
}

/// `theodb.olap(path)` — o agregado canônico do M62 (category, count(*), round(avg(amount),4)) sobre um Parquet,
/// own-code. Retorna linhas tipadas. Erro do DataFusion → erro tipado.
#[pg_extern]
fn olap(
    path: String,
) -> TableIterator<'static, (name!(category, String), name!(c, i64), name!(a, f64))> {
    let rows = with_runtime(olap_impl(path))
        .unwrap_or_else(|e| crate::pg::err_input(&format!("theodb.olap: {e}")));
    TableIterator::new(rows.into_iter())
}

async fn olap_impl(path: String) -> Result<Vec<(String, i64, f64)>, String> {
    let ctx = SessionContext::new();
    let df = ctx
        .read_parquet(&path, ParquetReadOptions::default())
        .await
        .map_err(|e| format!("read_parquet('{path}'): {e}"))?
        .aggregate(
            vec![col("category")],
            vec![count(lit(1i64)).alias("c"), avg(col("amount")).alias("a")],
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
            let av = (a.value(r) * 10000.0).round() / 10000.0; // round(...,4) — paridade M62/pg_duckdb
            out.push((cats[r].clone(), c.value(r), av));
        }
    }
    out.sort_by(|x, y| x.0.cmp(&y.0));
    Ok(out)
}

/// `theodb.read_parquet(path)` — leitor geral: cada linha Parquet → um `jsonb` (via arrow-json). Cobre todos os
/// tipos (escalares e nested) sem `SETOF record` dinâmico. Erro do DataFusion / JSON → erro tipado.
#[pg_extern]
fn read_parquet(path: String) -> SetOfIterator<'static, pgrx::JsonB> {
    let rows = with_runtime(read_parquet_impl(path))
        .unwrap_or_else(|e| crate::pg::err_input(&format!("theodb.read_parquet: {e}")));
    SetOfIterator::new(rows.into_iter())
}

async fn read_parquet_impl(path: String) -> Result<Vec<pgrx::JsonB>, String> {
    let ctx = SessionContext::new();
    let df = ctx
        .read_parquet(&path, ParquetReadOptions::default())
        .await
        .map_err(|e| format!("read_parquet('{path}'): {e}"))?;
    let batches = df.collect().await.map_err(|e| format!("collect: {e}"))?;
    batches_to_jsonb(&batches)
}

/// Serializa os RecordBatches em NDJSON (uma linha por row) via arrow-json e converte cada linha num `pgrx::JsonB`.
/// arrow-json cobre nested/list/struct nativamente — daí o shape `jsonb` do leitor geral.
fn batches_to_jsonb(batches: &[RecordBatch]) -> Result<Vec<pgrx::JsonB>, String> {
    let mut out: Vec<pgrx::JsonB> = Vec::new();
    for b in batches {
        if b.num_rows() == 0 {
            continue;
        }
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut w = LineDelimitedWriter::new(&mut buf);
            w.write(b).map_err(|e| format!("json write: {e}"))?;
            w.finish().map_err(|e| format!("json finish: {e}"))?;
        }
        for line in buf.split(|&c| c == b'\n') {
            if line.is_empty() {
                continue;
            }
            let v: serde_json::Value =
                serde_json::from_slice(line).map_err(|e| format!("json parse: {e}"))?;
            out.push(pgrx::JsonB(v));
        }
    }
    Ok(out)
}

/// O reader Parquet do DataFusion 54 pode emitir Utf8 (`StringArray`) OU Utf8View (`StringViewArray`).
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
