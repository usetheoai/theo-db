---
slug: m163-type-coverage-ab
version: 1.0
owner: theodb
created_at: 2026-07-27
cycle: discover
---

# Discovery Plan — M163 type-coverage A/B harness (prior art)

## Context

The M160–M162 retro (memory `retro-m160-m162-discipline`, `m161-expr-routing`) identified the project's most
expensive recurring defect: the A/B byte-identity oracle runs over the **ClickBench data**, which does not exercise
the **type space**, so type-class bugs (int-widening, temporal, float IEEE, collation) survive the A/B and are only
caught by council review AFTER a 14-min rebuild. M161 alone: council caught 1 BLOCKER (int±k `out_typoid` = column
type not `opresulttype` → `int2col+5` at 32767 errors) + 1 HIGH (temporal leak via `minmax_kind_of`) that the
`int4-int4` A/B never exercised. Same pattern in M151/M154/M157/M158. M163 builds a **per-type edge-value A/B harness**
that runs BEFORE review. This discovery investigates how mature columnar/SQL engines structure differential +
type-coverage testing, so M163's harness borrows a proven design instead of inventing one.

## Objective

Produce a blueprint that answers: **what per-type edge values + what differential-test structure do sqllogictest,
DataFusion, and DuckDB use to catch type-class bugs**, concrete enough that M163 can implement a `theodb_columnar` A/B
harness that (a) generates the right edge values per type, (b) asserts result equality the way the SOTA does, and (c)
would have caught the M161 `out_typoid` regression. Success = every research question answered with a `file:line`
citation into `knowledge-base/references/`, all four corners covered.

## In-scope / Out-of-scope

- **`knowledge-base/references/datafusion/`** — IN: `datafusion/sqllogictest/test_files/` (`.slt` files:
  `type_coercion.slt`, `cast_to_type.slt`, `optimizer_group_by_constant.slt`, `group.slt`), `datafusion/sqllogictest/`
  (the runner crate). OUT: the query engine internals (`datafusion/core/`, `physical-plan/`) — not test-harness prior art.
- **`knowledge-base/references/duckdb/`** — IN: `test/sql/types/` (per-type edge dirs: `float`, `decimal`, `date`),
  `test/sql/cast/`, `test/sql/aggregate/`, `test/fuzzer/`. OUT: `src/` engine, `third_party/`, `tools/` — not test prior art.

## ADRs

### ADR-1 — depth: borrow the STRUCTURE, not the corpus
**Decision:** investigate the *design* (edge-value catalog per type, equality-assertion mechanism, differential
structure), not to copy any `.slt`/`.test` corpus (licenses differ; DuckDB MIT, DataFusion Apache-2.0 — both permissive
but the corpus is theirs). M163 authors its own edge values for TheoDB's routed type classes. **Rationale:** Rule 9
(don't reinvent the *method*) + D1 (only permissive; even so, borrow the technique, write our own tests).

### ADR-2 — time budget: 3h datafusion, 2h duckdb
Focused prior-art read, not an exhaustive suite audit. Per-question stop condition: once the mechanism is cited, move on.

## Research questions

| # | Corner | Question | Method | Expected answer shape |
|---|---|---|---|---|
| Q1 | Techniques | How does DataFusion encode the **integer-widening** coercion rules (`int2±int4→int4`, `int4±int8→int8`) that M161's BLOCKER got wrong — as explicit expected-result rows per type pair? | Read `references/datafusion/datafusion/sqllogictest/test_files/type_coercion.slt` | list of (expr, input types, expected output type) rows → the edge-value catalog for int arithmetic |
| Q2 | Techniques | How does DuckDB structure **per-type edge-value** tests (min/max/overflow boundary) for a single type? What edge values does it enumerate for float (−0.0/NaN/inf) and decimal? | Read `references/duckdb/test/sql/types/float/` + `test/sql/types/decimal/` (grep for `-0.0`, `nan`, `inf`, overflow) | the canonical per-type edge value set to reuse |
| Q3 | Techniques | How does the SOTA test the **GROUP BY constant** case (the M161 honest-negative: PG eliminates const group keys)? Is it asserted as a plan/result invariant? | Read `references/datafusion/datafusion/sqllogictest/test_files/optimizer_group_by_constant.slt` | how a const-group-key is expected to behave → confirms M163 should assert the decline, not route |
| Q4 | Integration tests | How does the sqllogictest runner assert **result equality** — byte-level string match, sorted-rows, or a hash? How are NULL and float formatting handled in the comparison? | Read the `.slt` header/format docs + `references/datafusion/datafusion/sqllogictest/` runner (grep `sort`, `hash`, `NULL`, `normaliz`) | the equality-assertion mechanism M163's A/B oracle should mirror |
| Q5 | Integration tests | How does DuckDB's **cast/overflow** test assert the ERROR case (out-of-range → typed error, not wrong value)? — the negative-case lens M161's int±k range-check needs. | Read `references/duckdb/test/sql/cast/` (grep for expected-error markers, `Conversion`, `out of range`) | how to assert "must error 22003" in the harness |
| Q6 | Tools | What is the test RUNNER + invocation (the `.slt`/`.test` file format + the binary/crate that runs it) and how is it wired into CI? | Read a `.slt` file header + `references/datafusion/datafusion/sqllogictest/` (Cargo.toml / README / runner main) | the runner shape → decide M163: reuse sqllogictest format or a bespoke pytest harness |
| Q7 | Dependencies | What dependency does DataFusion's sqllogictest harness pull (the `sqllogictest` Rust crate vs a homegrown runner)? Version? | Grep `references/datafusion` for `sqllogictest` in `Cargo.toml` files | dep name + version → informs M163's build-vs-reuse decision (Rule 9 / parsimony rung 4) |

## Coverage Matrix

| Corner | Questions | Covered? |
|---|---|---|
| Techniques | Q1, Q2, Q3 | ✅ (3) |
| Integration tests | Q4, Q5 | ✅ (2) |
| Tools | Q6 | ✅ (1) |
| Dependencies | Q7 | ✅ (1) |

7 questions, all four corners ≥ 1, ≤ 3 per corner. Within budget.

## Halt-loop checkpoints (for /discover-execute)

- A question is `done` only when its answer cites a resolvable `references/…:line`. A `.slt`/`.test` path that does not
  `Path.exists()` → the question stays `blocked` with reason, never fabricated.
- The blueprint's `techniques` corner MUST end with a concrete **edge-value catalog per TheoDB routed type**
  (int2/int4/int8, float4/float8, timestamp/date/timestamptz, text, bool, NULL) — not just prose.

## Acceptance Criteria

- All 7 questions answered with a real `references/` citation.
- Blueprint contains: (a) the per-type edge-value catalog, (b) the equality-assertion mechanism, (c) an explicit
  statement of how the harness would catch the M161 `out_typoid` regression (positive control), (d) the build-vs-reuse
  decision (sqllogictest crate vs bespoke pytest) with the Rule-9/parsimony rationale.

## Global Definition of Done

- `/discover-confidence` ≥ SHIPPABLE_WITH_CAVEATS; all 4 coverage corners populated; no fabricated citation (per
  `rules/discover-blueprint-golden-rule.md`). Feeds `/to-plan m163-type-coverage-ab`.

## Cross-references

- Rules: `rules/testing.md` (§ 4.1 edge vs negative — the two lenses the harness must cover), `rules/discover-phd-rigor.md`
  (R0.1 acervo-first; R2 ≥2 sources), `rules/discover-blueprint-golden-rule.md`.
- Memory: `m161-expr-routing`, `retro-m160-m162-discipline`.
