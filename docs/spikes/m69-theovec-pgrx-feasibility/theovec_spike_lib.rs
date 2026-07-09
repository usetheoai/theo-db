//! SPIKE (ADR-D3, M69): prova que pgrx 0.16.1 consegue DEFINIR um tipo `vector` próprio
//! denso de dimensão-variável (varlena com flexible array), com layout `#[repr(C)]`
//! BYTE-IDÊNTICO ao `Vector` do pgvector (`vl_len_ i32 · dim i16 · unused i16 · f32[]`).
//!
//! Código ORIGINAL. Técnica de varlena aprendida (não copiada) — o parse/format espelha
//! `pgvector/src/vector.c` (PostgreSQL License, permissivo). VectorChord é AGPL: só estudo.
use core::ffi::CStr;
use pgrx::callconv::{Arg, ArgAbi, BoxRet, FcInfo};
use pgrx::datum::{Datum as DatumLt, FromDatum, IntoDatum, UnboxDatum};
use pgrx::pg_sys::{Datum, Oid};
use pgrx::pgrx_sql_entity_graph::metadata::{
    ArgumentError, Returns, ReturnsError, SqlMapping, SqlTranslatable,
};
use pgrx::prelude::*;
use std::ffi::CString;
use std::ptr::NonNull;

::pgrx::pg_module_magic!();

const MAX_DIM: usize = 16000;

/// On-disk layout — byte-idêntico ao `Vector` do pgvector (`vector.h:11-17`).
#[repr(C)]
struct TheoVecHeader {
    varlena: u32,      // varlena header (SET_VARSIZE little-endian: size << 2)
    dim: u16,          // == pgvector int16 dim
    unused: u16,       // == pgvector int16 unused (SEMPRE 0)
    elements: [f32; 0],
}

impl TheoVecHeader {
    #[inline]
    fn size_of(len: usize) -> usize {
        // 8 + 4*len — idêntico a pgvector VECTOR_SIZE (offsetof(x)=8)
        size_of::<Self>() + size_of::<f32>() * len
    }
}

/// Wrapper own-code sobre o varlena detoasted. Sempre copia no FromDatum (detoast_copy),
/// então é dono do ponteiro e o libera no Drop.
pub struct TheoVec(NonNull<TheoVecHeader>);

impl TheoVec {
    /// Constrói um novo varlena a partir de floats (palloc0 + header + copy).
    fn new(slice: &[f32]) -> Self {
        assert!(!slice.is_empty() && slice.len() <= MAX_DIM, "dim fora de 1..={MAX_DIM}");
        unsafe {
            let size = TheoVecHeader::size_of(slice.len());
            let ptr = pgrx::pg_sys::palloc0(size) as *mut TheoVecHeader;
            // SET_VARSIZE_4B (little-endian): comprimento << 2 nos 30 bits altos.
            (&raw mut (*ptr).varlena).write((size << 2) as u32);
            (&raw mut (*ptr).dim).write(slice.len() as u16);
            (&raw mut (*ptr).unused).write(0);
            std::ptr::copy_nonoverlapping(
                slice.as_ptr(),
                (&raw mut (*ptr).elements).cast::<f32>(),
                slice.len(),
            );
            TheoVec(NonNull::new(ptr).unwrap())
        }
    }

    unsafe fn from_datum_ptr(datum: Datum) -> Self {
        // detoast_copy: sempre possui uma cópia própria (libera no Drop).
        let raw = pgrx::pg_sys::pg_detoast_datum_copy(datum.cast_mut_ptr());
        let q = NonNull::new(raw.cast::<TheoVecHeader>()).unwrap();
        // sanity: unused==0 e size coerente
        let dim = (&raw const (*q.as_ptr()).dim).read() as usize;
        let sz = ((&raw const (*q.as_ptr()).varlena).read() as usize) >> 2;
        assert_eq!(sz, TheoVecHeader::size_of(dim), "varlena size != header");
        assert_eq!((&raw const (*q.as_ptr()).unused).read(), 0, "unused != 0");
        TheoVec(q)
    }

    fn as_slice(&self) -> &[f32] {
        unsafe {
            let dim = (&raw const (*self.0.as_ptr()).dim).read() as usize;
            std::slice::from_raw_parts((&raw const (*self.0.as_ptr()).elements).cast::<f32>(), dim)
        }
    }

    fn into_raw(self) -> *mut TheoVecHeader {
        let p = self.0.as_ptr();
        std::mem::forget(self);
        p
    }
}

impl Drop for TheoVec {
    fn drop(&mut self) {
        unsafe { pgrx::pg_sys::pfree(self.0.as_ptr().cast()) }
    }
}

// ---- pgrx datum plumbing (API ditada pelo pgrx 0.16.1) ----

impl FromDatum for TheoVec {
    unsafe fn from_polymorphic_datum(datum: Datum, is_null: bool, _oid: Oid) -> Option<Self> {
        if is_null { None } else { Some(unsafe { TheoVec::from_datum_ptr(datum) }) }
    }
}

impl IntoDatum for TheoVec {
    fn into_datum(self) -> Option<Datum> {
        Some(Datum::from(self.into_raw()))
    }
    fn type_oid() -> Oid {
        Oid::INVALID // tipo custom resolvido via SqlTranslatable no schema-gen
    }
    fn is_compatible_with(_: Oid) -> bool {
        true
    }
}

unsafe impl UnboxDatum for TheoVec {
    type As<'src> = TheoVec;
    #[inline]
    unsafe fn unbox<'src>(datum: DatumLt<'src>) -> Self::As<'src>
    where
        Self: 'src,
    {
        unsafe { TheoVec::from_datum_ptr(datum.sans_lifetime().cast_mut_ptr::<()>().into()) }
    }
}

unsafe impl SqlTranslatable for TheoVec {
    fn argument_sql() -> Result<SqlMapping, ArgumentError> {
        Ok(SqlMapping::As(String::from("theovec")))
    }
    fn return_sql() -> Result<Returns, ReturnsError> {
        Ok(Returns::One(SqlMapping::As(String::from("theovec"))))
    }
}

unsafe impl<'fcx> ArgAbi<'fcx> for TheoVec {
    unsafe fn unbox_arg_unchecked(arg: Arg<'_, 'fcx>) -> Self {
        let idx = arg.index();
        unsafe {
            arg.unbox_arg_using_from_datum()
                .unwrap_or_else(|| panic!("argument {idx} must not be null"))
        }
    }
}

unsafe impl BoxRet for TheoVec {
    unsafe fn box_into<'fcx>(self, fcinfo: &mut FcInfo<'fcx>) -> DatumLt<'fcx> {
        match self.into_datum() {
            Some(d) => unsafe { fcinfo.return_raw_datum(d) },
            None => fcinfo.return_null(),
        }
    }
}

// ---- parse/format (espelha pgvector vector_in/vector_out) ----

fn parse_theovec(text: &str) -> Vec<f32> {
    let s = text.trim();
    let inner = s
        .strip_prefix('[')
        .unwrap_or_else(|| panic!("malformed theovec: must start with \"[\" (got {text:?})"));
    let inner = inner
        .strip_suffix(']')
        .unwrap_or_else(|| panic!("malformed theovec: must end with \"]\" (got {text:?})"));
    let inner = inner.trim();
    if inner.is_empty() {
        panic!("theovec must have at least 1 dimension");
    }
    let out: Vec<f32> = inner
        .split(',')
        .map(|tok| {
            let t = tok.trim();
            let v: f32 = t
                .parse()
                .unwrap_or_else(|_| panic!("invalid input syntax for type theovec: {t:?}"));
            if v.is_nan() {
                panic!("NaN not allowed in theovec");
            }
            if v.is_infinite() {
                panic!("infinite value not allowed in theovec");
            }
            v
        })
        .collect();
    if out.len() > MAX_DIM {
        panic!("theovec cannot have more than {MAX_DIM} dimensions");
    }
    out
}

fn format_theovec(slice: &[f32]) -> String {
    let mut s = String::with_capacity(2 + slice.len() * 8);
    s.push('[');
    for (i, v) in slice.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&v.to_string());
    }
    s.push(']');
    s
}

// ---- funções I/O (declaradas em SQL pelo CREATE TYPE abaixo) ----

#[pg_extern(immutable, strict, parallel_safe)]
fn theovec_in(input: &CStr, _oid: Oid, typmod: i32) -> TheoVec {
    let text = input.to_str().expect("theovec input não-UTF8");
    let vals = parse_theovec(text);
    if typmod > 0 && typmod as usize != vals.len() {
        panic!("expected {typmod} dimensions, not {}", vals.len());
    }
    TheoVec::new(&vals)
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theovec_out(v: TheoVec) -> CString {
    CString::new(format_theovec(v.as_slice())).unwrap()
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theovec_typmod_in(list: pgrx::Array<&CStr>) -> i32 {
    let first = list.get(0).flatten().expect("typmod vazio");
    let n: i32 = first.to_str().unwrap().trim().parse().expect("typmod inválido");
    if n < 1 {
        panic!("dimensions for type theovec must be at least 1");
    }
    if n as usize > MAX_DIM {
        panic!("dimensions for type theovec cannot exceed {MAX_DIM}");
    }
    n
}

// ---- operador de distância L2 (o spike prova o binding operador↔tipo) ----

/// Length-coercion (typmod) cast — o Postgres chama isto para APLICAR `theovec(N)` em
/// inserts/atribuições. Espelha o pgvector `vector(vector,integer,boolean)`
/// (`vector.sql:134` + `CREATE CAST (vector AS vector)` `:154`). Sem isto, o typmod parseia
/// mas não enforça.
#[pg_extern(immutable, strict, parallel_safe, name = "theovec")]
fn theovec_typmod_cast(v: TheoVec, typmod: i32, _explicit: bool) -> TheoVec {
    let n = v.as_slice().len();
    if typmod >= 0 && typmod as usize != n {
        panic!("expected {typmod} dimensions, not {n}");
    }
    v
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theovec_l2_distance(a: TheoVec, b: TheoVec) -> f64 {
    let (x, y) = (a.as_slice(), b.as_slice());
    assert_eq!(x.len(), y.len(), "different theovec dimensions {} and {}", x.len(), y.len());
    let mut acc: f32 = 0.0;
    for i in 0..x.len() {
        let d = x[i] - y[i];
        acc += d * d;
    }
    (acc as f64).sqrt()
}

// ---- CREATE TYPE (shell → I/O → tipo completo) via extension_sql ----

extension_sql!(
    "CREATE TYPE theovec;",
    name = "theovec_shell",
    bootstrap,
);

extension_sql!(
    r#"
CREATE TYPE theovec (
    INPUT     = theovec_in,
    OUTPUT    = theovec_out,
    TYPMOD_IN = theovec_typmod_in,
    STORAGE   = external,
    INTERNALLENGTH = variable
);

CREATE OPERATOR <-> (
    LEFTARG = theovec, RIGHTARG = theovec,
    PROCEDURE = theovec_l2_distance, COMMUTATOR = '<->'
);

CREATE CAST (theovec AS theovec)
    WITH FUNCTION theovec(theovec, integer, boolean) AS IMPLICIT;
"#,
    name = "theovec_type",
    requires = ["theovec_shell", theovec_in, theovec_out, theovec_typmod_in, theovec_l2_distance, theovec_typmod_cast],
);

// ---- testes (pg_test, stack real pg17) ----

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn roundtrip_text_io() {
        let out = Spi::get_one::<String>("SELECT '[1,2,3]'::theovec::text").unwrap().unwrap();
        assert_eq!(out, "[1,2,3]", "round-trip text I/O do tipo próprio");
    }

    #[pg_test]
    fn typmod_dim_ok() {
        let ok = Spi::get_one::<String>("SELECT '[1,2,3]'::theovec(3)::text").unwrap().unwrap();
        assert_eq!(ok, "[1,2,3]", "typmod correto passa");
    }

    // Negative-case via o idioma pgrx: um pg ERROR (longjmp) NÃO é um panic Rust capturável
    // por catch_unwind — o pgrx casa a mensagem do erro esperado (paridade pgvector).
    // Enforcement de typmod no path REAL (coluna tipada) — o que importa em uso de produção.
    #[pg_test(error = "expected 3 dimensions, not 2")]
    fn typmod_dim_mismatch_rejected_on_column() {
        Spi::run("CREATE TEMP TABLE tt (e theovec(3))").unwrap();
        Spi::run("INSERT INTO tt VALUES ('[1,2]')").unwrap();
    }

    #[pg_test(error = "NaN not allowed in theovec")]
    fn nan_rejected() {
        Spi::run("SELECT '[1,NaN,3]'::theovec").unwrap();
    }

    #[pg_test]
    fn operator_l2_distance() {
        let d = Spi::get_one::<f64>("SELECT '[0,0]'::theovec <-> '[3,4]'::theovec")
            .unwrap()
            .unwrap();
        assert!((d - 5.0).abs() < 1e-6, "L2([0,0],[3,4])=5, got {d}");
    }

    #[pg_test]
    fn table_column_and_order_by() {
        Spi::run("CREATE TEMP TABLE tv (id int, e theovec(2))").unwrap();
        Spi::run("INSERT INTO tv VALUES (1,'[0,0]'),(2,'[1,1]'),(3,'[5,5]')").unwrap();
        let nearest =
            Spi::get_one::<i32>("SELECT id FROM tv ORDER BY e <-> '[0,0]'::theovec LIMIT 1")
                .unwrap()
                .unwrap();
        assert_eq!(nearest, 1, "coluna theovec + ORDER BY operador");
    }

    /// O GATE do spike: prova que o layout on-disk é BYTE-IDÊNTICO ao pgvector —
    /// um CAST binário (WITHOUT FUNCTION) reinterpreta os bytes sem reescrita.
    #[pg_test]
    fn binary_compat_with_pgvector() {
        Spi::run("CREATE EXTENSION IF NOT EXISTS vector").unwrap();
        Spi::run("CREATE CAST (vector AS theovec) WITHOUT FUNCTION AS IMPLICIT").unwrap();
        let out = Spi::get_one::<String>("SELECT ('[1,2,3]'::vector::theovec)::text")
            .unwrap()
            .unwrap();
        assert_eq!(out, "[1,2,3]", "cast binário pgvector→theovec sem função (layout idêntico)");
        // e o inverso
        Spi::run("CREATE CAST (theovec AS vector) WITHOUT FUNCTION AS IMPLICIT").unwrap();
        let back = Spi::get_one::<String>("SELECT ('[4,5,6]'::theovec::vector)::text")
            .unwrap()
            .unwrap();
        assert_eq!(back, "[4,5,6]", "cast binário theovec→pgvector sem função");
    }
}

#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {}
    pub fn postgresql_conf_options() -> Vec<&'static str> {
        vec![]
    }
}
