# Blueprint — byte-identical numeric-output integer aggregates (sum(int8), avg(int2/4/8))

Deep-research (Staff DB engineer) blueprint. Primary sources: PostgreSQL 17 `numeric.c`/`numeric.h`/`pg_aggregate.dat`,
pgrx 0.19 `numeric_support`, DataFusion 54 `sum.rs`; empirically validated against live PG17.

## Coverage Corner 1 — Integration Tests
Byte-identical A/B (`benchmarks/columnar_numeric_agg_ab.py`) for `sum(int8)` and `avg(int2/4/8)` at MULTIPLE magnitudes
that exercise PG's data-dependent avg scale (16 / 8 / 0 sig-digits): small values (scale 16), ~1e9 (scale 8), near-i64
(scale 0), and a sum exceeding i64 (i128 exactness). GROUP BY + scalar. `#[pg_test]` mirrors.

## Coverage Corner 2 — Dependencies
No new dependency. DataFusion `cast` + `sum` + `count` (already pulled), pgrx 0.19 `AnyNumeric` (already available).
Arrow `Decimal128Array`.

## Coverage Corner 3 — Tools
pgrx 0.19 / DataFusion 54 / Arrow 58; droplet c-8 for the in-PG A/B.

## Coverage Corner 4 — Techniques (the load-bearing research)

### PG avg(int) is `numeric_div(sum::numeric, count::numeric)` with DATA-DEPENDENT scale
`avg(int2/4)` finalfn `int8_avg`, `avg(int8)` finalfn `numeric_poly_avg` — both do `numeric_div(sum, count)` on
scale-0 operands. Result scale = `select_div_scale` = `Max(16 − qweight·4, 0)` clamped `[0,1000]` — shrinks as the sum
grows (verified live: avg small=scale16, avg(1e9)=scale8, avg(≈i64)=scale0). A FIXED-scale reimplementation would NOT
be byte-identical (numeric.c:9799, pg_aggregate.dat).

### PG sum(int8) is exact scale-0 numeric
`sum(int8)` finalfn `numeric_poly_sum` — i128 accumulator → `make_result`, no rounding, scale 0 (numeric.c:6092).

### pgrx AnyNumeric division IS PG numeric_div
`AnyNumeric / AnyNumeric` → `call_numeric_func(pg_sys::numeric_div, …)` = the same `DirectFunctionCall2(numeric_div)`
PG's avg finalfns use → identical `select_div_scale` → byte-identical, PROVIDED both operands are scale-0
(ops.rs:52, mod.rs:25). `AnyNumeric::from(i64/i32/i16/i128)` is scale-0 lossless (int8_numeric; i128>i64 → string
fallback `numeric_in`, still scale 0) (convert_anynumeric.rs:54-66). Empirically: manual `numeric_div(sum,count)` ==
`avg(int)` byte-for-byte.

### DataFusion: Int64 sum WRAPS; Decimal128 is the exact path
`sum(Int64)` uses `add_wrapping` → silently wraps on overflow (sum.rs:420) — CANNOT use for sum(int8). `sum(cast(col
AS Decimal128(38,0)))` → `Decimal128(38,0)` (widen precision, keep scale 0) → the i128 payload == the exact sum
(sum.rs:226-228). `sum: Option` → NULL on empty. `count(col)` → Int64.

## ADRs
- **ADR-N1:** compute `sum(cast(col AS Decimal128(38,0)))` (i128) + `count(col)` in DataFusion; build the PG numeric in
  Rust via `AnyNumeric::from(i128 sum)` (sum(int8)) or `AnyNumeric::from(sum) / AnyNumeric::from(count)` (avg) —
  delegating scale selection to PG's own `numeric_div`. Alternative rejected: DataFusion `avg(Decimal128)` — its
  output scale is fixed, NOT PG's data-dependent `select_div_scale` → not byte-identical.
- **ADR-N2:** an AggSpec may produce MORE THAN ONE DataFusion column (avg-int = sum + count). The batch→datum
  conversion consumes each spec's declared column count. Alternative rejected: a single-column avg — impossible without
  losing PG's scale.
- **ADR-N3:** zero-count guard → SQL NULL before the AnyNumeric division (PG's finalfns return NULL for count 0;
  dividing by scale-0 zero raises division_by_zero).

## Honest caveats
- i128 overflow boundary (~1.7e38): DataFusion Decimal128 sum `add_wrapping` matches PG's i128 accumulator boundary —
  astronomically unreachable, and PG shares it. Optional assert.
- Scope: `sum(int8)` + `avg(int2/4/8)`. `sum/avg(numeric)` (numeric COLUMN input) needs Arrow Decimal128 column decode
  in the columnar codec — deferred (bigger change). `sum(int2/4)`→int8 already shipped (M114).

## Evidence citations
PG17 numeric.c (select_div_scale L9799, numeric_poly_avg L6120, int8_avg L6821, numeric_poly_sum L6092) ·
numeric.h (NUMERIC_MIN_SIG_DIGITS=16 L53) · pg_aggregate.dat · pgrx 0.19 ops.rs L52 / mod.rs L25 /
convert_anynumeric.rs L54-66 · DataFusion 54 sum.rs L213/226/420. Empirical: live postgres:17-alpine.
