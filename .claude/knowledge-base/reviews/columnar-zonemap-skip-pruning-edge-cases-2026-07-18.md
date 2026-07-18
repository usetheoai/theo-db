# Edge Case Review — columnar-zonemap-skip-pruning

Date: 2026-07-18
Tasks analyzed: 4 (T1.1, T2.1, T2.2, T3.1)
Cases found: 7 (EDGE: 3, NEGATIVE: 4 | MUST FIX: 2, SHOULD TEST: 4, DOCUMENT: 1)

The stakes are asymmetric: this slice touches a **correctness-critical** path — a wrong skip silently drops rows from an aggregate (a wrong `sum`, no error). Both MUST-FIX are about the **predicate-extraction ↔ zone-map domain contract**: the const must be compared in the SAME numeric domain the chunk's min/max was computed in, and the consumer must never index the directory out of range. The plan's byte-identical A/B gate (D3) is a strong backstop, but it only catches what the fixture happens to exercise — the extractor bugs below can pass a naive fixture and corrupt a production query.

## MUST FIX

### EC-1: Const/operator type-domain mismatch → wrong skip → silently wrong aggregate
- **Affected task:** T2.1 (and consumed by T1.1)
- **Kind:** NEGATIVE (invalid/mismatched input to the min/max test)
- **Family:** Format / Boundary
- **Scenario:** `compute_minmax` writes a column's `min_bits`/`max_bits` in the COLUMN's domain (INT4 → i32 sign-extended to i64 bits; FLOAT4 → f32 widened to f64 bits — per `columnar_codec.rs:293-340`). When `admit` extracts `Var(col) OP Const`, the `Const` may be a DIFFERENT type than the column (cross-type operator `int48gt(int4, int8)`, or a `numeric`/`int8` literal against an `int4` column). If `admit` encodes `const_bits` in the CONST's type but `chunk_can_match` decodes it in the COLUMN's `MinMaxKind`, the two are compared in different domains → the exclusion test is WRONG → a chunk group with matching rows can be skipped → the aggregate silently loses those rows.
- **Impact:** Silent wrong `sum`/`count` on any filtered aggregate where the literal type ≠ the column type (extremely common — `WHERE int4col > 100` where `100` may plan as int8, or `WHERE float4col > 1.5` where `1.5` is float8). The worst failure class (Rule 8).
- **Suggested fix:** In `admit`, push a clause ONLY when the operator is the column-type-NATIVE btree comparison (operator input types == column type) AND resolve the `ZoneOp` from the **btree strategy number** (`BTLessStrategyNumber`…`BTGreaterStrategyNumber` via the operator's opfamily), not a hardcoded per-type OID list; encode `const_bits` in the column's `MinMaxKind` domain (coerce/verify the const type == column type first). Any type/operator mismatch → **do not push** (the executor still filters → correct, just unpruned). One `if` on the operator's input types + strategy lookup.

### EC-2: Predicate column index out of range → panic in the scan
- **Affected task:** T2.2
- **Kind:** NEGATIVE (defensive / invalid state)
- **Family:** Boundary
- **Scenario:** The skip consumer indexes `entries[cg * natts + p.col]`. If a `ZonePredicate.col` ever exceeds `natts` (a bug in `admit`, a dropped/added column between plan and exec, or a projection/attno confusion), the index is out of bounds → Rust panic → crashes the backend mid-scan.
- **Impact:** Backend crash (panic across the scan) instead of a graceful fallback. Low probability but a hard crash in a correctness-critical path.
- **Suggested fix:** Guard in the consumer: `if p.col >= natts { /* fail-safe: cannot evaluate → do not skip */ }` — treat an out-of-range predicate as "must scan" (fail-safe), never index OOB. One bounds check.

## SHOULD TEST

### EC-3: Signed two's-complement + float bit ordering in `chunk_can_match`
- **Affected task:** T1.1
- **Kind:** EDGE (boundary of a valid domain)
- **Suggested test:** `test_zonemap_signed_negative_range` — a chunk `[-50, -10]` (I8): `y < -100` → skip (excluded); `y > -30` → scan; `y = -20` → scan. Assert the typed decode (NOT raw-u64 compare, which orders `-1` as `u64::MAX` and would break). Plus `test_zonemap_float_neg_zero` — `[-0.0, 5.0]` with `y < 0.0` handled as f64 values. (Plan already lists `test_zonemap_signed_and_float` — make it assert BOTH the negative-int order and the raw-u64-would-be-wrong contrast explicitly.)

### EC-4: NaN / NULL in a chunk must not cause a wrong skip
- **Affected task:** T1.1 / T2.2
- **Kind:** EDGE (rare-but-real valid data)
- **Suggested test:** `test_zonemap_chunk_with_nulls_and_nan` — a float chunk whose non-NULL/non-NaN values are `[10, 20]` but that also holds NULL and NaN rows: `y > 100` → skip is SAFE (NULL/NaN rows never match `y > 100` anyway); `y > 15` → scan (some rows match). Assert the skip decision uses only the present-value min/max and that all-NULL/all-NaN → `has_minmax=false` → never skipped (fail-safe). Proves the "min/max over present values + executor is final authority" contract (D3).

### EC-5: Two-column qual (`WHERE y > z`) is ignored, not mis-pushed
- **Affected task:** T2.1
- **Kind:** NEGATIVE (non-`Var OP Const` shape)
- **Suggested test:** `test_admit_ignores_var_op_var` — `WHERE y > z` (both columns) yields 0 pushable predicates → the clause is left to the executor (result still correct, just unpruned). Guards against an extractor that grabs the first Var + wrongly treats the second Var as a const.

### EC-6: Byte-identical A/B must include a PARTIAL chunk group
- **Affected task:** T2.2 / T3.1
- **Kind:** EDGE (the composition of skip + executor re-check)
- **Suggested test:** `test_decode_columns_result_identical_on_off` (plan already lists it) — MUST include a mixed table with (a) a chunk group fully inside the range, (b) one fully outside (skipped), and (c) one that PARTIALLY overlaps (min/max intersects the range but some rows are outside). Case (c) is the one that proves the skip is an *admission* filter and the executor re-check drops the non-matching survivors — without it, a skip that wrongly dropped case (c) would pass. Assert the scalar aggregate is byte-identical.

## DOCUMENT

### EC-7: Skip is sound only for the position-independent aggregate path
- **Kind:** EDGE
- **Accepted risk:** Dropping a whole chunk group is correct for an AGGREGATE (`sum`/`count`), where output is position-independent and the skipped chunk provably contributes zero matching rows. A future ROW-RETURNING columnar scan (non-aggregate `SELECT *`) that reused this skip would need TID-aware pruning (the skipped rows must not shift TID mapping). This slice scopes to the M100 CustomScan aggregate path (D2), so it is sound today; note the constraint so a later row-returning consumer does not reuse `decode_columns`'s skip naively.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 2 (EC-3, EC-4) | 0 | 0 | 2 (EC-3, EC-4) | 0 |
| T2.1 | 0 | 2 (EC-1, EC-5) | 1 (EC-1) | 1 (EC-5) | 0 |
| T2.2 | 1 (EC-6) | 2 (EC-2, EC-4) | 1 (EC-2) | 1 (EC-6) | 1 (EC-7) |
| T3.1 | 1 (EC-6) | 0 | 0 | 0 | 0 |

**Coverage check:** every task touching an input boundary has both lenses. T1.1 (the pure test) is EDGE-heavy (numeric domains); T2.1 (extraction) is NEGATIVE-heavy (type/shape mismatches — where corruption enters); T2.2 (consumer) has both; T3.1 is the measured gate.

**Verdict:** PLAN NEEDS ADJUSTMENT

The two MUST-FIX are the same theme: **the predicate must be compared in the column's exact numeric domain, and the consumer must fail safe.** EC-1 (type-domain mismatch) is the single highest-stakes bug — it produces a silently wrong aggregate on the most common query shape (`WHERE int4col > literal`). The fix is not more code, it's a **stricter push gate**: only push column-type-native operators (resolved by btree strategy number), encode the const in the column's domain, and fall back (unpruned but correct) on any mismatch. Add a `## ADR D5` naming this "same-domain-or-fallback" contract, fold EC-1's gate into T2.1, EC-2's bounds guard into T2.2, and EC-6's partial-chunk-group case into the byte-identical test. Then re-run `/plan-confidence`.
