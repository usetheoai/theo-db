//! M69 — o tipo vetorial PRÓPRIO own-code `vector` (roadmap v4, blueprint veredito A).
//!
//! Layout `#[repr(C)]` BYTE-IDÊNTICO ao `Vector` do pgvector (`vl_len_ i32 · dim i16 · unused i16 ·
//! f32[]`; 8+4·dim bytes) — a pré-condição do cast binário `WITHOUT FUNCTION` (coexistência em M69,
//! migração grátis em M70). Coexiste com `public.vector` (pgvector) SEM colisão: o tipo próprio é
//! M70: o tipo é `public.vector` (drop-in — o pgvector foi REMOVIDO; sem colisão). O flip (ADR-0029 D1)
//! faz o theodb_rs prover o tipo + os schemas theodb/ai; o umbrella `theodb` requer o theodb_rs.
//!
//! Código ORIGINAL. Técnica de varlena aprendida de fontes permissivas (`pgvector` = PostgreSQL
//! License, `vector.c/.h`; `postgres.h` SET_VARSIZE; docs pgrx). VectorChord é AGPL (D1) = SÓ estudo.
//! Viabilidade provada pelo spike ADR-D3 (7/7 pg_test, `wiki/references/m69-theovec-pgrx-feasibility/`).
use crate::pg;
use crate::vec;
use core::ffi::CStr;
use pgrx::callconv::{Arg, ArgAbi, BoxRet, FcInfo};
use pgrx::datum::{Datum as DatumLt, FromDatum, IntoDatum, UnboxDatum};
use pgrx::pg_sys::{Datum, Oid};
use pgrx::pgrx_sql_entity_graph::metadata::{
    ArgumentError, ReturnsError, ReturnsRef, SqlMappingRef, SqlTranslatable, TypeOrigin,
};
use pgrx::prelude::*;
use std::ffi::CString;
use std::ptr::NonNull;

const MAX_DIM: usize = 16000;

/// On-disk layout — byte-idêntico ao `Vector` do pgvector (`vector.h:11-17`).
#[repr(C)]
struct VecHeader {
    varlena: u32, // varlena header (SET_VARSIZE little-endian: size << 2)
    dim: u16,     // == pgvector int16 dim
    unused: u16,  // == pgvector int16 unused (SEMPRE 0)
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
            pg::err_input("vector must have at least 1 dimension");
        }
        if slice.len() > MAX_DIM {
            pg::err_input(&format!("vector cannot have more than {MAX_DIM} dimensions"));
        }
        unsafe {
            let size = VecHeader::size_of(slice.len());
            // SET_VARSIZE_4B usa os 30 bits altos; com MAX_DIM=16000, size ≤ 64008 (cabe folgado).
            // Guard à prova de futuro (HIGH-2 review) caso MAX_DIM suba: o shift não pode transbordar.
            debug_assert!(size < (1 << 30), "vector varlena size overflow");
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
        unsafe {
            let raw = pgrx::pg_sys::pg_detoast_datum_copy(datum.cast_mut_ptr());
            let q = NonNull::new(raw.cast::<VecHeader>()).unwrap();
            let dim = (&raw const (*q.as_ptr()).dim).read() as usize;
            let sz = ((&raw const (*q.as_ptr()).varlena).read() as usize) >> 2;
            if sz != VecHeader::size_of(dim) {
                pg::err_input("vector: corrupt varlena (size mismatch)");
            }
            if (&raw const (*q.as_ptr()).unused).read() != 0 {
                pg::err_input("vector: expected unused to be 0");
            }
            Vector(q)
        }
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

// M98 — pgrx 0.18 One-Compile: `SqlTranslatable` is now compile-time consts (was method-based). `public.vector`
// is a MANUAL mapping to an existing SQL type — it is created by our hand-written `extension_sql!` bootstrap
// (`CREATE TYPE vector`, bare/unqualified = public), NOT by a `#[derive(PostgresType)]`. Per the v18 migration
// guide, a manual mapping to an existing SQL type uses `TypeOrigin::External` so the emitted SQL is the bare
// `ARGUMENT_SQL` literal (`vector`, matching the drop-in M70 contract), NOT the module-path-qualified
// `theodb_rs.vector` that `ThisExtension` would emit. The SQL name stays `vector` (byte-identical, no REINDEX /
// no user-SQL change — the m69 round-trip tests are the oracle).
unsafe impl SqlTranslatable for Vector {
    const TYPE_IDENT: &'static str = pgrx::pgrx_resolved_type!(Vector);
    const TYPE_ORIGIN: TypeOrigin = TypeOrigin::External;
    const ARGUMENT_SQL: Result<SqlMappingRef, ArgumentError> = Ok(SqlMappingRef::literal("vector"));
    const RETURN_SQL: Result<ReturnsRef, ReturnsError> =
        Ok(ReturnsRef::One(SqlMappingRef::literal("vector")));
}

unsafe impl<'fcx> ArgAbi<'fcx> for Vector {
    unsafe fn unbox_arg_unchecked(arg: Arg<'_, 'fcx>) -> Self {
        let idx = arg.index();
        unsafe {
            arg.unbox_arg_using_from_datum().unwrap_or_else(|| {
                crate::pg::err_input(&format!("argument {idx} must not be null"))
            })
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
        None => pg::err_input("invalid input syntax for type vector: must start with \"[\""),
    };
    let inner = match inner.strip_suffix(']') {
        Some(r) => r,
        None => pg::err_input("invalid input syntax for type vector: junk after closing"),
    };
    let inner = inner.trim();
    if inner.is_empty() {
        pg::err_input("vector must have at least 1 dimension");
    }
    let mut out = Vec::new();
    for tok in inner.split(',') {
        let t = tok.trim();
        let v: f32 = match t.parse() {
            Ok(v) => v,
            Err(_) => pg::err_input(&format!("invalid input syntax for type vector: \"{t}\"")),
        };
        if v.is_nan() {
            pg::err_input("NaN not allowed in vector");
        }
        if v.is_infinite() {
            pg::err_input("infinite value not allowed in vector");
        }
        out.push(v);
        // Fail-fast dentro do loop (M2 review, espelha vector.c:205) — não acumula um Vec gigante
        // antes de rejeitar (mitiga alocação de input adversário com milhões de tokens).
        if out.len() > MAX_DIM {
            pg::err_input(&format!("vector cannot have more than {MAX_DIM} dimensions"));
        }
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
        Err(_) => pg::err_input("vector: input não-UTF8"),
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
        None => pg::err_input("vector: typmod vazio"),
    };
    let n: i32 = match first.to_str().unwrap_or("").trim().parse() {
        Ok(n) => n,
        Err(_) => pg::err_input("invalid type modifier for vector"),
    };
    if n < 1 {
        pg::err_input("dimensions for type vector must be at least 1");
    }
    if n as usize > MAX_DIM {
        pg::err_input(&format!("dimensions for type vector cannot exceed {MAX_DIM}"));
    }
    n
}

/// Length-coercion cast — o Postgres chama isto para APLICAR `vector(N)` em inserts/atribuições
/// (espelha pgvector `vector(vector,integer,boolean)` + `CREATE CAST (vector AS vector)`, vector.sql:134,154).
#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_coerce(v: Vector, typmod: i32, _explicit: bool) -> Vector {
    let n = v.as_slice().len();
    if typmod > 0 && typmod as usize != n {
        pg::err_input(&format!("expected {typmod} dimensions, not {n}"));
    }
    v
}

// ---- recv/send binário (wire; unused==0; espelha vector.c:369-416) ----

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_recv(mut internal: pgrx::datum::Internal, _oid: Oid, typmod: i32) -> Vector {
    unsafe {
        let buf: *mut pgrx::pg_sys::StringInfoData =
            internal.get_mut().expect("vector recv: null StringInfo");
        let dim = pgrx::pg_sys::pq_getmsgint(buf, 2) as i16;
        let unused = pgrx::pg_sys::pq_getmsgint(buf, 2) as i16;
        if dim < 1 {
            pg::err_input("vector must have at least 1 dimension");
        }
        if dim as usize > MAX_DIM {
            pg::err_input(&format!("vector cannot have more than {MAX_DIM} dimensions"));
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
                pg::err_input("NaN not allowed in vector");
            }
            if f.is_infinite() {
                pg::err_input("infinite value not allowed in vector");
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
        pg::err_input(&format!("different vector dimensions {x} and {y}"));
    }
}

// ---- casts: real[] / float8[] / text ----

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_from_real_array(arr: Vec<f32>) -> Vector {
    for v in &arr {
        if v.is_nan() {
            pg::err_input("NaN not allowed in vector");
        }
        if v.is_infinite() {
            pg::err_input("infinite value not allowed in vector");
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

// ---- DDL: CREATE TYPE vector + operadores + casts ----
// O shell type é o ÚNICO bootstrap (pgrx só permite um; o usa p/ as funcs I/O referenciarem o tipo).
// M70: o schema `theodb` (antes do umbrella — flip ADR-D1) é criado no bloco do catálogo (autotune.rs).
// REQUIRED (M98 review M2): `SqlTranslatable for Vector` is `TypeOrigin::External`, so pgrx does NOT emit a
// `CREATE TYPE` — THIS bootstrap is the SOLE creator of the `vector` type. Removing it silently breaks the
// extension (the type never gets created). The External⇄bootstrap coupling is load-bearing.
extension_sql!("CREATE TYPE vector;", name = "vector_shell", bootstrap,);

// M70 (flip ADR-D1): o theodb_rs provê o schema `theodb` (antes vinha do umbrella). Bloco nomeado que
// os catálogos `theodb.*` (autotune, vectorizer) declaram em `requires` p/ garantir a ordem de criação.
extension_sql!(
    "CREATE SCHEMA IF NOT EXISTS theodb; CREATE SCHEMA IF NOT EXISTS ai;",
    name = "theodb_schema_bootstrap",
);

extension_sql!(
    r#"
CREATE TYPE vector (
    INPUT     = theodb_vector_in,
    OUTPUT    = theodb_vector_out,
    RECEIVE   = theodb_vector_recv,
    SEND      = theodb_vector_send,
    TYPMOD_IN = theodb_vector_typmod_in,
    STORAGE   = external,
    INTERNALLENGTH = variable
);

CREATE CAST (vector AS vector)
    WITH FUNCTION theodb_vector_coerce(vector, integer, boolean) AS IMPLICIT;

CREATE OPERATOR <-> (
    LEFTARG = vector, RIGHTARG = vector,
    PROCEDURE = theodb_vector_l2_distance, COMMUTATOR = '<->'
);
CREATE OPERATOR <#> (
    LEFTARG = vector, RIGHTARG = vector,
    PROCEDURE = theodb_vector_neg_inner_product, COMMUTATOR = '<#>'
);
CREATE OPERATOR <=> (
    LEFTARG = vector, RIGHTARG = vector,
    PROCEDURE = theodb_vector_cosine_distance, COMMUTATOR = '<=>'
);

CREATE CAST (real[] AS vector)  WITH FUNCTION theodb_vector_from_real_array(real[]);
CREATE CAST (vector AS real[])  WITH FUNCTION theodb_vector_to_real_array(vector);
CREATE CAST (double precision[] AS vector) WITH FUNCTION theodb_vector_from_float8_array(double precision[]);
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
        let out = Spi::get_one::<String>("SELECT '[1,2,3]'::vector::text").unwrap().unwrap();
        assert_eq!(out, "[1,2,3]");
    }

    #[pg_test(error = "NaN not allowed in vector")]
    fn nan_rejected() {
        Spi::run("SELECT '[1,NaN,3]'::vector").unwrap();
    }

    #[pg_test(error = "vector must have at least 1 dimension")]
    fn dim0_rejected() {
        Spi::run("SELECT '[]'::vector").unwrap();
    }

    #[pg_test(error = "invalid input syntax for type vector: must start with \"[\"")]
    fn malformed_no_bracket() {
        Spi::run("SELECT '1,2,3'::vector").unwrap();
    }

    #[pg_test]
    fn dim_boundary() {
        // dim=1 (mínimo) e um vetor grande round-trip
        assert_eq!(Spi::get_one::<String>("SELECT '[1]'::vector::text").unwrap().unwrap(), "[1]");
        let big = format!("[{}]", (0..128).map(|i| i.to_string()).collect::<Vec<_>>().join(","));
        let n = Spi::get_one::<i64>(&format!("SELECT array_length(('{big}'::vector)::real[],1)"))
            .unwrap()
            .unwrap();
        assert_eq!(n, 128);
    }

    #[pg_test(error = "vector cannot have more than 16000 dimensions")]
    fn dim_over_max_rejected() {
        Spi::run("SELECT ('[' || array_to_string(array_fill(1.0::real, ARRAY[16001]), ',') || ']')::vector").unwrap();
    }

    #[pg_test]
    fn typmod_ok() {
        assert_eq!(
            Spi::get_one::<String>("SELECT '[1,2,3]'::vector(3)::text").unwrap().unwrap(),
            "[1,2,3]"
        );
    }

    #[pg_test(error = "expected 3 dimensions, not 2")]
    fn typmod_mismatch_on_column() {
        Spi::run("CREATE TEMP TABLE tt (e vector(3))").unwrap();
        Spi::run("INSERT INTO tt VALUES ('[1,2]')").unwrap();
    }

    #[pg_test]
    fn datum_roundtrip_no_uaf() {
        // EC-1: cast recebe E retorna o mesmo ptr, 1000× — pega double-free/UAF.
        Spi::run(
            "DO $$ BEGIN FOR i IN 1..1000 LOOP PERFORM ('[1,2,3]'::vector)::vector(3); END LOOP; END $$",
        )
        .unwrap();
    }

    #[pg_test]
    fn operators_match_kernels() {
        let l2 =
            Spi::get_one::<f64>("SELECT '[0,0]'::vector <-> '[3,4]'::vector").unwrap().unwrap();
        assert!((l2 - 5.0).abs() < 1e-6, "L2=5, got {l2}");
        let ip =
            Spi::get_one::<f64>("SELECT '[1,2]'::vector <#> '[3,4]'::vector").unwrap().unwrap();
        assert!((ip - (-11.0)).abs() < 1e-6, "neg-ip=-(1*3+2*4)=-11, got {ip}");
        let cos =
            Spi::get_one::<f64>("SELECT '[1,0]'::vector <=> '[0,1]'::vector").unwrap().unwrap();
        assert!((cos - 1.0).abs() < 1e-6, "cosine([1,0],[0,1])=1, got {cos}");
    }

    #[pg_test]
    fn casts_array_roundtrip() {
        let back = Spi::get_one::<String>("SELECT (ARRAY[1,2,3]::real[]::vector)::real[]::text")
            .unwrap()
            .unwrap();
        assert_eq!(back, "{1,2,3}");
        let f8 = Spi::get_one::<String>("SELECT (ARRAY[1.5,2.5]::float8[]::vector)::text")
            .unwrap()
            .unwrap();
        assert_eq!(f8, "[1.5,2.5]");
    }

    #[pg_test]
    fn table_column_and_order_by() {
        Spi::run("CREATE TEMP TABLE tv (id int, e vector(2))").unwrap();
        Spi::run("INSERT INTO tv VALUES (1,'[0,0]'),(2,'[1,1]'),(3,'[5,5]')").unwrap();
        let nearest =
            Spi::get_one::<i32>("SELECT id FROM tv ORDER BY e <-> '[0,0]'::vector LIMIT 1")
                .unwrap()
                .unwrap();
        assert_eq!(nearest, 1);
    }

    // M70: a paridade byte-a-byte com o pgvector foi provada e RELEASED no M69 (v0.59.0,
    // `binary_compat_with_pgvector` sobre `md5(vector_send)` em dims 1/3/5/128/300). No M70 o pgvector
    // é REMOVIDO — o tipo `vector` é 100% own-code, então esse teste (que exigia o pgvector coexistindo)
    // não roda mais na suíte standalone. A migração de instalações com pgvector (via intermediário `real[]`,
    // não byte-cast) está em `wiki/guides/pgvector-migration.md`; a paridade byte já está coberta no M69 v0.59.0.

    #[pg_test(error = "vector cannot have more than 16000 dimensions")]
    fn parse_fail_fast_over_max() {
        // M2: rejeita antes de acumular Vec gigante (checagem no loop)
        Spi::run("SELECT ('[' || array_to_string(array_fill(1.0::real, ARRAY[16050]), ',') || ']')::vector").unwrap();
    }

    #[pg_test(error = "invalid input syntax for type vector: \"\"")]
    fn parse_pathological_double_comma() {
        // M1: token vazio entre vírgulas (paridade: pgvector também rejeita)
        Spi::run("SELECT '[1,,3]'::vector").unwrap();
    }

    #[pg_test]
    fn copy_binary_roundtrip() {
        Spi::run("CREATE TEMP TABLE cb (e vector(3))").unwrap();
        Spi::run("INSERT INTO cb VALUES ('[1,2,3]'),(NULL),('[4,5,6]')").unwrap();
        // round-trip via COPY BINARY exercita recv/send (unused==0 no wire)
        Spi::run("CREATE TEMP TABLE cb2 (e vector(3))").unwrap();
        Spi::run("COPY cb TO '/tmp/theodb_cb.bin' WITH (FORMAT binary)").unwrap();
        Spi::run("COPY cb2 FROM '/tmp/theodb_cb.bin' WITH (FORMAT binary)").unwrap();
        let n =
            Spi::get_one::<i64>("SELECT count(*) FROM cb2 WHERE e IS NOT NULL").unwrap().unwrap();
        assert_eq!(n, 2, "COPY BINARY preservou 2 não-nulos");
        let first = Spi::get_one::<String>(
            "SELECT e::text FROM cb2 WHERE e IS NOT NULL ORDER BY e <-> '[0,0,0]'::vector LIMIT 1",
        )
        .unwrap()
        .unwrap();
        assert_eq!(first, "[1,2,3]");
    }
}
