//! M69 — o tipo vetorial PRÓPRIO own-code `theodb.vector` (roadmap v4, blueprint veredito A).
//!
//! Layout `#[repr(C)]` BYTE-IDÊNTICO ao `Vector` do pgvector (`vl_len_ i32 · dim i16 · unused i16 ·
//! f32[]`; 8+4·dim bytes) — a pré-condição do cast binário `WITHOUT FUNCTION` (coexistência em M69,
//! migração grátis em M70). Coexiste com `public.vector` (pgvector) SEM colisão: o tipo próprio é
//! `theodb.vector` (schema `theodb`); o M70 fará `SET SCHEMA public` ⇒ drop-in (ADR-D5).
//!
//! Código ORIGINAL. Técnica de varlena aprendida de fontes permissivas (`pgvector` = PostgreSQL
//! License, `vector.c/.h`; `postgres.h` SET_VARSIZE; docs pgrx). VectorChord é AGPL (D1) = SÓ estudo.
//! Viabilidade provada pelo spike ADR-D3 (7/7 pg_test, `docs/spikes/m69-theovec-pgrx-feasibility/`).
use crate::pg;
use crate::vec;
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

const MAX_DIM: usize = 16000;

/// On-disk layout — byte-idêntico ao `Vector` do pgvector (`vector.h:11-17`).
#[repr(C)]
struct VecHeader {
    varlena: u32,      // varlena header (SET_VARSIZE little-endian: size << 2)
    dim: u16,          // == pgvector int16 dim
    unused: u16,       // == pgvector int16 unused (SEMPRE 0)
    elements: [f32; 0],
}

impl VecHeader {
    #[inline]
    fn size_of(len: usize) -> usize {
        size_of::<Self>() + size_of::<f32>() * len // 8 + 4*len == pgvector VECTOR_SIZE
    }
}

/// Wrapper own-code sobre o varlena detoasted. Dono do ponteiro (detoast_copy no FromDatum);
/// libera no `Drop` EXCETO quando `into_raw()` transfere a posse (evita double-free — EC-1).
pub struct Vector(NonNull<VecHeader>);

impl Vector {
    fn from_floats(slice: &[f32]) -> Self {
        if slice.is_empty() {
            pg::err_input("theodb.vector must have at least 1 dimension");
        }
        if slice.len() > MAX_DIM {
            pg::err_input(&format!("theodb.vector cannot have more than {MAX_DIM} dimensions"));
        }
        unsafe {
            let size = VecHeader::size_of(slice.len());
            let ptr = pgrx::pg_sys::palloc0(size) as *mut VecHeader;
            (&raw mut (*ptr).varlena).write((size << 2) as u32); // SET_VARSIZE_4B little-endian
            (&raw mut (*ptr).dim).write(slice.len() as u16);
            (&raw mut (*ptr).unused).write(0);
            std::ptr::copy_nonoverlapping(
                slice.as_ptr(),
                (&raw mut (*ptr).elements).cast::<f32>(),
                slice.len(),
            );
            Vector(NonNull::new(ptr).unwrap())
        }
    }

    unsafe fn from_datum_ptr(datum: Datum) -> Self {
        let raw = pgrx::pg_sys::pg_detoast_datum_copy(datum.cast_mut_ptr());
        let q = NonNull::new(raw.cast::<VecHeader>()).unwrap();
        let dim = (&raw const (*q.as_ptr()).dim).read() as usize;
        let sz = ((&raw const (*q.as_ptr()).varlena).read() as usize) >> 2;
        if sz != VecHeader::size_of(dim) {
            pg::err_input("theodb.vector: corrupt varlena (size mismatch)");
        }
        if (&raw const (*q.as_ptr()).unused).read() != 0 {
            pg::err_input("theodb.vector: expected unused to be 0");
        }
        Vector(q)
    }

    fn as_slice(&self) -> &[f32] {
        unsafe {
            let dim = (&raw const (*self.0.as_ptr()).dim).read() as usize;
            std::slice::from_raw_parts((&raw const (*self.0.as_ptr()).elements).cast::<f32>(), dim)
        }
    }

    /// Transfere a posse do ponteiro para o Datum (mem::forget ⇒ o Drop NÃO libera — EC-1).
    fn into_raw(self) -> *mut VecHeader {
        let p = self.0.as_ptr();
        std::mem::forget(self);
        p
    }
}

impl Drop for Vector {
    fn drop(&mut self) {
        unsafe { pgrx::pg_sys::pfree(self.0.as_ptr().cast()) }
    }
}

// ---- pgrx datum plumbing (API ditada pelo pgrx 0.16.1 — receita do spike) ----

impl FromDatum for Vector {
    unsafe fn from_polymorphic_datum(datum: Datum, is_null: bool, _oid: Oid) -> Option<Self> {
        if is_null { None } else { Some(unsafe { Vector::from_datum_ptr(datum) }) }
    }
}

impl IntoDatum for Vector {
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

unsafe impl UnboxDatum for Vector {
    type As<'src> = Vector;
    #[inline]
    unsafe fn unbox<'src>(datum: DatumLt<'src>) -> Self::As<'src>
    where
        Self: 'src,
    {
        unsafe { Vector::from_datum_ptr(datum.sans_lifetime().cast_mut_ptr::<()>().into()) }
    }
}

unsafe impl SqlTranslatable for Vector {
    fn argument_sql() -> Result<SqlMapping, ArgumentError> {
        Ok(SqlMapping::As(String::from("theodb.vector")))
    }
    fn return_sql() -> Result<Returns, ReturnsError> {
        Ok(Returns::One(SqlMapping::As(String::from("theodb.vector"))))
    }
}

unsafe impl<'fcx> ArgAbi<'fcx> for Vector {
    unsafe fn unbox_arg_unchecked(arg: Arg<'_, 'fcx>) -> Self {
        let idx = arg.index();
        unsafe {
            arg.unbox_arg_using_from_datum()
                .unwrap_or_else(|| panic!("argument {idx} must not be null"))
        }
    }
}

unsafe impl BoxRet for Vector {
    unsafe fn box_into<'fcx>(self, fcinfo: &mut FcInfo<'fcx>) -> DatumLt<'fcx> {
        match self.into_datum() {
            Some(d) => unsafe { fcinfo.return_raw_datum(d) },
            None => fcinfo.return_null(),
        }
    }
}

// ---- parse/format (espelha pgvector theodb_vector_in/theodb_vector_out, PostgreSQL License) ----

fn parse(text: &str) -> Vec<f32> {
    let s = text.trim();
    let inner = match s.strip_prefix('[') {
        Some(r) => r,
        None => pg::err_input("invalid input syntax for type theodb.vector: must start with \"[\""),
    };
    let inner = match inner.strip_suffix(']') {
        Some(r) => r,
        None => pg::err_input("invalid input syntax for type theodb.vector: junk after closing"),
    };
    let inner = inner.trim();
    if inner.is_empty() {
        pg::err_input("theodb.vector must have at least 1 dimension");
    }
    let mut out = Vec::new();
    for tok in inner.split(',') {
        let t = tok.trim();
        let v: f32 = match t.parse() {
            Ok(v) => v,
            Err(_) => pg::err_input(&format!(
                "invalid input syntax for type theodb.vector: \"{t}\""
            )),
        };
        if v.is_nan() {
            pg::err_input("NaN not allowed in theodb.vector");
        }
        if v.is_infinite() {
            pg::err_input("infinite value not allowed in theodb.vector");
        }
        out.push(v);
    }
    out
}

fn format(slice: &[f32]) -> String {
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

// ---- funções I/O (declaradas em SQL pelo CREATE TYPE) ----

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_in(input: &CStr, _oid: Oid, typmod: i32) -> Vector {
    let text = match input.to_str() {
        Ok(t) => t,
        Err(_) => pg::err_input("theodb.vector: input não-UTF8"),
    };
    let vals = parse(text);
    if typmod > 0 && typmod as usize != vals.len() {
        pg::err_input(&format!("expected {typmod} dimensions, not {}", vals.len()));
    }
    Vector::from_floats(&vals)
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_out(v: Vector) -> CString {
    CString::new(format(v.as_slice())).unwrap()
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_typmod_in(list: pgrx::Array<&CStr>) -> i32 {
    let first = match list.get(0).flatten() {
        Some(c) => c,
        None => pg::err_input("theodb.vector: typmod vazio"),
    };
    let n: i32 = match first.to_str().unwrap_or("").trim().parse() {
        Ok(n) => n,
        Err(_) => pg::err_input("invalid type modifier for theodb.vector"),
    };
    if n < 1 {
        pg::err_input("dimensions for type theodb.vector must be at least 1");
    }
    if n as usize > MAX_DIM {
        pg::err_input(&format!("dimensions for type theodb.vector cannot exceed {MAX_DIM}"));
    }
    n
}

/// Length-coercion cast — o Postgres chama isto para APLICAR `theodb.vector(N)` em inserts/atribuições
/// (espelha pgvector `vector(vector,integer,boolean)` + `CREATE CAST (vector AS vector)`, vector.sql:134,154).
#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_coerce(v: Vector, typmod: i32, _explicit: bool) -> Vector {
    let n = v.as_slice().len();
    if typmod >= 0 && typmod as usize != n {
        pg::err_input(&format!("expected {typmod} dimensions, not {n}"));
    }
    v
}

// ---- recv/send binário (wire; unused==0; espelha vector.c:369-416) ----

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_recv(mut internal: pgrx::datum::Internal, _oid: Oid, typmod: i32) -> Vector {
    unsafe {
        let buf: *mut pgrx::pg_sys::StringInfoData =
            internal.get_mut().expect("theodb.vector recv: null StringInfo");
        let dim = pgrx::pg_sys::pq_getmsgint(buf, 2) as i16;
        let unused = pgrx::pg_sys::pq_getmsgint(buf, 2) as i16;
        if dim < 1 {
            pg::err_input("theodb.vector must have at least 1 dimension");
        }
        if dim as usize > MAX_DIM {
            pg::err_input(&format!("theodb.vector cannot have more than {MAX_DIM} dimensions"));
        }
        if typmod > 0 && typmod != dim as i32 {
            pg::err_input(&format!("expected {typmod} dimensions, not {dim}"));
        }
        if unused != 0 {
            pg::err_input(&format!("expected unused to be 0, not {unused}"));
        }
        let mut vals = Vec::with_capacity(dim as usize);
        for _ in 0..dim {
            let f = pgrx::pg_sys::pq_getmsgfloat4(buf);
            if f.is_nan() {
                pg::err_input("NaN not allowed in theodb.vector");
            }
            if f.is_infinite() {
                pg::err_input("infinite value not allowed in theodb.vector");
            }
            vals.push(f);
        }
        Vector::from_floats(&vals)
    }
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_send(v: Vector) -> Vec<u8> {
    // Wire binário big-endian (network order), idêntico ao pgvector: int16 dim, int16 unused(0),
    // dim×float4. Construído direto no Vec<u8> (→ bytea) — sem StringInfo, robusto e sem FFI de send.
    let slice = v.as_slice();
    let mut out = Vec::with_capacity(4 + slice.len() * 4);
    out.extend_from_slice(&(slice.len() as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // unused
    for &f in slice {
        out.extend_from_slice(&f.to_be_bytes());
    }
    out
}

// ---- operadores de distância (reuso dos kernels vec.rs — D3, Regra 9) ----

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_l2_distance(a: Vector, b: Vector) -> f64 {
    check_dims(&a, &b);
    vec::l2_distance(a.as_slice(), b.as_slice())
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_neg_inner_product(a: Vector, b: Vector) -> f64 {
    check_dims(&a, &b);
    -vec::inner_product(a.as_slice(), b.as_slice()) // <#> é o inner-product NEGATIVO (pgvector)
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_cosine_distance(a: Vector, b: Vector) -> f64 {
    check_dims(&a, &b);
    vec::cosine_distance(a.as_slice(), b.as_slice())
}

fn check_dims(a: &Vector, b: &Vector) {
    let (x, y) = (a.as_slice().len(), b.as_slice().len());
    if x != y {
        pg::err_input(&format!("different theodb.vector dimensions {x} and {y}"));
    }
}

// ---- casts: real[] / float8[] / text ----

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_from_real_array(arr: Vec<f32>) -> Vector {
    for v in &arr {
        if v.is_nan() {
            pg::err_input("NaN not allowed in theodb.vector");
        }
        if v.is_infinite() {
            pg::err_input("infinite value not allowed in theodb.vector");
        }
    }
    Vector::from_floats(&arr)
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_to_real_array(v: Vector) -> Vec<f32> {
    v.as_slice().to_vec()
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_from_float8_array(arr: Vec<f64>) -> Vector {
    let f32s: Vec<f32> = arr.iter().map(|&x| x as f32).collect();
    theodb_vector_from_real_array(f32s)
}

// ---- DDL: CREATE TYPE theodb.vector + operadores + casts (o schema `theodb` já existe via umbrella) ----

extension_sql!(
    "CREATE TYPE theodb.vector;",
    name = "vector_shell",
    bootstrap,
);

extension_sql!(
    r#"
CREATE TYPE theodb.vector (
    INPUT     = theodb_vector_in,
    OUTPUT    = theodb_vector_out,
    RECEIVE   = theodb_vector_recv,
    SEND      = theodb_vector_send,
    TYPMOD_IN = theodb_vector_typmod_in,
    STORAGE   = external,
    INTERNALLENGTH = variable
);

CREATE CAST (theodb.vector AS theodb.vector)
    WITH FUNCTION theodb_vector_coerce(theodb.vector, integer, boolean) AS IMPLICIT;

CREATE OPERATOR <-> (
    LEFTARG = theodb.vector, RIGHTARG = theodb.vector,
    PROCEDURE = theodb_vector_l2_distance, COMMUTATOR = '<->'
);
CREATE OPERATOR <#> (
    LEFTARG = theodb.vector, RIGHTARG = theodb.vector,
    PROCEDURE = theodb_vector_neg_inner_product, COMMUTATOR = '<#>'
);
CREATE OPERATOR <=> (
    LEFTARG = theodb.vector, RIGHTARG = theodb.vector,
    PROCEDURE = theodb_vector_cosine_distance, COMMUTATOR = '<=>'
);

CREATE CAST (real[] AS theodb.vector)  WITH FUNCTION theodb_vector_from_real_array(real[]);
CREATE CAST (theodb.vector AS real[])  WITH FUNCTION theodb_vector_to_real_array(theodb.vector);
CREATE CAST (double precision[] AS theodb.vector) WITH FUNCTION theodb_vector_from_float8_array(double precision[]);
"#,
    name = "vector_type",
    requires = [
        "vector_shell",
        theodb_vector_in,
        theodb_vector_out,
        theodb_vector_recv,
        theodb_vector_send,
        theodb_vector_typmod_in,
        theodb_vector_coerce,
        theodb_vector_l2_distance,
        theodb_vector_neg_inner_product,
        theodb_vector_cosine_distance,
        theodb_vector_from_real_array,
        theodb_vector_to_real_array,
        theodb_vector_from_float8_array,
    ],
);

// ---- testes (pg_test, stack real pg17 + pgvector coexistindo) ----

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    #[pg_test]
    fn roundtrip_text_io() {
        let out = Spi::get_one::<String>("SELECT '[1,2,3]'::theodb.vector::text").unwrap().unwrap();
        assert_eq!(out, "[1,2,3]");
    }

    #[pg_test(error = "NaN not allowed in theodb.vector")]
    fn nan_rejected() {
        Spi::run("SELECT '[1,NaN,3]'::theodb.vector").unwrap();
    }

    #[pg_test(error = "theodb.vector must have at least 1 dimension")]
    fn dim0_rejected() {
        Spi::run("SELECT '[]'::theodb.vector").unwrap();
    }

    #[pg_test(error = "invalid input syntax for type theodb.vector: must start with \"[\"")]
    fn malformed_no_bracket() {
        Spi::run("SELECT '1,2,3'::theodb.vector").unwrap();
    }

    #[pg_test]
    fn dim_boundary() {
        // dim=1 (mínimo) e um vetor grande round-trip
        assert_eq!(
            Spi::get_one::<String>("SELECT '[1]'::theodb.vector::text").unwrap().unwrap(),
            "[1]"
        );
        let big = format!("[{}]", (0..128).map(|i| i.to_string()).collect::<Vec<_>>().join(","));
        let n = Spi::get_one::<i64>(&format!("SELECT array_length(('{big}'::theodb.vector)::real[],1)"))
            .unwrap()
            .unwrap();
        assert_eq!(n, 128);
    }

    #[pg_test(error = "theodb.vector cannot have more than 16000 dimensions")]
    fn dim_over_max_rejected() {
        Spi::run("SELECT ('[' || array_to_string(array_fill(1.0::real, ARRAY[16001]), ',') || ']')::theodb.vector").unwrap();
    }

    #[pg_test]
    fn typmod_ok() {
        assert_eq!(
            Spi::get_one::<String>("SELECT '[1,2,3]'::theodb.vector(3)::text").unwrap().unwrap(),
            "[1,2,3]"
        );
    }

    #[pg_test(error = "expected 3 dimensions, not 2")]
    fn typmod_mismatch_on_column() {
        Spi::run("CREATE TEMP TABLE tt (e theodb.vector(3))").unwrap();
        Spi::run("INSERT INTO tt VALUES ('[1,2]')").unwrap();
    }

    #[pg_test]
    fn datum_roundtrip_no_uaf() {
        // EC-1: cast recebe E retorna o mesmo ptr, 1000× — pega double-free/UAF.
        Spi::run(
            "DO $$ BEGIN FOR i IN 1..1000 LOOP PERFORM ('[1,2,3]'::theodb.vector)::theodb.vector(3); END LOOP; END $$",
        )
        .unwrap();
    }

    #[pg_test]
    fn operators_match_kernels() {
        let l2 = Spi::get_one::<f64>("SELECT '[0,0]'::theodb.vector <-> '[3,4]'::theodb.vector")
            .unwrap()
            .unwrap();
        assert!((l2 - 5.0).abs() < 1e-6, "L2=5, got {l2}");
        let ip = Spi::get_one::<f64>("SELECT '[1,2]'::theodb.vector <#> '[3,4]'::theodb.vector")
            .unwrap()
            .unwrap();
        assert!((ip - (-11.0)).abs() < 1e-6, "neg-ip=-(1*3+2*4)=-11, got {ip}");
        let cos = Spi::get_one::<f64>("SELECT '[1,0]'::theodb.vector <=> '[0,1]'::theodb.vector")
            .unwrap()
            .unwrap();
        assert!((cos - 1.0).abs() < 1e-6, "cosine([1,0],[0,1])=1, got {cos}");
    }

    #[pg_test]
    fn casts_array_roundtrip() {
        let back = Spi::get_one::<String>("SELECT (ARRAY[1,2,3]::real[]::theodb.vector)::real[]::text")
            .unwrap()
            .unwrap();
        assert_eq!(back, "{1,2,3}");
        let f8 = Spi::get_one::<String>("SELECT (ARRAY[1.5,2.5]::float8[]::theodb.vector)::text")
            .unwrap()
            .unwrap();
        assert_eq!(f8, "[1.5,2.5]");
    }

    #[pg_test]
    fn table_column_and_order_by() {
        Spi::run("CREATE TEMP TABLE tv (id int, e theodb.vector(2))").unwrap();
        Spi::run("INSERT INTO tv VALUES (1,'[0,0]'),(2,'[1,1]'),(3,'[5,5]')").unwrap();
        let nearest = Spi::get_one::<i32>(
            "SELECT id FROM tv ORDER BY e <-> '[0,0]'::theodb.vector LIMIT 1",
        )
        .unwrap()
        .unwrap();
        assert_eq!(nearest, 1);
    }

    /// GATE: layout byte-idêntico ao pgvector — cast binário WITHOUT FUNCTION nos 2 sentidos, dim variado.
    #[pg_test]
    fn binary_compat_with_pgvector() {
        Spi::run("CREATE EXTENSION IF NOT EXISTS vector").unwrap();
        Spi::run("CREATE CAST (vector AS theodb.vector) WITHOUT FUNCTION AS IMPLICIT").unwrap();
        Spi::run("CREATE CAST (theodb.vector AS vector) WITHOUT FUNCTION AS IMPLICIT").unwrap();
        for dims in ["[1,2,3]", "[7]", "[10,20,30,40,50]"] {
            let fwd = Spi::get_one::<String>(&format!(
                "SELECT ('{dims}'::vector::theodb.vector)::text"
            ))
            .unwrap()
            .unwrap();
            assert_eq!(fwd, dims, "cast pgvector→theodb.vector dim-variado");
            let back = Spi::get_one::<String>(&format!(
                "SELECT ('{dims}'::theodb.vector::vector)::text"
            ))
            .unwrap()
            .unwrap();
            assert_eq!(back, dims, "cast theodb.vector→pgvector dim-variado");
        }
    }

    #[pg_test]
    fn copy_binary_roundtrip() {
        Spi::run("CREATE TEMP TABLE cb (e theodb.vector(3))").unwrap();
        Spi::run("INSERT INTO cb VALUES ('[1,2,3]'),(NULL),('[4,5,6]')").unwrap();
        // round-trip via COPY BINARY exercita recv/send (unused==0 no wire)
        Spi::run("CREATE TEMP TABLE cb2 (e theodb.vector(3))").unwrap();
        Spi::run("COPY cb TO '/tmp/theodb_cb.bin' WITH (FORMAT binary)").unwrap();
        Spi::run("COPY cb2 FROM '/tmp/theodb_cb.bin' WITH (FORMAT binary)").unwrap();
        let n = Spi::get_one::<i64>("SELECT count(*) FROM cb2 WHERE e IS NOT NULL").unwrap().unwrap();
        assert_eq!(n, 2, "COPY BINARY preservou 2 não-nulos");
        let first = Spi::get_one::<String>("SELECT e::text FROM cb2 WHERE e IS NOT NULL ORDER BY e <-> '[0,0,0]'::theodb.vector LIMIT 1").unwrap().unwrap();
        assert_eq!(first, "[1,2,3]");
    }
}
