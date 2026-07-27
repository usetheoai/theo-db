---
slug: m163-type-coverage-ab
milestone_id: M163
created_at: 2026-07-27
goal: ship a bespoke pytest per-type A/B differential harness that runs any columnar-routing change against a synthetic theodb_columnar table across the type-edge catalog and fails on a seeded out_typoid-class divergence, closing the recurring "A/B passes, review finds a type-class bug" gap
---

# M163 — type-coverage A/B harness

## Goal

Ship `benchmarks/columnar_type_ab.py` — a per-type differential A/B harness that, for each routed admit-path × each
type-edge, asserts **byte-identical (diverged=0) OR correct-decline (no Custom Scan)** against a synthetic
`theodb_columnar` table, and includes a **positive control** (a seeded divergence the harness MUST flag) — proving it
would catch the M161 `out_typoid` BLOCKER before review. Metric: `pytest test_columnar_type_ab.py` green + the positive
control demonstrably fails a seeded-wrong comparison.

## Context

From the M160-M162 retro (blueprint `m163-type-coverage-ab-blueprint.md`, memory `retro-m160-m162-discipline`): the
ClickBench A/B doesn't exercise the type space, so type-class bugs (M151/M154/M157/M161) survive to council review after
a 14-min rebuild. This harness borrows the SOTA differential structure (DataFusion sqllogictest typed-table + DuckDB
per-type edge dirs) with TheoDB's own edge catalog + the shipped symmetric-EXCEPT oracle. ADR-1: bespoke pytest, NOT the
`sqllogictest` crate (differential ≠ compare-to-hash; reuse the M156-M162 oracle; no new dep).

## Baseline Context

### Files that will be touched

| File | LoC | git sha | Role | Extend how |
|---|---|---|---|---|
| `benchmarks/columnar_type_ab.py` (NEW) | 0 | — | the harness itself | create |
| `benchmarks/test_columnar_type_ab.py` (NEW) | 0 | — | pytest unit tests + positive control | create |
| `docs/benchmarks/m163-type-coverage-verdict.md` (NEW) | 0 | — | live-run evidence | create |
| `rules/testing.md` | ~120 | d9fff03 | test discipline SoT | append: document the harness as a pre-review gate |

### Current callers / dependents

The harness is a **leaf** — nothing in production imports it (it is test infrastructure invoked manually / in CI). It DEPENDS on (reads the pattern of, does NOT import): `benchmarks/m162_timing.py:_conn`/`session_setup` (77 LoC, sha 0846a0b — psycopg2 conn + GUC setup) and `benchmarks/run_m128_clickbench.py:_bench_query` (302 LoC, sha 0846a0b — the shipped symmetric-EXCEPT A/B oracle). It exercises the behavior of `theodb_rs/src/am/columnar_agg.rs:classify_target_node` (:649, 2733 LoC, sha d9fff03) without editing it.

### Domain glossary

- **admit-path** — a class `classify_target_node` routes to the columnar CustomScan (zone-pred, IN-list, group-expr, agg).
- **type-edge** — a boundary value of a PG type (e.g. int2 `32767`, float `-0.0`/`NaN`) where a type-class bug surfaces.
- **differential A/B** — running the SAME query against columnar `hits` (ON) and heap `hits_heap` (OFF) and comparing.
- **symmetric-EXCEPT oracle** — `(A EXCEPT B) UNION ALL (B EXCEPT A)` count; `0` ⇒ byte-identical result sets.
- **positive control** — a deliberately-divergent A/B pair the harness MUST flag (`diverged>0`); its own self-test.
- **correct-decline** — a query that (by the M161 fail-closed contract) routes to the NATIVE plan (no Custom Scan); a PASS, not a divergence.

### Architecture boundaries affected

None crossed — the harness lives entirely in `benchmarks/` (test tooling) + a doc note in `rules/`. No `theodb_rs/` production code, no DIP boundary, no new public export (per `rules/architecture.md § composition root` — this is test infra, outside the layered core). Conn: `PGHOST=127.0.0.1 PGPORT=5432 PGDATABASE=<test> PGUSER=postgres` (system PG18 + theodb_rs installed). Synthetic table is small (dozens of rows) — no benchmark load, runs in seconds. Git sha at plan time: v0.153.0.

## Prior Art & Related Work

Blueprint `m163-type-coverage-ab-blueprint.md` (the edge catalog + differential structure). Sources:
`references/datafusion/datafusion/sqllogictest/test_files/type_coercion.slt` (widening as expected-rows),
`references/duckdb/test/sql/types/float/` (float edges), `references/duckdb/test/sql/cast/cast_error_location.test`
(negative-error assertion). Internal: M156-M162 symmetric-EXCEPT oracle.

## ADRs

### ADR-1 — bespoke pytest differential harness (not sqllogictest crate)
Per blueprint ADR-1. Alternatives: adopt `sqllogictest 0.29.1` — REJECTED (Rust runner for a Python differential need;
`.slt` compare-to-recorded model can't express cross-path equality without drifting golden output). Reuse the shipped
oracle. Rule 9 / parsimony rung 4.

### ADR-2 — positive control as the seeded-regression evidence
Per blueprint ADR-2. The harness ships a known-divergent pair that MUST report `diverged>0`; if it ever reports
`diverged=0` the harness itself is broken (self-test). This proves detection without a rebuild-with-injected-bug.
Alternative (rebuild TheoDB with reverted int±k fix) — stronger, kept as optional manual confidence, not the gate.

## Phase 1 — the differential oracle + edge catalog

### T1.1 — synthetic table + per-type edge catalog
#### Why this step
The whole value is exercising the TYPE space the benchmark data misses (the int2=32767 that triggers the M161 BLOCKER).
The catalog is the load-bearing artifact (blueprint Corner 4).
#### TDD
- RED: `test_edge_catalog_has_all_routed_types` — assert the catalog dict has entries for int2/int4/int8, float4/8,
  timestamp/date/timestamptz, text, bool, NULL, and that int2 includes 32767 and float includes `-0.0`/`nan`.
- GREEN: build the catalog + a function that CREATEs a synthetic `theodb_columnar` table + heap twin populated with it.
#### Files to edit
`benchmarks/columnar_type_ab.py` (NEW), `benchmarks/test_columnar_type_ab.py` (NEW).
#### Concurrency tests
(none — single-threaded)
#### Acceptance criteria
- [ ] `pytest benchmarks/test_columnar_type_ab.py -k catalog` exits 0: asserts the catalog dict has ≥1 edge value for each of {int2,int4,int8,float4,float8,timestamp,date,timestamptz,text,bool}, that int2 contains 32767 and float contains -0.0 and nan, and that `setup_tables()` loads an equal non-zero row count into both `hits` (columnar) and `hits_heap` (`SELECT count(*)` on each returns the same N > 0).

### T1.2 — the differential assertion (byte-identical OR correct-decline)
#### Why this step
The M161 contract is "route byte-identically OR decline to native". The oracle must accept BOTH, and only FAIL on a
routed-but-divergent result (the actual bug shape).
#### TDD
- RED: `test_assert_pair_flags_divergence` — given two temp tables with a deliberately different row, the oracle returns
  `diverged>0` (the **positive control**). `test_assert_pair_passes_identical` — identical → `diverged=0`.
- RED negative: `test_declined_query_is_ok_not_diverged` — a query that declines to native (no Custom Scan in EXPLAIN)
  is reported as `declined`, NOT as a spurious `diverged` failure.
- GREEN: implement `ab_check(sql)` = EXPLAIN → if Custom Scan: symmetric-EXCEPT columnar-vs-heap → diverged; else
  `declined`. Result = PASS iff (routed AND diverged=0) OR declined.
#### Concurrency tests
(none — single-threaded)
#### Acceptance criteria
- [ ] `pytest benchmarks/test_columnar_type_ab.py -k oracle` exits 0: `ab_check` on a seeded-divergent pair returns diverged>0 (positive control), on an identical pair returns diverged=0, and on a native-plan query (EXPLAIN has no `Custom Scan`) returns status='declined' (not a diverged failure).

## Phase 2 — integration: the routed classes across the type edges

### T2.1 — drive every admit-path × edge through the oracle (live theodb_columnar)
#### Why this step
DoD: prove the harness runs end-to-end against real theodb_columnar and would catch the M161 regression.
#### TDD
- Integration: for IN-list / int±k / extract / agg × the type edges, run `ab_check`; assert every routed one is
  `diverged=0` and every declined one (temporal±int, int8-result, IN(NULL), non-C-collation text) is `declined`.
- The int2 `c2+5 @ 32767` case: on current (fixed) code → `diverged=0`; documented as the row that, on the M161 buggy
  code, would have flagged (the positive control proves the detection mechanism).
#### Files to edit
`benchmarks/columnar_type_ab.py` (the driver), `docs/benchmarks/m163-type-coverage-verdict.md` (NEW — the run evidence).
#### Concurrency tests
(none — single-threaded)
#### Failure scenarios
A routed query diverges (real bug) → harness exits non-zero + names the (path, type, value). A declined query that
SHOULD route → flagged as a coverage regression.
#### Acceptance criteria
- [ ] `python3 benchmarks/columnar_type_ab.py` exits 0 on v0.153.0: every routed (path × type-edge) reports diverged=0 and every expected-decline (temporal±int, int8-result, IN(NULL), non-C-collation text) reports status='declined'; the run table (path, type, value, status, diverged) is written to `docs/benchmarks/m163-type-coverage-verdict.md`.

## Failure scenarios

The harness's only external I/O is the **psycopg2 connection to the local PG**. Failure modes + how the harness handles them:

- **Connection refused / PG down** (the M162 box-instability class): `_conn()` raises `OperationalError` → the harness exits non-zero with a clear "PG unreachable" message, never a silent pass. Test: point at a dead port → assert `OperationalError` surfaces (not swallowed).
- **Backend crash / connection dropped mid-check** (OOM, admin shutdown): a per-check fresh connection (the M162 `m162_timing.py` pattern) isolates a crash to one check → that check is recorded ERRORED, the rest continue. Test: kill a backend mid-run (or mock a dropped cursor) → the check is ERRORED, not a false `diverged=0`.
- **statement_timeout on a pathological synthetic query**: `SET statement_timeout` bounds each check; a timeout is recorded ERRORED (not PASS). Test: a deliberately-slow query under a 1s timeout → ERRORED surfaced.
- **theodb_rs extension absent** (fresh DB without `CREATE EXTENSION`): the synthetic-table setup fails fast with "theodb_columnar unavailable" rather than silently falling back to a heap table that would make every A/B trivially `diverged=0` (a false-green). Test: a DB without the extension → setup raises, harness aborts.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Per-type edge catalog + synthetic tables | T1.1 |
| Differential oracle (byte-identical OR decline; positive control) | T1.2 |
| Full admit-path × edge matrix live + evidence | T2.1 |
| External-I/O failure handling (conn/crash/timeout/missing-extension) | Failure scenarios § |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Synthetic table ≠ production type-mix — the harness could give false confidence | medium | Document it as COMPLEMENTARY to the ClickBench A/B, never a replacement (verdict doc + `rules/testing.md`) | implementer |
| Coverage rot — a new routed class ships without its type-edges, harness silently under-covers | medium | `test_edge_catalog_has_all_routed_types` FAILS when a routed type has no edges (checklist assertion in the suite) | the assertion |
| Small-table false-green — the columnar swap may not fire on a dozens-of-rows synthetic table (cost-based planner) | low | Assert EXPLAIN shows `Custom Scan` in the ON arm before comparing; a declined ON arm is `declined`, not a trivial `diverged=0` (M158 lesson) | T1.2 |
| PG/box instability during the live run (M162 class) | low | Fresh connection per check + `statement_timeout`; a crash → ERRORED for that check, others continue (see `## Failure scenarios`) | T2.1 |

## Unresolved Questions

- Does the columnar-agg swap fire on a dozens-of-rows synthetic table, or does the planner pick a trivial plan? Resolved at T1.2 via EXPLAIN assertion (route or the test is invalid). (none other — every decision resolved.)

## Global Definition of Done

- [ ] `/plan-confidence` verdict ∈ {SHIPPABLE, SHIPPABLE_WITH_CAVEATS} (score ≥ 70, no hard cap).
- [ ] `pytest benchmarks/test_columnar_type_ab.py` exits 0 (all unit tests + positive control pass) AND `python3 benchmarks/columnar_type_ab.py` exits 0 (full live matrix: routed→diverged=0, expected-decline→declined) on v0.153.0.
- [ ] `/code-quality` verdict ∉ {FAIL_HARD, INVALID} (HARD count = 0); council review returns READY_TO_MERGE.
- [ ] `git tag` shows v0.154.0 AND `grep '^## M163' ROADMAP.md` shows `[x]`.

## Final Phase: Integration Validation

- [ ] `docs/benchmarks/m163-type-coverage-verdict.md` exists with the run table (≥1 row per routed path) AND `grep -c columnar_type_ab rules/testing.md` ≥ 1 (harness documented as a pre-review gate).
