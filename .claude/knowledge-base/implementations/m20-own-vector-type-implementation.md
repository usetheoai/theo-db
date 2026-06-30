# Implementation summary: m20-own-vector-type

**Plan:** `knowledge-base/plans/m20-own-vector-type-plan.md` (plan-confidence SHIPPABLE)
**Blueprint:** `knowledge-base/discoveries/blueprints/m20-own-vector-type-blueprint.md` (discover-confidence 97.3)
**milestone_id:** M20
**Branch:** develop
**Commit:** df12554
**Status:** IMPLEMENTATION_COMPLETE

## Goal

Own Rust f32-parity distance functions (`theodb.l2_distance`/`inner_product`/`cosine_distance`) over
pgvector's binary `vector` layout, proven byte-identical (within f32 SIMD tolerance) to pgvector on its
regression oracle + a reproducible benchmark.

## What shipped, per task

### Phase 1+2 — `theodb_rs/src/vec.rs` (the f32-parity distance math)
- `l2_distance`/`inner_product`/`cosine_distance(&[f32], &[f32]) -> f64` matching pgvector's `vector.c`:
  **f32 accumulation** (the bit determinant — ADR D2), `sqrt`/divide in f64, cosine clamps to [-1,1] then
  `1.0 - sim`. `check_dims` → typed 22023 on mismatch (parity with pgvector `CheckDims`). The `<#>` distance
  is `-inner_product` (a sign, not a separate algorithm — KISS).
- **Parsimony refinement of ADR D1 (GREEN-phase deliberation):** instead of an `unsafe #[repr(C)]` FFI struct
  (the blueprint's observation — pgvectorscale needs it only as an index AM on raw datums), the ops take
  pgrx-native `Vec<f32>` mapped from pgvector's lossless `vector::real[]` cast (`pgvector/sql/vector.sql:157`).
  Same coexistence + exact f32 values + f32-parity, with **zero unsafe** and pgrx handling detoast — which
  **closes edge-case EC-1** (TOAST) without any unsafe code. Documented as a refinement, not a deviation
  (ADR D1's INTENT — coexistence, read pgvector's values, no competing type — is fully met).
- Rust `#[pg_test]` unit tests (oracle values + dim=1 boundary + an f32-vs-f64-accumulation discriminating
  test proving f32 accumulation + dim-mismatch 22023).

### Phase 3 — `theodb_rs/src/lib.rs` (SQL surface, coexistence)
- 3 `#[pg_extern]` (`_vec_l2`/`_vec_ip`/`_vec_cosine`, `Vec<f32>`) + `extension_sql!` creating
  `theodb.l2_distance`/`inner_product`/`cosine_distance(vector, vector) RETURNS float8`, `LANGUAGE sql
  IMMUTABLE STRICT PARALLEL SAFE`, casting `a::real[]`. **Coexistence (ADR D1):** NEW functions — pgvector's
  `<->`/`<#>`/`<=>` operators + type + HNSW/IVFFlat/DiskANN indexes + embed/hybrid/import are untouched.
  REVOKE-from-PUBLIC parity.
- **Wiring triad:** caller = the `theodb.*` SQL wrappers; integration test = `test_vector_ops.py`;
  observability = typed 22023 on dim-mismatch + STRICT NULL handling.

### Phase 4 — `benchmarks/tests/test_vector_ops.py` (parity gate)
- 17 tests: `theodb.*` vs LIVE pgvector on oracle rows + boundaries (dim=1, high-dim 1536 & 16000, NaN/inf,
  dim-mismatch raises, NULL→NULL, `<#>` = `-inner_product`, REVOKE). Parity asserted within `REL_TOL=1e-5`
  (ADR D3 — pgvector's SIMD reorders the f32 sum; observed divergence ~1e-6, pure low-bit noise).

### Phase 5 — benchmark (`benchmarks/bench_vector_ops.py` + `docs/benchmarks/m20-vector-ops-parity.md`)
- Reproducible: max REL |Δ| ~1e-6 across all 3 ops (numeric parity PROVEN) + perf ~3× scalar-vs-SIMD
  (honest — M20 is parity, not beating pgvector's SIMD; SIMD is M21+). Records the live pgvector version
  (0.8.3, CK-1). Gates on parity (exit 1 if rel |Δ| > tolerance).

### Migration decision (M20 DoD)
- **COEXISTENCE** (ADR D1): TheoDB owns the distance computation in Rust at f32-parity, reading pgvector's
  values via `::real[]`; pgvector's type/operators/indexes stay. Data-compat: total (no on-disk change).

## Validation evidence

- **161 passed, 4 skipped** full SQL integration suite (vector_ops 17 + nl/hybrid/unified/embed/ai/import/
  install/retirement) against `theo-db:m20` — zero regression.
- Benchmark: numeric parity ~1e-6 rel (proven), perf delta documented honestly.
- code-quality skill verdict PASS (languages config empty — substantive gate is clippy, below).
- No new dependency (pgrx + std).

## ADRs honored
- D1 coexistence (FUNCTIONS, no competing type/operator) — refined to `::real[]`/`Vec<f32>` (no unsafe).
- D2 f32 accumulation (proven by the discriminating unit test + the bench parity).
- D3 parity to text within f32-SIMD tolerance (no false bit-exact claim).
