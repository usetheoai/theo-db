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

// ---- ordem total: igualdade e comparação (B-033) ----
//
// Por que existe: sem estes operadores o PostgreSQL não sabe ordenar o tipo, e cinco padrões de app
// pgvector falham — `WHERE e = …`, `SELECT DISTINCT e`, `GROUP BY e`, `ORDER BY e` e `UNIQUE` sobre a
// coluna. Só o caminho ANN (`ORDER BY e <-> …`) funcionava. O `ADR-0029 § D2` promete drop-in "sem
// mudança de código", e a promessa falhava na CONSULTA — com mensagem do PostgreSQL que não cita
// TheoDB, então o usuário não descobria que trocou de implementação.
//
// SEMÂNTICA: paridade byte-a-byte com `vector_cmp_internal` do pgvector, consultado na fonte upstream
// (ADR D1 do plano `b033-vector-btree`). Compara elementos até `min(dim_a, dim_b)` e só então desempata
// por dimensão. NÃO chama `check_dims`: ao contrário das distâncias acima, comparar vetores de
// dimensões diferentes é legal e produz ordem, nunca erro.
//
// A escolha NÃO é estética. Duas alternativas foram rejeitadas por quebrarem coisas diferentes:
//   - dimensão como chave primária (minha suposição inicial) ordena diferente do pgvector, o que
//     consertaria a incompatibilidade antiga criando uma nova;
//   - igualdade com tolerância (`|a-b| < eps`) quebra TRANSITIVIDADE, e um btree sobre relação
//     não-transitiva corrompe em silêncio — a busca deixa de encontrar linhas que existem.
//
// PRÉ-CONDIÇÃO que torna isto uma ordem total: NaN e infinito são rejeitados na entrada (ver
// `theodb_vector_in` e o cast de array). Sem essa garantia, `partial_cmp` sobre f32 não seria total —
// NaN não é comparável nem a si mesmo — e o índice ficaria incoerente.
fn vector_cmp_internal(a: &Vector, b: &Vector) -> i32 {
    let (x, y) = (a.as_slice(), b.as_slice());
    for (l, r) in x.iter().zip(y.iter()) {
        // `partial_cmp` só devolve None sob NaN, impossível aqui pela pré-condição acima. O
        // `expect` documenta o invariante em vez de mascará-lo com um fallback silencioso.
        match l.partial_cmp(r).expect("NaN em vector: rejeitado na entrada, não deveria existir") {
            std::cmp::Ordering::Less => return -1,
            std::cmp::Ordering::Greater => return 1,
            std::cmp::Ordering::Equal => {}
        }
    }
    // Prefixo comum idêntico: o mais curto vem antes.
    x.len().cmp(&y.len()) as i32
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_cmp(a: Vector, b: Vector) -> i32 {
    vector_cmp_internal(&a, &b)
}

// Os seis operadores derivam da MESMA comparação (ADR D3): seis implementações independentes seriam
// duplicação de conhecimento, e uma divergência entre `=` e `cmp` produziria um btree incoerente com
// os operadores que o consultam.
#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_eq(a: Vector, b: Vector) -> bool {
    vector_cmp_internal(&a, &b) == 0
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_ne(a: Vector, b: Vector) -> bool {
    vector_cmp_internal(&a, &b) != 0
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_lt(a: Vector, b: Vector) -> bool {
    vector_cmp_internal(&a, &b) < 0
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_le(a: Vector, b: Vector) -> bool {
    vector_cmp_internal(&a, &b) <= 0
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_gt(a: Vector, b: Vector) -> bool {
    vector_cmp_internal(&a, &b) > 0
}

#[pg_extern(immutable, strict, parallel_safe)]
fn theodb_vector_ge(a: Vector, b: Vector) -> bool {
    vector_cmp_internal(&a, &b) >= 0
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

-- B-033 — os operadores de ORDEM. Família distinta dos de distância acima: aqueles alimentam as
-- opclasses dos AMs ANN, estes alimentam o btree. Adicionar não muda a resolução de nenhum caminho
-- existente.
--
-- COMMUTATOR/NEGATOR e as funções de seletividade replicam `pgvector/sql/vector.sql` verbatim: são o
-- que o planejador usa para reescrever predicados e estimar cardinalidade. Omiti-los não quebraria a
-- consulta, mas produziria planos piores em silêncio — que é a forma cara de errar aqui.
CREATE OPERATOR = (
    LEFTARG = vector, RIGHTARG = vector, PROCEDURE = theodb_vector_eq,
    COMMUTATOR = = , NEGATOR = <> , RESTRICT = eqsel, JOIN = eqjoinsel
);
CREATE OPERATOR <> (
    LEFTARG = vector, RIGHTARG = vector, PROCEDURE = theodb_vector_ne,
    COMMUTATOR = <> , NEGATOR = = , RESTRICT = eqsel, JOIN = eqjoinsel
);
CREATE OPERATOR < (
    LEFTARG = vector, RIGHTARG = vector, PROCEDURE = theodb_vector_lt,
    COMMUTATOR = > , NEGATOR = >= , RESTRICT = scalarltsel, JOIN = scalarltjoinsel
);
CREATE OPERATOR <= (
    LEFTARG = vector, RIGHTARG = vector, PROCEDURE = theodb_vector_le,
    COMMUTATOR = >= , NEGATOR = > , RESTRICT = scalarlesel, JOIN = scalarlejoinsel
);
CREATE OPERATOR > (
    LEFTARG = vector, RIGHTARG = vector, PROCEDURE = theodb_vector_gt,
    COMMUTATOR = < , NEGATOR = <= , RESTRICT = scalargtsel, JOIN = scalargtjoinsel
);
CREATE OPERATOR >= (
    LEFTARG = vector, RIGHTARG = vector, PROCEDURE = theodb_vector_ge,
    COMMUTATOR = <= , NEGATOR = < , RESTRICT = scalargesel, JOIN = scalargejoinsel
);

-- `DEFAULT` é o que faz `CREATE UNIQUE INDEX ON t (e)` e `ORDER BY e` funcionarem sem o usuário
-- nomear a opclass. O nome `vector_ops` não colide com as `vector_l2_ops`/`vector_cosine_ops` do shim:
-- nomes de opclass são únicos POR MÉTODO DE ACESSO, e aquelas são do AM `hnsw`.
CREATE OPERATOR CLASS vector_ops
    DEFAULT FOR TYPE vector USING btree AS
    OPERATOR 1 < ,
    OPERATOR 2 <= ,
    OPERATOR 3 = ,
    OPERATOR 4 >= ,
    OPERATOR 5 > ,
    FUNCTION 1 theodb_vector_cmp(vector, vector);

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
        // B-033 — sem estas 7 arestas o pgrx pode emitir os CREATE OPERATOR antes das funções que
        // eles referenciam, e o CREATE EXTENSION falha. É a classe de defeito que não aparece na
        // compilação, só na instalação.
        theodb_vector_cmp,
        theodb_vector_eq,
        theodb_vector_ne,
        theodb_vector_lt,
        theodb_vector_le,
        theodb_vector_gt,
        theodb_vector_ge,
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

    // ---- B-033: ordem total do tipo vector ----

    /// T1.1 — `vector_cmp` replica `vector_cmp_internal` do pgvector.
    ///
    /// O quarto caso é o que separa a semântica correta da suposição com que este trabalho começou:
    /// `[1,3]` vs `[1,2,9]` devolve **1** porque o SEGUNDO elemento já decide. Se a dimensão fosse a
    /// chave primária, `[1,3]` (dim 2) viria antes de `[1,2,9]` (dim 3) e o resultado seria -1.
    #[pg_test]
    fn cmp_matches_upstream_semantics() {
        let c = |a: &str, b: &str| {
            Spi::get_one_with_args::<i32>(
                "SELECT theodb_vector_cmp($1::vector, $2::vector)",
                &[a.into(), b.into()],
            )
            .unwrap_or_else(|e| panic!("cmp({a},{b}) falhou: {e:?}"))
            .expect("cmp devolveu NULL para entrada não-nula")
        };
        assert_eq!(c("[1,2]", "[1,3]"), -1, "elemento menor decide");
        assert_eq!(c("[1,2]", "[1,2]"), 0, "iguais");
        assert_eq!(c("[1,2]", "[1,2,0]"), -1, "prefixo igual: o mais curto vem antes");
        assert_eq!(c("[1,3]", "[1,2,9]"), 1, "o ELEMENTO decide antes da dimensão");
        assert_eq!(c("[-5]", "[1]"), -1, "negativos ordenam corretamente");
    }

    /// T1.1 — a comparação é uma ordem TOTAL, que é a pré-condição do btree.
    ///
    /// Se falhar, o índice não fica "um pouco errado": ele deixa de encontrar linhas que existem, em
    /// silêncio. O conjunto inclui dimensões diferentes de propósito — é onde uma implementação
    /// ingênua quebra a antissimetria.
    #[pg_test]
    fn cmp_is_a_total_order() {
        let vs = ["[1,2]", "[1,3]", "[1,2,0]", "[-5]", "[1]", "[9,9,9]"];
        let c = |a: &str, b: &str| {
            Spi::get_one_with_args::<i32>(
                "SELECT theodb_vector_cmp($1::vector, $2::vector)",
                &[a.into(), b.into()],
            )
            .unwrap()
            .unwrap()
        };
        for a in vs {
            assert_eq!(c(a, a), 0, "reflexividade falhou em {a}");
        }
        for a in vs {
            for b in vs {
                assert_eq!(
                    c(a, b).signum(),
                    -c(b, a).signum(),
                    "antissimetria falhou entre {a} e {b}"
                );
            }
        }
        for a in vs {
            for b in vs {
                for k in vs {
                    if c(a, b) <= 0 && c(b, k) <= 0 {
                        assert!(c(a, k) <= 0, "transitividade falhou: {a} <= {b} <= {k}");
                    }
                }
            }
        }
    }

    /// T1.3 — os cinco padrões que o B-033 mediu falhando. Os MESMOS cinco, não um genérico.
    #[pg_test]
    fn pgvector_query_patterns_work() {
        Spi::run(
            "CREATE TABLE b033 (id int, e vector(3));
             INSERT INTO b033 VALUES (1,'[1,2,3]'),(2,'[1,2,3]'),(3,'[9,9,9]');",
        )
        .unwrap();

        let eq = Spi::get_one::<i64>("SELECT count(*) FROM b033 WHERE e = '[1,2,3]'::vector")
            .unwrap()
            .unwrap();
        assert_eq!(eq, 2, "WHERE e = ...");

        let distinct = Spi::get_one::<i64>("SELECT count(*) FROM (SELECT DISTINCT e FROM b033) x")
            .unwrap()
            .unwrap();
        assert_eq!(distinct, 2, "SELECT DISTINCT e");

        let grouped = Spi::get_one::<i64>("SELECT count(*) FROM (SELECT e FROM b033 GROUP BY e) x")
            .unwrap()
            .unwrap();
        assert_eq!(grouped, 2, "GROUP BY e");

        let first =
            Spi::get_one::<String>("SELECT e::text FROM b033 ORDER BY e LIMIT 1").unwrap().unwrap();
        assert_eq!(first, "[1,2,3]", "ORDER BY e");

        // O quinto: a opclass DEFAULT existe, então o índice é criável sem nomeá-la. Sobre uma
        // tabela SEM duplicata, para provar que CONSTRÓI (a rejeição de duplicata é o teste seguinte).
        Spi::run(
            "CREATE TABLE b033u (e vector(3)); INSERT INTO b033u VALUES ('[1,2,3]'),('[9,9,9]');",
        )
        .unwrap();
        Spi::run("CREATE UNIQUE INDEX b033u_ix ON b033u (e)").unwrap();
    }

    /// T1.4 — a opclass btree NÃO rouba o caminho ANN.
    ///
    /// Adicionar `=` e `<` ao tipo dá ao planejador alternativas que ele não tinha. O risco não é erro,
    /// é REGRESSÃO SILENCIOSA: uma consulta de similaridade passar a resolver por outro caminho e ficar
    /// lenta sem ninguém notar. Este teste é rede, não alvo — deve passar antes e depois da mudança.
    #[pg_test]
    fn ann_path_still_uses_the_ann_index() {
        Spi::run(
            "CREATE TABLE b033ann (id int, e vector(3));
             INSERT INTO b033ann SELECT g, ('['||g||','||(g*2)||','||(g*3)||']')::vector
               FROM generate_series(1,200) g;
             CREATE INDEX b033ann_ix ON b033ann USING theodb_hnsw (e theodb_hnsw_l2_ops);
             SET enable_seqscan = off;",
        )
        .unwrap();

        let mut linhas = String::new();
        Spi::connect(|c| {
            let t = c
                .select(
                    "EXPLAIN (COSTS OFF) SELECT id FROM b033ann ORDER BY e <-> '[10,20,30]'::vector LIMIT 3",
                    None,
                    &[],
                )
                .unwrap();
            for row in t {
                if let Ok(Some(l)) = row.get::<String>(1) {
                    linhas.push_str(&l);
                    linhas.push('\n');
                }
            }
        });

        assert!(
            linhas.contains("b033ann_ix"),
            "a consulta ANN deixou de usar o índice `theodb_hnsw`; plano obtido:\n{linhas}"
        );
    }

    /// T1.3 — o índice único REJEITA duplicata. Construir sem rejeitar não provaria nada.
    ///
    /// Usa `#[pg_test(error = …)]` e não `assert!(result.is_err())`: no pgrx um `ERROR` do PostgreSQL
    /// não retorna como `Err`, ele faz longjmp e aborta a transação — o `assert` nunca chegaria a ser
    /// avaliado. A primeira versão deste teste cometeu esse erro e reprovou **com o produto correto**,
    /// mostrando no log exatamente a violação que ela queria provar. É o idioma que o resto deste
    /// arquivo já usa em sete testes.
    ///
    /// A mensagem é a COMPLETA, com o nome do índice: o pgrx compara por igualdade exata
    /// (`framework.rs:174`, `Some(received) == expected`), não por conter. Uma primeira tentativa com
    /// só o prefixo reprovou de novo — com o produto certo pela segunda vez. Por isso o índice é
    /// nomeado à mão: assim a string do erro fica sob controle deste teste, em vez de depender da
    /// convenção de nomes automática do PostgreSQL.
    #[pg_test(error = "duplicate key value violates unique constraint \"b033d_uq\"")]
    fn unique_index_rejects_duplicate() {
        Spi::run(
            "CREATE TABLE b033d (e vector(3));
             CREATE UNIQUE INDEX b033d_uq ON b033d (e);
             INSERT INTO b033d VALUES ('[4,5,6]');",
        )
        .unwrap();
        Spi::run("INSERT INTO b033d VALUES ('[4,5,6]')").unwrap();
    }
}
