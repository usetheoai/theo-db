//! Lakehouse Parquet own-code (M143) — lê/escreve Parquet externo via DataFusion + Arrow (Apache-2.0, já no
//! binário; **sem DuckDB**). Substitui a superfície M62 que dependia do `pg_duckdb`.
//!
//! Superfície (todas `#[pg_extern]` → schema `public`, e **REVOKE ALL FROM PUBLIC** no `extension_sql!` abaixo —
//! escrita/leitura de arquivo server-side é privilégio superuser, como o `COPY … TO file`; least-privilege):
//! - `public.olap(path)` → `(category, c, a)` tipado — agregado M62 (paridade byte-a-byte vs pg_duckdb).
//! - `public.read_parquet(path)` → `SETOF jsonb` — leitor geral (schema arbitrário via arrow-json, todos os tipos).
//! - `public.write_parquet(rel, path)` → bigint — materializa uma tabela em Parquet (SPI → Arrow → ArrowWriter).
//! A superfície de usuário é `theodb.htap_refresh(rel)`/`theodb.olap(rel)` (theodb_rs/sql/surface/85-htap.sql), que chamam estas.
//!
//! Segurança/robustez (M143 review): (1) block_on sob `HeldInterrupts` (mesma invariante do `df_executor` — um
//! longjmp do PG não pode saltar o runtime tokio sem Drop); (2) `GreedyMemoryPool(work_mem)` limita a memória do
//! DataFusion (parquet grande → erro tipado, não OOM); (3) o `FROM` do write usa o nome CANÔNICO resolvido via
//! `$1::regclass::text` (injection-safe, não interpolação crua); (4) temp de escrita único por-backend + cleanup.
//! Erros do DataFusion/SPI/JSON viram erro tipado (`crate::pg::err_input` = ereport ERROR), nunca panic.

use datafusion::arrow::array::{Array, Float64Array, Int64Array, StringArray, StringViewArray};
use datafusion::arrow::json::writer::LineDelimitedWriter;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::execution::memory_pool::GreedyMemoryPool;
use datafusion::execution::runtime_env::RuntimeEnvBuilder;
use datafusion::functions_aggregate::expr_fn::{avg, count};
use datafusion::prelude::{ParquetReadOptions, SessionConfig, SessionContext, col, lit};
use pgrx::prelude::*;

/// RAII sobre `InterruptHoldoffCount` (mesmo padrão de `am/df_executor.rs` e `am/datafusion_probe.rs` — replicado
/// deliberadamente porque é um guard C-macro): segura interrupções através do `block_on` síncrono para que um
/// cancel/`statement_timeout`/`proc_exit` do PG (siglongjmp C) não salte por cima do runtime tokio vivo + os
/// frames Rust sem rodar `Drop`. A rede de panic do pgrx NÃO captura um longjmp C.
struct HeldInterrupts;
impl HeldInterrupts {
    fn hold() -> Self {
        unsafe { pg_sys::InterruptHoldoffCount += 1 };
        HeldInterrupts
    }
}
impl Drop for HeldInterrupts {
    fn drop(&mut self) {
        unsafe { pg_sys::InterruptHoldoffCount -= 1 };
    }
}

/// Marcador de classe que um erro carrega desde a origem: mensagem prefixada com isto vira **58030
/// (io_error)**; sem o prefixo, **22023 (invalid_parameter_value)**.
///
/// M146 (review F-arch-1). A primeira versão adivinhava a classe pelo prefixo humano da mensagem
/// (`"fsync "`, `"rename "`), o que rotulava `criar`/`write batch`/`close` — justamente onde ENOSPC e EIO
/// aparecem — como erro de parâmetro do usuário. Marcar na origem custa um literal e elimina a adivinhação:
/// quem SABE que a operação é de I/O é quem a executou, não quem lê a string depois.
const IO_PREFIX: &str = "io:";

/// `work_mem` (KB → bytes), piso de 64 KB — o teto de memória do DataFusion (mesmo do df_executor).
fn work_mem_bytes() -> usize {
    (unsafe { pg_sys::work_mem }.max(64) as usize) * 1024
}

/// `SessionContext` limitado por `work_mem` (GreedyMemoryPool) — um parquet maior que work_mem → erro tipado do
/// DataFusion, não OOM do backend. Um único partition (backend single-thread).
fn bounded_ctx() -> Result<SessionContext, String> {
    let runtime = RuntimeEnvBuilder::new()
        .with_memory_pool(std::sync::Arc::new(GreedyMemoryPool::new(work_mem_bytes())))
        .build_arc()
        .map_err(|e| format!("runtime env: {e}"))?;
    let config = SessionConfig::new().with_target_partitions(1);
    Ok(SessionContext::new_with_config_rt(config, runtime))
}

/// Roda `f` num runtime tokio current-thread SOB `HeldInterrupts`. `enable_all()` liga o driver de I/O que o
/// `object_store` (LocalFileSystem) usa para ler/escrever o arquivo (o df_executor lê de memória e não precisa).
fn with_runtime<T, F>(f: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    let _held = HeldInterrupts::hold();
    rt.block_on(f)
}

/// `theodb`/`public.olap(path)` — o agregado canônico do M62 (category, count(*), round(avg(amount),4)) sobre um
/// Parquet, own-code. Retorna linhas tipadas.
#[pg_extern]
fn olap(
    path: String,
) -> TableIterator<'static, (name!(category, String), name!(c, i64), name!(a, f64))> {
    let rows = with_runtime(olap_impl(path))
        .unwrap_or_else(|e| crate::pg::err_input(&format!("theodb.olap: {e}")));
    TableIterator::new(rows.into_iter())
}

async fn olap_impl(path: String) -> Result<Vec<(String, i64, f64)>, String> {
    let ctx = bounded_ctx()?;
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
        let c =
            b.column(1).as_any().downcast_ref::<Int64Array>().ok_or("col c (count) não é Int64")?;
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

/// `theodb`/`public.read_parquet(path)` — leitor geral: cada linha Parquet → um `jsonb` (via arrow-json). Cobre
/// todos os tipos (escalares e nested) sem `SETOF record` dinâmico. Limitado por work_mem (bounded_ctx).
#[pg_extern]
fn read_parquet(path: String) -> SetOfIterator<'static, pgrx::JsonB> {
    let rows = with_runtime(read_parquet_impl(path))
        .unwrap_or_else(|e| crate::pg::err_input(&format!("theodb.read_parquet: {e}")));
    SetOfIterator::new(rows.into_iter())
}

async fn read_parquet_impl(path: String) -> Result<Vec<pgrx::JsonB>, String> {
    let ctx = bounded_ctx()?;
    let df = ctx
        .read_parquet(&path, ParquetReadOptions::default())
        .await
        .map_err(|e| format!("read_parquet('{path}'): {e}"))?;
    let batches = df.collect().await.map_err(|e| format!("collect: {e}"))?;
    batches_to_jsonb(&batches)
}

/// Converte RecordBatches em `pgrx::JsonB`, um documento por linha, **sem passar por texto**.
///
/// B-096 — MEDIDO em 2026-08-21 sobre 2M linhas do mesmo arquivo: o parser Parquet lê e agrega em
/// **25 ms**, o Postgres constrói 2M `jsonb` nativos em **435 ms**, e esta função levava **4650 ms**.
/// Os ~4200 ms de diferença eram a travessia `Arrow → texto NDJSON → serde_json::Value → JsonB`:
/// cada linha era serializada em JSON e imediatamente re-parseada. Nem o parser nem o `jsonb` eram o
/// gargalo — a travessia era.
///
/// A conversão agora vai direto do array Arrow para `serde_json::Value`. `batches_to_jsonb_via_texto`
/// permanece como ORÁCULO dos testes de equivalência: a mudança é de implementação, e um ganho de
/// tempo que altere o documento entregue ao usuário não é ganho, é defeito com benchmark.
fn batches_to_jsonb(batches: &[RecordBatch]) -> Result<Vec<pgrx::JsonB>, String> {
    use datafusion::arrow::array::{
        BooleanArray, Float32Array, Int32Array, LargeStringArray, UInt32Array, UInt64Array,
    };
    use serde_json::{Map, Value};

    let mut out: Vec<pgrx::JsonB> = Vec::new();
    for b in batches {
        if b.num_rows() == 0 {
            continue;
        }
        let esquema = b.schema();
        let campos: Vec<String> =
            esquema.fields().iter().map(|f| f.name().clone()).collect();

        // Uma closure por COLUNA, resolvida uma vez, em vez de um `match` de tipo por celula.
        // Com 2M linhas a diferenca entre resolver o tipo 4 vezes e 8 milhoes de vezes e o ponto.
        type Extrator<'a> = Box<dyn Fn(usize) -> Value + 'a>;
        let mut extratores: Vec<Extrator> = Vec::with_capacity(b.num_columns());
        for c in 0..b.num_columns() {
            let col = b.column(c).as_ref();
            let nulos = col.logical_nulls();
            let nulo = move |r: usize| nulos.as_ref().is_some_and(|n| n.is_null(r));

            macro_rules! numerico {
                ($t:ty) => {{
                    let a = col.as_any().downcast_ref::<$t>().expect("downcast conferido acima");
                    Box::new(move |r: usize| {
                        if nulo(r) { Value::Null } else { Value::from(a.value(r)) }
                    }) as Extrator
                }};
            }

            let e: Extrator = if col.as_any().is::<Int64Array>() {
                numerico!(Int64Array)
            } else if col.as_any().is::<Int32Array>() {
                numerico!(Int32Array)
            } else if col.as_any().is::<UInt64Array>() {
                numerico!(UInt64Array)
            } else if col.as_any().is::<UInt32Array>() {
                numerico!(UInt32Array)
            } else if col.as_any().is::<Float64Array>() {
                numerico!(Float64Array)
            } else if col.as_any().is::<Float32Array>() {
                numerico!(Float32Array)
            } else if col.as_any().is::<BooleanArray>() {
                numerico!(BooleanArray)
            } else if let Some(a) = col.as_any().downcast_ref::<StringArray>() {
                Box::new(move |r| if nulo(r) { Value::Null } else { Value::from(a.value(r)) })
            } else if let Some(a) = col.as_any().downcast_ref::<StringViewArray>() {
                Box::new(move |r| if nulo(r) { Value::Null } else { Value::from(a.value(r)) })
            } else if let Some(a) = col.as_any().downcast_ref::<LargeStringArray>() {
                Box::new(move |r| if nulo(r) { Value::Null } else { Value::from(a.value(r)) })
            } else {
                // FAIL-CLOSED, e deliberado: um tipo nao coberto (nested, temporal, decimal) cai na
                // via por TEXTO, que os cobre via arrow-json. Inventar uma representacao aqui daria
                // um documento diferente do que o usuario recebia — silenciosamente.
                return batches_to_jsonb_via_texto(batches);
            };
            extratores.push(e);
        }

        for r in 0..b.num_rows() {
            let mut m = Map::with_capacity(campos.len());
            for (c, nome) in campos.iter().enumerate() {
                // CHAVE NULA É OMITIDA, não emitida como `null`.
                //
                // MEDIDO pelo teste de equivalência, que falhou na primeira implementação: o
                // `arrow-json` OMITE campos nulos, e eu os emitia como `null`. Em `jsonb` isso não é
                // cosmético — `doc ? 'nome'` e `jsonb_object_keys(doc)` dariam respostas diferentes
                // das que o usuário recebia. Um ganho de tempo que muda o documento não é ganho.
                let v = extratores[c](r);
                if !v.is_null() {
                    m.insert(nome.clone(), v);
                }
            }
            out.push(pgrx::JsonB(Value::Object(m)));
        }
    }
    Ok(out)
}

/// A via por texto original, preservada como ORÁCULO dos testes de equivalência e como fallback para
/// tipos que a conversão direta não cobre (nested, temporal, decimal — arrow-json os cobre).
fn batches_to_jsonb_via_texto(batches: &[RecordBatch]) -> Result<Vec<pgrx::JsonB>, String> {
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

/// `theodb`/`public.write_parquet(rel, path)` — materializa uma tabela PG num arquivo Parquet own-code (substitui
/// o `COPY … (FORMAT parquet)` do pg_duckdb). Lê as linhas via SPI, constrói arrays Arrow por tipo, escreve um
/// arquivo único via `parquet::arrow::ArrowWriter` (síncrono; escrita atômica temp+rename, temp único por-backend).
/// `rel` é resolvido/validado via `$1::regclass` e o `FROM` usa o nome CANÔNICO (injection-safe). Retorna o nº de
/// linhas. Tipo não-suportado → erro tipado (fail-closed). Escalares (int2/4/8, float4/8, bool, text) na v1.
#[pg_extern]
fn write_parquet(rel: String, path: String) -> i64 {
    write_parquet_impl(&rel, &path).unwrap_or_else(|e| {
        // M146 (review F2 + F-arch-1): falha de I/O NÃO é erro de parâmetro. A primeira versão adivinhava a
        // classe pelo PREFIXO da mensagem, o que deixava `criar`/`write batch`/`close` — justamente as falhas
        // de disco mais prováveis (ENOSPC, EIO) — rotuladas 22023 "você passou um parâmetro ruim". Agora o
        // erro CARREGA sua classe desde a origem (`IO_PREFIX`), então a rota não depende de adivinhação.
        match e.strip_prefix(IO_PREFIX) {
            Some(io) => crate::pg::err_io(&format!("theodb.write_parquet: {io}")),
            None => crate::pg::err_input(&format!("theodb.write_parquet: {e}")),
        }
    })
}

// M145 T1.1: `enum Col` + 4 helpers extraídos de `write_parquet_impl` (CC 35 → ≤ 25 por lizard),
// comportamento preservado (fail-closed no OID + atomicidade de escrita idênticos). Um builder Arrow por
// coluna, escolhido pelo OID; tipo não-suportado na ESCRITA → erro tipado (fail-closed).
enum Col {
    I16(datafusion::arrow::array::Int16Builder),
    I32(datafusion::arrow::array::Int32Builder),
    I64(datafusion::arrow::array::Int64Builder),
    F32(datafusion::arrow::array::Float32Builder),
    F64(datafusion::arrow::array::Float64Builder),
    Bool(datafusion::arrow::array::BooleanBuilder),
    Str(datafusion::arrow::array::StringBuilder),
}

/// OID Postgres → `(Field, builder)`. Tipo não-suportado na escrita → erro tipado (fail-closed) — NUNCA
/// silenciosamente pulado (a coluna não pode sumir do Parquet).
fn col_builder_for(
    name: &str,
    oid: u32,
) -> Result<(datafusion::arrow::datatypes::Field, Col), String> {
    use datafusion::arrow::array::{
        BooleanBuilder, Float32Builder, Float64Builder, Int16Builder, Int32Builder, Int64Builder,
        StringBuilder,
    };
    use datafusion::arrow::datatypes::{DataType, Field};
    let (dt, col) = match oid {
        21 => (DataType::Int16, Col::I16(Int16Builder::new())),
        23 => (DataType::Int32, Col::I32(Int32Builder::new())),
        20 => (DataType::Int64, Col::I64(Int64Builder::new())),
        700 => (DataType::Float32, Col::F32(Float32Builder::new())),
        701 => (DataType::Float64, Col::F64(Float64Builder::new())),
        16 => (DataType::Boolean, Col::Bool(BooleanBuilder::new())),
        25 | 1042 | 1043 => (DataType::Utf8, Col::Str(StringBuilder::new())),
        other => {
            return Err(format!(
                "coluna '{name}': tipo OID {other} não suportado na escrita Parquet own-code (v1: int2/4/8, \
                 float4/8, bool, text). Legível via read_parquet; escrita ampla é follow-on."
            ));
        }
    };
    Ok((Field::new(name, dt, true), col))
}

/// Alimenta cada builder com os valores de UMA linha SPI (1-indexed). Erro de leitura de coluna → erro tipado.
fn append_row(builders: &mut [Col], row: &pgrx::spi::SpiHeapTupleData) -> Result<(), String> {
    for (i, b) in builders.iter_mut().enumerate() {
        let ord = i + 1; // SPI é 1-indexed
        match b {
            Col::I16(x) => {
                x.append_option(row.get::<i16>(ord).map_err(|e| format!("col {ord}: {e}"))?)
            }
            Col::I32(x) => {
                x.append_option(row.get::<i32>(ord).map_err(|e| format!("col {ord}: {e}"))?)
            }
            Col::I64(x) => {
                x.append_option(row.get::<i64>(ord).map_err(|e| format!("col {ord}: {e}"))?)
            }
            Col::F32(x) => {
                x.append_option(row.get::<f32>(ord).map_err(|e| format!("col {ord}: {e}"))?)
            }
            Col::F64(x) => {
                x.append_option(row.get::<f64>(ord).map_err(|e| format!("col {ord}: {e}"))?)
            }
            Col::Bool(x) => {
                x.append_option(row.get::<bool>(ord).map_err(|e| format!("col {ord}: {e}"))?)
            }
            Col::Str(x) => {
                x.append_option(row.get::<String>(ord).map_err(|e| format!("col {ord}: {e}"))?)
            }
        }
    }
    Ok(())
}

/// Finaliza cada builder num `ArrayRef` Arrow (mesma ORDEM das colunas).
fn finish_arrays(builders: Vec<Col>) -> Vec<datafusion::arrow::array::ArrayRef> {
    use datafusion::arrow::array::ArrayRef;
    use std::sync::Arc;
    builders
        .into_iter()
        .map(|b| match b {
            Col::I16(mut x) => Arc::new(x.finish()) as ArrayRef,
            Col::I32(mut x) => Arc::new(x.finish()) as ArrayRef,
            Col::I64(mut x) => Arc::new(x.finish()) as ArrayRef,
            Col::F32(mut x) => Arc::new(x.finish()) as ArrayRef,
            Col::F64(mut x) => Arc::new(x.finish()) as ArrayRef,
            Col::Bool(mut x) => Arc::new(x.finish()) as ArrayRef,
            Col::Str(mut x) => Arc::new(x.finish()) as ArrayRef,
        })
        .collect()
}

/// Escrita atômica E DURÁVEL: temp ÚNICO por-backend (evita corrida de dois writes no mesmo path) →
/// `fsync` do arquivo → `rename` (commit) → `fsync` do diretório-pai; cleanup do temp em qualquer erro
/// (nunca temp órfão, nunca arquivo parcial publicado). O temp fica no mesmo dir do path (rename não
/// cross-device).
///
/// CONTRATO EXATO em erro (M146, review F5 — o texto anterior prometia mais do que o mecanismo entrega):
/// se o `rename` tem sucesso e só o `fsync` do DIRETÓRIO falha, a função retorna `Err` com o arquivo **já
/// publicado** em `path` — íntegro (o conteúdo foi fsyncado antes do rename), porém com a durabilidade da
/// entrada de diretório incerta até o próximo fsync/checkpoint do FS. O chamador não consegue distinguir
/// "nada publicado" de "publicado, entrada de diretório não confirmada"; em ambos os casos a ação correta é
/// re-executar o export, que é idempotente. O `durable_rename` do PostgreSQL tem exatamente a mesma
/// propriedade (retorna −1 depois do rename), então a divergência é consciente, não acidental.
fn atomic_write_parquet(
    batch: &RecordBatch,
    schema: std::sync::Arc<datafusion::arrow::datatypes::Schema>,
    path: &str,
) -> Result<(), String> {
    use datafusion::parquet::arrow::ArrowWriter;
    let tmp = format!("{path}.{}.tmp", unsafe { pg_sys::MyProcPid });
    let write_res = (|| -> Result<(), String> {
        // `criar` é o único passo AMBÍGUO: ENOENT/EACCES/ENOTDIR/ENAMETOOLONG dizem "o path que você passou
        // não serve" (22023); ENOSPC/EIO/EROFS dizem "o disco falhou" (58030). Classificar os dois como a
        // mesma coisa manda o operador para o lado errado, então a decisão é pelo errno, não pelo passo.
        let file = std::fs::File::create(&tmp).map_err(|e| {
            const PATH_ERRNOS: [i32; 5] = [
                2,  /*ENOENT*/
                13, /*EACCES*/
                20, /*ENOTDIR*/
                22, /*EINVAL*/
                36, /*ENAMETOOLONG*/
            ];
            let is_path = e.raw_os_error().is_some_and(|n| PATH_ERRNOS.contains(&n));
            let mark = if is_path { "" } else { IO_PREFIX };
            format!("{mark}criar '{tmp}': {e}")
        })?;
        // M146 T1.3 — durabilidade, seguindo o protocolo do `durable_rename` do PostgreSQL
        // (`src/backend/storage/file/fd.c:782` + os helpers `fsync_fname`/`fsync_parent_path`): o rename é
        // atômico apenas para a ENTRADA DE DIRETÓRIO; sem fsync o conteúdo pode não estar em disco, e sem
        // fsync do DIRETÓRIO-PAI o próprio rename pode se perder num crash. Antes daqui o export era atômico
        // porém NÃO durável (o crate não tinha um único `sync_all`).
        //
        // O fd é CLONADO antes de o `File` ser movido para o writer: `try_clone` é `dup(2)` — descritor novo,
        // MESMA open file description, MESMO inode — e `fsync` é por-inode, então o `sync_all` abaixo sincroniza
        // exatamente o que o writer escreveu. Não há janela de dados presos em buffer: `close()` faz
        // `finish()` → `flush()` do `TrackedWrite`, e `std::fs::File` não tem buffer próprio.
        //
        // Escolhido depois que `finish()` SEGUIDO de `into_inner()` falhou em runtime com
        // "SerializedFileWriter already finished" (`assert_previous_writer_closed`, pois `finished` já era true).
        // CORREÇÃO de uma afirmação anterior deste comentário (achado do review do M146): `into_inner()` sozinho
        // NÃO produziria arquivo sem footer — ele chama `write_metadata()` internamente. O erro estava em
        // encadear os dois, não no `into_inner`. Mantemos o `try_clone` porque preserva o `close()` original
        // intocado e é o caminho já medido em produção.
        let fsync_handle =
            file.try_clone().map_err(|e| format!("{IO_PREFIX}try_clone '{tmp}': {e}"))?;
        let mut w =
            ArrowWriter::try_new(file, schema, None).map_err(|e| format!("ArrowWriter: {e}"))?;
        w.write(batch).map_err(|e| format!("{IO_PREFIX}write batch: {e}"))?;
        w.close().map_err(|e| format!("{IO_PREFIX}close: {e}"))?;
        fsync_handle.sync_all().map_err(|e| format!("{IO_PREFIX}fsync '{tmp}': {e}"))?;
        std::fs::rename(&tmp, path)
            .map_err(|e| format!("{IO_PREFIX}rename '{tmp}'→'{path}': {e}"))?;
        // fsync do diretório-pai. Parent vazio (path relativo simples) → ".", como o upstream faz em
        // `fd.c:3885-3886`. Falha de fsync EM DIRETÓRIO é tolerada para EBADF/EINVAL (há filesystems que não
        // suportam), espelhando `fd.c:3822-3825`; qualquer outro erro propaga como `Err` — nunca PANIC, pois
        // o export é refazível e a fonte da verdade continua disponível (`durable_rename` também repassa o
        // elevel do caller em vez de forçar PANIC).
        let parent = std::path::Path::new(path).parent();
        let dir = match parent {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => std::path::Path::new("."),
        };
        // errno definidos localmente (Linux) para NÃO adicionar a dep `libc` — o crate não a declara e o D2
        // deste milestone é "zero dependência nova"; a stdlib já expõe o errno cru via `raw_os_error()`.
        const EBADF: i32 = 9;
        const EINVAL: i32 = 22;
        match std::fs::File::open(dir).and_then(|d| d.sync_all()) {
            Ok(()) => {}
            Err(e) if matches!(e.raw_os_error(), Some(EBADF) | Some(EINVAL)) => {}
            Err(e) => return Err(format!("{IO_PREFIX}fsync dir '{}': {e}", dir.display())),
        }
        Ok(())
    })();
    if write_res.is_err() {
        let _ = std::fs::remove_file(&tmp); // cleanup best-effort do temp órfão
    }
    write_res
}

fn write_parquet_impl(rel: &str, path: &str) -> Result<i64, String> {
    use datafusion::arrow::datatypes::{Field, Schema};
    use std::sync::Arc;

    // Resolve o nome CANÔNICO (valida que `rel` é uma relação real via ::regclass — lança se não) + as colunas.
    // Tudo via SPI PARAMETRIZADO ($1) — sem interpolação crua (injection-safe). O nome canônico é PG-quotado.
    let (qname, cols): (String, Vec<(String, u32)>) = Spi::connect(|c| {
        let qt = c
            .select("SELECT $1::regclass::text", Some(1), &[rel.into()])
            .map_err(|e| format!("resolve rel '{rel}': {e}"))?;
        let qname: String = qt
            .into_iter()
            .next()
            .and_then(|r| r.get::<String>(1).ok().flatten())
            .ok_or_else(|| format!("relação '{rel}' inválida"))?;
        let ct = c
            .select(
                "SELECT attname::text, atttypid::oid FROM pg_attribute \
                 WHERE attrelid = $1::regclass AND attnum > 0 AND NOT attisdropped ORDER BY attnum",
                None,
                &[rel.into()],
            )
            .map_err(|e| format!("catálogo de '{rel}': {e}"))?;
        let mut cols = Vec::new();
        for row in ct {
            let name: String = row.get(1).map_err(|e| format!("attname: {e}"))?.unwrap_or_default();
            let oid: pg_sys::Oid =
                row.get(2).map_err(|e| format!("atttypid: {e}"))?.ok_or("atttypid nulo")?;
            cols.push((name, oid.to_u32()));
        }
        Ok::<_, String>((qname, cols))
    })?;
    if cols.is_empty() {
        return Err(format!("relação '{rel}' sem colunas"));
    }

    // Um builder Arrow por coluna (fail-closed no OID não-suportado, via `col_builder_for`).
    let mut builders: Vec<Col> = Vec::with_capacity(cols.len());
    let mut fields: Vec<Field> = Vec::with_capacity(cols.len());
    for (name, oid) in &cols {
        let (field, col) = col_builder_for(name, *oid)?;
        fields.push(field);
        builders.push(col);
    }

    // Lê as linhas (via o nome CANÔNICO — injection-safe) e alimenta os builders por OID.
    let mut nrows: i64 = 0;
    Spi::connect(|c| {
        let sql = format!("SELECT * FROM {qname}");
        let t = c.select(&sql, None, &[]).map_err(|e| format!("select {qname}: {e}"))?;
        for row in t {
            append_row(&mut builders, &row)?;
            nrows += 1;
        }
        Ok::<_, String>(())
    })?;

    let arrays = finish_arrays(builders);
    let schema = Arc::new(Schema::new(fields));
    let batch =
        RecordBatch::try_new(schema.clone(), arrays).map_err(|e| format!("record batch: {e}"))?;
    atomic_write_parquet(&batch, schema, path)?;
    Ok(nrows)
}

/// O reader Parquet do DataFusion 54 pode emitir Utf8 (`StringArray`) OU Utf8View (`StringViewArray`).
fn extract_strings(arr: &dyn Array) -> Result<Vec<String>, String> {
    if let Some(a) = arr.as_any().downcast_ref::<StringArray>() {
        Ok((0..a.len()).map(|i| a.value(i).to_string()).collect())
    } else if let Some(a) = arr.as_any().downcast_ref::<StringViewArray>() {
        Ok((0..a.len()).map(|i| a.value(i).to_string()).collect())
    } else {
        Err(format!("coluna category é {:?}, não uma string array", arr.data_type()))
    }
}

// Least-privilege (M143 review HIGH-1): escrita/leitura de arquivo server-side é privilégio superuser (como o
// `COPY … TO file`, que exige superuser / pg_write_server_files). O default do pgrx é GRANT EXECUTE TO PUBLIC —
// REVOKE explícito para que a superfície `theodb.htap_refresh`/`olap` (também REVOKEd no theodb_rs/sql/surface/85-htap.sql) não seja
// contornável chamando as primitivas `public.*` direto. `requires` garante que o REVOKE roda após o CREATE.
extension_sql!(
    r#"
REVOKE ALL ON FUNCTION public.write_parquet(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.read_parquet(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.olap(text) FROM PUBLIC;
"#,
    name = "parquet_revoke_public",
    requires = [write_parquet, read_parquet, olap],
);

// M184 — o audit de maturidade mediu ZERO testes próprios neste módulo, contra uma nota que exigia
// "testado" (`wiki/benchmarks/m184-pilares-superficie-medida-verdict.md`). A crash-safety do pilar já
// tinha cobertura (`isolation/crash_parquet.sh`); a unitária não tinha nenhuma.
//
// `#[pg_test]` (não `#[test]`) porque o crate liga símbolos do Postgres: um binário de teste standalone
// falharia no link de `errstart`/`errmsg` — o mesmo motivo documentado em `sq8.rs` e `sbq.rs`.
#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    // `use super::*` já traz `Float64Array`, `StringArray` e `StringViewArray` — o módulo pai os importa
    // na linha 17. Reimportá-los aqui seria E0252 (nome definido duas vezes).
    use super::*;
    use pgrx::prelude::*;

    // `extract_strings` aceita DUAS representações de string do Arrow. O leitor de Parquet devolve uma
    // ou outra conforme o schema do arquivo, então tratar só `StringArray` quebraria em arquivo real —
    // e o bug seria silencioso até alguém ler um Parquet com view.
    #[pgrx::pg_test]
    fn extract_strings_aceita_string_array() {
        let arr = StringArray::from(vec!["alfa", "beta", "gama"]);
        let got = extract_strings(&arr).expect("StringArray deve ser aceita");
        assert_eq!(got, vec!["alfa", "beta", "gama"]);
    }

    #[pgrx::pg_test]
    fn extract_strings_aceita_string_view_array() {
        let arr = StringViewArray::from(vec!["alfa", "beta"]);
        let got = extract_strings(&arr).expect("StringViewArray deve ser aceita");
        assert_eq!(got, vec!["alfa", "beta"]);
    }

    // Caso NEGATIVO (`rules/testing.md` § 4.1): tipo errado devolve erro TIPADO com o tipo real na
    // mensagem, em vez de entrar em pânico ou devolver vazio. Um `Vec` vazio aqui viraria "categoria sem
    // linhas" rio abaixo — dado errado, não erro.
    #[pgrx::pg_test]
    fn extract_strings_recusa_tipo_nao_string_com_erro_tipado() {
        let arr = Float64Array::from(vec![1.0, 2.0]);
        let err = extract_strings(&arr).expect_err("Float64Array não é string — deve falhar");
        assert!(err.contains("não uma string array"), "mensagem sem o motivo: {err}");
        assert!(err.contains("Float64"), "mensagem não diz o tipo real recebido: {err}");
    }

    // Borda: array vazio é VÁLIDO (um Parquet pode ter zero linhas) e não pode ser confundido com erro.
    #[pgrx::pg_test]
    fn extract_strings_aceita_array_vazio() {
        let arr = StringArray::from(Vec::<&str>::new());
        assert!(extract_strings(&arr).expect("vazio é válido").is_empty());
    }

    // `work_mem_bytes` alimenta o `GreedyMemoryPool` que limita o DataFusion. Zero ou negativo viraria
    // pool degenerado — o caminho que o M169 mostrou custar caro quando falha por recurso.
    #[pgrx::pg_test]
    fn work_mem_bytes_e_positivo() {
        assert!(work_mem_bytes() > 0, "pool de memória do DataFusion não pode ser zero");
    }

    // A superfície é superuser-only por desenho (M143 review HIGH-1). O REVOKE vive num `extension_sql!`
    // e some silenciosamente num refactor — este teste é o que faz a perda aparecer.
    #[pgrx::pg_test]
    fn primitivas_de_arquivo_sao_revogadas_de_public() {
        for f in ["write_parquet(text, text)", "read_parquet(text)", "olap(text)"] {
            let sql = format!("SELECT has_function_privilege('public', 'public.{f}', 'EXECUTE')");
            let granted = Spi::get_one::<bool>(&sql).expect("consulta de privilégio falhou");
            assert_eq!(
                granted,
                Some(false),
                "public.{f} deveria estar REVOKEd de PUBLIC — I/O de arquivo server-side é superuser-only"
            );
        }
    }

    // B-096 — a conversão Arrow→jsonb não pode passar por texto.
    //
    // MEDIDO em 2026-08-21 (máquina de desenvolvimento, contêiner, 2M linhas, mesmo arquivo):
    //   parser Parquet + agregação no DataFusion .......   25 ms
    //   piso de 2M linhas por SRF no PG ................  315 ms
    //   piso de 2M jsonb construídos pelo próprio PG ...  435 ms
    //   `read_parquet` → 2M jsonb ...................... 4650 ms
    //
    // O parser NÃO é o gargalo, e `jsonb` em si também não. Os ~4200 ms restantes são a travessia
    // Arrow → texto NDJSON → `serde_json::Value` → `JsonB`: 90% do tempo num round-trip por TEXTO,
    // serializando e re-parseando cada linha.
    //
    // Estes testes fixam a EQUIVALÊNCIA: a conversão direta tem de produzir exatamente o mesmo valor
    // que a via texto produzia. A mudança é de implementação, e um ganho de tempo que altere o que o
    // usuário recebe não é um ganho — é um defeito com benchmark.
    #[pgrx::pg_test]
    fn batch_para_jsonb_preserva_escalares() {
        use datafusion::arrow::array::{Float64Array, Int64Array, StringArray};
        use datafusion::arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc;

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("amount", DataType::Float64, false),
            Field::new("category", DataType::Utf8, false),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(vec![1i64, 2])),
                Arc::new(Float64Array::from(vec![1.5f64, 2.5])),
                Arc::new(StringArray::from(vec!["alfa", "beta"])),
            ],
        )
        .expect("batch válido");

        let direto = batches_to_jsonb(&[batch.clone()]).expect("conversão direta");
        let via_texto = batches_to_jsonb_via_texto(&[batch]).expect("conversão por texto");

        assert_eq!(direto.len(), 2, "uma linha por row");
        let a: Vec<serde_json::Value> = direto.iter().map(|j| j.0.clone()).collect();
        let b: Vec<serde_json::Value> = via_texto.iter().map(|j| j.0.clone()).collect();
        assert_eq!(a, b, "a conversão direta tem de produzir o MESMO valor que a via texto");
    }

    // Nulo é o caso onde as duas vias divergem com mais facilidade: `null` no JSON, `None` no Arrow.
    // Uma via que omitisse a chave e outra que a emitisse como `null` dariam documentos diferentes —
    // e o consumidor extrai colunas por chave.
    #[pgrx::pg_test]
    fn batch_para_jsonb_preserva_nulos() {
        use datafusion::arrow::array::{Int64Array, StringArray};
        use datafusion::arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc;

        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, true),
            Field::new("nome", DataType::Utf8, true),
        ]));
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(vec![Some(1i64), None])),
                Arc::new(StringArray::from(vec![None, Some("beta")])),
            ],
        )
        .expect("batch válido");

        let direto = batches_to_jsonb(&[batch.clone()]).expect("conversão direta");
        let via_texto = batches_to_jsonb_via_texto(&[batch]).expect("conversão por texto");

        let a: Vec<serde_json::Value> = direto.iter().map(|j| j.0.clone()).collect();
        let b: Vec<serde_json::Value> = via_texto.iter().map(|j| j.0.clone()).collect();
        assert_eq!(a, b, "nulos divergindo entre as duas vias mudaria o documento do usuário");
    }

    // Borda: batch sem linhas continua sendo válido e não pode virar linha fantasma.
    #[pgrx::pg_test]
    fn batch_vazio_nao_produz_linha() {
        use datafusion::arrow::array::Int64Array;
        use datafusion::arrow::datatypes::{DataType, Field, Schema};
        use std::sync::Arc;

        let schema =
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let batch =
            RecordBatch::try_new(schema, vec![Arc::new(Int64Array::from(Vec::<i64>::new()))])
                .expect("batch vazio é válido");

        assert!(batches_to_jsonb(&[batch]).expect("vazio é válido").is_empty());
    }


    // ── B-008 — arquivo truncado ou corrompido: erro TIPADO, nunca crash do backend ────────────────────────────
    //
    // O `CLAUDE.md` deste projeto trata crash de backend como severidade MAXIMA. `read_parquet` le arquivo do
    // sistema de arquivos do SERVIDOR, e um arquivo chega truncado por transferencia interrompida, corrompido
    // por disco, ou simplesmente nao e Parquet. O bullet 2 do B-008 pede isto e nao existia teste.
    //
    // COMO A SOBREVIVENCIA DO BACKEND E PROVADA, e por que nao por um `SELECT 1`:
    // a primeira versao destes testes fazia `SELECT 1` depois da leitura para "provar que o backend vive".
    // Isso NAO prova nada e reprova por conta propria — dentro de um `#[pg_test]` tudo roda numa transacao, e
    // um `ereport ERROR` a ABORTA; o `SELECT 1` seguinte falha com "current transaction is aborted" mesmo com
    // o processo perfeitamente vivo. A prova real e o teste CHEGAR AO FIM: se o backend morresse, o arnes do
    // pgrx perderia a conexao e reportaria a queda, nao uma asercao.

    /// Escreve `bytes` num arquivo temporario unico por processo e devolve o caminho.
    fn arquivo_temp(nome: &str, bytes: &[u8]) -> String {
        let caminho = format!("/tmp/theodb_b008_{}_{nome}", std::process::id());
        std::fs::write(&caminho, bytes).expect("escrever o arquivo de teste");
        caminho
    }

    /// As tres formas de o arquivo estar errado, no `impl` — que devolve `Result`, entao nada aborta a
    /// transacao e as tres podem ser exercitadas no mesmo teste.
    #[pgrx::pg_test]
    fn b008_arquivo_invalido_erra_em_vez_de_devolver_linhas() {
        for (nome, bytes, porque) in [
            ("lixo", &b"isto nao e um arquivo parquet, e nunca foi"[..],
             "bytes arbitrarios tem de ser recusados pelo magic number"),
            ("truncado", &b"PAR1"[..],
             "so o magic header, sem footer — o caso da transferencia interrompida"),
            ("vazio", &b""[..],
             "arquivo vazio tem de ERRAR; devolver zero linhas seria dado errado, nao erro"),
        ] {
            let p = arquivo_temp(nome, bytes);
            let r = with_runtime(read_parquet_impl(p.clone()));
            let _ = std::fs::remove_file(&p);
            assert!(r.is_err(), "{nome}: {porque} — devolveu {r:?}");
        }
    }

    /// A superficie SQL: o `impl` acima devolve `Result`, mas o usuario chama `read_parquet`, que converte
    /// o erro em `ereport` via `err_input`. Este teste prova a PONTE — que o erro chega tipado, com SQLSTATE,
    /// em vez de panico solto atravessando a fronteira C.
    #[pgrx::pg_test]
    fn b008_a_superficie_sql_devolve_erro_tipado_para_arquivo_invalido() {
        let p = arquivo_temp("sql", b"nem de longe parquet");
        let caminho = p.clone();
        let caught = PgTryBuilder::new(move || {
            // A funcao PUBLICA, nao o `impl`: e ela que chama `err_input`.
            let _ = read_parquet(caminho.clone());
            None
        })
        .catch_others(|e| {
            let r = match &e {
                pgrx::pg_sys::panic::CaughtError::PostgresError(r)
                | pgrx::pg_sys::panic::CaughtError::ErrorReport(r) => r,
                pgrx::pg_sys::panic::CaughtError::RustPanic { ereport, .. } => ereport,
            };
            Some((format!("{:?}", r.sql_error_code()), r.message().to_string()))
        })
        .execute();
        let _ = std::fs::remove_file(&p);
        let (code, msg) = caught.expect("ler arquivo invalido tem de falhar, nao devolver linhas");
        assert!(!code.is_empty(), "o erro carrega SQLSTATE — nao e panico solto: {code}");
        assert!(
            msg.contains("read_parquet") || msg.to_lowercase().contains("parquet"),
            "a mensagem nomeia a operacao para quem le o log: {msg}"
        );
    }

}
