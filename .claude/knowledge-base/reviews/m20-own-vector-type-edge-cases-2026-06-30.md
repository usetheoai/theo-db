# Discover Edge Case Review — m20-own-vector-type

Date: 2026-06-30
Discovery plan analyzed: .claude/knowledge-base/discoveries/plans/m20-own-vector-type-plan.md
Research questions analyzed: 8
Edge cases found: 4 (MUST FIX: 0, SHOULD TEST: 2, DOCUMENT: 2)

All 11 cited reference paths verified to exist. The distance oracle (Q5) is present in
`pgvector/test/sql/vector_type.sql` (l2_distance×5, cosine_distance×11, inner_product×4 rows). No MUST-FIX —
the plan is well-grounded. Two halt-loop checkpoints + two accepted-risk notes below.

## MUST FIX

_None._

## SHOULD TEST

### EC-1: pgvector version skew (clone is v0.8.3; the theo-db image may install a different pgvector)
- **Affected question:** Q1, Q2 (binary layout + distance formulas are version-bearing)
- **Scenario:** `/discover-execute` reads pgvector **0.8.3** (`references/pgvector/META.json`) for the varlena layout + distance accumulation order, but the running `theo-db:m19` image installs pgvector via its own apt/build pin — if that version differs, the "parity" target the blueprint states could be off.
- **Suggested halt-loop checkpoint:** before answering Q1/Q2, record the exact pgvector version read (0.8.3) in the blueprint; add a blueprint note that IMPLEMENT MUST cross-check the image's installed `pgvector` version and confirm the `vector` send/recv format + distance formulas are unchanged (they have been stable since ≤0.5, but the check is the parity guarantee).

### EC-2: vectorchord's `sphere_vector` composite ≠ the core vector FFI pattern
- **Affected question:** Q3, Q4
- **Scenario:** vectorchord exposes BOTH a core pgvector-FFI wrapper (`VectorInput`/`VectorHeader` in `src/datatype/memory_vector.rs` — the real parity/coexistence pattern) AND a separate `sphere_vector` COMPOSITE type with `_vchord_vector_sphere_l2_in`/`_ip_in`/`_cosine_in` operators (`src/datatype/operators_vector.rs`, `sql/install/vchord--1.1.1.sql:730-780`) used for sphere/range predicates. `/discover-execute` could mistake the `sphere_*` operators for the core distance ops and model the wrong thing.
- **Suggested halt-loop checkpoint:** Q3/Q4 MUST extract the **core** representation (`VectorInput` FFI over pgvector's `VectorHeader`) as the parity/coexistence pattern, and explicitly note that `sphere_vector` + `_vchord_vector_sphere_*` are a DISTINCT range-query feature out of M20 scope.

## DOCUMENT

### EC-3: pgvector sibling types (halfvec / sparsevec / bit) are out of M20 scope
- **Accepted risk:** `references/pgvector/test/sql/` contains `halfvec.sql`, `sparsevec`/`bit` tests and `src/` has sibling type files. M20 targets the `vector` (float4) type + its 3 ops ONLY. `/discover-execute` stays in `vector.c`/`vector.h`/`vector.sql` and does NOT read `halfvec.c`/`sparsevec.c`. Documented so the loop does not wander.

### EC-4: accumulator float-width is THE numeric-parity determinant
- **Accepted risk:** pgvector stores `float4` (f32) but its distance loops accumulate in `double` (f64) — the storage width and the accumulation width differ, and the accumulation width is what determines bit-exact parity. Q2 already targets "f32 acc vs f64"; this note hard-pins that the blueprint answer for Q2 MUST state the accumulator type per op (and that a Rust port must match it — f64 accumulation — to be parity-exact). Not a plan change; a precision requirement on the Q2 answer.

## Summary

| Question | Edges found | MUST FIX | SHOULD TEST | DOCUMENT |
|----------|-------------|----------|-------------|----------|
| Q1 | 1 | 0 | 1 (EC-1) | 0 |
| Q2 | 2 | 0 | 1 (EC-1) | 1 (EC-4) |
| Q3 | 1 | 0 | 1 (EC-2) | 0 |
| Q4 | 1 | 0 | 1 (EC-2) | 0 |
| Q5 | 0 | 0 | 0 | 0 |
| Q6-Q8 | 0 | 0 | 0 | 0 |
| (scope) | 1 | 0 | 0 | 1 (EC-3) |

**Verdict:** DISCOVERY PLAN OK (absorb EC-1 + EC-2 as halt-loop checkpoints; EC-3 + EC-4 are precision notes).
