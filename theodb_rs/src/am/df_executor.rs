//! M100 Phase A — the DataFusion vectorized executor over the `theodb_columnar` TAM (de-risk slice).
//!
//! This slice proves the pillar's most dangerous FFI seam over REAL columnar data: decode a columnar table's
//! visible stripes into Arrow arrays, register them as a DataFusion table, and drive a vectorized aggregate to
//! completion with a synchronous `block_on` inside this backend — under a `HeldInterrupts` guard so a mid-flight
//! query-cancel cannot siglongjmp past the live tokio runtime and abort the process (blueprint Q1 / M98 probe).
//!
//! Scope (Phase A): result-equivalence of a `count(*)` + `sum(<numeric>)` aggregate columnar-vectorized vs a heap
//! table. The `work_mem` MemoryPool (errors-not-panics), per-batch interrupt safe-points, projection pushdown,
//! min/max skip-pruning consumption, and the planner `CustomScan` integration are the later M100 phases (B/C/D).
//! Own-code glue (ADR-0042 D3 / M100 D1); Apache-2.0 `datafusion`/`arrow` are the adopted engine (Rule 9).
#![allow(non_snake_case)]

use datafusion::arrow::array::{
    Array, ArrayRef, BooleanArray, Date32Array, Decimal128Array, Float32Array, Float64Array,
    Int16Array, Int32Array, Int64Array, StringArray, TimestampMicrosecondArray,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::error::DataFusionError;
use datafusion::functions_aggregate::expr_fn::{avg, count, count_distinct, max, min, sum};
use datafusion::prelude::{Expr, SessionContext, cast, col, lit};
use datafusion::scalar::ScalarValue;
use pgrx::AnyNumeric;
use pgrx::datum::FromDatum;
use pgrx::prelude::*;
use std::ffi::CStr;
use std::sync::Arc;

/// RAII emulating C's `HOLD_INTERRUPTS()`/`RESUME_INTERRUPTS()` (macros over `InterruptHoldoffCount`). Holds
/// interrupts across the synchronous `block_on` so a mid-flight cancel/`proc_exit` cannot drop the tokio runtime
/// and crash the backend (mirrors `datafusion_probe.rs`; the per-batch safe-point granularity is M100 Phase D).
struct HeldInterrupts;
impl HeldInterrupts {
    fn hold() -> Self {
        unsafe { pg_sys::InterruptHoldoffCount += 1 };
        HeldInterrupts
    }
}
impl Drop for HeldInterrupts {
    fn drop(&mut self) {
        // `InterruptHoldoffCount` é `uint32`, e `errfinish` o ZERA antes do longjmp (postgres `elog.c:531`,
        // deliberadamente, para permitir `ereport` de dentro de um holdoff). No unwind ele já vale 0, então um
        // `-= 1` incondicional dá wrap para u32::MAX e desliga o processamento de interrupções pelo resto da vida
        // do backend. Só se cura porque o pgrx re-lança o erro e outro `errfinish` zera de novo — o que depende do
        // panic nunca ser capturado. Guardar é grátis e não depende dessa invariante (achado de review; é o mesmo
        // idioma do `lwlock.rs` do pgrx).
        // `InterruptHoldoffCount` é `volatile uint32` (miscadmin.h) — UNSIGNED, então `saturating_sub` satura
        // em 0 e é exatamente o `if > 0 { -= 1 }` que estava aqui, sem o `if` que o clippy recusa
        // (`implicit_saturating_sub`). A verificação do tipo não é cerimônia: sobre um inteiro COM sinal
        // `0.saturating_sub(1)` daria `-1`, e a mesma sugestão do lint teria mudado o comportamento.
        unsafe {
            pg_sys::InterruptHoldoffCount = pg_sys::InterruptHoldoffCount.saturating_sub(1);
        }
    }
}

/// Há um cancelamento/terminação pendente? **Só LÊ os flags — nunca chama `ProcessInterrupts`.**
///
/// O hold de interrupções existe para que uma interrupção não caia dentro do executor do DataFusion, que não tem
/// nada a ver com a máquina de erro do PostgreSQL. Mas com o M168 ele passou a cobrir a leitura de TODAS as
/// páginas — antes o decode acontecia fora dele. Sem um safe-point, um scan de 100M linhas ignora Ctrl-C,
/// `statement_timeout` e `pg_terminate_backend` do começo ao fim (achado de review).
///
/// POR QUE LER-E-DEVOLVER-`Err`, E NÃO CHAMAR `check_for_interrupts!()` AQUI. A razão **não** é a que uma versão
/// anterior deste comentário afirmava. Ela dizia que o macro dispararia um `siglongjmp` pulando os frames Rust,
/// porque "`pg_guard_ffi_boundary` não embrulha chamadas comuns de `pg_sys`". **Isso é falso**, e a fonte
/// vendorizada o refuta em três pontos: o **único** bloco `extern "C-unwind"` do `pgrx-pg-sys-0.19.0` carrega
/// `#[pgrx_macros::pg_guard]` (`src/include/pg18.rs:35462`), com `ProcessInterrupts` dentro dele (`:39525`); o
/// macro `pg_guard` reescreve **cada** função do bloco para `pg_guard_ffi_boundary(move || …)`
/// (`pgrx-macros-0.19.0/src/rewriter.rs:184-193`); e o `ffi.rs:85` declara que a função "is used to protect
/// **every** bindgen-generated Postgres `extern "C-unwind"` function". O PG `ERROR` vira `panic_any`, e os frames
/// Rust **desenrolam**. Este repositório já dizia isso corretamente em `am/build.rs:466` e em
/// `Cargo.toml:85-86` — o comentário anterior contradizia o próprio crate.
///
/// **Isso importa muito além deste arquivo:** há **quatro** `check_for_interrupts!()` vivos em laços de
/// como caminho de produção. Sob o racional falso eles seriam ou declarados BLOCKER em bloco, ou removidos, e o
/// `CREATE INDEX` ficaria incancelável. A falsidade custaria mais do que o defeito que ela alegava corrigir.
///
/// As razões **verdadeiras** para o desenho ler-e-devolver-`Err` são duas, e bastam:
///
/// 1. **Não desenrolar por dentro de frames async de terceiros.** Um panic levantado dentro do `poll_next`
///    atravessa o executor do tokio e o plano físico do DataFusion — código que não escrevemos e cuja
///    exception-safety não auditamos. Devolver `Err` faz o DataFusion desmontar o plano pelo caminho que ele
///    próprio testa, e o panic acontece depois, em código nosso.
/// 2. **Ponto de cancelamento determinístico.** O erro sobe pela mesma borda por onde sobe qualquer outro erro do
///    stream, então há um lugar só onde raciocinar sobre o que já foi liberado.
///
/// Nada disso torna o desenho anterior *inseguro* — torna este mais fácil de auditar.
///
/// SAFETY: leitura de globais do backend; só pode ser chamada na thread do backend.
unsafe fn interrupt_is_pending() -> bool {
    // `InterruptPending` sozinho também cobre eventos benignos (barreiras, notificações), que não devem abortar um
    // scan — ele é o flag que o PG arma primeiro. O que decide é o segundo termo: os flags que `ProcessInterrupts`
    // de fato transforma em ERROR/FATAL.
    //
    // NÃO É UMA LISTA EXAUSTIVA, e duas versões anteriores deste comentário afirmaram que era ("os dois", depois
    // "são quatro"). O que está coberto:
    //
    //   * `QueryCancelPending`         -> ERROR (`tcop/postgres.c:3372`) — Ctrl-C, statement_timeout
    //   * `ProcDiePending`             -> FATAL (`:3300`)                — pg_terminate_backend
    //   * `TransactionTimeoutPending`  -> **FATAL** (`:3460`)            — transaction_timeout
    //   * `ClientConnectionLost`       -> FATAL (`:3341`)                — defense-in-depth, ver abaixo
    //
    // Sem `TransactionTimeoutPending`, um `SET transaction_timeout` sobre um scan longo era ignorado do começo ao
    // fim: o handler (`postinit.c:1409-1414`) arma SÓ esse flag, nunca `QueryCancelPending`.
    //
    // O QUE FICA DESCOBERTO, e não dá para fechar daqui: `RecoveryConflictPending` (`:3432` ->
    // `ProcessRecoveryConflictInterrupts`, ERROR/FATAL) é `static volatile sig_atomic_t` em `postgres.c:171` —
    // escopo de arquivo, sem binding no pgrx, invisível para uma extensão. **Consequência real: num hot standby,
    // um scan colunar streaming longo continua ignorando conflitos de recuperação até terminar**, atrasando o
    // replay além do `max_standby_streaming_delay`. É a mesma classe de buraco que o `TransactionTimeoutPending`
    // acabou de fechar, e a ação correta aqui é declará-lo, não fingir cobertura (achado de review).
    //
    // `ClientConnectionLost` é defense-in-depth e provavelmente inalcançável nesta janela: ele é armado em
    // `pqcomm.c:1411` (um *send* que falhou — um scan longo não envia nada) e dentro do próprio
    // `ProcessInterrupts` (`:3334`), que retorna cedo enquanto o holdoff está de pé. Custa uma leitura de `i32`;
    // fica, mas sem alegar que fecha um buraco vivo.
    //
    // `IdleInTransactionSessionTimeoutPending` fica DE FORA de propósito: por definição ele só dispara com a sessão
    // ociosa, e um scan em curso não é uma sessão ociosa.
    //
    // Todos são `sig_atomic_t` (i32), não `bool` — comparar com 0 é o que o C faz.
    (unsafe { pg_sys::InterruptPending }) != 0
        && (unsafe { pg_sys::QueryCancelPending } != 0
            || unsafe { pg_sys::ProcDiePending } != 0
            || unsafe { pg_sys::ClientConnectionLost } != 0
            || unsafe { pg_sys::TransactionTimeoutPending } != 0)
}

/// Map the decoded columnar columns (name, atttypid, per-row stored bytes) to an Arrow schema + arrays. The stored
/// bytes are the codec encoding (fixed: attlen LE bytes; varlena: logical payload). Builtin type OIDs (pg_type.dat,
/// ABI-stable) drive the Arrow `DataType`.
pub(super) fn build_arrow(
    cols: &[(String, u32, Vec<Option<Vec<u8>>>)],
) -> Result<(Schema, Vec<ArrayRef>), String> {
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(cols.len());
    for (name, typid, values) in cols {
        let (dt, arr) = cells_to_array(*typid, values)?;
        fields.push(Field::new(name, dt, true));
        arrays.push(arr);
    }
    Ok((Schema::new(fields), arrays))
}

/// M160 — build the Arrow schema + arrays from `DecodedColumn`s (the pushdown fast path). `FixedRaw` columns (non-null
/// fixed-width) go through `fixed_raw_array` (one typed `Vec<T>` per column, no per-cell alloc); `Cells` columns reuse
/// `cells_to_array` (byte-identical to the legacy `build_arrow`). This is the M160 win applied only to the hot path.
pub(super) fn build_arrow_from_decoded(
    cols: &[(String, u32, super::columnar::DecodedColumn)],
) -> Result<(Schema, Vec<ArrayRef>), String> {
    use super::columnar::DecodedColumn;
    let mut fields = Vec::with_capacity(cols.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(cols.len());
    for (name, typid, dc) in cols {
        let (dt, arr) = match dc {
            DecodedColumn::FixedRaw { bytes, width, row_count } => {
                fixed_raw_array(*typid, bytes, *width, *row_count)?
            }
            DecodedColumn::Cells(values) => cells_to_array(*typid, values)?,
        };
        fields.push(Field::new(name, dt, true));
        arrays.push(arr);
    }
    Ok((Schema::new(fields), arrays))
}

/// M160 — build an Arrow `PrimitiveArray` for a non-null fixed-width column directly from the contiguous little-endian
/// value stream: one pre-sized typed `Vec<T>` (endian-safe `from_le_bytes` fill) handed zero-copy to Arrow via
/// `PrimitiveArray::from(Vec<T>)`. No per-cell `Vec<u8>` boxing (the deep-dive flamegraph's dominant cost). BYTE-IDENTICAL
/// to `cells_to_array` for these types because that path also does a plain `from_le_bytes` with no epoch/other transform
/// (df_executor build_arrow comment: "the stored bytes ARE the internal int"). Only types in `fixed_arrow_width` reach here.
fn fixed_raw_array(
    typid: u32,
    bytes: &[u8],
    width: usize,
    row_count: usize,
) -> Result<(DataType, ArrayRef), String> {
    if bytes.len() != width * row_count {
        return Err(format!(
            "df_executor: fixed-raw size {} != {width}*{row_count} (typid {typid})",
            bytes.len()
        ));
    }
    Ok(match typid {
        21 => (
            DataType::Int16,
            Arc::new(Int16Array::from(
                bytes.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>(),
            )),
        ),
        23 => (
            DataType::Int32,
            Arc::new(Int32Array::from(
                bytes
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect::<Vec<_>>(),
            )),
        ),
        20 => (
            DataType::Int64,
            Arc::new(Int64Array::from(
                bytes
                    .chunks_exact(8)
                    .map(|c| i64::from_le_bytes(c.try_into().unwrap()))
                    .collect::<Vec<_>>(),
            )),
        ),
        700 => (
            DataType::Float32,
            Arc::new(Float32Array::from(
                bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect::<Vec<_>>(),
            )),
        ),
        701 => (
            DataType::Float64,
            Arc::new(Float64Array::from(
                bytes
                    .chunks_exact(8)
                    .map(|c| f64::from_le_bytes(c.try_into().unwrap()))
                    .collect::<Vec<_>>(),
            )),
        ),
        1082 => (
            DataType::Date32,
            Arc::new(Date32Array::from(
                bytes
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect::<Vec<_>>(),
            )),
        ),
        1114 | 1184 => (
            DataType::Timestamp(TimeUnit::Microsecond, None),
            Arc::new(TimestampMicrosecondArray::from(
                bytes
                    .chunks_exact(8)
                    .map(|c| i64::from_le_bytes(c.try_into().unwrap()))
                    .collect::<Vec<_>>(),
            )),
        ),
        other => {
            return Err(format!("df_executor: fixed_raw_array unexpected typid {other}"));
        }
    })
}

/// The legacy per-cell → Arrow conversion (extracted from `build_arrow` unchanged) — used by the cell path (nullable /
/// varlena / text) and by `arrow_cache`. Kept byte-identical: every arm is a plain `from_le_bytes`/`from_utf8_lossy`.
fn cells_to_array(typid: u32, values: &[Option<Vec<u8>>]) -> Result<(DataType, ArrayRef), String> {
    Ok(match typid {
        21 => (
            DataType::Int16,
            Arc::new(Int16Array::from_iter(
                values.iter().map(|v| v.as_ref().map(|b| i16::from_le_bytes([b[0], b[1]]))),
            )),
        ),
        23 => (
            DataType::Int32,
            Arc::new(Int32Array::from_iter(
                values
                    .iter()
                    .map(|v| v.as_ref().map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
            )),
        ),
        20 => (
            DataType::Int64,
            Arc::new(Int64Array::from_iter(
                values
                    .iter()
                    .map(|v| v.as_ref().map(|b| i64::from_le_bytes(b[..8].try_into().unwrap()))),
            )),
        ),
        700 => (
            DataType::Float32,
            Arc::new(Float32Array::from_iter(
                values
                    .iter()
                    .map(|v| v.as_ref().map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
            )),
        ),
        701 => (
            DataType::Float64,
            Arc::new(Float64Array::from_iter(
                values
                    .iter()
                    .map(|v| v.as_ref().map(|b| f64::from_le_bytes(b[..8].try_into().unwrap()))),
            )),
        ),
        16 => (
            DataType::Boolean,
            Arc::new(BooleanArray::from_iter(
                values.iter().map(|v| v.as_ref().map(|b| b.first().copied().unwrap_or(0) != 0)),
            )),
        ),
        25 | 1042 | 1043 => (
            DataType::Utf8,
            Arc::new(StringArray::from_iter(
                values.iter().map(|v| v.as_ref().map(|b| String::from_utf8_lossy(b).into_owned())),
            )),
        ),
        // Temporal: the stored bytes ARE the internal int (timestamp/timestamptz = int64 μs, date = int32 days).
        // Both mapped to a naive (tz=None) Arrow type — the comparison is on the raw int domain (tz is display
        // only), so `build_filter_expr`'s matching-typed literal compares correctly (D3).
        1114 | 1184 => (
            DataType::Timestamp(TimeUnit::Microsecond, None),
            Arc::new(TimestampMicrosecondArray::from_iter(
                values
                    .iter()
                    .map(|v| v.as_ref().map(|b| i64::from_le_bytes(b[..8].try_into().unwrap()))),
            )),
        ),
        1082 => (
            DataType::Date32,
            Arc::new(Date32Array::from_iter(
                values
                    .iter()
                    .map(|v| v.as_ref().map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
            )),
        ),
        other => {
            return Err(format!(
                "df_executor: unsupported column type oid {other} (Phase A: int2/4/8, float4/8, bool, text)"
            ));
        }
    })
}

/// Whether this OID is safe as a GROUP BY key / `COUNT(DISTINCT)` column for the vectorized path. DataFusion groups
/// by byte-equality; PG groups by the type's + collation's equality. `bpchar`/`char(n)` (OID 1042) is EXCLUDED
/// (review MEDIUM): `bpchareq` ignores trailing blanks (a TYPE semantic, `varchar.c:756-773`), so `'ab'` and `'ab '`
/// are equal in PG but byte-different in storage → the byte-keyed hash would over-count. text (25) / varchar (1043)
/// have no such trailing-blank rule, and their deterministic-collation equality IS byte-equality (`varlena.c` memcmp
/// tiebreak). char(n)-with-length is padded uniformly and would be safe, but is indistinguishable from bare bpchar by
/// OID alone → excluded conservatively (char(n) GROUP BY declines to native; rare, ClickBench uses none).
pub(super) fn arrow_supported_group_type(typoid: u32) -> bool {
    matches!(typoid, 21 | 23 | 20 | 700 | 701 | 16 | 25 | 1043 | 1114 | 1184 | 1082)
}

/// A supported aggregate for the vectorized columnar path — ONLY shapes whose Arrow result type maps to the exact
/// PostgreSQL output type (M114 blueprint E1/E2/E3): `count(*)`→int8, `sum(float8)`→float8, `sum(int2/int4)`→int8
/// (Arrow Int64 = PG bigint, no overflow), `avg(float8)`→float8. Numeric-output shapes (`avg(int)`, `sum(int8)`,
/// `sum(float4)`) are DECLINED at admit time (ADR-M114-1) — never reach here.
pub(super) enum AggSpec {
    CountStar,
    SumFloat8(String),
    /// `sum(int2/int4)` → PG int8. DataFusion coerces int2/int4 → Int64; the datum is int8 (no overflow).
    SumInt(String),
    /// M166 — `sum(int2_col ± const)` → PG int8 (ClickBench q29). Admitted ONLY for an int2 base column with an int4
    /// operator result whose per-row `col ± delta` provably stays in int4 (so PG raises no 22003 and the Int64 sum is
    /// exact). `delta` folds the operator sign (`col - k` = `col + (-k)`). Output kind = SumInt (Arrow Int64 → PG int8).
    SumIntAddConst {
        col: String,
        delta: i64,
    },
    /// `avg(float8)` → PG float8. DataFusion `avg` → Float64.
    AvgFloat8(String),
    /// `sum(int8)` → PG `numeric` (exact). DataFusion `sum(cast(col AS Decimal128(38,0)))` → i128; the datum is a PG
    /// numeric via `AnyNumeric` (blueprint ADR-N1 — Int64 sum would wrap).
    SumInt8Numeric(String),
    /// `avg(int2/4/8)` → PG `numeric` (data-dependent scale). Decomposed to `sum(cast Decimal128(38,0))` + `count`;
    /// the datum is `AnyNumeric(sum) / AnyNumeric(count)` = PG's `numeric_div` (ADR-N1/N2). Emits TWO batch columns.
    AvgIntNumeric(String),
    /// `min(col)`/`max(col)` on an ordered native type → PG output type = the input column type. DataFusion `min()`/
    /// `max()` yields an Arrow array of the source column's type; the datum is emitted by `arrow_value_to_datum`
    /// against the carried output typoid (columnar-minmax blueprint ADR-MM1/MM3). NaN-correct because it decodes
    /// actual values — unlike the Phase-B directory fold which gates `max(float)` out.
    MinCol(String, u32),
    MaxCol(String, u32),
    /// `count(DISTINCT col)` → PG int8. DataFusion `count_distinct(col)` = exact `DistinctCountAccumulator`
    /// (`distinct: true` over the exact `count_udaf`) → Int64; the datum is int8 (M154, same path as CountStar).
    /// NEVER approx/HLL (ADR-M154-1) — the A/B gate demands byte-identity with PG COUNT(DISTINCT).
    CountDistinct(String),
}

impl AggSpec {
    /// The source column this aggregate reads (None for `count(*)`), for projection.
    fn col_name(&self) -> Option<&str> {
        match self {
            AggSpec::CountStar => None,
            AggSpec::SumFloat8(n)
            | AggSpec::SumInt(n)
            | AggSpec::SumIntAddConst { col: n, .. }
            | AggSpec::AvgFloat8(n)
            | AggSpec::SumInt8Numeric(n)
            | AggSpec::AvgIntNumeric(n)
            | AggSpec::MinCol(n, _)
            | AggSpec::MaxCol(n, _)
            | AggSpec::CountDistinct(n) => Some(n.as_str()),
        }
    }

    /// The number of DataFusion output columns this aggregate produces (avg-int decomposes to sum + count — ADR-N2).
    fn ncols(&self) -> usize {
        match self {
            AggSpec::AvgIntNumeric(_) => 2,
            _ => 1,
        }
    }
}

/// Push the aliased DataFusion aggregate expression(s) for one `AggSpec` (usually 1; avg-int emits sum + count).
/// Aliases are sequential (`a{k}`) by the running column position, so a multi-column spec keeps unique aliases.
fn push_agg_exprs(spec: &AggSpec, exprs: &mut Vec<Expr>) {
    let k = exprs.len();
    match spec {
        AggSpec::CountStar => exprs.push(count(lit(1i64)).alias(format!("a{k}"))),
        AggSpec::SumFloat8(name) | AggSpec::SumInt(name) => {
            exprs.push(sum(col(name.as_str())).alias(format!("a{k}")))
        }
        // M166 — sum over `int2_col ± const`: widen the base column to Int64 and add the (signed) delta before summing,
        // mirroring the group IntAddConst `cast(col, Int64) + lit(delta)` idiom. The per-row value is in int4 range by
        // the admit gate, so the Int64 sum is byte-identical to PG's `sum(int4) → int8`. Output kind = SumInt.
        AggSpec::SumIntAddConst { col: name, delta } => exprs.push(
            sum(cast(col(name.as_str()), DataType::Int64) + lit(*delta)).alias(format!("a{k}")),
        ),
        AggSpec::AvgFloat8(name) => exprs.push(avg(col(name.as_str())).alias(format!("a{k}"))),
        AggSpec::SumInt8Numeric(name) => {
            exprs.push(sum(dec128_cast(name)).alias(format!("a{k}")));
        }
        AggSpec::AvgIntNumeric(name) => {
            exprs.push(sum(dec128_cast(name)).alias(format!("a{k}")));
            exprs.push(count(col(name.as_str())).alias(format!("a{}", k + 1)));
        }
        AggSpec::MinCol(name, _) => exprs.push(min(col(name.as_str())).alias(format!("a{k}"))),
        AggSpec::MaxCol(name, _) => exprs.push(max(col(name.as_str())).alias(format!("a{k}"))),
        AggSpec::CountDistinct(name) => {
            exprs.push(count_distinct(col(name.as_str())).alias(format!("a{k}")))
        }
    }
}

/// `cast(col AS Decimal128(38,0))` — the exact integer-sum path (Int64 sum wraps; blueprint ADR-N1).
fn dec128_cast(name: &str) -> Expr {
    datafusion::logical_expr::cast(col(name), DataType::Decimal128(38, 0))
}

/// Decode the columnar table's projected columns into one Arrow `RecordBatch` (projection pushdown). Projects the
/// `sum` columns PLUS every zone-map predicate's column (so the DataFusion Filter can re-check it — ADR D3), and
/// always ≥ 1 column so `count(*)` has a row count. Passes the predicates + `skip` to `decode_columns` so the
/// min/max zone-map can skip proven-non-matching chunk groups.
unsafe fn decode_to_batch(
    rel: pg_sys::Relation,
    sum_cols: &[String],
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
) -> Result<RecordBatch, String> {
    let mut proj: Vec<usize> = Vec::new();
    for name in sum_cols {
        let idx = unsafe { super::columnar::column_index(rel, name) }
            .ok_or_else(|| format!("df_executor: column '{name}' not found"))?;
        if !proj.contains(&idx) {
            proj.push(idx);
        }
    }
    for p in predicates {
        if !proj.contains(&p.col) {
            proj.push(p.col); // the filter column MUST be decoded so the DataFusion Filter can re-check it (D3)
        }
    }
    // M156 — the text-predicate column MUST be decoded (as a Utf8 array) so the DataFusion Filter can re-check it.
    // Text predicates never drive zone-map skipping (only `predicates` is passed to `decode_columns` below).
    for t in text_predicates {
        if !proj.contains(&t.col) {
            proj.push(t.col);
        }
    }
    // M161 — the IN-list column MUST be decoded so the DataFusion Filter can re-check `col IN (…)` (D3 final authority).
    // IN-list never drives zone-map skipping (only `predicates` is passed to `decode_columns` below).
    for ip in in_predicates {
        if !proj.contains(&ip.col) {
            proj.push(ip.col);
        }
    }
    if proj.is_empty() {
        proj.push(0); // count(*) needs a column to establish the row count
    }
    // M160 — decode_columns_v2 returns fixed-width non-null columns as `FixedRaw` (contiguous LE bytes, no per-cell
    // alloc); build_arrow_from_decoded turns those into Arrow via one typed Vec<T> per column. Cell columns are
    // byte-identical to the legacy path. This is the hot pushdown path only (vindex/arrow_cache keep decode_columns).
    let cols = unsafe { super::columnar::decode_columns_v2(rel, Some(&proj), predicates, skip)? };
    let (schema, arrays) = build_arrow_from_decoded(&cols)?;
    RecordBatch::try_new(Arc::new(schema), arrays)
        .map_err(|e| format!("df_executor: arrow batch: {e}"))
}

/// Build the DataFusion filter `Expr` for the pushed zone-map predicates (a conjunction of `col <op> const`), the
/// FINAL authority over rows in surviving chunk groups (ADR D3 — the skip is only an admission filter). `col` is
/// resolved to its name via the tupdesc; the literal is typed to the column's `MinMaxKind` (matching `build_arrow`).
unsafe fn build_filter_expr(
    rel: pg_sys::Relation,
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
) -> Option<Expr> {
    use super::columnar_codec::MinMaxKind;
    use super::zonemap::{TextOp, ZoneOp};
    let tupdesc = unsafe { (*rel).rd_att };
    let natts = unsafe { (*tupdesc).natts } as usize;
    let mut acc: Option<Expr> = None;
    for p in predicates {
        if p.col >= natts {
            continue; // fail-safe (EC-2): unknown column → do not build a filter term on it
        }
        let att = unsafe { super::tupdesc_attr(tupdesc, p.col) };
        let name =
            unsafe { CStr::from_ptr((*att).attname.data.as_ptr()).to_string_lossy().into_owned() };
        let c = col(name.as_str());
        let b = p.const_bits;
        // Temporal columns share the I8/I4 min/max domain but need an Arrow-typed literal so the Filter matches the
        // Timestamp/Date column type built in `build_arrow` (a bare Int64 lit would type-mismatch). Intercept by OID
        // BEFORE the MinMaxKind dispatch. tz=None to match build_arrow (raw-int compare — D3).
        let val = match unsafe { (*att).atttypid.to_u32() } {
            1114 | 1184 => lit(ScalarValue::TimestampMicrosecond(Some(b as i64), None)),
            1082 => lit(ScalarValue::Date32(Some(b as i64 as i32))),
            _ => match super::columnar::minmax_kind_of(unsafe { (*att).atttypid.to_u32() }) {
                MinMaxKind::I2 => lit(b as i64 as i16),
                MinMaxKind::I4 => lit(b as i64 as i32),
                MinMaxKind::I8 => lit(b as i64),
                MinMaxKind::Bool => lit(b != 0),
                MinMaxKind::F4 => lit(f64::from_bits(b) as f32),
                MinMaxKind::F8 => lit(f64::from_bits(b)),
                MinMaxKind::None => continue, // not min/max-able → cannot have been pushed
            },
        };
        let e = match p.op {
            ZoneOp::Lt => c.lt(val),
            ZoneOp::Le => c.lt_eq(val),
            ZoneOp::Eq => c.eq(val),
            ZoneOp::Ge => c.gt_eq(val),
            ZoneOp::Gt => c.gt(val),
            ZoneOp::Ne => c.not_eq(val), // M151 — `<>` filter-only (never pruned); the executor's final authority
        };
        acc = Some(match acc {
            Some(prev) => prev.and(e),
            None => e,
        });
    }
    // M156 — text predicates: `col <op> 'needle'` over the decoded Utf8 column. LIKE/NOT LIKE use DataFusion's
    // default `\` escape (planner rejects any other; None == backslash, matching PG's default) — proven byte-identical
    // by the A/B `LIKE 'a\%b'`. `<>`/NOT LIKE against NULL yield NULL → row excluded, same 3-valued logic as PG.
    for t in text_predicates {
        if t.col >= natts {
            continue; // fail-safe: unknown column → do not build a filter term on it
        }
        let att = unsafe { super::tupdesc_attr(tupdesc, t.col) };
        let name =
            unsafe { CStr::from_ptr((*att).attname.data.as_ptr()).to_string_lossy().into_owned() };
        let c = col(name.as_str());
        let val = lit(ScalarValue::Utf8(Some(t.needle.clone())));
        let e = match t.op {
            TextOp::Eq => c.eq(val),
            TextOp::Ne => c.not_eq(val),
            TextOp::Like => c.like(val),
            TextOp::NotLike => c.not_like(val),
        };
        acc = Some(match acc {
            Some(prev) => prev.and(e),
            None => e,
        });
    }
    // M161 — integer IN-list: `col IN (c0, c1, …)` over the decoded integer column. The extractor admitted ONLY the
    // integer class (I2/I4/I8) with no NULL element and `=`/useOr semantics, so `col.in_list(lits, false)` is exactly
    // an OR of `=` — DataFusion's final authority, byte-identical to PG's ScalarArrayOpExpr. Literals are typed to the
    // column's MinMaxKind so the Filter matches the Arrow column type built in `build_arrow` (a bare Int64 lit would
    // type-mismatch an Int32 column).
    for ip in in_predicates {
        if ip.col >= natts || ip.consts.is_empty() {
            continue; // fail-safe: unknown column / empty list → do not build a filter term on it
        }
        let att = unsafe { super::tupdesc_attr(tupdesc, ip.col) };
        let name =
            unsafe { CStr::from_ptr((*att).attname.data.as_ptr()).to_string_lossy().into_owned() };
        let c = col(name.as_str());
        let kind = super::columnar::minmax_kind_of(unsafe { (*att).atttypid.to_u32() });
        let lits: Vec<Expr> = ip
            .consts
            .iter()
            .map(|&v| match kind {
                MinMaxKind::I2 => lit(v as i16),
                MinMaxKind::I4 => lit(v as i32),
                _ => lit(v), // I8 (extractor admits only the integer class)
            })
            .collect();
        let e = c.in_list(lits, false);
        acc = Some(match acc {
            Some(prev) => prev.and(e),
            None => e,
        });
    }
    acc
}

/// Run the aggregates over the columnar table via a vectorized DataFusion plan under `HeldInterrupts`; return one
/// `(Datum, is_null)` per agg, in `aggs` order, ready to store in a `TupleTableSlot`. `count(*)`→`int8` Datum;
/// `sum(float8)`→`float8` Datum. This is the executor the planner `CustomScan` (Phase C) drives at exec time.
/// `predicates`/`skip`: the zone-map pushdown — the batch is filtered by the predicate (D3 final authority) and the
/// decode skips proven-non-matching chunk groups.
pub(super) unsafe fn run_columnar_aggs(
    rel: pg_sys::Relation,
    aggs: &[AggSpec],
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
) -> Result<Vec<(pg_sys::Datum, bool)>, String> {
    let agg_cols: Vec<String> =
        aggs.iter().filter_map(|a| a.col_name().map(str::to_string)).collect();
    let filter = unsafe { build_filter_expr(rel, predicates, text_predicates, in_predicates) };
    let mut exprs = Vec::with_capacity(aggs.len());
    for a in aggs {
        push_agg_exprs(a, &mut exprs);
    }
    let batches = unsafe {
        run_df_over_columnar(
            rel,
            &agg_cols,
            predicates,
            text_predicates,
            in_predicates,
            skip,
            move |df| {
                // Zone-map predicate as the FINAL authority over surviving rows (D3): filter BEFORE aggregating.
                let df = match filter {
                    Some(f) => df.filter(f)?,
                    None => df,
                };
                df.aggregate(vec![], exprs)
            },
        )?
    };
    unsafe { aggs_from_batches(&batches, aggs) }
}

/// M169 — run a DataFusion plan over the columnar relation, streaming one chunk-group at a time when possible.
///
/// THE single place the aggregate paths choose between the streaming source and the eager whole-relation batch.
/// Both `run_columnar_aggs` (scalar) and `run_columnar_grouped_aggs` (grouped) route through it, because the
/// measured target spans BOTH: of the three `byte array offset overflow` instances at 100M, q20 is scalar
/// (`COUNT(*) … WHERE URL LIKE`) and q33/q34 are grouped (`GROUP BY URL`). Two copies of this decision would
/// drift, and the drift would be invisible to a test that exercises only one of the two shapes.
///
/// **The fail-open is CONDITIONAL — the top-k's shape copied verbatim would reintroduce the defect (ADR-5).**
/// There, recuing to eager is safe: the eager path is the pre-M168 behaviour and it works, while the `TopK`
/// retention grows with `k` against a constant streaming pool, so a large `k` can exhaust a pool that eager
/// served. Here the eager path is *sometimes* the defect: `decode_to_batch` builds each text column as ONE Arrow
/// `Utf8` array with `i32` offsets, so past `i32::MAX` bytes of values it cannot succeed at all.
///
/// So the fallback is gated on the exact byte total (`ScanPlan::varlena_raw_len_max_per_column`), not on a heuristic:
///
/// - projected varlena bytes **< `i32::MAX`** → eager cannot fail *for this reason* → fall back, exactly as the
///   top-k does. This is the case the plan refused to punish: at 1M with a small `work_mem` the streaming pool is
///   tiny, and a query eager served would otherwise start failing.
/// - projected varlena bytes **≥ `i32::MAX`** → eager provably CANNOT succeed → the typed `ResourcesExhausted`
///   rises as an error saying to raise `work_mem`. An error that names its remedy is actionable; a panic is not.
///
/// **Precision about what the pre-check does NOT promise.** Below the threshold it rules out the *offset*
/// overflow, and only that. The eager path still materialises every projected row into one Arrow batch, so a wide
/// fixed-width relation can exhaust memory with `varlena_bytes = 0`. That is not a regression this introduces —
/// it is exactly the pre-M169 behaviour of the aggregate — but reading the condition as "eager will succeed"
/// would be wrong, and someone will read it.
///
/// The eager path also runs, as the CORRECT answer rather than a fallback, when the GUC is off (the paired
/// "before" arm of the measurement) or when `open_streaming_source` declines — unflushed pending rows (a scan
/// planned from visible stripes alone cannot see this transaction's own writes) or nothing visible at all.
unsafe fn run_df_over_columnar<F>(
    rel: pg_sys::Relation,
    proj_cols: &[String],
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
    build: F,
) -> Result<Vec<RecordBatch>, String>
where
    // `Clone` (not just `FnOnce`) because the conditional fail-open may run the SAME plan twice — once streamed,
    // once eager. Rebuilding the closure at each call site instead would put the plan in two places, which is how
    // the two arms would silently drift apart.
    F: FnOnce(
            datafusion::dataframe::DataFrame,
        ) -> Result<datafusion::dataframe::DataFrame, DataFusionError>
        + Clone
        + Send
        + 'static,
{
    // ZERADO NA ENTRADA, antes da decisão de rota. O reset que existia estava só no ramo do fail-open, e a
    // varredura do review encontrou QUATRO caminhos que terminam no eager: o fail-open (que zerava), o declínio
    // por `has_unflushed_pending` (antes do `new()`, então o contador guardava o valor da consulta ANTERIOR), o
    // declínio por nada visível (DEPOIS de um `next()`, então o contador ficava em 1) e a GUC desligada. Nos três
    // últimos o oráculo de não-vacuidade leria `> 0` e afirmaria "passou pelo stream" sobre uma resposta que veio
    // do eager — a MESMA classe de falso-verde que o reset do fail-open corrigiu, uma linha adiante. Zerar aqui
    // faz `0` significar, sem exceção, "o scan colunar mais recente não streamou". Corrigir a instância e deixar
    // os irmãos vivos é o defeito recorrente desta sessão; esta é a varredura da classe.
    super::columnar::reset_stream_cg_count();
    if super::columnar_agg::ENABLE_COLUMNAR_AGG_STREAM.get()
        && let Some((part, varlena_bytes)) = unsafe {
            open_streaming_source(rel, proj_cols, predicates, text_predicates, in_predicates, skip)?
        }
    {
        match unsafe { run_df_collect_streaming(part, build.clone()) } {
            Ok(batches) => return Ok(batches),
            // `find_root()`, NÃO `match` na variante exata — a mesma disciplina do top-k (`:1346-1353`), e pela
            // mesma razão: o DataFusion embrulha `ResourcesExhausted` em `Context(_, Box(ResourcesExhausted))`
            // num caminho vizinho (`sorts/sort.rs` usa `err_with_oom_context`), e casar a variante nua deixa o
            // recuo de fora justamente quando ele é necessário. Hoje o agregado devolve o erro CRU
            // (`aggregates/grouped_hash_stream.rs:955`), então a diferença é LATENTE — mas a direção da falha é
            // pior aqui que no top-k: esta GUC é default ON, logo um `ResourcesExhausted` embrulhado viraria erro
            // duro em consultas que o caminho eager pré-M169 servia. Achado do SEPA; a AC do plano exige
            // `find_root()` textualmente.
            // M169 T4.1 — a classe FOI AMPLIADA depois da corrida completa a 100M, que mediu q08/q09
            // (`COUNT(DISTINCT …) GROUP BY …`) REGREDINDO de `ok` para erro. Elas não quebraram por defeito
            // novo: vinham passando porque a pool do caminho eager é dimensionada pelo BATCH
            // (`:798`, `max(work_mem, batch*2)`), e a 100M isso dava ~2,5 GB. Sem o batch O(N) — que é
            // exatamente o que este milestone remove — a pool virou `work_mem`-derivada, o agregado passou a
            // derramar, e o derrame falhou. Ou seja: a pool generosa era efeito colateral do próprio O(N).
            Err(e) if stream_failure_is_fail_open(&e, varlena_bytes) => {
                // INCONDICIONAL, não atrás do flag de trace — mesma decisão e mesmo racional do top-k
                // (`:1354-1358`): sem isto o usuário não tem sinal de que a consulta acabou de trocar de perfil
                // de memória e de latência.
                pgrx::log!("theodb_agg_stream_fallback: {e}");
                // E o contador precisa VOLTAR A ZERO. Ele é a prova de não-vacuidade que
                // `benchmarks/m169_agg_stream.sql` usa para afirmar "o braço on passou mesmo pelo stream"; sem o
                // reset ele reteria as chamadas da tentativa que FALHOU, e o gate leria `calls > 0` enquanto a
                // resposta veio do eager. Falso-verde no meu próprio oráculo (achado do SEPA).
                super::columnar::reset_stream_cg_count();
            }
            Err(e) => return Err(format!("df_executor: datafusion: {e}")),
        }
    }
    let batch = unsafe {
        decode_to_batch(rel, proj_cols, predicates, text_predicates, in_predicates, skip)?
    };
    unsafe { run_df_collect(batch, build) }
}

/// Extract one `(Datum, is_null)` per agg from the single scalar-aggregate result batch.
///
/// Shared by the eager (`run_aggs_on_batch`, also the M101 heap-Arrow-cache caller) and the streaming path, so
/// the two cannot drift in how they read the aggregate output — a drift a test exercising one path would miss.
unsafe fn aggs_from_batches(
    batches: &[RecordBatch],
    aggs: &[AggSpec],
) -> Result<Vec<(pg_sys::Datum, bool)>, String> {
    let b = batches.first().ok_or("df_executor: no result batch")?;
    let mut result = Vec::with_capacity(aggs.len());
    let mut off = 0; // batch column cursor — a multi-column spec (avg-int) consumes >1 column
    for a in aggs {
        result.push(agg_datum(b, off, 0, a)?);
        off += a.ncols();
    }
    Ok(result)
}

/// Run the aggregates over an already-built Arrow `RecordBatch` via a vectorized DataFusion plan under
/// `HeldInterrupts` + a `work_mem` MemoryPool + `target_partitions=1`. Shared by the M100 columnar path
/// (`run_columnar_aggs`) and the M101 heap-authoritative Arrow cache path (`arrow_cache.rs`).
pub(super) unsafe fn run_aggs_on_batch(
    batch: RecordBatch,
    aggs: &[AggSpec],
    filter: Option<Expr>,
) -> Result<Vec<(pg_sys::Datum, bool)>, String> {
    let mut exprs = Vec::with_capacity(aggs.len());
    for a in aggs {
        push_agg_exprs(a, &mut exprs);
    }

    let batches = unsafe {
        run_df_collect(batch, move |df| {
            // Zone-map predicate as the FINAL authority over surviving rows (D3): filter BEFORE aggregating.
            let df = match filter {
                Some(f) => df.filter(f)?,
                None => df,
            };
            df.aggregate(vec![], exprs)
        })?
    };
    unsafe { aggs_from_batches(&batches, aggs) }
}

/// Marcador da falha de criação de arquivo de spill do DataFusion. Vem de
/// `datafusion-physical-plan-54.0.0/src/spill/mod.rs:311`, onde um `File::create` que falha é embrulhado por
/// `exec_datafusion_err!` — portanto chega como `Execution`, **não** como `ResourcesExhausted` nem `IoError`.
/// Casar por substring é frágil a um upgrade do DataFusion, e é uma escolha consciente: a alternativa (casar
/// `Execution` inteiro) recuaria para o eager em erro genuíno do braço streaming, mascarando defeito nosso —
/// ver o teste `unrelated_execution_error_does_not_fail_open`. Um bump de major do DataFusion DEVE reverificar
/// esta string.
const SPILL_CREATE_MARKER: &str = "Failed to create partition file";

/// Constrói o `DiskManager` do DataFusion apontando para o diretório temporário DO POSTGRESQL, com teto
/// derivado de `temp_file_limit`.
///
/// **Por que isto não é cosmético (achado do review do M169).** O default do `RuntimeEnvBuilder` derrama em
/// `std::env::temp_dir()` — `TMPDIR` ou `/tmp` —, o que tem quatro consequências, todas fora do controle do
/// operador:
///
/// 1. **`temp_file_limit` não se aplica.** O `File::create` do DataFusion é `std::fs`, não passa pelo `fd.c`,
///    então o GUC com que um DBA contém consultas famintas fica inerte.
/// 2. **`log_temp_files` e `pg_stat_database.temp_bytes` ficam cegos** — quem for depurar "de onde vieram 40 GB"
///    não acha por instrumento do PostgreSQL.
/// 3. **`/tmp` é tmpfs em boa parte das distros com systemd.** Ali "derramar para disco para limitar a memória"
///    é *alocar mais RAM*, e o teto que a pool promete deixa de existir. O OOM-killer numa instância PostgreSQL
///    não mata o backend: mata o postmaster.
/// 4. **Vaza arquivo em morte anormal.** `ereport(FATAL)` chama `proc_exit`, que não roda `Drop`. O PostgreSQL
///    limpa `pgsql_tmp` no restart; não limpa `/tmp`.
///
/// Isto **não era pré-existente na prática**: antes do M169 a pool do agregado era dimensionada pelo batch O(N)
/// (~2,5 GB a 100M) e nada derramava. Este milestone tornou o spill um caminho de produção, com a GUC ligada por
/// default — então a configuração passa a ser responsabilidade dele.
///
/// Falha ao resolver o diretório é **não-fatal**: cai no default do DataFusion, porque perder o spill inteiro
/// seria pior que derramá-lo no lugar errado. O trace diz qual caminho foi tomado.
fn spill_disk_manager() -> Option<datafusion::execution::disk_manager::DiskManagerBuilder> {
    use datafusion::execution::disk_manager::{DiskManagerBuilder, DiskManagerMode};
    // SAFETY: `DataDir` é um `*mut c_char` global do postmaster, válido durante toda a vida do backend.
    let data_dir = unsafe {
        let p = pg_sys::DataDir;
        if p.is_null() {
            return None;
        }
        std::ffi::CStr::from_ptr(p).to_str().ok()?.to_owned()
    };
    let tmp = std::path::PathBuf::from(data_dir).join("base").join("pgsql_tmp");
    if !tmp.is_dir() {
        return None; // não criamos o diretório: quem o cria é o PostgreSQL, e criá-lo aqui mascararia um cluster estranho
    }
    let mut b = DiskManagerBuilder::default().with_mode(DiskManagerMode::Directories(vec![tmp]));
    // `temp_file_limit` é em KB e `-1` significa ilimitado — nesse caso deixamos o teto do DataFusion.
    let limit_kb = unsafe { pg_sys::temp_file_limit };
    if limit_kb > 0 {
        b = b.with_max_temp_directory_size(u64::from(limit_kb.unsigned_abs()) * 1024);
    }
    Some(b)
}

/// A falha do braço streaming autoriza recuar para o caminho eager?
///
/// Duas condições, e a segunda é a que torna o recuo **seguro**:
///
/// 1. a falha é de RECURSO — pool estourada (`ResourcesExhausted`) ou spill que não pôde criar arquivo. Um
///    erro qualquer NÃO entra: recuar nele esconderia um defeito do streaming atrás de um resultado certo.
/// 2. o eager PODE ter sucesso — com `varlena_bytes >= i32::MAX` ele bate no teto de offsets do Arrow por
///    construção, e tentar seria trocar um erro por outro, mais lento.
///
/// `find_root()` e não `match` na variante nua: o DataFusion embrulha em `Context(_, Box(_))` em caminhos
/// vizinhos, e casar a variante crua deixa o recuo de fora justamente quando ele é necessário.
fn stream_failure_is_fail_open(e: &DataFusionError, varlena_bytes: u64) -> bool {
    if varlena_bytes >= i32::MAX as u64 {
        return false;
    }
    match e.find_root() {
        DataFusionError::ResourcesExhausted(_) => true,
        DataFusionError::Execution(msg) => msg.contains(SPILL_CREATE_MARKER),
        _ => false,
    }
}

/// Build the DataFusion runtime (bounded `work_mem` MemoryPool + `target_partitions=1` — M100 D3 safety), read the
/// Arrow batch, let `build` finish the plan (filter/aggregate), and `collect` under `HeldInterrupts`. Shared by the
/// scalar (`run_aggs_on_batch`) and grouped (`run_columnar_grouped_aggs`) paths so the tokio/pool/interrupt discipline
/// lives in ONE place (DRY).
unsafe fn run_df_collect<F>(batch: RecordBatch, build: F) -> Result<Vec<RecordBatch>, String>
where
    F: FnOnce(
        datafusion::dataframe::DataFrame,
    ) -> Result<datafusion::dataframe::DataFrame, DataFusionError>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| format!("df_executor: tokio runtime: {e}"))?;
    // The pool must fit the decoded input batch: the aggregate path decodes a FEW columns (small batch, work_mem is
    // plenty), but the M158 top-k path decodes the FULL projection for all rows into one Arrow batch — which can be
    // hundreds of MB for a wide `SELECT *`. Size the pool to the batch's actual memory + headroom so a legitimate large
    // input is not rejected as "Resources exhausted"; the batch already lives in RAM regardless. (Honest caveat: the
    // top-k path is therefore O(N) memory in the decoded batch — unlike the native top-N heapsort's O(k). M158
    // mitigated this by defaulting the GUC OFF; M167 flipped the default ON, so the mitigation is now the plan-time
    // decode-size guard in `try_swap_topk` (M167 ADR-4): a candidate whose estimated decode dwarfs `work_mem`
    // declines to the native plan instead of being admitted here.)
    let work_mem_bytes = (unsafe { pg_sys::work_mem.max(64) } as usize) * 1024;
    let batch_bytes = batch.get_array_memory_size();
    // M167 § 6 left the peak of the decoded batch UNMEASURED: `VmRSS` is dominated by `shared_buffers` mapped into
    // every backend, so per-PID sampling never isolated it. The number was in-process the whole time — this is the
    // one line that exposes it. It is the O(N) quantity the M167 DoD bullet 2b is about, so it is also the baseline
    // any claim of "the path is now O(k)" has to be measured against, by the same instrument.
    if super::columnar_agg::admit_trace_enabled() {
        pgrx::warning!(
            "theodb_decode_batch: rows={} bytes={} work_mem_bytes={}",
            batch.num_rows(),
            batch_bytes,
            work_mem_bytes
        );
    }
    let pool_bytes = work_mem_bytes.max(batch_bytes.saturating_mul(2)) + 64 * 1024 * 1024;
    let held = HeldInterrupts::hold();
    let out: Result<Vec<RecordBatch>, DataFusionError> = rt.block_on(async move {
        use datafusion::execution::memory_pool::GreedyMemoryPool;
        use datafusion::execution::runtime_env::RuntimeEnvBuilder;
        use datafusion::prelude::SessionConfig;
        let mut rb = RuntimeEnvBuilder::new()
            .with_memory_pool(std::sync::Arc::new(GreedyMemoryPool::new(pool_bytes)));
        // Spill sob `pgsql_tmp` nos DOIS construtores — ver `spill_disk_manager`. A assimetria entre eles é
        // exatamente como o M168 nasceu com a guarda de linhas pendentes num lado só.
        if let Some(dm) = spill_disk_manager() {
            rb = rb.with_disk_manager_builder(dm);
        }
        let runtime = rb.build_arc()?;
        let config = SessionConfig::new().with_target_partitions(1);
        let ctx = SessionContext::new_with_config_rt(config, runtime);
        let df = ctx.read_batch(batch)?;
        build(df)?.collect().await
    });
    drop(held);
    drop(rt);
    out.map_err(|e| format!("df_executor: DataFusion: {e}"))
}

/// M157/M161 — exec-side decoding of one expression group key (from the 3rd `custom_private` channel). `func`:
/// 0=DateTrunc (out timestamp), 1=ExtractField (out numeric — minute/hour), 2=IntAddConst (`base ± delta`, out int).
/// `base_name` is the base column.
pub(super) struct GroupExprExec {
    pub base_name: String,
    pub func: i32,
    pub unit: String,
    pub delta: i64,
    pub out_typoid: u32,
}

/// Grouped columnar aggregate (M100 GROUP BY pushdown). Decode the group + sum columns, run
/// `.aggregate(group_exprs, agg_exprs)`, and materialize the multi-row result in the PG output-target order given by
/// `layout` (ADR-2): each output slot is either group key `idx` (batch col `idx`) or agg `idx` (batch col
/// `ngroup+idx`). `group_cols` is `(name, typoid)` per key (typoid drives the reverse Arrow→Datum conversion).
/// `predicates` + `skip` apply the zone-map skip-pruning + DataFusion Filter (M114 GROUP BY+WHERE); empty = no WHERE.
/// Returns one inner Vec per group, in `layout` (target) order. The CALLER runs this in a durable memory context
/// (ADR-3) for text group-key datums.
pub(super) unsafe fn run_columnar_grouped_aggs(
    rel: pg_sys::Relation,
    group_cols: &[(String, u32)],
    group_key_exprs_spec: &[GroupExprExec], // M157/M161 — expression group keys (date_trunc / extract / int±k / const)
    aggs: &[AggSpec],
    layout: &[(u8, usize)],
    const_outs: &[(i64, u32)], // M165 — projected integer constant output cells (layout kind=3)
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
) -> Result<Vec<Vec<(pg_sys::Datum, bool)>>, String> {
    use datafusion::functions::datetime::expr_fn::{date_part, date_trunc};
    // Project bare group columns ∪ group-expr base columns ∪ agg columns (count(*) needs no column; decode_to_batch
    // also projects predicate cols and guarantees ≥1).
    let mut proj_cols: Vec<String> = group_cols.iter().map(|(n, _)| n.clone()).collect();
    for g in group_key_exprs_spec {
        if !g.base_name.is_empty() && !proj_cols.iter().any(|p| p == &g.base_name) {
            proj_cols.push(g.base_name.clone());
        }
    }
    for a in aggs {
        if let Some(n) = a.col_name() {
            if !proj_cols.iter().any(|p| p == n) {
                proj_cols.push(n.to_string());
            }
        }
    }
    let filter = unsafe { build_filter_expr(rel, predicates, text_predicates, in_predicates) };

    // Grouping keys: bare columns FIRST, then the expression exprs (M157/M161). The output batch columns follow this
    // order: [bare_0..bare_{ncols-1}, expr_0..expr_{nexpr-1}, agg columns…].
    //   func 0 DateTrunc    — date_trunc(unit, ts) (tz-independent timestamp; timestamptz declined at admit, ADR-2).
    //   func 1 ExtractField — cast(date_part(unit, ts) → Int64): minute/hour are epoch-invariant + integer-valued;
    //                          grouped as Int64, materialized as PG numeric (AnyNumeric) so it equals extract()'s type.
    //   func 2 IntAddConst  — cast(col → Int64) + delta: the i64 compute is exact (int2/int4 column ± int const; an
    //                          int8 result is declined at admit), so grouping is exact; the result-type range-check
    //                          happens at materialize (out_typoid = opresulttype, int2/int4 → reproduces PG 22003).
    let ncols = group_cols.len();
    let ngroup = ncols + group_key_exprs_spec.len();
    let mut group_exprs: Vec<Expr> = group_cols.iter().map(|(n, _)| col(n.as_str())).collect();
    for g in group_key_exprs_spec {
        let e = match g.func {
            0 => {
                date_trunc(lit(ScalarValue::Utf8(Some(g.unit.clone()))), col(g.base_name.as_str()))
            }
            1 => cast(
                date_part(lit(ScalarValue::Utf8(Some(g.unit.clone()))), col(g.base_name.as_str())),
                DataType::Int64,
            ),
            2 => cast(col(g.base_name.as_str()), DataType::Int64) + lit(g.delta),
            other => return Err(format!("df_executor: bad group-expr func {other}")),
        };
        group_exprs.push(e);
    }
    let mut agg_exprs = Vec::with_capacity(aggs.len());
    // Per-agg batch-column offset (relative to the first agg column) — a multi-column spec (avg-int) shifts the rest.
    let mut agg_off: Vec<usize> = Vec::with_capacity(aggs.len());
    for a in aggs {
        agg_off.push(agg_exprs.len());
        push_agg_exprs(a, &mut agg_exprs);
    }
    // Filter BEFORE aggregating (SQL WHERE … GROUP BY — M114 blueprint E4); the zone-map skip above is only an
    // admission filter, the DataFusion Filter is the final row authority.
    //
    // M169: streamed per chunk-group when possible (see `run_df_over_columnar`). This is where TWO of the three
    // measured `byte array offset overflow` instances live — q33/q34 group by URL, so the eager path built the
    // whole URL column as one i32-offset Arrow array. What streaming does NOT reduce is the hash table's own
    // state, which is O(distinct groups): q32 (`GROUP BY WatchID, ClientIP`, near-unique key) times out for that
    // reason and this change does not move it. Claiming otherwise would be selling a fix that does not happen.
    let batches = unsafe {
        run_df_over_columnar(
            rel,
            &proj_cols,
            predicates,
            text_predicates,
            in_predicates,
            skip,
            move |df| {
                let df = match filter {
                    Some(f) => df.filter(f)?,
                    None => df,
                };
                df.aggregate(group_exprs, agg_exprs)
            },
        )?
    };

    // DataFusion output columns: [group_0..group_{ngroup-1}, agg columns…]. Agg `idx` starts at batch column
    // `ngroup + agg_off[idx]` (a multi-column spec shifts the rest). Emit rows in `layout` (target) order.
    let mut rows: Vec<Vec<(pg_sys::Datum, bool)>> = Vec::new();
    for b in &batches {
        for r in 0..b.num_rows() {
            let mut row_out = Vec::with_capacity(layout.len());
            for &(kind, idx) in layout {
                let cell = match kind {
                    0 => {
                        // bare group column — batch col `idx`.
                        let typoid =
                            group_cols.get(idx).ok_or("df_executor: layout group idx oob")?.1;
                        arrow_value_to_datum(b.column(idx), r, typoid)?
                    }
                    2 => {
                        // M157/M161 — expression group-expr — batch col `ncols + idx`, materialized per variant.
                        let g = group_key_exprs_spec
                            .get(idx)
                            .ok_or("df_executor: layout group-expr idx oob")?;
                        group_expr_cell(b.column(ncols + idx), r, g)?
                    }
                    3 => {
                        // M165 — const-out cell: a projected integer literal, the SAME value in every row (no batch
                        // column — the const is neither grouped nor aggregated). Rebuild the by-value int Datum from
                        // its stored (i64, typoid). NOT a grouping key → excluded from the `gk` sort below.
                        let &(val, typoid) =
                            const_outs.get(idx).ok_or("df_executor: layout const idx oob")?;
                        const_out_datum(val, typoid)?
                    }
                    _ => {
                        let a = aggs.get(idx).ok_or("df_executor: layout agg idx oob")?;
                        agg_datum(b, ngroup + agg_off[idx], r, a)?
                    }
                };
                row_out.push(cell);
            }
            rows.push(row_out);
        }
    }
    // Sort the (few) group rows ASCending nulls-last by the group-key output slots — reproduces the GroupAgg order for
    // the M115 Agg-swap of an AGG_SORTED node. Numeric/temporal keys only (text AGG_SORTED is declined at swap time);
    // a Rust sort over the small grouped result avoids the DataFusion sort's memory-pool reservation.
    let gk: Vec<(usize, u32)> = layout
        .iter()
        .enumerate()
        .filter_map(|(slot, &(kind, idx))| match kind {
            // fail-safe (council-rust-pgrx LOW): `.get()` not `[idx]` — a corrupt layout drops the sort key rather
            // than panicking across the C boundary (matches the materialization loop's `.get(idx).ok_or()?` pattern).
            0 => group_cols.get(idx).map(|g| (slot, g.1)),
            2 => group_key_exprs_spec.get(idx).map(|g| (slot, g.out_typoid)), // M157/M161 — expr key out_typoid
            _ => None,
        })
        .collect();
    if !gk.is_empty() && !gk.iter().any(|&(_, t)| matches!(t, 25 | 1042 | 1043)) {
        rows.sort_by(|a, b| {
            for &(slot, typ) in &gk {
                let ord = cmp_group_datum(a[slot], b[slot], typ);
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            std::cmp::Ordering::Equal
        });
    }
    Ok(rows)
}

// ---------------------------------------------------------------------------------------------------------------
// M168 — streaming source: one Arrow batch per columnar chunk-group, so the top-k's peak is a chunk-group + k
// instead of the whole relation (measured at 809,738,352 bytes for ClickBench q23 before this existed).
// ---------------------------------------------------------------------------------------------------------------

/// `MemoryPool` que delega tudo e registra a MARCA D'ÁGUA do reservado.
///
/// Existe porque `GreedyMemoryPool::reserved()` lido **depois** do `block_on` é zero por construção: tudo que
/// reserva — plano, streams, o `TopK` — é construído e destruído dentro do bloco async, e `MemoryReservation`
/// libera no `Drop`. Reportar esse zero como prova de que o TopK não reteve nada seria vender um não-resultado
/// como medição: o instrumento não conseguiria retornar outra coisa (achado de review).
///
/// A pergunta real é a retenção **durante** a execução, e é isso que `peak` responde: cada `grow`/`try_grow`
/// bem-sucedido atualiza o máximo.
#[derive(Debug)]
struct PeakTrackingPool {
    inner: datafusion::execution::memory_pool::GreedyMemoryPool,
    peak: std::sync::atomic::AtomicUsize,
}

impl PeakTrackingPool {
    fn new(limit: usize) -> Self {
        Self {
            inner: datafusion::execution::memory_pool::GreedyMemoryPool::new(limit),
            peak: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    fn note(&self) {
        use datafusion::execution::memory_pool::MemoryPool as _;
        let now = self.inner.reserved();
        self.peak.fetch_max(now, std::sync::atomic::Ordering::Relaxed);
    }
    fn peak(&self) -> usize {
        self.peak.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl std::fmt::Display for PeakTrackingPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use datafusion::execution::memory_pool::MemoryPool as _;
        write!(f, "PeakTrackingPool(peak={}, reserved={})", self.peak(), self.inner.reserved())
    }
}

impl datafusion::execution::memory_pool::MemoryPool for PeakTrackingPool {
    fn name(&self) -> &str {
        "PeakTrackingPool"
    }
    fn register(&self, c: &datafusion::execution::memory_pool::MemoryConsumer) {
        self.inner.register(c);
    }
    fn unregister(&self, c: &datafusion::execution::memory_pool::MemoryConsumer) {
        self.inner.unregister(c);
    }
    fn grow(&self, r: &datafusion::execution::memory_pool::MemoryReservation, additional: usize) {
        self.inner.grow(r, additional);
        self.note();
    }
    fn shrink(&self, r: &datafusion::execution::memory_pool::MemoryReservation, shrink: usize) {
        self.inner.shrink(r, shrink);
    }
    fn try_grow(
        &self,
        r: &datafusion::execution::memory_pool::MemoryReservation,
        additional: usize,
    ) -> datafusion::error::Result<()> {
        let out = self.inner.try_grow(r, additional);
        if out.is_ok() {
            self.note();
        }
        out
    }
    fn reserved(&self) -> usize {
        datafusion::execution::memory_pool::MemoryPool::reserved(&self.inner)
    }
    /// Encaminhado porque o default do trait devolve `Unknown` enquanto o `inner` é `Finite`. Hoje o único
    /// consumidor em DF 54 é a listagem de config, mas o método existe para quem precisa do teto — um upgrade
    /// que passe a usá-lo em decisão de spill faria a pool mentir em silêncio. É o que o `TrackConsumersPool`
    /// do upstream faz (achado de review).
    fn memory_limit(&self) -> datafusion::execution::memory_pool::MemoryLimit {
        datafusion::execution::memory_pool::MemoryPool::memory_limit(&self.inner)
    }
}

/// A `RecordBatchStream` that decodes one chunk-group per poll.
///
/// SAFETY (M168 ADR-2): holds a `pg_sys::Relation`, which is only valid on its backend thread. DataFusion demands
/// `Send` here. That is sound ONLY because `run_df_collect` drives this on a `new_current_thread` runtime via
/// `block_on` with `target_partitions(1)` — the stream is polled on the backend thread and nowhere else. The
/// `ThreadAffinity` inside `ColumnarChunkStream` asserts exactly that on every `next()`, so a future switch to a
/// multi-threaded runtime panics instead of corrupting memory.
struct ChunkGroupBatchStream {
    schema: SchemaRef,
    inner: super::columnar::ColumnarChunkStream,
    predicates: Vec<super::zonemap::ZonePredicate>,
    skip: bool,
    pending: Option<RecordBatch>, // the probe batch decoded to learn the schema; served first
    done: bool,
}

unsafe impl Send for ChunkGroupBatchStream {}

impl futures::Stream for ChunkGroupBatchStream {
    type Item = Result<RecordBatch, DataFusionError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;
        if let Some(b) = self.pending.take() {
            return Poll::Ready(Some(Ok(b)));
        }
        if self.done {
            return Poll::Ready(None);
        }
        // O hold de interrupções agora envolve TODA a execução do plano, então sem isto um scan de 100M linhas
        // ignora Ctrl-C, `statement_timeout` e `pg_terminate_backend` do início ao fim. A fronteira de
        // chunk-group é exatamente o "per-batch interrupt safe-point" que o cabeçalho deste módulo promete —
        // antes do M168 ela não existia porque não havia batches. Achado de review.
        //
        // A afirmação de afinidade vem PRIMEIRO: o safe-point lê globais do PostgreSQL sem sincronização, e numa
        // thread estranha essa leitura aconteceria antes de a asserção do ADR-2 poder disparar. A filosofia deste
        // módulo é assever a invariante, não confiar nela — então ela é asseverada antes de tocar o global.
        //
        // O cancelamento vira `Err`, não `ereport`: ver `interrupt_is_pending`. Devolver o erro aqui faz o
        // stream, o plano e o runtime tokio desenrolarem pelos `Drop`s do Rust; o PostgreSQL só levanta o
        // cancelamento depois, em `run_df_collect_streaming`, com tudo isso já liberado.
        unsafe {
            self.inner.assert_owning_thread("ChunkGroupBatchStream::poll_next");
            if interrupt_is_pending() {
                self.done = true;
                return Poll::Ready(Some(Err(DataFusionError::Execution(
                    "theodb: query canceled".into(),
                ))));
            }
        }
        let preds = std::mem::take(&mut self.predicates);
        let skip = self.skip;
        let stepped = unsafe { self.inner.next(&preds, skip) };
        self.predicates = preds;
        match stepped {
            Ok(None) => {
                self.done = true;
                Poll::Ready(None)
            }
            Ok(Some(cols)) => match build_arrow_from_decoded(&cols).and_then(|(sc, arrays)| {
                RecordBatch::try_new(Arc::new(sc), arrays)
                    .map_err(|e| format!("df_executor: arrow batch: {e}"))
            }) {
                Ok(b) => {
                    // Same instrument as the eager path's `theodb_decode_batch`, so before/after are comparable
                    // by construction. It also PROVES which path ran: `_stream` lines can only come from here,
                    // and their count is the number of chunk-groups actually streamed. Without this, an oracle
                    // that passes says nothing about whether the streaming path was exercised at all.
                    if super::columnar_agg::admit_trace_enabled() {
                        pgrx::warning!(
                            "theodb_decode_batch_stream: rows={} bytes={}",
                            b.num_rows(),
                            b.get_array_memory_size()
                        );
                    }
                    Poll::Ready(Some(Ok(b)))
                }
                Err(e) => {
                    self.done = true;
                    Poll::Ready(Some(Err(DataFusionError::Execution(e))))
                }
            },
            Err(e) => {
                self.done = true;
                Poll::Ready(Some(Err(DataFusionError::Execution(e))))
            }
        }
    }
}

impl datafusion::physical_plan::RecordBatchStream for ChunkGroupBatchStream {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}

/// The `PartitionStream` DataFusion registers as a table. Holds the planned scan until `execute` turns it into a
/// live stream; only ever executed once (a top-k reads its input exactly once).
///
/// SAFETY: see `ChunkGroupBatchStream` — same ADR-2 justification, same runtime constraint, same assertion.
struct ColumnarPartition {
    schema: SchemaRef,
    state: std::sync::Mutex<Option<ChunkGroupBatchStream>>,
}

impl std::fmt::Debug for ColumnarPartition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ColumnarPartition({} fields)", self.schema.fields().len())
    }
}

// Sem `unsafe impl` aqui de propósito: `SchemaRef` é Send+Sync e `Mutex<T>` é Send+Sync sempre que `T: Send`, o
// que o impl de `ChunkGroupBatchStream` já fornece. Um `unsafe impl` nesta struct não compraria nada hoje e
// abençoaria em silêncio qualquer campo não-Send acrescentado depois — trocando um erro de compilação por UB
// (achado de review). A única bênção manual do módulo fica onde o ponteiro cru de fato está.

impl datafusion::physical_plan::streaming::PartitionStream for ColumnarPartition {
    fn schema(&self) -> &SchemaRef {
        &self.schema
    }

    fn execute(
        &self,
        _ctx: Arc<datafusion::execution::TaskContext>,
    ) -> datafusion::physical_plan::SendableRecordBatchStream {
        match self.state.lock().ok().and_then(|mut g| g.take()) {
            Some(st) => Box::pin(st),
            // Executing twice would silently return nothing, which is the kind of empty-but-green result this
            // milestone's oracles exist to catch. Fail loudly instead.
            None => Box::pin(datafusion::physical_plan::stream::RecordBatchStreamAdapter::new(
                self.schema.clone(),
                futures::stream::once(async {
                    Err(DataFusionError::Execution(
                        "df_executor: columnar partition executed twice (top-k reads its input once)".into(),
                    ))
                }),
            )),
        }
    }
}

/// Build the streaming source for a top-k: plan the scan, decode ONE chunk-group eagerly to learn the exact Arrow
/// schema (DataFusion asks for the schema before executing), and hand the rest back as a lazy stream.
///
/// The probe costs one chunk-group, which is the same order as the streaming peak — it does not reintroduce O(N).
unsafe fn open_streaming_source(
    rel: pg_sys::Relation,
    proj_names: &[String],
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
    // M169 ADR-5 — the second element is `ScanPlan::varlena_raw_len_max_per_column`: o MAIOR total por coluna entre as varlena projetadas — por coluna porque o teto do Arrow é de um array, e somar entre colunas recusaria o recuo onde o eager serviria (achado do review). O total das
    // variable-length columns. It is returned from HERE, not recomputed by the caller, because the only way to
    // get it is from the `ScanPlan`, and this function moves that plan into the stream. Re-planning to ask the
    // question would repeat the directory read, which is the scan's O(N) term.
) -> Result<Option<(Arc<ColumnarPartition>, u64)>, String> {
    let mut proj: Vec<usize> = Vec::new();
    let mut want = |idx: usize, proj: &mut Vec<usize>| {
        if !proj.contains(&idx) {
            proj.push(idx);
        }
    };
    for name in proj_names {
        let idx = unsafe { super::columnar::column_index(rel, name) }
            .ok_or_else(|| format!("df_executor: column '{name}' not found"))?;
        want(idx, &mut proj);
    }
    for pr in predicates {
        want(pr.col, &mut proj);
    }
    for t in text_predicates {
        want(t.col, &mut proj);
    }
    for ip in in_predicates {
        want(ip.col, &mut proj);
    }
    if proj.is_empty() {
        proj.push(0);
    }
    // FAIL-CLOSED: a scan planned from `read_visible_stripes` alone cannot see rows this transaction wrote but has
    // not flushed into a stripe. `decode_columns_v2` guards on exactly this and falls back to the cell path; the
    // streaming planner was extracted from BELOW that guard and inherited none of it, so a mixed state
    // (`BEGIN; INSERT; SELECT … ORDER BY … LIMIT` over a table that already has stripes) silently lost the new
    // rows. Declining here hands the query to the eager path, which handles it correctly. Found in review — the
    // ClickBench oracles cannot reach this shape, because they bulk-load and then only read.
    if unsafe { super::columnar::has_unflushed_pending(rel) } {
        return Ok(None);
    }
    let plan = unsafe { super::columnar::plan_columnar_scan(rel, Some(&proj))? };
    // Read the byte total BEFORE the plan is moved into the stream — afterwards it is unreachable.
    let varlena_bytes = plan.varlena_raw_len_max_per_column();
    let mut inner = super::columnar::ColumnarChunkStream::new(rel, plan);
    // Probe: the schema has to be exact before DataFusion executes, and only decoded data reveals it.
    let first = unsafe { inner.next(predicates, skip)? };
    let Some(cols) = first else {
        return Ok(None); // nothing visible — caller falls back to the batch path, which handles empty correctly
    };
    let (sc, arrays) = build_arrow_from_decoded(&cols)?;
    let schema: SchemaRef = Arc::new(sc);
    let probe = RecordBatch::try_new(schema.clone(), arrays)
        .map_err(|e| format!("df_executor: arrow batch: {e}"))?;
    // The probe IS a chunk-group, so it must be traced like every other one. Without this the instrumented count
    // was 99 of 100 (CHUNK_GROUP_ROWS = 10_000, so 1M rows = 100 groups) and the reported maximum was a maximum
    // over 99 — the probe could have been the largest and nobody would know. Review finding, M168.
    if super::columnar_agg::admit_trace_enabled() {
        pgrx::warning!(
            "theodb_decode_batch_stream: rows={} bytes={} probe=1",
            probe.num_rows(),
            probe.get_array_memory_size()
        );
    }
    Ok(Some((
        Arc::new(ColumnarPartition {
            schema: schema.clone(),
            state: std::sync::Mutex::new(Some(ChunkGroupBatchStream {
                schema,
                inner,
                predicates: predicates.to_vec(),
                skip,
                pending: Some(probe),
                done: false,
            })),
        }),
        varlena_bytes,
    )))
}

/// M158 — late-materialization top-k over a columnar table: `SELECT <proj> [WHERE <pushable>] ORDER BY <key> LIMIT k`.
/// Decodes {proj ∪ key ∪ filter} columns ONCE into an Arrow batch, then runs `filter → sort([key]) → limit(k)` in
/// DataFusion (vectorized TopK, O(N log k)) and materializes ONLY the k surviving rows back to PG Datums via
/// `arrow_value_to_datum` — so the per-row `form_row`/`palloc` cost (M148: ~80% of the eager scan) is paid for k rows,
/// not N. `proj_cols` = (name, typoid) in output-target order; `sort_key` must resolve in the decoded batch. Returns
/// one inner Vec per surviving row, in sort order (the CALLER runs this in a durable memory context for varlena datums).
pub(super) unsafe fn run_columnar_topk(
    rel: pg_sys::Relation,
    proj_cols: &[(String, u32)],
    sort_keys: &[(String, bool, bool)], // M167 — (name, ascending, nulls_first) in ORDER BY position
    k: usize,
    predicates: &[super::zonemap::ZonePredicate],
    text_predicates: &[super::zonemap::TextPredicate],
    in_predicates: &[super::zonemap::InListPredicate],
    skip: bool,
) -> Result<Vec<Vec<(pg_sys::Datum, bool)>>, String> {
    // Project all output columns ∪ the sort key (decode_to_batch also folds in the predicate columns + guarantees ≥1).
    let mut proj_names: Vec<String> = proj_cols.iter().map(|(n, _)| n.clone()).collect();
    for (key, _, _) in sort_keys {
        if !proj_names.iter().any(|n| n == key) {
            proj_names.push(key.clone());
        }
    }
    let filter = unsafe { build_filter_expr(rel, predicates, text_predicates, in_predicates) };
    let order_by: Vec<_> =
        sort_keys.iter().map(|(name, asc, nf)| col(name.as_str()).sort(*asc, *nf)).collect();

    // Mesmo reset de entrada do caminho agregado (`:686`), e aqui ele importa MAIS: o acessor
    // `theodb_columnar_stream_chunk_groups` se documenta como "quantas vezes o top-k streamado avançou o
    // cursor", ou seja este é o consumidor PRIMÁRIO do contador — e era justamente o que ficara sem reset.
    // Um `ORDER BY … LIMIT k` grande que recua para o eager reportava as chamadas da tentativa que FALHOU.
    super::columnar::reset_stream_cg_count();
    // M168 — stream one chunk-group at a time so the peak is a chunk-group + k, not the whole relation. The
    // eager path below stays as the fallback for the one case the stream cannot serve (nothing visible), and
    // because a source that yields zero batches is exactly the empty-but-green result the oracles guard against.
    if super::columnar_agg::ENABLE_COLUMNAR_TOPK_STREAM.get()
        && let Some((part, _varlena_bytes)) = unsafe {
            open_streaming_source(
                rel,
                &proj_names,
                predicates,
                text_predicates,
                in_predicates,
                skip,
            )?
        }
    {
        // O top-k IGNORA o total de bytes, e a razão é de ESCOPO, não de segurança: o eager daqui materializa a
        // relação inteira do mesmo jeito, então tem o MESMO teto de offsets i32 — o fail-open abaixo pode cair
        // nele e estourar por largura de texto. O M169 foi escopado ao caminho agregado (é lá que estão as três
        // instâncias medidas), e estender o pré-check ao top-k é uma mudança de comportamento que este milestone
        // não mediu. Fica dito aqui em vez de descoberto depois.
        let order_stream = order_by.clone();
        let filter_stream = filter.clone();
        // FAIL-OPEN, não `?`. A pool do streaming é constante em `k` enquanto a retenção do TopK cresce com ele
        // (`topk/mod.rs` faz `reservation.try_resize(self.size())`), e a admissão não limita `k`. Sem isto, um
        // `ORDER BY … LIMIT 500000` largo que o caminho eager servia passa a ERRAR por default, e a única saída
        // seria uma GUC que o usuário não sabe que existe (achado de review). Cair no eager custa uma re-execução
        // no caminho de falha e nada no caminho feliz — mesma forma do decline de linhas pendentes acima.
        match unsafe {
            run_df_collect_streaming(part, move |df| {
                let df = match filter_stream {
                    Some(f) => df.filter(f)?,
                    None => df,
                };
                df.sort(order_stream)?.limit(0, Some(k))
            })
        } {
            Ok(batches) => return unsafe { rows_from_batches(&batches, proj_cols) },
            // TIPADO, não catch-all. O fail-open existe por UM motivo: a retenção do `TopK` cresce com `k`
            // enquanto a pool do streaming é constante, então um `k` grande pode estourar o que o caminho eager
            // servia. Esse caso é `ResourcesExhausted` e nada mais.
            //
            // Engolir qualquer `Err` era mais largo que o argumento que o justifica, e um review mostrou duas
            // consequências concretas: (a) neutraliza a jusante um guard escrito para falhar ALTO — o
            // `columnar partition executed twice`, que vira uma linha de log e uma re-execução silenciosa; e
            // (b) com o safe-point corrigido, o cancelamento chega aqui como `Execution`, e engoli-lo faria a
            // consulta ignorar `statement_timeout`/Ctrl-C **e ainda refazer o scan inteiro pelo caminho eager**.
            // `find_root()`, não `match` na variante exata. O DataFusion embrulha `ResourcesExhausted` em
            // `Context(_, Box(ResourcesExhausted))` num caminho irmão — o `err_with_oom_context` do
            // `ExternalSorter` (`datafusion-physical-plan-54.0.0/src/sorts/sort.rs:756-762`). Hoje o nosso plano
            // não passa por lá, mas um upgrade menor de DF que roteie um sort com `fetch` por esse caminho faria o
            // `match` por variante ERRAR em silêncio, e o fail-open viraria erro duro — exatamente a regressão que
            // ele existe para evitar (achado de review). `find_root` desembrulha `Context`/`External`/`Shared`
            // (`datafusion-common-54.0.0/src/error.rs:436`).
            // NÃO usa `stream_failure_is_fail_open` (`:775`) DE PROPÓSITO, e a razão é medida, não estilística: o
            // predicado de lá também cobre a falha de CRIAÇÃO DE ARQUIVO DE SPILL, e este caminho não cria um.
            // Com `fetch = Some(k)` o `SortExec` roteia para o operador `TopK`
            // (`datafusion-physical-plan-54.0.0/src/sorts/sort.rs:1216-1249`), que mantém o heap em memória; o
            // `create_in_progress_file("Sorting")` (`sort.rs:419`) vive no ramo `ExternalSorter`, alcançado só
            // quando `fetch == None`. Nosso plano é `df.sort(...).limit(0, Some(k))`, logo cai no primeiro.
            // A varredura foi FEITA: `ResourcesExhausted` tem exatamente 2 ocorrências no crate — esta e a `:706`.
            // O que torna isto frágil é a REACHABILITY, não o predicado: se um upgrade do DataFusion parar de
            // empurrar o `limit` como `fetch` no `SortExec`, este ramo passa pelo `ExternalSorter` e AÍ a classe
            // de spill se aplica aqui também. Quem mexer no pushdown do limit deve reavaliar esta linha.
            Err(e) if matches!(e.find_root(), DataFusionError::ResourcesExhausted(_)) => {
                // INCONDICIONAL. Esconder isto atrás do flag de trace deixaria o usuário sem sinal de que a
                // consulta acabou de trocar de perfil de memória e de latência. É evento raro (uma vez por
                // consulta, só quando a pool estoura), então o custo é zero no caminho feliz. Vai para o log do
                // servidor, não para o cliente.
                pgrx::log!("theodb_topk_stream_fallback: {e}");
                // cai para o caminho eager abaixo, que é o comportamento pré-M168
            }
            // Erro de integridade de dados, cancelamento, guard de re-execução: não são o caso que o fail-open
            // cobre. Sobem.
            //
            // JANELA RESIDUAL, declarada em vez de "corrigida". Uma versão anterior punha um
            // `check_for_interrupts!()` aqui, alegando resgatar o SQLSTATE 57014 quando o `check_for_interrupts!()`
            // pós-`drop(held)` virasse no-op (holdoff externo ou `CritSectionCount != 0` —
            // `tcop/postgres.c:3275` retorna cedo). **Aquela linha era inerte exatamente na janela que alegava
            // cobrir** (achado de review): entre uma chamada e outra nada toca `InterruptHoldoffCount` nem
            // `CritSectionCount`, então se a primeira retornou cedo a segunda retorna cedo pelo mesmo motivo. Era
            // uma correção que não corrigia, publicada no CHANGELOG como se corrigisse.
            //
            // O que fica, honestamente: sob um holdoff externo o cancelamento sai como erro interno em vez de
            // 57014. É uma janela estreita e defensável — sob holdoff externo nada é interrompível de qualquer
            // forma. Tipar o cancelamento (um marcador em `DataFusionError::External` + `downcast_ref`) resolveria
            // de verdade, e é o caminho se algum dia isso doer; hoje seria código para um caso que ninguém alcança
            // (parsimony ladder, degrau 1).
            Err(e) => return Err(format!("df_executor: datafusion: {e}")),
        }
    }

    let batch = unsafe {
        decode_to_batch(rel, &proj_names, predicates, text_predicates, in_predicates, skip)?
    };
    // filter (WHERE, the final authority — D3) → sort by the key (PG order for numeric/temporal/det-collation text) →
    // limit k (DataFusion's TopK: a bounded heap, never materializing all N as tuples).
    let batches = unsafe {
        run_df_collect(batch, move |df| {
            let df = match filter {
                Some(f) => df.filter(f)?,
                None => df,
            };
            df.sort(order_by)?.limit(0, Some(k))
        })?
    };

    unsafe { rows_from_batches(&batches, proj_cols) }
}

/// M168 — the streaming twin of `run_df_collect`: registers a lazy `PartitionStream` instead of one materialized
/// batch, so `SortExec: TopK(fetch=k)` pulls chunk-groups and keeps only k rows.
///
/// The memory pool is sized to `work_mem` alone here — there is no giant input batch to accommodate, which is the
/// whole point. If the pool were still sized to the full decode, the O(k) claim would be theatre.
unsafe fn run_df_collect_streaming<F>(
    part: Arc<ColumnarPartition>,
    build: F,
    // Devolve o `DataFusionError` **tipado**, não uma `String`. O chamador precisa distinguir "a pool estourou" de
    // "os dados estão corrompidos" / "a consulta foi cancelada" para decidir se o fail-open se aplica, e casar
    // substring de mensagem de erro para isso seria exatamente o que a Regra 8 proíbe (erros explícitos e tipados).
    // A conversão para `String` acontece no chamador, depois da classificação.
) -> Result<Vec<RecordBatch>, DataFusionError>
where
    F: FnOnce(
            datafusion::dataframe::DataFrame,
        ) -> Result<datafusion::dataframe::DataFrame, DataFusionError>
        + Send
        + 'static,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|e| DataFusionError::Execution(format!("df_executor: tokio runtime: {e}")))?;
    let work_mem_bytes = (unsafe { pg_sys::work_mem.max(64) } as usize) * 1024;
    // O caminho eager dimensionava `max(work_mem, 2*batch) + 64MB`. Aqui não há batch gigante — esse é o ponto —
    // mas o TopK ainda retém k linhas, e o guard do M167 limita bytes FÍSICOS (comprimidos), não a pegada Arrow.
    // Dimensionar só por `work_mem` fazia um top-k largo com k grande, que o caminho eager servia, falhar com
    // "Resources exhausted" (achado de review). O múltiplo cobre a retenção do TopK sem reintroduzir O(N).
    let pool_bytes = work_mem_bytes.saturating_mul(2) + 64 * 1024 * 1024;
    let schema =
        datafusion::physical_plan::streaming::PartitionStream::schema(part.as_ref()).clone();
    let held = HeldInterrupts::hold();
    // A pool é criada FORA do bloco async e clonada para dentro, para que a MARCA D'ÁGUA possa ser lida depois.
    // Sem isso, o único número instrumentado é o maior batch individual — e a retenção do TopK (que segura cada
    // batch com linha sobrevivente) fica não medida. Um reviewer apontou que essa omissão é o que sustentava, sem
    // prova, a alegação de que o footprint total não chega perto do orçamento do guard; e o comentário logo acima
    // sobre `Resources exhausted` é evidência in-repo de que ela chega a ~128 MB. Agora é medida, não argumentada.
    use datafusion::execution::memory_pool::MemoryPool;
    let tracked = std::sync::Arc::new(PeakTrackingPool::new(pool_bytes));
    let pool_probe = std::sync::Arc::clone(&tracked);
    let pool: std::sync::Arc<dyn MemoryPool> = tracked;
    let out: Result<Vec<RecordBatch>, DataFusionError> = rt.block_on(async move {
        use datafusion::execution::runtime_env::RuntimeEnvBuilder;
        use datafusion::prelude::SessionConfig;
        let mut rb = RuntimeEnvBuilder::new().with_memory_pool(pool);
        if let Some(dm) = spill_disk_manager() {
            rb = rb.with_disk_manager_builder(dm);
        }
        let runtime = rb.build_arc()?;
        // target_partitions(1) is load-bearing, not tuning: it is what keeps the single `PartitionStream` on the
        // calling (backend) thread, which is what makes the `unsafe impl Send` on it sound (M168 ADR-2).
        let config = SessionConfig::new().with_target_partitions(1);
        let ctx = SessionContext::new_with_config_rt(config, runtime);
        let table = datafusion::catalog::streaming::StreamingTable::try_new(
            schema,
            vec![part as Arc<dyn datafusion::physical_plan::streaming::PartitionStream>],
        )?;
        ctx.register_table("cg", Arc::new(table))?;
        let df = ctx.table("cg").await?;
        build(df)?.collect().await
    });
    drop(held);
    // AQUI, e não no meio do `block_on`. Neste ponto o runtime tokio, o `SessionContext`, o plano físico, as k
    // linhas retidas do TopK e o `RecordBatch` em voo já foram liberados pelos `Drop`s do Rust — e o holdoff
    // já caiu. Só agora é seguro deixar o PostgreSQL fazer o `siglongjmp` dele: não há mais frame Rust vivo para
    // ele pular. Ver `interrupt_is_pending` para o porquê de o safe-point não poder fazer isto lá dentro.
    pgrx::check_for_interrupts!();
    if super::columnar_agg::admit_trace_enabled() {
        pgrx::warning!(
            // M169: renomeado de `theodb_topk_pool`. Esta função passou a servir TAMBÉM o agregado (escalar e
            // agrupado), então o rótulo antigo passaria a mentir — quem lesse `topk_pool` numa linha emitida por
            // um `GROUP BY` concluiria a coisa errada sobre qual caminho consumiu a pool. Os artefatos do M168
            // guardam a string antiga de propósito: eles registram o que foi emitido NAQUELE binário.
            "theodb_stream_pool: peak_reserved={} reserved_at_end={} pool_limit={}",
            pool_probe.peak(),
            pool_probe.reserved(),
            pool_bytes
        );
    }
    out
}

/// Emit the surviving rows: one Datum per output column (in target order), located in the result batch by NAME
/// (the schema carries the decoded projection; filter/sort/limit preserve it). Only ≤ k rows are materialized.
///
/// Shared by the eager and the M168 streaming path so both produce byte-identical output by construction rather
/// than by two copies staying in agreement.
///
/// SAFETY: constructs PG Datums from Arrow values; must run on the backend thread, inside a memory context that
/// outlives the caller's use of the returned Datums.
unsafe fn rows_from_batches(
    batches: &[RecordBatch],
    proj_cols: &[(String, u32)],
) -> Result<Vec<Vec<(pg_sys::Datum, bool)>>, String> {
    let mut rows: Vec<Vec<(pg_sys::Datum, bool)>> = Vec::new();
    for b in batches {
        let schema = b.schema();
        let idxs: Vec<usize> = proj_cols
            .iter()
            .map(|(n, _)| {
                schema.index_of(n).map_err(|e| format!("df_executor: topk output col '{n}': {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        for r in 0..b.num_rows() {
            let mut row_out = Vec::with_capacity(proj_cols.len());
            for (oc, &bi) in proj_cols.iter().zip(idxs.iter()) {
                row_out.push(arrow_value_to_datum(b.column(bi), r, oc.1)?);
            }
            rows.push(row_out);
        }
    }
    Ok(rows)
}

/// Compare two group-key `(Datum, is_null)` cells for the ASCending-nulls-last ordering of the swapped grouped result
/// (numeric/temporal/bool types — text is declined upstream). NULL sorts last (PG default for ASC).
fn cmp_group_datum(
    a: (pg_sys::Datum, bool),
    b: (pg_sys::Datum, bool),
    typoid: u32,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.1, b.1) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater, // nulls last
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    unsafe {
        match typoid {
            20 | 1114 | 1184 => i64::from_datum(a.0, false).cmp(&i64::from_datum(b.0, false)),
            23 | 1082 => i32::from_datum(a.0, false).cmp(&i32::from_datum(b.0, false)),
            21 => i16::from_datum(a.0, false).cmp(&i16::from_datum(b.0, false)),
            16 => bool::from_datum(a.0, false).cmp(&bool::from_datum(b.0, false)),
            700 => f32::from_datum(a.0, false)
                .partial_cmp(&f32::from_datum(b.0, false))
                .unwrap_or(Ordering::Equal),
            701 => f64::from_datum(a.0, false)
                .partial_cmp(&f64::from_datum(b.0, false))
                .unwrap_or(Ordering::Equal),
            _ => Ordering::Equal,
        }
    }
}

/// Convert one aggregate's cell(s) at `batch[col..][row]` to a PG `(Datum, is_null)`. Single-column for count/sum/avg
/// (`count(*)`→int8, `sum(int2/4)`→int8, `sum/avg(float8)`→float8, `sum(int8)`→numeric); TWO columns for `avg(int)`
/// (sum + count → `AnyNumeric(sum)/AnyNumeric(count)` = PG `numeric_div`, ADR-N1/N2). Shared by the scalar (row 0) and
/// grouped (per row) paths.
fn agg_datum(
    b: &RecordBatch,
    col: usize,
    row: usize,
    spec: &AggSpec,
) -> Result<(pg_sys::Datum, bool), String> {
    let arr = b.column(col);
    // NULL propagation: an empty/all-NULL group makes the (first) aggregate cell null → SQL NULL (ADR-N3).
    if arr.is_null(row) {
        return Ok((pg_sys::Datum::from(0usize), true));
    }
    let datum = match spec {
        // int8 output: count(*), count(DISTINCT col), sum(int2/int4), sum(int2_col ± const) (DataFusion → Int64 = PG bigint).
        AggSpec::CountStar
        | AggSpec::SumInt(_)
        | AggSpec::SumIntAddConst { .. }
        | AggSpec::CountDistinct(_) => {
            let v = arr
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("df_executor: agg not Int64")?
                .value(row);
            v.into_datum().ok_or("df_executor: int8 datum")?
        }
        // float8 output: sum(float8), avg(float8) (DataFusion → Float64 = PG double precision).
        AggSpec::SumFloat8(_) | AggSpec::AvgFloat8(_) => {
            let v = arr
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or("df_executor: agg not Float64")?
                .value(row);
            v.into_datum().ok_or("df_executor: float8 datum")?
        }
        // numeric output: sum(int8) = exact Decimal128(38,0) i128 → AnyNumeric (scale 0). Int64 sum would wrap.
        // A Decimal128(38,0) overflow (>10^38, unreachable at realistic row counts) surfaces as a DataFusion error
        // at run_df_collect, never a panic across the C boundary.
        AggSpec::SumInt8Numeric(_) => {
            let s = arr
                .as_any()
                .downcast_ref::<Decimal128Array>()
                .ok_or("df_executor: sum-int8 not Decimal128")?
                .value(row);
            AnyNumeric::from(s).into_datum().ok_or("df_executor: numeric datum")?
        }
        // numeric output: avg(int) = AnyNumeric(sum) / AnyNumeric(count) = PG numeric_div (data-dependent scale).
        AggSpec::AvgIntNumeric(_) => {
            let s = arr
                .as_any()
                .downcast_ref::<Decimal128Array>()
                .ok_or("df_executor: avg-int sum not Decimal128")?
                .value(row);
            let cnt_arr = b.column(col + 1);
            // count(col) counts non-NULLs; a zero count (all-NULL group) → SQL NULL, never a divide-by-zero (ADR-N3).
            if cnt_arr.is_null(row) {
                return Ok((pg_sys::Datum::from(0usize), true));
            }
            let n = cnt_arr
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("df_executor: avg-int count not Int64")?
                .value(row);
            if n == 0 {
                return Ok((pg_sys::Datum::from(0usize), true));
            }
            (AnyNumeric::from(s) / AnyNumeric::from(n))
                .into_datum()
                .ok_or("df_executor: numeric datum")?
        }
        // min/max output = input column type. The DataFusion min()/max() result array has the source column's Arrow
        // type, so the build_arrow reverse emits the exact native datum against the carried output typoid (ADR-MM3).
        // `arr.is_null(row)` (guarded above) already handled the empty/all-NULL → NULL case.
        AggSpec::MinCol(_, typoid) | AggSpec::MaxCol(_, typoid) => {
            return arrow_value_to_datum(arr, row, *typoid);
        }
    };
    Ok((datum, false))
}

/// Reverse of `build_arrow` for a GROUP BY key cell: an Arrow array value at `row` → a PG `(Datum, is_null)` of the
/// group column's PG type. Covers every OID `build_arrow` produces. NOTE (temporal): Arrow Date32/Timestamp have a
/// 1970 epoch but `build_arrow` stuffed the raw PG-epoch bytes in; GROUP BY only uses raw-value equality/hash (never
/// Arrow's temporal semantics), and we pull the same raw int back out here → the PG value round-trips exactly.
/// The CALLER must run in a durable memory context (ADR-3): a text/varlena datum is palloc'd here.
fn arrow_value_to_datum(
    arr: &dyn Array,
    row: usize,
    typoid: u32,
) -> Result<(pg_sys::Datum, bool), String> {
    if arr.is_null(row) {
        return Ok((pg_sys::Datum::from(0usize), true));
    }
    let d = match typoid {
        21 => arr
            .as_any()
            .downcast_ref::<Int16Array>()
            .ok_or("df_executor: gk not i16")?
            .value(row)
            .into_datum(),
        23 => arr
            .as_any()
            .downcast_ref::<Int32Array>()
            .ok_or("df_executor: gk not i32")?
            .value(row)
            .into_datum(),
        20 => arr
            .as_any()
            .downcast_ref::<Int64Array>()
            .ok_or("df_executor: gk not i64")?
            .value(row)
            .into_datum(),
        700 => arr
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or("df_executor: gk not f32")?
            .value(row)
            .into_datum(),
        701 => arr
            .as_any()
            .downcast_ref::<Float64Array>()
            .ok_or("df_executor: gk not f64")?
            .value(row)
            .into_datum(),
        16 => arr
            .as_any()
            .downcast_ref::<BooleanArray>()
            .ok_or("df_executor: gk not bool")?
            .value(row)
            .into_datum(),
        25 | 1042 | 1043 => arr
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or("df_executor: gk not str")?
            .value(row)
            .to_string()
            .into_datum(),
        // timestamp/timestamptz: the raw i64 IS the timestamptz Datum (by-value μs); date: the raw i32 IS the date
        // Datum (by-value days) — same raw value we stored in build_arrow (epoch interpretation is irrelevant here).
        1114 | 1184 => arr
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .ok_or("df_executor: gk not ts")?
            .value(row)
            .into_datum(),
        1082 => arr
            .as_any()
            .downcast_ref::<Date32Array>()
            .ok_or("df_executor: gk not date")?
            .value(row)
            .into_datum(),
        other => return Err(format!("df_executor: unsupported group key oid {other}")),
    };
    Ok((d.ok_or("df_executor: group key datum")?, false))
}

/// M161 — materialize one EXPRESSION group-key cell (`GroupExprExec`). A DateTrunc array is already the native output
/// type → delegate to `arrow_value_to_datum`. ExtractField/IntAddConst compute WIDENED to Int64 in DataFusion,
/// so their arrow column is `Int64` and needs a variant-specific reverse: extract → PG `numeric` (AnyNumeric, scale 0 —
/// minute/hour are integer-valued); int±k → the base int type with a RANGE CHECK that fails when PG would raise 22003.
fn group_expr_cell(
    arr: &dyn Array,
    row: usize,
    g: &GroupExprExec,
) -> Result<(pg_sys::Datum, bool), String> {
    if arr.is_null(row) {
        return Ok((pg_sys::Datum::from(0usize), true));
    }
    match g.func {
        1 => {
            // ExtractField → numeric: the Int64 field value → AnyNumeric (exact, scale 0) = PG extract()'s numeric.
            let v = arr
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("df_executor: extract group key not Int64")?
                .value(row);
            Ok((
                AnyNumeric::from(v).into_datum().ok_or("df_executor: extract numeric datum")?,
                false,
            ))
        }
        2 => {
            // IntAddConst → the operator RESULT type (int2/int4 only; int8 result declined at admit) with a range
            // check: PG raises 22003 when the result overflows the result type, and our widened i64 value that does not
            // fit reproduces that failure (both plans error on the same overflowing datum). The i64 compute itself is
            // always exact here (int2/int4 column ± int const), so a value in range is byte-identical to PG's.
            let v = arr
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or("df_executor: int-arith group key not Int64")?
                .value(row);
            let d = match g.out_typoid {
                21 => {
                    i16::try_from(v).map_err(|_| "smallint out of range".to_string())?.into_datum()
                }
                23 => {
                    i32::try_from(v).map_err(|_| "integer out of range".to_string())?.into_datum()
                }
                other => return Err(format!("df_executor: int-arith bad out typoid {other}")),
            };
            Ok((d.ok_or("df_executor: int-arith datum")?, false))
        }
        // DateTrunc (func 0) — the Timestamp arrow array already IS the native output type.
        _ => arrow_value_to_datum(arr, row, g.out_typoid),
    }
}

/// M165 — materialize one const-out cell (layout kind=3): a projected integer literal. `admit` admitted ONLY the
/// integer class {int2,int4,int8} + non-NULL, so the stored i64 rebuilds the exact by-value PG Datum (int2/4/8 are
/// pass-through — the Datum IS the integer, same as `arrow_value_to_datum`'s int arms). Fail-closed on any other typoid.
fn const_out_datum(val: i64, typoid: u32) -> Result<(pg_sys::Datum, bool), String> {
    let d = match typoid {
        21 => (val as i16).into_datum(),
        23 => (val as i32).into_datum(),
        20 => val.into_datum(),
        other => return Err(format!("df_executor: const-out bad typoid {other}")),
    };
    Ok((d.ok_or("df_executor: const-out datum")?, false))
}

/// Run `count(*)`, `sum(<num_col>)` over the columnar table and format `count=N;sum=X` (Phase A/B test driver — the
/// planner-integrated automatic path is Phase C `columnar_agg.rs`).
unsafe fn run_columnar_agg(rel: pg_sys::Relation, num_col: &str) -> Result<String, String> {
    let r = unsafe {
        run_columnar_aggs(
            rel,
            &[AggSpec::CountStar, AggSpec::SumFloat8(num_col.to_string())],
            &[],
            &[],
            &[],
            false,
        )?
    };
    let c = unsafe { i64::from_datum(r[0].0, r[0].1).unwrap_or(0) };
    let s = unsafe { f64::from_datum(r[1].0, r[1].1).unwrap_or(0.0) };
    Ok(format!("count={c};sum={s:.4}"))
}

/// M100 Phase A test driver — a `count(*)` + `sum(<num_col>)` aggregate over a `theodb_columnar` table, executed
/// through the DataFusion vectorized path. Test-only (gated behind `pg_test`) until the planner `CustomScan`
/// integration (Phase C) makes the path automatic.
#[cfg(any(test, feature = "pg_test"))]
#[pg_extern]
fn theodb_df_columnar_agg(rel_oid: pg_sys::Oid, num_col: String) -> String {
    unsafe {
        let rel = pg_sys::relation_open(rel_oid, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        let res = run_columnar_agg(rel, &num_col);
        pg_sys::relation_close(rel, pg_sys::AccessShareLock as pg_sys::LOCKMODE);
        match res {
            Ok(s) => s,
            Err(e) => error!("{e}"),
        }
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// M100 Phase A — a DataFusion vectorized `count(*)` + `sum(measure)` over a `theodb_columnar` table is
    /// result-identical to the same aggregate over a heap table (50k rows). Proves the async-in-C DataFusion seam
    /// over REAL columnar Arrow batches: decode stripes → Arrow arrays → `block_on` a vectorized aggregate under
    /// `HeldInterrupts` → the exact count/sum. This is the de-risk that gates the planner `CustomScan` wiring (C).
    #[pg_test]
    fn m100_df_columnar_agg_matches_heap() {
        Spi::run("CREATE TABLE m100_c (id int, measure float8) USING theodb_columnar").unwrap();
        Spi::run("CREATE TABLE m100_h (id int, measure float8)").unwrap();
        let gen_sql = "SELECT g, (g * 1.5)::float8 FROM generate_series(1, 50000) g";
        Spi::run(&format!("INSERT INTO m100_c {gen_sql}")).unwrap();
        Spi::run(&format!("INSERT INTO m100_h {gen_sql}")).unwrap();

        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm100_c'::regclass::oid").unwrap().unwrap();
        let df_result = Spi::get_one_with_args::<String>(
            "SELECT theodb_df_columnar_agg($1, 'measure')",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();

        let hc = Spi::get_one::<i64>("SELECT count(*) FROM m100_h").unwrap().unwrap();
        let hs = Spi::get_one::<f64>("SELECT sum(measure) FROM m100_h").unwrap().unwrap();
        let expected = format!("count={hc};sum={hs:.4}");
        assert_eq!(
            df_result, expected,
            "DataFusion columnar aggregate must match the heap aggregate"
        );

        Spi::run("DROP TABLE m100_c").unwrap();
        Spi::run("DROP TABLE m100_h").unwrap();
    }

    /// M100 Phase B — projection pushdown: an aggregate over ONE column of a WIDE (6-column) columnar table decodes
    /// only that column (the other 5 chunks are never `read_chunked`/zstd-decoded) and still returns the correct
    /// result. Proves the projection lever end-to-end; the decode-skip is asserted by correctness over a wide table
    /// where decoding everything would be wasteful (a decode counter is a later instrumentation nicety).
    #[pg_test]
    fn m100_projection_decodes_only_aggregated_column() {
        Spi::run(
            "CREATE TABLE m100_w (a int, b text, c float8, d bigint, e bool, measure float8) USING theodb_columnar",
        )
        .unwrap();
        Spi::run(
            "INSERT INTO m100_w SELECT g, 'row-'||g, g*0.5, g::bigint, g%2=0, (g*2.5)::float8 \
             FROM generate_series(1, 30000) g",
        )
        .unwrap();
        let oid = Spi::get_one::<pg_sys::Oid>("SELECT 'm100_w'::regclass::oid").unwrap().unwrap();
        let df_result = Spi::get_one_with_args::<String>(
            "SELECT theodb_df_columnar_agg($1, 'measure')",
            &[oid.into()],
        )
        .unwrap()
        .unwrap();
        let hc: i64 = 30000;
        let hs: f64 = (1..=30000i64).map(|g| g as f64 * 2.5).sum();
        assert_eq!(
            df_result,
            format!("count={hc};sum={hs:.4}"),
            "projected aggregate over a wide table must be correct"
        );
        Spi::run("DROP TABLE m100_w").unwrap();
    }
}

#[cfg(test)]
mod m160_fixed_raw_tests {
    //! M160 — the zero-copy `fixed_raw_array` fast path MUST produce byte-identical Arrow to the legacy per-cell
    //! `cells_to_array` for every fast-eligible fixed-width type. Pure-Arrow (no pg_sys); compares `ArrayData`.
    use super::{cells_to_array, fixed_raw_array};

    fn cells_of<const W: usize>(vals: &[[u8; W]]) -> Vec<Option<Vec<u8>>> {
        vals.iter().map(|b| Some(b.to_vec())).collect()
    }
    fn contiguous<const W: usize>(vals: &[[u8; W]]) -> Vec<u8> {
        vals.iter().flat_map(|b| b.iter().copied()).collect()
    }

    #[test]
    fn fixed_raw_matches_cells_across_fast_types() {
        // (typid, width, sample little-endian value bytes)
        // int4 (23): 1, -2, 1_000_000
        let i32v = [1i32.to_le_bytes(), (-2i32).to_le_bytes(), 1_000_000i32.to_le_bytes()];
        // int8 (20): 0, i64::MIN, 42
        let i64v = [0i64.to_le_bytes(), i64::MIN.to_le_bytes(), 42i64.to_le_bytes()];
        // float8 (701): 1.5, -0.0, 3.25
        let f64v = [1.5f64.to_le_bytes(), (-0.0f64).to_le_bytes(), 3.25f64.to_le_bytes()];
        // int2 (21): 7, -1, 30000
        let i16v = [7i16.to_le_bytes(), (-1i16).to_le_bytes(), 30000i16.to_le_bytes()];
        // date (1082) int32 days: 100, 0, 20000
        let datev = [100i32.to_le_bytes(), 0i32.to_le_bytes(), 20000i32.to_le_bytes()];
        // timestamp (1114) int64 μs: 123, -456, 999_999_999
        let tsv = [123i64.to_le_bytes(), (-456i64).to_le_bytes(), 999_999_999i64.to_le_bytes()];

        macro_rules! check {
            ($typid:expr, $w:expr, $arr:expr) => {{
                let (dt_f, af) =
                    fixed_raw_array($typid, &contiguous(&$arr), $w, $arr.len()).unwrap();
                let (dt_c, ac) = cells_to_array($typid, &cells_of(&$arr)).unwrap();
                assert_eq!(dt_f, dt_c, "DataType mismatch for typid {}", $typid);
                assert_eq!(af.to_data(), ac.to_data(), "Arrow data mismatch for typid {}", $typid);
            }};
        }
        check!(23, 4, i32v);
        check!(20, 8, i64v);
        check!(701, 8, f64v);
        check!(21, 2, i16v);
        check!(1082, 4, datev);
        check!(1114, 8, tsv);
    }

    #[test]
    fn fixed_raw_rejects_wrong_size() {
        // 5 bytes for a width-4 / 1-row column must be a typed error, never a panic across C.
        assert!(fixed_raw_array(23, &[0u8; 5], 4, 1).is_err());
    }
}

#[cfg(test)]
mod m169_fail_open_tests {
    //! M169 T4.1 — a classificação da falha do braço streaming. Puro (sem `pg_sys`), então roda em `cargo test`.
    //!
    //! O RED destes testes não foi escrito no vácuo: é a regressão MEDIDA na corrida de 43 consultas a 100M, em
    //! que q08/q09 saíram de `ok` para `error:XX000` com exatamente a mensagem de spill abaixo. O predicado
    //! anterior só casava `ResourcesExhausted` e portanto não recuava.
    use super::{SPILL_CREATE_MARKER, stream_failure_is_fail_open};
    use datafusion::error::DataFusionError;

    /// A mensagem real, copiada da corrida (o `Os { code: 24 }` é `EMFILE`).
    fn spill_error() -> DataFusionError {
        DataFusionError::Execution(format!(
            "(Hint: you may increase the file descriptor limit with shell command 'ulimit -n 4096') \
             {SPILL_CREATE_MARKER} at \"/tmp/.tmpXYZ\": Os {{ code: 24, kind: Uncategorized }}"
        ))
    }

    #[test]
    fn spill_file_creation_failure_authorises_fail_open() {
        assert!(
            stream_failure_is_fail_open(&spill_error(), 1_000),
            "a falha de spill é de recurso — o eager servia estas consultas antes do M169"
        );
    }

    #[test]
    fn wrapped_spill_failure_is_found_through_context() {
        let wrapped =
            DataFusionError::Context("GroupedHashAggregate".into(), Box::new(spill_error()));
        assert!(
            stream_failure_is_fail_open(&wrapped, 1_000),
            "find_root() tem de atravessar o Context — casar a variante nua perde o recuo"
        );
    }

    #[test]
    fn resources_exhausted_still_authorises_fail_open() {
        let e = DataFusionError::ResourcesExhausted("pool limit".into());
        assert!(
            stream_failure_is_fail_open(&e, 1_000),
            "a classe original não pode ter sido perdida"
        );
    }

    #[test]
    fn unrelated_execution_error_does_not_fail_open() {
        // O teste mais importante: recuar em erro genuíno produziria a resposta certa pelo caminho errado e
        // esconderia um defeito do streaming para sempre.
        let e = DataFusionError::Execution("Divide by zero".into());
        assert!(!stream_failure_is_fail_open(&e, 1_000));
    }

    #[test]
    fn offset_overflow_risk_blocks_fail_open_even_for_a_resource_failure() {
        // Com varlena >= i32::MAX o eager falha por construção: recuar trocaria um erro por outro, mais lento.
        assert!(!stream_failure_is_fail_open(&spill_error(), i32::MAX as u64));
    }
}
