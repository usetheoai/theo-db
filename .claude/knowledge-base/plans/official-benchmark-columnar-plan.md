---
slug: official-benchmark-columnar
milestone_id: M128
created_at: 2026-07-20
goal: Ship TheoDB's ClickBench entry (create/queries/glue + results) over theodb_columnar, measured on real hits with the cold/hot protocol + a byte-identical result A/B
---

# Plan — M128 Official benchmark: COLUMNAR pillar (ClickBench entry over theodb_columnar)

## Goal

Deliver TheoDB's **ClickBench entry** (the copy-the-`postgresql/`-directory contract: `create.sql` /
`queries.sql` / glue + a `results.json` of raw timing triples) running the 43 ClickBench queries over a
`theodb_columnar` table, **measured** on the real ClickBench `hits` dataset (subsampled) with the ClickBench
cold/hot protocol, plus the retained **byte-identical result A/B** (columnar CustomScan vs heap-native — the
correctness oracle ClickBench lacks). Applies the ADR-0050 adopt-and-wrap pattern proven in M127.

**Single metric:** a reproducible ClickBench-format run (`benchmarks/clickbench/theodb/results.json`) with the
43 queries' cold + hot timings over `theodb_columnar` on real `hits`, AND a byte-identical result A/B PASS
(columnar vs heap) on the aggregate queries — recorded in `docs/benchmarks/m128-clickbench-columnar.md`.

## Context

Implements ADR-0050 (adopt-and-wrap) for the columnar pillar; the second application of the pattern the M127 vector
pilot proved. The discovery blueprint
(`knowledge-base/discoveries/blueprints/official-db-benchmark-harness-blueprint.md`) selected ClickBench as the
canonical OLAP benchmark: 43 queries over one `hits` table, load-then-3-runs (cold=1st, hot=min-of-hot), geomean
combine, single-node; entry = copy the `postgresql/` directory + a results JSON + a PR. It found ClickBench is
**timing-only** (`check` is `SELECT 1`, no result oracle) — so the retained byte-identical result A/B is the
correctness half of the wrap layer. The `hits` dataset is **CC-BY-NC-SA** → CI-download only, never bundled.

## Baseline Context

Repo state: git sha `8532779`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `benchmarks/clickbench/theodb/create.sql` | — | (NEW) | The `hits` table DDL (105 cols) with `USING theodb_columnar`. |
| `benchmarks/clickbench/theodb/queries.sql` | — | (NEW) | The 43 ClickBench queries (adapted from the ClickBench `postgresql/` reference). |
| `benchmarks/run_m128_clickbench.py` | — | (NEW) | Driver: load hits subsample → cold/hot 3-run per query → results.json + the byte-identical result A/B. |
| `benchmarks/theodb_bench/regression.py` | 44 | byte-identical A/B (M127) | Reused/extended for row-set result comparison (the correctness oracle). |
| `benchmarks/theodb_bench/test_clickbench.py` | — | (NEW) | Unit tests for the driver's cold/hot fold + result-A/B helpers. |

### Current callers / dependents (verified `file:line`)

- `theodb_rs/src/am/columnar.rs:209` — `CREATE ACCESS METHOD theodb_columnar` (the TableAM the hits table uses).
- `theodb_rs/src/am/columnar_agg.rs:22` — `theodb.enable_columnar_agg` GUC (default OFF) — the vectorized aggregate CustomScan the aggregate ClickBench queries exercise when on.
- `theodb_rs/src/am/guc.rs:313` — `theodb.enable_columnar_agg` zone-map min/max skip GUC.
- `benchmarks/columnar_groupby_ab.py:14` — the existing `USING theodb_columnar` + byte-identical-vs-heap A/B pattern (the wrap correctness oracle precedent to reuse).
- `benchmarks/theodb_bench/regression.py` (M127) — the byte-identical comparison the result A/B extends.

### Domain glossary

- **ClickBench protocol** — 43 queries over the `hits` table; each run 3× (cold = 1st run after cache flush, hot = min of the 2 warm runs); cross-query summary = geometric mean; single-node.
- **hits** — the ClickBench dataset (real Yandex Metrica web-analytics, 99.9M rows × 105 cols, ~100 GB full); CC-BY-NC-SA (CI-download only).
- **theodb_columnar** — TheoDB's append-only column-major TableAM (M99–M115) with a vectorized aggregate CustomScan (GROUP BY / count / sum / avg + zone-map skip).
- **result A/B** — the retained correctness oracle: the same query over the columnar table vs a heap copy MUST return byte-identical rows (ClickBench itself validates nothing).

### Architecture boundaries affected

Per `rules/architecture.md`: pure benchmark tooling (Python + SQL, `benchmarks/`) — NO production Rust change.
The entry is the ClickBench directory contract + a driver over the existing `theodb_columnar` TAM + `enable_columnar_agg` GUC. No engine/API/on-disk change.

## Prior Art & Related Work

- Blueprint (web-evidenced, 2026-07-20): ClickBench protocol + the `postgresql/` entry contract
  (create.sql/queries.sql/glue + results JSON, PR flow) + the timing-only / no-result-oracle finding. Sources:
  github.com/ClickHouse/ClickBench + benchmark.clickhouse.com.
- Internal: `benchmarks/columnar_groupby_ab.py` (+ siblings) already prove the byte-identical columnar-vs-heap A/B
  on `theodb_columnar`; `benchmarks/theodb_bench/regression.py` (M127) is the reusable byte-identical comparator.

## ADRs

### ADR M128-1 — copy the ClickBench `postgresql/` contract, swap the table AM to theodb_columnar

**Decision:** the entry is a faithful copy of ClickBench's `postgresql/` directory (create.sql / queries.sql / the
glue hook scripts / a template.json), with the `hits` DDL using `USING theodb_columnar` and
`theodb.enable_columnar_agg=on`. We do NOT rewrite the 43 queries.

**Rationale (cites blueprint + `rules/parsimony-ladder.md` rung 4):** the ClickBench entry contract is a fixed
directory shape; copying it verbatim (only the AM clause differs) is the field-faithful, minimal path and makes a
later leaderboard PR mechanical. Rewriting queries would break comparability.

**Alternatives rejected:**
- **Author a bespoke OLAP query set** — REJECTED: not ClickBench-comparable (defeats the adopt purpose).
- **Load hits into a heap table only** — REJECTED: the columnar PILLAR is the point; hits goes into `theodb_columnar`
  (the heap copy exists only as the A/B correctness reference).

### ADR M128-2 — subsampled hits on a self-hosted box; full-100GB canonical run + leaderboard PR are follow-ups

**Decision:** measure on a **subsampled `hits`** (e.g. 1M rows) on the self-hosted droplet, honestly labeled; the
full 99.9M-row run on the canonical `c6a.4xlarge` + the public leaderboard PR are tracked operational follow-ups.

**Rationale (cites blueprint license + Rule 3):** the full hits is ~100 GB (impractical on the shared droplet) and
CC-BY-NC-SA (CI-download only, never bundled). A 1M subsample proves the entry + the columnar path + the result
A/B end-to-end on REAL hits data; the canonical-scale/box run is the follow-up (M127 precedent).

**Alternatives rejected:** synthetic hits-like data — REJECTED (not real; the point is real ClickBench data).

## Dependencies

`## Dependencies`: **none new** — reuses `psycopg2` (already in `benchmarks/requirements.txt`) + the existing
`theodb_columnar` TAM. The `hits` subsample is CI-downloaded from `datasets.clickhouse.com` (CC-BY-NC-SA), never
vendored. No crate/pip added.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| ClickBench entry contract (create/queries/glue + results) over theodb_columnar | T1 (the entry directory) |
| Measured cold/hot 43-query run on real hits subsample | T2 (driver + droplet run) |
| Byte-identical result A/B (columnar vs heap) — the correctness oracle ClickBench lacks | T3 (result A/B) |

## Phase 1 — the ClickBench entry + driver

### T1.1 — the ClickBench `theodb` entry directory

#### Why this step
The entry contract is what a public ClickBench leaderboard PR submits (ADR M128-1). Reasoning: fetch the ClickBench
`postgresql/` reference (create.sql + queries.sql), adapt the `hits` DDL to `USING theodb_columnar`, keep the 43
queries verbatim, and add the glue (a thin `benchmark.sh` + `template.json`).

#### Files to edit
- `benchmarks/clickbench/theodb/{create.sql,queries.sql,benchmark.sh,template.json}` (NEW).

#### TDD
- RED: a unit test asserts `queries.sql` parses to exactly 43 non-empty statements and `create.sql` contains
  `USING theodb_columnar` + the 105 `hits` columns.
- GREEN: the entry files are present and parse.
- REFACTOR: keep the query text byte-identical to the ClickBench reference (only the AM clause differs).

#### Concurrency tests
(none — single-threaded) — the entry files are static SQL; the driver runs queries sequentially (ClickBench single-node protocol).

#### Failure scenarios
- The ClickBench reference is unreachable at fetch time → the driver exits UNBENCHMARKED (no fabricated timings); the entry files, once committed, are self-contained.

#### Acceptance criteria
- `test_clickbench.py` asserts 43 queries + the `USING theodb_columnar` DDL; the query text matches the ClickBench reference.

#### DoD
- The `benchmarks/clickbench/theodb/` directory exists with the 4 contract files; the query count test passes.

### T2.1 — driver: load hits subsample + cold/hot 43-query run

#### Why this step
The measured metric. Reasoning: load a 1M-row `hits` subsample (CI-download) into the `theodb_columnar` table via
`create.sql`, then run each of the 43 queries 3× (cold after a cache flush, 2 hot), record the raw `[t1,t2,t3]`
triple per query into `results.json` (the ClickBench format), compute cold=1st / hot=min-of-hot / geomean.

#### Files to edit
- `benchmarks/run_m128_clickbench.py` (NEW); `benchmarks/clickbench/theodb/results.json` (NEW, produced);
  `docs/benchmarks/m128-clickbench-columnar.{md,json}` (NEW).

#### TDD
- RED: with `theodb.enable_columnar_agg=on`, an aggregate query (e.g. `SELECT count(*)`) runs and returns a result;
  the driver asserts every query completes (or is recorded as ERRORED with the message) and the 3-run triple is
  captured — a query that silently returns nothing is a FAIL.
- GREEN: the driver produces `results.json` with 43 entries (timing triples or explicit ERROR), on the real hits
  subsample on the droplet.

#### Failure scenarios
- **hits download HTTP failure / 5xx** → the driver exits UNBENCHMARKED cleanly (no fabricated timings).
- **A query unsupported by theodb_columnar** (e.g. an exotic expression) → recorded as `ERRORED` with the typed PG
  error message, NOT silently skipped or counted as fast (honest per-query status).
- **DB unreachable mid-run** → typed error surfaced; partial results discarded.

#### Concurrency tests
(none — single-threaded) — ClickBench is single-node sequential; the driver runs one query at a time.

#### Acceptance criteria
- `benchmarks/clickbench/theodb/results.json` has 43 entries (timing triple OR explicit ERROR); ≥1 aggregate query
  uses the columnar CustomScan (verified via EXPLAIN); the cold/hot/geomean summary is computed.

#### DoD
- The measured `results.json` + the docs artifact exist on the real hits subsample; re-runnable via `run_m128`.

## Phase 2 — the retained correctness oracle

### T3.1 — byte-identical result A/B (columnar vs heap) — what ClickBench lacks

#### Why this step
Blueprint: ClickBench is timing-only (`check` = `SELECT 1`) — a wrong-but-fast engine could top the board.
Reasoning: for the aggregate/filter queries that produce a comparable result set, run the SAME query over the
`theodb_columnar` table AND a heap copy, and assert the row sets are byte-identical (reusing
`theodb_bench/regression.py` + the `columnar_groupby_ab.py` pattern) — the correctness oracle the leaderboard lacks.

#### Files to edit
- `benchmarks/run_m128_clickbench.py` (the A/B pass); `benchmarks/theodb_bench/test_clickbench.py` (unit tests).

#### TDD
- RED: a query whose columnar result differs from the heap result (injected in a test fixture) → the A/B reports
  NOT byte-identical + the offending query; identical → PASS.
- GREEN: on the real hits subsample, the aggregate queries return byte-identical columnar-vs-heap row sets.

#### Concurrency tests
(none — single-threaded) — deterministic result comparison.

#### Failure scenarios
- **A columnar query result diverges from heap** → the A/B FAILS LOUDLY (the correctness oracle doing its job — a
  columnar-pushdown bug caught, exactly what ClickBench cannot catch).
- **Non-comparable query** (ORDER-dependent without ORDER BY) → excluded from the A/B with an explicit reason, not silently passed.

#### Acceptance criteria
- The result A/B PASSES (byte-identical columnar-vs-heap) on the aggregate queries; any exclusion is explicit;
  a divergence would FAIL the gate.

#### DoD
- `docs/benchmarks/m128-clickbench-columnar.md` records the cold/hot Pareto + the byte-identical result-A/B verdict
  + the per-query supported/errored status, honestly labeled subsampled + self-hosted box.

## Failure scenarios

- **hits dataset download HTTP failure / 5xx** — the driver exits `UNBENCHMARKED` (no fabricated timings), like the M127 GloVe path.
- **A ClickBench query unsupported by theodb_columnar** — recorded as `ERRORED` with the typed PG error, never silently skipped or timed as fast (honest per-query status).
- **Columnar result diverges from heap on the result A/B** — the correctness oracle FAILS loudly (the capability ClickBench lacks, catching a pushdown bug).

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Subsampled hits (1M) on a self-hosted box → not leaderboard-comparable | MEDIUM | Honestly labeled (ADR M128-2); full-100GB + canonical-box + leaderboard PR are tracked follow-ups (M127 precedent) | implementer |
| Some of the 43 queries may be unsupported by theodb_columnar's pushdown (fall to native PG executor or error) | MEDIUM | Every query's status recorded honestly (columnar-CustomScan / native / ERRORED); the pillar claim is scoped to what runs, not inflated | implementer |
| hits is CC-BY-NC-SA (non-permissive) | LOW | CI-download only from datasets.clickhouse.com, never vendored (D1-safe rule: non-permissive data never enters the shipped tree); documented | implementer |

## Unresolved Questions

- How many of the 43 queries does theodb_columnar's vectorized pushdown accelerate vs fall to native PG? Resolved
  at plan time: **measured, not assumed** — the driver records each query's plan (CustomScan vs native) + status;
  the honest count is an output of T2, not a precondition.
- (none other — every in-scope decision is resolved at plan time.)

## Global DoD

- `benchmarks/clickbench/theodb/{create.sql,queries.sql,benchmark.sh,template.json,results.json}` (the entry
  contract) + `run_m128_clickbench.py` + unit tests green.
- A MEASURED cold/hot 43-query run over `theodb_columnar` on real (subsampled) hits + a byte-identical result A/B
  PASS (columnar vs heap) + per-query supported/errored status, in `docs/benchmarks/m128-clickbench-columnar.md` —
  honestly labeled subsampled + self-hosted box.
- No production Rust change; no new dependency; hits never bundled (CC-BY-NC-SA). CHANGELOG `[Unreleased]`.
  `/code-quality` ∉ {FAIL_HARD, INVALID}.
