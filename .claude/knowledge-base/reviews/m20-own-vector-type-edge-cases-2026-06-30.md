# Edge Case Review — m20-own-vector-type

Date: 2026-06-30
Tasks analyzed: 6 (T1.1, T1.2, T2.1, T3.1, T4.1, T5.1)
Cases found: 5 (EDGE: 3, NEGATIVE: 2 | MUST FIX: 1, SHOULD TEST: 3, DOCUMENT: 1)

The plan already covers dim-mismatch (22023), NULL, zero-norm cosine, SIMD-text parity (ADR D3), version
drift (CK-1), and FFI-read safety as a HIGH risk. One MUST-FIX surfaced: the datum may be TOASTed.

## MUST FIX

### EC-1: the `vector` datum may be TOASTed (compressed/external) — reader MUST detoast
- **Affected task:** T1.1 (reader), T3.1 (`#[pg_extern]` receiving the datum)
- **Kind:** NEGATIVE (invalid assumption → memory corruption)
- **Family:** Format / Resource
- **Scenario:** PostgreSQL may store a `vector` column value TOASTed (compressed or out-of-line) for large
  dims. Casting the RAW datum pointer to `#[repr(C)] PgVectorBytes` and reading `x[]` without detoasting reads
  compressed/garbage bytes → wrong result or segfault. pgvector's own `DatumGetVector(x)` macro is
  `((Vector *) PG_DETOAST_DATUM(x))` (`references/pgvector/src/vector.h:7`) — it ALWAYS detoasts.
- **Impact:** crash / wrong distance on large or compressed vectors (silent data corruption in results).
- **Suggested fix:** the reader detoasts first — receive the arg via pgrx as a detoasted varlena (e.g. accept
  the pgrx `vector` arg through a `FromDatum` that calls `pg_sys::pg_detoast_datum`, mirroring pgvector's
  `DatumGetVector` + pgvectorscale's `PgVector::from_datum`), THEN cast to the struct. Add to T1.1's reader.

## SHOULD TEST

### EC-2: minimum valid dim (dim=1) boundary
- **Affected task:** T2.1, T4.1
- **Kind:** EDGE
- **Suggested test:** `test_l2_dim1_boundary` — assert `theodb.l2_distance('[3]','[0]') == pgvector` (=3); the
  smallest valid vector still computes correctly.

### EC-3: maximum dim (VECTOR_MAX_DIM=16000) boundary
- **Affected task:** T4.1
- **Kind:** EDGE
- **Suggested test:** add a max-dim (or near-max, e.g. 16000) parity row to `test_highdim_*` — assert
  `theodb.*` == pgvector at the dim ceiling (no overflow / truncation in the f32 sum). The plan's 1536 row
  becomes one of two boundary rows.

### EC-4: NaN / inf component in an input vector
- **Affected task:** T2.1, T4.1
- **Kind:** NEGATIVE (degenerate-but-acceptable input)
- **Suggested test:** `test_nan_inf_parity` — assert `theodb.*('[NaN]','[1]')` and `'[3e38]','[3e38]'`
  (overflow → inf) match pgvector's output exactly (parity-to-live pgvector — neither masks NaN/inf).

## DOCUMENT

### EC-5: SIMD low-bit divergence
- **Kind:** EDGE
- **Accepted risk:** already ADR D3 — parity asserted to pgvector's rounded TEXT output; bit-exact vs a SIMD
  build is best-effort. No action beyond the documented ADR.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 0 | 1 (EC-1) | 1 | 0 | 0 |
| T1.2 | 0 | 0 | 0 | 0 | 0 |
| T2.1 | 1 (EC-2) | 1 (EC-4) | 0 | 2 | 0 |
| T3.1 | 0 | 1 (EC-1) | (EC-1) | 0 | 0 |
| T4.1 | 2 (EC-2,EC-3) | 1 (EC-4) | 0 | 3 | 0 |
| T5.1 | 0 | 0 | 0 | 0 | 1 (EC-5) |

**Coverage check:** every input-boundary task has an EDGE + a NEGATIVE case considered.

**Verdict:** PLAN NEEDS ADJUSTMENT (absorb EC-1 MUST-FIX: detoast in the reader; add EC-2/EC-3/EC-4 as tests).
