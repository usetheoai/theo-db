# Blueprint: M20 — Own `vector` type + distance operators in Rust/pgrx (pgvector parity)

> **Version 1.0** — How to implement TheoDB's own `vector` distance operators (and an own Rust representation
> of the type) at **numeric parity** with pgvector, by reading the SOTA (pgvector C) + two pgrx peers
> (pgvectorscale, vectorchord). Headline finding: **both SOTA pgrx peers FFI-wrap pgvector's exact `#[repr(C)]`
> binary layout and accumulate distances in `f32` — neither forks the type.** Therefore the evidence-backed M20
> decision is **COEXISTENCE** (own Rust ops + an `#[repr(C)]` binary-compatible representation that reads
> pgvector's on-disk bytes), NOT substitution (a competing type would fork data + break HNSW/IVFFlat +
> pgvectorscale DiskANN + the `theodb.embed`/`hybrid`/`import` surface). This blueprint informs the M20 plan.

**Slug:** `m20-own-vector-type`
**Source plan:** `.claude/knowledge-base/discoveries/plans/m20-own-vector-type-plan.md`
**Owner:** paulohenriquevn
**Generated:** 2026-06-30 via `/discover-execute` (protocol executed in-context; bounded 7-question read+synthesize)
**Confidence verdict:** TBD (updated by `/discover-confidence`)

## Context

M20 (ROADMAP-v2.md:104) wants an own `vector` type + 3 distance ops (`<=>`/`<->`/`<#>`) in Rust at numeric
parity, **measurement-first** — "só substitui pgvector quando a paridade for provada". The whole TheoDB
surface is built on pgvector's `vector`. This blueprint reads how pgvector lays out the type + computes
distances (the parity spec), and how the two SOTA pgrx peers represent vectors + wire operators in Rust.

**CK-1 (version read):** the pgvector clone investigated is **v0.8.3** (`references/pgvector/META.json`). The
`vector` binary format (`vl_len_`/`dim`/`unused`/`float[]`) and the distance formulas have been stable since
≤0.5; IMPLEMENT MUST cross-check the `theo-db` image's installed pgvector version and confirm send/recv +
formulas unchanged (the parity guarantee). **CK-4 (scope):** only the `vector` (float4) type + its 3 ops are
in scope; `halfvec`/`sparsevec`/`bit` sibling types are excluded.

## Objective

Decide HOW to implement the own Rust `vector` ops at numeric parity and WHETHER to coexist vs substitute —
with evidence. This blueprint answers that: **coexist via a binary-compatible `#[repr(C)]` representation +
own Rust f32-accumulating distance functions**, and hands the implementation shape to `/to-plan`.

---

## Coverage Corner 1 — Integration Tests

### pgvector — the numeric-parity ORACLE (Q4)

pgvector's regression suite is the bit-exact parity oracle TheoDB will replay against its Rust ops.

- **Pattern**: SQL-in / expected-out golden files. Distance assertions live in
  `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:89-98` — `l2_distance`×5,
  `inner_product`, `cosine_distance` rows — with expected results in
  `.claude/knowledge-base/references/pgvector/test/expected/vector_type.out`.
- **Replayable rows** (the parity fixtures): `l2_distance('[0,0]','[3,4]')` (=5), `l2_distance('[0,0]','[0,1]')`
  (=1), `inner_product('[1,2]','[3,4]')` (=11), plus dim-mismatch + overflow rows
  (`l2_distance('[1,2]','[3]')` → error; `l2_distance('[3e38]','[-3e38]')` → overflow behavior). Source:
  `.claude/knowledge-base/references/pgvector/test/sql/vector_type.sql:89-93`.
- **Coverage TheoDB will mirror**: the M20 parity suite re-issues these exact inputs against TheoDB's own ops
  and asserts byte-identical text output to pgvector's `.out` (modulo SIMD low-bit variance — see Techniques).

### vectorchord + pgvectorscale — peer test shape (Q5)

- **vectorchord** uses **sqllogictest (`.slt`)** golden files:
  `.claude/knowledge-base/references/vectorchord/tests/general/distance.slt` (distance assertions) and
  `.claude/knowledge-base/references/vectorchord/tests/general/vector.slt` (type ops). SLT = SQL + expected
  result blocks, runner-agnostic.
- **pgvectorscale** uses **Python pytest against a running container**:
  `.claude/knowledge-base/references/pgvectorscale/tests/test_basic_operations.py` (+
  `tests/test_concurrent_inserts.py`) — psycopg-style asserts, the SAME shape as TheoDB's existing
  `benchmarks/tests/test_*.py` suite.
- **Decision for M20**: mirror pgvectorscale's pytest-against-container shape (TheoDB already runs that) for the
  parity suite, replaying pgvector's oracle rows; add Rust `#[pg_test]` unit tests for the pure distance fns
  (matches theodb_rs's existing `#[pg_test]` convention).

---

## Coverage Corner 2 — Dependencies

| Project | pgrx pin | Extra crates for the type/ops? | License |
|---|---|---|---|
| theodb_rs (us) | `=0.16.1` (current) | — | Apache-2.0 (D1 permissive) |
| pgvectorscale | `=0.16.1` (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/Cargo.toml:31`) | none for the FFI `#[repr(C)]` struct — uses `pgrx` + `std::mem::MaybeUninit` only (`pg_vector.rs:2`) | PostgreSQL License |
| vectorchord | `=0.17.0` (`.claude/knowledge-base/references/vectorchord/Cargo.toml:43`) | own `VectorHeader`/`VectorInput` via `pgrx` + `std` (`src/datatype/memory_vector.rs`) | AGPL — **read-for-technique only (D1 bars AGPL in the package)** |

- **Finding**: the FFI-wrap pattern needs **NO crate beyond pgrx + std** — pgvectorscale (our exact pgrx 0.16.1)
  proves the `#[repr(C)]` struct + `__IncompleteArrayField<c_float>` + `MaybeUninit<i16>` works with pgrx 0.16.1.
- **License caveat (honest)**: vectorchord is **AGPL** — usable as a *technique reference* (read-only) but its
  CODE must NOT be copied into the Apache-2.0 package (CLAUDE.md TheoDB rule 2 / D1). The implementable pattern
  comes from **pgvectorscale (PostgreSQL License — permissive)**, which is also our exact pgrx version. This
  feeds `/deps-audit` at PLAN: M20 likely adds ZERO new dependencies.

---

## Coverage Corner 3 — Tools

- **pgvectorscale**: Python pytest against a container (`tests/test_basic_operations.py`) + `cargo pgrx test`
  for Rust unit tests (pgrx 0.16.1 harness — same as theodb_rs).
- **vectorchord**: sqllogictest runner over `.slt` files (`tests/general/*.slt`).
- **Adoption for M20**: reuse TheoDB's existing dual harness — (a) `benchmarks/tests/test_*.py` pytest against
  the built `theo-db` image (the OBSERVABLE parity gate, replaying pgvector's oracle), and (b) `cargo pgrx test`
  for the pure-fn Rust unit tests (note the known CI limitation: `#[pg_test]` needs a pgrx-managed pg, so the
  container pytest is the gate that actually runs in CI — same as M18/M19). No new tooling required.

---

## Coverage Corner 4 — Techniques

### Q1 — pgvector `vector` binary layout (the format any parity-preserving Rust type must read)

`.claude/knowledge-base/references/pgvector/src/vector.h:11-17`:

```c
typedef struct Vector {
    int32 vl_len_;   /* varlena header (do not touch directly!) */
    int16 dim;       /* number of dimensions */
    int16 unused;    /* reserved, always zero */
    float x[FLEXIBLE_ARRAY_MEMBER];   /* dim × float4 payload */
} Vector;
```

- `VECTOR_SIZE(dim) = offsetof(Vector, x) + sizeof(float)*dim` (`vector.h:6`); `VECTOR_MAX_DIM 16000`
  (`vector.h:4`). Binary recv/send order: `dim` (int16), `unused` (int16), then `dim` float4
  (`vector.c:370-415`).
- **Parity requirement**: a Rust representation MUST be `#[repr(C)]` with this exact field order to read/write
  pgvector's bytes byte-for-byte.

### Q2 — distance formulas + accumulation order (THE parity determinant — CK-3 CORRECTED)

From `.claude/knowledge-base/references/pgvector/src/vector.c`:

| Op | Function (line) | Accumulator | Loop | Post |
|---|---|---|---|---|
| `<->` L2 | `VectorL2SquaredDistance` (`:554-568`) → `l2_distance` (`:573-583`) | **`float` (f32)** | `distance += diff*diff` (diff = `ax[i]-bx[i]`, f32) | `sqrt((double) squared)` |
| `<#>` IP | `VectorInnerProduct` (`:601-611`) → `vector_negative_inner_product` (`:631-641`) | **`float` (f32)** | `distance += ax[i]*bx[i]` | `(double) -distance` |
| `<=>` cosine | `VectorCosineSimilarity` (`:643-660`) → `cosine_distance` (`:665-689`) | **`float` (f32)** sums (similarity, norma, normb) | `similarity += a*b; norma += a*a; normb += b*b` | `(double)sim / sqrt((double)norma*(double)normb)`, clamp to [-1,1], return `1.0 - sim` |

- **CK-3 CORRECTION (parity-critical):** pgvector stores `float4` AND **accumulates the distance sums in `float`
  (f32)** — NOT `double`. The `(double)` casts are only on the FINAL `sqrt`/division. A Rust port MUST
  accumulate in **`f32`** (e.g. `iter().map(...).sum::<f32>()`) to match; using `f64` accumulation would
  DIVERGE. This is the single most important parity fact and is only visible by reading the loop bodies
  (validates plan ADR D2).
- **Cosine special-cases** (`vector.c:677-688`): clamp similarity to [-1,1] AFTER computing; MSVC-only explicit
  NaN propagation; zero-norm → division yields inf/nan (no explicit guard in the core path). A parity port
  replicates: f32 sums → f64 divide → clamp → `1.0 - sim`.
- **SIMD honesty**: pgvector marks these `VECTOR_TARGET_CLONES` (auto-vectorized / SIMD target clones). SIMD
  changes summation ORDER, so pgvector's own output can vary in the lowest f32 bits across `-march`. "Numeric
  parity" therefore means matching the **scalar f32-accumulation algorithm**; bit-exactness vs a SIMD build is
  best-effort (the parity suite should assert to pgvector's text output, which rounds, absorbing low-bit noise).

### Q3 — pgrx peers: representation + operator wiring (coexistence pattern, CK-2)

- **pgvectorscale** (`.claude/knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/pg_vector.rs:9-24`)
  defines `#[repr(C)] struct PgVectorInternal { vl_len_: i32, dim: i16, unused: MaybeUninit<i16>, x:
  __IncompleteArrayField<c_float> }` — explicitly "**Ported from pg_vector code**" — and `to_slice(&self) ->
  &[f32]` reads pgvector's payload directly. Its Rust distance fns accumulate in **f32**
  (`distance/mod.rs:212` `inner_product_unoptimized(...) -> f32` via `.sum()`; `:217` `distance_cosine_unoptimized`),
  matching pgvector. This is the binary-compatible coexistence pattern: **read pgvector's bytes via FFI, don't
  fork the type.**
- **vectorchord** (`.claude/knowledge-base/references/vectorchord/src/datatype/memory_vector.rs:24,49`)
  independently confirms it: `#[repr(C)] struct VectorHeader { varlena: u32, dim: u16, unused: u16, elements:
  [f32;0] }` + `VectorInput<'a>(NonNull<VectorHeader>, …)` (FromDatum at `:145`) — an FFI wrapper over
  pgvector's binary format. **CK-2:** vectorchord ALSO defines a separate `sphere_vector` COMPOSITE type +
  `_vchord_vector_sphere_*` operators (`src/datatype/operators_vector.rs:22-90`,
  `sql/install/vchord--1.1.1.sql:730-780`) — that is a DISTINCT range-query feature, OUT of M20 scope; the
  core vector parity pattern is `VectorInput`, not `sphere_vector`.
- **Operator/opclass wiring**: pgvector declares `CREATE OPERATOR <-> / <#> / <=>`
  (`.claude/knowledge-base/references/pgvector/sql/vector.sql:174,179,184`) bound to `l2_distance` /
  `vector_negative_inner_product` / `cosine_distance`, and opclasses `vector_l2_ops`/`vector_ip_ops`/
  `vector_cosine_ops`. In pgrx, the idiom is `#[pg_extern]` distance fns + `extension_sql!` declaring the
  operators/opclasses (vectorchord: `operators_vector.rs` + `sql/install/...`; pgvectorscale opclasses:
  `.claude/knowledge-base/references/pgvectorscale/pgvectorscale/sql/vectorscale--0.8.0--0.9.0.sql:120-145`) —
  exactly theodb_rs's existing `#[pg_extern]` + `extension_sql!` pattern (M17-M19).

---

## Cross-cutting Comparison

Side-by-side of the three reference implementations on the dimensions M20 must decide:

| Dimension | pgvector (C, SOTA) | pgvectorscale (Rust/pgrx 0.16.1) | vectorchord (Rust/pgrx 0.17.0) |
|---|---|---|---|
| Type representation | `struct Vector { int32 vl_len_; int16 dim; int16 unused; float x[] }` (`vector.h:11-17`) | `#[repr(C)] PgVectorInternal { i32, i16, MaybeUninit<i16>, __IncompleteArrayField<c_float> }` — "Ported from pg_vector" (`pg_vector.rs:9-16`) | `#[repr(C)] VectorHeader { u32, u16, u16, [f32;0] }` + `VectorInput` FromDatum (`memory_vector.rs:24,49,145`) |
| Forks the type? | n/a (defines it) | **No** — FFI-wraps pgvector's bytes (`to_slice()->&[f32]`) | **No** — `VectorInput` FFI-wraps pgvector's binary format |
| Distance accumulator | **`float` (f32)** sums; `sqrt`/divide in `double` (`vector.c:557,604,646`) | **`f32`** (`.sum::<f32>()`, `distance/mod.rs:212,217`) | (uses pgvector format; index-side quantization out of scope) |
| Operator wiring | `CREATE OPERATOR <->/<#>/<=>` + opclasses (`sql/vector.sql:174,179,184`) | opclasses in `vectorscale--0.8.0--0.9.0.sql:120-145` | `#[pg_extern]` + `extension_sql!` (`operators_vector.rs`, `vchord--1.1.1.sql:760-780`) |
| License | PostgreSQL License (permissive) | PostgreSQL License (permissive) | **AGPL** (read-for-technique only; D1 bars in-package) |
| Test harness | SQL golden `.out` (`test/expected/vector_type.out`) | pytest-against-container (`tests/test_basic_operations.py`) | sqllogictest `.slt` (`tests/general/distance.slt`) |

**Convergent finding:** the two independent pgrx peers BOTH chose binary-compatible FFI over a competing type,
and BOTH accumulate in f32 — strong evidence for the M20 coexistence + f32-parity decision. The implementable
(permissive) model is **pgvectorscale** (our exact pgrx 0.16.1); vectorchord corroborates the pattern but its
AGPL code is reference-only.

---

## ADRs

### D1 — COEXISTENCE, not substitution (the migration decision — M20 DoD)

**Decision:** TheoDB implements its own Rust distance ops (`<->`/`<#>`/`<=>`) + an `#[repr(C)]` binary-compatible
representation that reads pgvector's exact on-disk `vector` bytes. It does **NOT** define a competing storage
type and does **NOT** replace pgvector's `vector`.

**Evidence:** both SOTA pgrx peers (pgvectorscale — our exact pgrx 0.16.1, PostgreSQL-License; vectorchord)
FFI-wrap pgvector's `#[repr(C)]` layout rather than fork the type (Q3). A competing type would fork existing
data and break HNSW/IVFFlat + pgvectorscale DiskANN indexes + `theodb.embed`/`hybrid`/`import` (all built on
pgvector `vector`). Coexistence reduces dependency on pgvector's *operator code* (TheoDB owns the f32-parity
computation in Rust) while staying 100% binary-compatible — the honest reading of "reduzir dependência do
pgvector" (measurement-first).

**Alternatives:** (a) own competing type — REJECTED (forks data + breaks indexes, contradicts parsimony ladder);
(b) keep pgvector ops untouched — REJECTED (delivers no M20 own-code). 

### D2 — f32 accumulation for bit-parity (not f64)

**Decision:** the Rust ops accumulate sums in **`f32`** (`.sum::<f32>()`), `sqrt`/division in `f64`, clamp
cosine to [-1,1], matching pgvector's `vector.c` exactly.

**Evidence:** pgvector accumulates in `float` (Q2, `vector.c:557/604/646`); pgvectorscale's Rust does the same
(`distance/mod.rs:212`). f64 accumulation would diverge from the oracle.

### D3 — parity is to pgvector's TEXT output (SIMD low-bit honesty)

**Decision:** the parity suite asserts TheoDB ops == pgvector's regression `.out` text values; bit-exactness vs
a SIMD-vectorized pgvector build is best-effort (documented), since `VECTOR_TARGET_CLONES` reorders f32 sums.

**Evidence:** Q2 SIMD note. Avoids a false "bit-exact" claim (public-copy.md / honesty).

## Blocked questions (if any)

None — Q1-Q7 all answered with verified citations.

## Recommendations (for /to-plan)

1. **Implement** `theodb_rs` Rust distance fns (`l2`/`ip`/`cosine`) reading a `#[repr(C)]` pgvector-compatible
   struct (pgvectorscale `PgVectorInternal` pattern), f32-accumulating (ADR-2), exposed as `#[pg_extern]` +
   `extension_sql!` operators/opclasses in an own schema/namespace that can coexist with pgvector's.
2. **Parity gate**: a pytest-against-container suite replaying pgvector's `vector_type.sql` oracle rows +
   `cargo pgrx test` unit tests for the pure fns.
3. **Benchmark** (M20 DoD): reproducible numeric-parity proof (own vs pgvector on the oracle rows) + a perf
   delta (own f32-scalar vs pgvector SIMD) — mean±std ≥3 runs, `docs/benchmarks/` (currently `UNBENCHMARKED`).
4. **Migration decision = coexistence** (ADR-1) — document in the plan; do NOT replace pgvector's type/indexes.
5. **No new dependency** expected (Q6) — `/deps-audit` should confirm pgrx + std suffice.
