---
slug: m163-type-coverage-ab
version: 1.0
cycle: discover
sources: 2 (datafusion sqllogictest, duckdb test suite) — both permissive (Apache-2.0 / MIT)
---

# Blueprint — M163 type-coverage A/B harness

## Objective (recap)

Design a `theodb_columnar` A/B harness that runs BEFORE review and catches type-class routing bugs (the M161 `out_typoid`
BLOCKER + temporal leak) that the ClickBench A/B never exercises. Borrow the STRUCTURE from the SOTA differential/
type-coverage suites; author TheoDB's own edge values + assertions.

## Coverage Corner 1 — Integration Tests (how the SOTA asserts equality + errors)

- **sqllogictest format** (`references/datafusion/datafusion/sqllogictest/test_files/type_coercion.slt`,
  `group.slt`, `optimizer_group_by_constant.slt`): each test declares a typed table (`TINYINT/SMALLINT/INT/BIGINT/FLOAT`
  — per-type columns, `group.slt:20-27`), then `statement ok` / `query <coltypes>` with the expected rows after `----`,
  or `query error <regex>` for the negative case (`type_coercion.slt:46,50` — `query error DataFusion error: … Cannot
  coerce arithmetic expression …`). Equality is by NORMALIZED value with an explicit sort mode (`rowsort`/`nosort`) so
  row-order and NULL/float formatting don't cause false diffs.
- **Negative-case error assertion** (`references/duckdb/test/sql/cast/cast_error_location.test:11,107` "out of range
  integer cast"; `struct_to_map.test:213` `Conversion Error: Could not convert string 'duck' to INT32`): the expected
  behaviour of an out-of-range cast is asserted as the SPECIFIC error string, not merely "it throws" — exactly the
  lens M161's int±k range-check (reproduce PG `22003`) needs.

**Borrowed mechanism for M163:** the assertion is (a) equal-when-valid by normalized value (M163 uses the proven
**symmetric-EXCEPT** oracle from M156-M162 → `diverged=0`, which already handles NULL/float by value and is order-
independent), (b) the specific-typed-error for the negative case (M163 asserts the query DECLINES to native OR raises
the expected SQLSTATE, per class).

## Coverage Corner 2 — Dependencies (what the runner pulls)

- DataFusion's sqllogictest harness depends on the **`sqllogictest = "0.29.1"`** Rust crate
  (`references/datafusion/datafusion/sqllogictest/Cargo.toml:61`) — a single-engine expected-result runner (compares one
  engine's output to a recorded expected block/hash).
- **Decision (Rule 9 / parsimony rung 4-5):** M163 does NOT adopt the `sqllogictest` crate. TheoDB's need is
  **differential** (columnar `hits` vs heap `hits_heap`, SAME live PG session) — cross-path equality, not
  compare-to-recorded-hash — and the project already has the psycopg2 symmetric-EXCEPT A/B pattern shipped across
  M156-M162 (`benchmarks/run_m128_clickbench.py:_bench_query`). Adopting a Rust test-runner + its `.slt` corpus format
  for a Python differential need is reinventing sideways. **M163 = a bespoke pytest harness** that reuses the proven
  oracle and borrows only the SOTA's edge-value catalog + negative-error discipline. No new dependency (pytest +
  psycopg2 already present).

## Coverage Corner 3 — Tools (runner shape + CI wiring)

- The `.slt`/`.test` files are run by a dedicated binary/crate in CI (DataFusion: `cargo test` over the sqllogictest
  crate; DuckDB: its `unittest` runner over `.test` files). Per-type edge tests live in dedicated files
  (`references/duckdb/test/sql/types/float/{nan_arithmetic,infinity_test,ieee_floating_points,nan_cast}.test`).
- **M163 tool shape:** a `pytest` module (`benchmarks/columnar_type_ab.py` + `test_columnar_type_ab.py`) invoked as a
  pre-`/review` gate; a synthetic `theodb_columnar` table populated with the edge catalog below, plus a heap twin for
  the differential. Wired as a documented gate in `rules/testing.md` (the harness the routing changes must pass).

## Coverage Corner 4 — Techniques (the edge-value catalog — the load-bearing deliverable)

Per-type edge values (union of DuckDB's per-type dirs + the M151/M154/M157/M161 traps), for every routed TheoDB type:

| PG type | Edge values to seed | The bug-class it catches |
|---|---|---|
| int2 (21) | `-32768`, `-1`, `0`, `1`, `32767` + `col+5`/`col-5` (widening `int2±int4→int4`) | **M161 BLOCKER** — `out_typoid`: `int2col+5`@32767 → PG int4 `32772`, wrong-out_typoid errors |
| int4 (23) | `INT_MIN`, `-1`, `0`, `1`, `INT_MAX` + `col+3000000000` (→ int8 result, must DECLINE) | int4±int8→int8 widening (M161 fail-closed decline) |
| int8 (20) | `BIGINT_MIN`, `0`, `BIGINT_MAX` + `col+k` overflow | int8-result decline (M161 MEDIUM) |
| float4/8 (700/701) | `-0.0`, `0.0`, `NaN`, `inf`, `-inf` | **M154** float IEEE (`-0.0==0.0`, distinct-NaN) — DISTINCT/GROUP BY divergence |
| timestamp (1114) | epoch-2000 boundary, sub-second µs | **M157** epoch (µs-since-2000 vs Arrow-1970); `extract(minute)` epoch-invariance |
| timestamptz (1184) | non-UTC session tz | M157 — must DECLINE (session_timezone divergence) |
| date (1082) | calendar-shift boundary | temporal gate leak (**M161 HIGH** — `minmax_kind_of` folds date→I4); `date+1` must DECLINE |
| text (25/1043) | non-C-collation string pair, non-UTF-8 bytes, empty, NULL | **M153/M158** collation (byte-order vs collation-order); **M156** non-UTF-8 panic |
| bool (16) | true, false, NULL | completeness |
| (all) | NULL, IN(NULL,…) | 3-valued logic (M161 IN-list decline) |

**Differential-test structure (borrowed from SQLancer's differential idea + sqllogictest's typed-table form):** for
each routed admit-path (zone-pred, IN-list, group-expr, agg) × each type edge, run the query against columnar `hits`
(ON) and heap `hits_heap` (OFF) and assert **either** `diverged=0` (routed, byte-identical) **or** the query DECLINES to
native (EXPLAIN shows no `Custom Scan`) — the M161 fail-closed contract. This is exactly the "byte-identical OR
correct-decline" invariant, now over the TYPE space instead of the benchmark data.

## ADRs

### ADR-1 — bespoke pytest differential harness, NOT the sqllogictest crate
**Decision + rationale:** see Corner 2. Differential (columnar vs heap) ≠ compare-to-recorded-hash; reuse the shipped
symmetric-EXCEPT oracle; no new dep. **Alternative rejected:** adopt `sqllogictest 0.29.1` — REJECTED (Rust runner for a
Python differential need; the `.slt` expected-block model doesn't express cross-path equality without recording golden
output that would drift).

### ADR-2 — positive control replaces a 2nd rebuild (evidence, not workaround)
**Decision:** the DoD's "catch the M161 out_typoid regression" is proven by a **built-in positive control** — a
deliberately-divergent A/B pair the harness MUST flag (`diverged>0`) — alongside the real edges (which must be
`diverged=0`). This is the canonical differential-testing self-test (a known-bad seed proving the oracle detects). It
demonstrates detection on ONE TheoDB build without a 14-min rebuild-with-injected-bug. **Alternative:** rebuild TheoDB
with the reverted int±k fix — stronger but 2× build cost; the positive control is the SOTA-standard, sufficient
evidence. (A one-off manual rebuild-seed can still be run as extra confidence if the box is up.)

## How the harness would have caught the M161 BLOCKER (the milestone's raison d'être)

The M161 BLOCKER: int±k used the column type as `out_typoid`. With the int2 edge row (`c2=32767`) + the query
`SELECT c2+5, count(*) … GROUP BY c2+5`: on the buggy code the columnar path emits `out_typoid=int2` → `i16::try_from
(32772)` errors (or wrong type OID), while heap yields int4 `32772` → the harness's A/B reports `diverged>0` (or an ON
error) → FAIL, before review. The ClickBench A/B (ClientIP int4−int4) never seeds a 32767 int2 → never triggers it.
That is the exact gap M163 closes.

## Cross-references

- Rules: `rules/testing.md` (§4.1 edge vs negative), `rules/discover-phd-rigor.md`, `rules/discover-blueprint-golden-rule.md`.
- Sources: `references/datafusion/datafusion/sqllogictest/test_files/{type_coercion,group,optimizer_group_by_constant}.slt`,
  `references/datafusion/datafusion/sqllogictest/Cargo.toml:61`, `references/duckdb/test/sql/types/float/`,
  `references/duckdb/test/sql/cast/cast_error_location.test`.
- Memory: `m161-expr-routing`, `retro-m160-m162-discipline`.
