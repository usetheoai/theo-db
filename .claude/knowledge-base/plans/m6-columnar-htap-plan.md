---
slug: m6-columnar-htap
created_at: 2026-06-28
goal: Prove + measure permissive columnar/HTAP analytics (pg_mooncake) vs row-store, with the row-vs-columnar plan evidence
---

# Plan: Columnar / HTAP analytics — prove, measure, gate (M6)

> **Version 1.0** — Deliver M6 (Analytics colunar / HTAP) with evidence, measurement-first: (1) prove the
> **pg_mooncake** (MIT, DuckDB+Iceberg lakehouse) columnstore-mirror capability on TheoDB's engine; (2) measure
> an analytical query on the columnstore mirror **vs the row-store** with real timing + correctness; (3) capture
> the **row-vs-columnar plan choice** as an EXPLAIN artifact (`Custom Scan (DuckDBScan)` on the mirror vs
> `Seq Scan` on the row table — DoD-2); (4) document the **honest D2 framing** (lakehouse on disk, NOT AlloyDB's
> in-memory columnar). Embedding pg_mooncake+pg_duckdb (heavy Rust+pgrx+DuckDB build) into the shipped
> `theo-db:dev` is the measurement-gated adoption step (pg_mooncake supports pg17 — Makefile; pgduckdb:17-main
> base exists), mirroring the M7-S2 BM25 precedent.

## Goal

> Enable the TheoDB team to decide columnar adoption on evidence by proving pg_mooncake's columnstore-mirror
> works and measuring an analytical query vs the row-store, measured by the columnar harness reporting (a) the
> mirror result == the row-store result (correctness), (b) timing for both paths, and (c) the EXPLAIN proving
> `DuckDBScan` on the mirror vs `Seq Scan` on the row table.

## Context

ROADMAP `### M6` (dep M1 ✅): permissive columnar/HTAP via pg_mooncake (MIT). The discovery blueprint
`.claude/knowledge-base/discoveries/blueprints/m6-columnar-htap-blueprint.md` (SHIPPABLE_WITH_CAVEATS 89)
resolved risk #1 (pg_mooncake Makefile lists pg14–18 → PG17 supported), proved the capability live on the
canonical distribution (`avg(price) FROM trades_iceberg` = 208.5; EXPLAIN DuckDBScan vs SeqScan), and found the
build is heavy (Rust+pgrx+DuckDB+pg_duckdb; no pg17 prebuilt .so). Per measurement-first (ADR 0002) + the M7-S2
precedent, M6 proves+measures the capability and gates the heavy shipped-image embedding. The honest D2 framing
(ADR 0002): TheoDB's columnar is a DuckDB+Iceberg **lakehouse on disk**, a competitive-different bet from
AlloyDB's **in-memory** columnar (the in-memory peers Citus/Hydra are AGPL-barred by D1) — not a copy.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `packaging/Dockerfile.columnar` (NEW) | 0 | — | (to be created) throwaway pg_mooncake-on-PG17 measurement image (FROM pgduckdb:17-main + built pg_mooncake-pg17); NOT the shipped image | — |
| `benchmarks/theodb_bench/columnar.py` (NEW) | 0 | — | (to be created) columnar-vs-row measurement driver | — |
| `benchmarks/theodb_bench/db.py` | 232 | `7a738c3` (2026-06-28) | `VectorDB` adapter | existing methods backward-compatible; columnar helpers additive |
| `benchmarks/tests/test_columnar.py` (NEW) | 0 | — | (to be created) columnar measurement test, gated on pg_mooncake present | — |
| `packaging/license-sweep.sh` | 75 | `7a738c3` (2026-06-28) | reproducible AGPL sweep (M1 + BM25 candidates) | existing checks stay; pg_mooncake/pg_duckdb MIT verdicts additive |
| `docs/packaging/license-audit.md` | (exists) | — | committed license evidence | append a columnar-deps section |
| `docs/analytics/columnar-htap.md` (NEW) | 0 | — | (to be created) row-vs-columnar plan doc + D2 honesty + measured numbers | — |
| `docs/benchmarks/m6-columnar-vs-row.md` (NEW) | 0 | — | (to be created) the measured analytical-query report | — |
| `.github/workflows/ci.yml` | (exists) | — | CI | existing jobs stay; add `columnar-measure` job |
| `CHANGELOG.md` | (exists) | — | Public contract | `[Unreleased]` gets the M6 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `VectorDB` in `benchmarks/theodb_bench/db.py`
  - **Callers:** `harness.py`, `__main__.py`, `tests/test_db.py`, `tests/test_integration.py`, `tests/test_bm25.py`.
  - **External:** no. New columnar helpers (`pg_mooncake_available`, `create_columnstore_mirror`, `explain_plan`, `timed_query`) are additive (no existing-method signature change).
- **Symbol:** `license-sweep.sh` — invoked manually + CI; additive MIT verdicts for pg_mooncake/pg_duckdb do not change its exit-contract.

Enumerated via `grep -rln 'VectorDB\|license-sweep' --include='*.py' --include='*.sh' --include='*.yml' benchmarks/ packaging/ .github/`.

### Domain glossary

- **columnstore mirror** — a pg_mooncake table (`CALL mooncake.create_table('mirror','base')`) that auto-syncs from a row table and stores columnar in Iceberg/DuckDB; queried like a normal table.
- **DuckDBScan** — the `Custom Scan` node pg_duckdb injects so the columnstore-mirror query executes in DuckDB's vectorized columnar engine (vs the heap `Seq Scan` for the row table) — the observable row-vs-columnar plan choice.
- **lakehouse (D2)** — columnar storage on disk (DuckDB+Iceberg), as opposed to AlloyDB's in-memory column store. The honest TheoDB framing.
- **HTAP** — hybrid transactional/analytical processing: fast analytics over live transactional data.
- **measurement-first** — adopt a heavy dependency into the shipped image only after a reproducible benchmark justifies it (ADR 0002; M7-S2 precedent).

### Architecture boundaries affected

Per `rules/architecture.md`: pg_mooncake/pg_duckdb would be **infrastructure** extensions inside the DB image,
but this slice keeps them in a **throwaway** measurement image (`packaging/Dockerfile.columnar`), NOT the
shipped `Dockerfile`, so the distribution's dependency surface is unchanged until the measurement justifies the
heavy build (measurement-first + YAGNI). The columnar harness is **dev-only tooling** (client via `psycopg`);
the new helpers ride the existing `VectorDB` adapter (DIP). No product-layer code.

## Prior Art & Related Work

- **Internal blueprint (design source):** `.claude/knowledge-base/discoveries/blueprints/m6-columnar-htap-blueprint.md` — pg_mooncake capability, PG17 support, DuckDBScan-vs-SeqScan plan, build cost, D2 honesty.
- **Internal (M7-S2 precedent):** `packaging/Dockerfile.bm25` + `benchmarks/tests/test_bm25.py` + `packaging/license-sweep.sh` § (c) — the throwaway-image + measurement-first + reproducible-license pattern this slice mirrors.
- **Reference:** `.claude/knowledge-base/references/pg_mooncake/README.md` (columnstore-mirror quickstart), `Makefile` (pg17 support), `Dockerfile` (build recipe), `LICENSE` (MIT); `.claude/knowledge-base/references/duckdb/README.md` (columnar engine).
- **External:** pg_mooncake (`https://github.com/Mooncake-Labs/pg_mooncake`), pg_duckdb (`https://github.com/duckdb/pg_duckdb`), DuckDB (`https://duckdb.org/why_duckdb`), AlloyDB columnar SOTA (`https://cloud.google.com/blog/...alloydb-for-postgresql-columnar-engine`).
- **ADR:** `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (the lakehouse-vs-in-memory D2 framing).

## Objective

- [ ] A throwaway `packaging/Dockerfile.columnar` provides pg_mooncake on PostgreSQL 17 (built from source; pg_duckdb base) — NOT the shipped image.
- [ ] `VectorDB` columnar helpers: detect pg_mooncake, create a columnstore mirror, capture EXPLAIN, time a query.
- [ ] The columnar harness runs an analytical query (scan-heavy aggregate/group-by over a non-trivial row count) on the mirror vs the row-store and reports correctness (results match) + timing for both.
- [ ] The row-vs-columnar plan choice is captured: `DuckDBScan` on the mirror vs `Seq Scan` on the row table (DoD-2).
- [ ] license-sweep asserts pg_mooncake + pg_duckdb are MIT (permissive); doc records it.
- [ ] `docs/analytics/columnar-htap.md` documents the plan choice + the D2 honesty (lakehouse, not in-memory); `docs/benchmarks/m6-columnar-vs-row.md` records the measured numbers.
- [ ] CI `columnar-measure` job builds the throwaway image + runs the measurement.

## ADRs

### D1 — Prove + measure pg_mooncake columnar; gate shipped-image adoption on the measurement

**Decision:** Build pg_mooncake on PostgreSQL 17 in a **throwaway** image and measure the columnar analytical
query vs the row-store. Do NOT add pg_mooncake/pg_duckdb to the shipped `Dockerfile` in this slice — that
heavy-build adoption is gated on this measurement.

**Rationale:** measurement-first (ADR 0002); the build is heavy (Rust+pgrx+DuckDB+pg_duckdb); the M7-S2 BM25
precedent measured-then-gated. Proves the capability + the plan choice without bloating the shipped image's
dependency surface (YAGNI) until justified.

**Alternatives considered:** *Ship pg_mooncake in the distribution now* — rejected (premature; heavy build;
sync-overhead unmeasured). *Measure only on the official PG18 image* — rejected if the PG17 build succeeds (we
measure on TheoDB's real PG17 engine); the official image is the fallback substrate only if the build is
infeasible (documented honestly). *In-memory columnar (Citus/Hydra)* — rejected (AGPL, D1; different bet).

**Consequences:** M6 delivers capability + measurement + plan evidence; shipped image unchanged; adoption is a
future, evidence-gated ADR.

### D2 — Honest framing: DuckDB+Iceberg lakehouse, NOT in-memory (PRD D2)

**Decision:** All docs state plainly that TheoDB's columnar is a **DuckDB+Iceberg lakehouse on disk**, a
competitive-different bet from AlloyDB's **in-memory** columnar engine — not a literal copy.

**Rationale:** CLAUDE.md TheoDB rule 7 (honesty about trade-offs) + ADR 0002 (the in-memory peers are AGPL-barred
by D1, forcing the permissive lakehouse bet). Performance is a claim, not opinion (rule 5) — numbers are measured
or `UNBENCHMARKED`.

**Alternatives considered:** *Claim AlloyDB columnar parity* — rejected (dishonest; different architecture).

**Consequences:** the doc frames the columnar pillar honestly; no in-memory parity is claimed.

### D3 — Row-vs-columnar plan choice proven via EXPLAIN (DoD-2)

**Decision:** The DoD-2 "plan choice documented" is satisfied by a captured EXPLAIN artifact: the columnstore
mirror query plans as `Custom Scan (DuckDBScan)` (DuckDB vectorized columnar) while the same query on the row
table plans as `Seq Scan` + `Aggregate`. The harness asserts the mirror plan contains `DuckDBScan`.

**Rationale:** deterministic, observable evidence (vs prose). Matches the live discovery finding.

**Alternatives considered:** *Document the plan choice in prose only* — rejected (not verifiable). 

**Consequences:** the test asserts the plan-choice artifact; the doc embeds it.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Heavy build (Rust+pgrx+DuckDB+pg_duckdb) — slow/fragile in CI | Medium | Throwaway image only (not shipped); `timeout-minutes` on the CI job; if the PG17 build is infeasible, the official pg_mooncake distribution is the documented fallback substrate (honest) | DB |
| Row↔columnar sync overhead (risk #2) | Medium | Documented honestly; the measurement reports it; `UNBENCHMARKED` for the sync cost until measured | Bench |
| pg_mooncake requires `shared_preload_libraries=pg_duckdb,pg_mooncake` + `wal_level=logical` | Medium | Set in the throwaway image; the test skips cleanly if the extension is absent (no silent green) | DB |
| Over-claiming AlloyDB in-memory parity | Low | D2 honesty in every doc; no parity claim | Docs |
| The columnar win may be small on a tiny fixture | Low | Use a non-trivial row count (≥ 100k) for a scan-heavy aggregate; report measured numbers honestly (rule 5) | Bench |

## Unresolved Questions

- Q1 — Measure on the PG17 build or the official PG18 image? Resolved at plan time: **PG17 throwaway build is primary** (TheoDB's real engine); the official PG18 image is the documented fallback only if the build is infeasible.
- Q2 — Row count for the analytical workload? Resolved: ≥ 100k rows, a scan-heavy aggregate/group-by (where columnar shows its advantage); the exact number recorded in the report.
- Q3 — Ship pg_mooncake in theo-db:dev now? Resolved: no — measurement-first gate (D1); adoption is a future ADR.

## Dependencies

M6 adds **no new runtime dependency to the shipped distribution** (the shipped `Dockerfile` is unchanged — D1).
pg_mooncake/pg_duckdb are built only in the throwaway `Dockerfile.columnar` (measurement substrate).

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| `pg_mooncake` (throwaway image only) | latest (pg17 build) | columnstore mirror | MIT | identified permissive; NOT in shipped image |
| `pg_duckdb` (throwaway base `pgduckdb:17-main`) | 17-main | DuckDB-in-Postgres engine | MIT | dependency of pg_mooncake; throwaway only |
| `psycopg2-binary` (harness, dev-only) | as in requirements.txt | DB client | LGPL (dev-only) | already a dev dep |

No CVE audit delta on the shipped distribution: zero new declared runtime dependencies in the shipped image.

## Dependency Graph

```
Phase 1 (Dockerfile.columnar pg17 + license-sweep MIT verdicts) ──▶ Phase 3 (report + doc + CI + CHANGELOG)
                                                                          ▲
Phase 2 (columnar helpers + measurement driver + test) ───────────────────┘
```

## Phase 1: Throwaway pg_mooncake-PG17 image + license verdicts

**Objective:** Provide pg_mooncake on PG17 (throwaway) + the reproducible MIT license verdicts.

### T1.1 — `packaging/Dockerfile.columnar` (pg17) + license-sweep MIT verdicts

#### Objective
Create the throwaway pg_mooncake-on-PG17 image and add pg_mooncake/pg_duckdb MIT verdicts to the license sweep.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — creates `packaging/Dockerfile.columnar` (FROM `pgduckdb/pgduckdb:17-main`, build
   pg_mooncake-pg17 via cargo-pgrx + the duckdb_mooncake submodule, configure
   `shared_preload_libraries=pg_duckdb,pg_mooncake` + `wal_level=logical`); extends `license-sweep.sh` with
   pg_mooncake + pg_duckdb MIT verdicts (re-fetched from canonical repos).

2. **Why it is necessary now** — the throwaway image is the measurement substrate (Phase 2 runs against it);
   the license verdicts are the D1 evidence (MIT permissive, no AGPL) for the columnar deps.

#### Evidence
- Build recipe: `.claude/knowledge-base/references/pg_mooncake/Dockerfile` (cargo-pgrx@0.16.1, `cargo pgrx init --pg17`, `make package`, runtime base `pgduckdb/pgduckdb:NN-main` + the 3 GUCs).
- PG17 support: `.claude/knowledge-base/references/pg_mooncake/Makefile:20` (pg14–18).
- License: `.claude/knowledge-base/references/pg_mooncake/LICENSE` (MIT); sweep pattern `packaging/license-sweep.sh:42-71`.

#### Files to edit
```
packaging/Dockerfile.columnar — (NEW) FROM pgduckdb/pgduckdb:17-main + build pg_mooncake-pg17 + GUCs (throwaway)
packaging/license-sweep.sh — add pg_mooncake + pg_duckdb MIT verdicts (§ (d))
docs/packaging/license-audit.md — append a columnar-deps section
```

#### Deep file dependency analysis
- `Dockerfile.columnar` (NEW): mirrors the upstream recipe but targets pg17; not referenced by the shipped image.
- `license-sweep.sh` (Baseline row, invariant: existing apt/cargo/BM25 checks + exit-contract preserved): additive `bm25_license`-style fetch for pg_mooncake/pg_duckdb.
- `docs/packaging/license-audit.md` (Baseline row): additive section.

#### Deep Dives
- **Build:** `cargo install --locked cargo-pgrx@0.16.1` + `cargo pgrx init --pg17=$(which pg_config)` + `git clone --recurse-submodules` + `PG_VERSION=pg17 make package`, then copy the built artifacts onto a `pgduckdb/pgduckdb:17-main` runtime + append the 3 GUCs to `postgresql.conf.sample`.
- **License verdict:** fetch `pg_mooncake/LICENSE` (MIT) + `pg_duckdb/LICENSE` (MIT) from canonical repos; UNVERIFIED if unfetchable (never assume).

#### Tasks
1. Write `packaging/Dockerfile.columnar` (pg17 build + GUCs).
2. Add pg_mooncake/pg_duckdb MIT verdicts to `license-sweep.sh` § (d).
3. Append the columnar-deps section to `docs/packaging/license-audit.md`.

#### TDD
```
RED:     `bash packaging/license-sweep.sh` before the columnar block → no pg_mooncake verdict line.
GREEN:   after the block, the sweep prints "pg_mooncake: MIT (permissive)" + "pg_duckdb: MIT (permissive)" and exits 0.
REFACTOR: reuse the existing fetch helper; else "None expected".
VERIFY:  bash packaging/license-sweep.sh | grep -ci 'pg_mooncake.*permissive\|pg_duckdb.*permissive'  (expect >= 2)
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — a Dockerfile + a shell sweep + a markdown doc; no shared mutable state.

#### Acceptance Criteria
- [ ] `docker build -f packaging/Dockerfile.columnar -t theo-db-columnar .` exits `0` (pg_mooncake-pg17 builds) OR, if the heavy build is infeasible, the fallback substrate (official image) is documented and the test points at it — recorded honestly in the report.
- [ ] `bash packaging/license-sweep.sh` prints pg_mooncake=MIT + pg_duckdb=MIT verdicts and exits `0` — `bash packaging/license-sweep.sh | grep -c -iE 'pg_mooncake.*permissive|pg_duckdb.*permissive'` returns `>= 2`.
- [ ] `docs/packaging/license-audit.md` has a columnar-deps section — `grep -c -i 'pg_mooncake' docs/packaging/license-audit.md` returns `> 0`.
- [ ] Pass: lint — `bash -n packaging/license-sweep.sh` exits `0`.

#### DoD
- [ ] All tasks completed and validated
- [ ] license-sweep exits 0 with the columnar MIT verdict lines
- [ ] CHANGELOG `[Unreleased]` updated
- [ ] File-size budget respected

## Phase 2: Columnar measurement (mirror vs row-store)

**Objective:** Measure an analytical query on the columnstore mirror vs the row-store + capture the plan choice.

### T2.1 — Columnar helpers + measurement driver + test

#### Objective
Add `VectorDB` columnar helpers + a driver that measures the analytical query (mirror vs row) + an integration test gated on pg_mooncake.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — adds `VectorDB.pg_mooncake_available()`, `create_columnstore_mirror(mirror, base)`,
   `explain_plan(sql)`, `timed_query(sql)`; creates `benchmarks/theodb_bench/columnar.py::run_columnar_vs_row`
   that seeds a row table (≥ 100k rows), creates a mirror, runs a scan-heavy aggregate on both, returns
   {row: {result, ms, plan}, columnar: {result, ms, plan}}; adds `benchmarks/tests/test_columnar.py` gated on
   pg_mooncake present.

2. **Why it is necessary now** — this is the DoD-1 measurement (analytical query vs row-store) + the DoD-2 plan
   evidence (DuckDBScan vs SeqScan), the functional core of M6.

#### Evidence
- Adapter pattern: `benchmarks/theodb_bench/db.py` (existing helpers); test gating pattern `benchmarks/tests/test_bm25.py`.
- Columnstore surface: `.claude/knowledge-base/references/pg_mooncake/README.md` (`CALL mooncake.create_table`).
- Plan choice: blueprint Q4 (DuckDBScan vs SeqScan).

#### Files to edit
```
benchmarks/theodb_bench/db.py — add pg_mooncake_available/create_columnstore_mirror/explain_plan/timed_query (additive)
benchmarks/theodb_bench/columnar.py — (NEW) run_columnar_vs_row driver
benchmarks/tests/test_columnar.py — (NEW) measurement test, skip cleanly if pg_mooncake absent
```

#### Deep file dependency analysis
- `db.py` (Baseline row, invariant: existing methods backward-compatible): additive helpers; existing vector/FTS/hybrid/BM25 methods untouched.
- `columnar.py` (NEW): pure driver over the adapter.
- `test_columnar.py` (NEW): `integration` marker; `pytest.skip` if `pg_mooncake` not available (no silent green).

#### Deep Dives
- **Workload:** a `metrics(id bigint, category text, amount double precision)` row table seeded with ≥ 100k rows; the analytical query `SELECT category, count(*), avg(amount) FROM <t> GROUP BY category` on the mirror vs the row table.
- **Correctness:** assert the mirror result set == the row result set (same groups + aggregates within float tolerance).
- **Plan (DoD-2):** `explain_plan` over the mirror query asserts the plan text contains `DuckDBScan`; over the row query contains `Seq Scan`.
- **Timing:** `timed_query` wraps `time.perf_counter` around each; both reported (the comparison is honest — small fixtures may not show a columnar win; report the numbers).
- **Skip:** `pg_mooncake_available` → `CREATE EXTENSION pg_mooncake CASCADE` succeeds + `mooncake.create_table` exists; else skip with a clear reason.

#### Pseudo-code / Signatures
```pseudocode
# db.py
def pg_mooncake_available(self) -> bool: ...        # extension creatable
def create_columnstore_mirror(self, mirror, base): CALL mooncake.create_table(mirror, base)
def explain_plan(self, sql) -> str: EXPLAIN <sql> -> joined text
def timed_query(self, sql) -> (rows, ms)
# columnar.py
def run_columnar_vs_row(db, n=100000) -> dict:
    db.seed metrics(n); db.create_columnstore_mirror('metrics_cs','metrics')
    q = "SELECT category, count(*) c, avg(amount) a FROM %s GROUP BY category ORDER BY category"
    row = db.timed_query(q % 'metrics'); col = db.timed_query(q % 'metrics_cs')
    return {row:{result,ms,plan=explain_plan(q%'metrics')}, columnar:{result,ms,plan=explain_plan(q%'metrics_cs')}}
```

#### Tasks
1. Add the columnar helpers to `db.py`.
2. Create `columnar.py::run_columnar_vs_row`.
3. Write `test_columnar.py` (gated; asserts correctness match + DuckDBScan plan + finite timings).

#### TDD
```
RED:     test_columnar_skips_without_extension() — pg_mooncake absent → clean skip (no silent green).
RED:     test_columnar_mirror_matches_row() [integration] — mirror aggregate == row aggregate (correctness). MUST fail before columnar.py.
RED:     test_columnar_uses_duckdb_plan() — EXPLAIN of the mirror query contains 'DuckDBScan'; the row query contains 'Seq Scan' (DoD-2).
RED:     test_columnar_reports_timings() — both row.ms and columnar.ms are finite/positive.
GREEN:   Implement Dockerfile.columnar + db helpers + driver so all pass against the throwaway image.
REFACTOR: fold the seed SQL into a helper; else "None expected".
VERIFY:  docker build -f packaging/Dockerfile.columnar -t theo-db-columnar . && (run w/ preload) && cd benchmarks && pytest -m integration tests/test_columnar.py -q
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — read-only analytical queries + a one-time seed; no shared mutable state under concurrency.

#### Acceptance Criteria
- [ ] `test_columnar_skips_without_extension` skips cleanly on an image without pg_mooncake — `pytest -m integration tests/test_columnar.py -k skips` exits `0`.
- [ ] Against the throwaway image, `test_columnar_mirror_matches_row` passes — the columnstore mirror aggregate equals the row-store aggregate (`pytest -k matches` exits `0`).
- [ ] `test_columnar_uses_duckdb_plan` passes — the mirror query plan contains `DuckDBScan`, the row query `Seq Scan` (`pytest -k plan` exits `0`).
- [ ] `test_columnar_reports_timings` passes — both timings finite/positive.
- [ ] Pass: lint — `cd benchmarks && ruff check theodb_bench tests/test_columnar.py`; dead-code `vulture` clean.
- [ ] Pass: size — every changed/new file `wc -l` < 500.

#### DoD
- [ ] All tasks completed and validated
- [ ] Columnar measurement green against the throwaway image; skip green without it
- [ ] Zero lint warnings
- [ ] CHANGELOG `[Unreleased]` updated
- [ ] File-size budget respected

## Phase 3: Measured report + doc + CI

**Objective:** Record the measured numbers + the plan choice + the D2 honesty; gate it in CI.

### T3.1 — `docs/benchmarks/m6-columnar-vs-row.md` + `docs/analytics/columnar-htap.md` + `columnar-measure` CI job

#### Objective
Run the measurement, write the report + the analytics doc, add the CI job.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — runs `run_columnar_vs_row` against the throwaway image; writes
   `docs/benchmarks/m6-columnar-vs-row.md` (measured row vs columnar timings + the EXPLAIN artifact + the
   correctness note) and `docs/analytics/columnar-htap.md` (how to enable pg_mooncake, the row-vs-columnar plan
   choice, the D2 honesty); adds a `columnar-measure` CI job (build throwaway image + run the measurement).

2. **Why it is necessary now** — the wiring triad: the report is the measured evidence (DoD-1), the doc is the
   observable contract (DoD-2 + DoD-3 honesty), CI keeps it reproducible. Per `public-copy.md`, only measured
   numbers; the D2 honesty is explicit.

#### Evidence
- Report/doc conventions: `docs/benchmarks/` (M2/M7), `docs/analytics/` (new); D2 framing `docs/adr/0002-...md`.
- CI job pattern: `.github/workflows/ci.yml` `bm25-measure` job.

#### Files to edit
```
docs/benchmarks/m6-columnar-vs-row.md — (NEW) measured row vs columnar timings + EXPLAIN artifact + correctness
docs/analytics/columnar-htap.md — (NEW) enable pg_mooncake + row-vs-columnar plan choice + D2 honesty
.github/workflows/ci.yml — add columnar-measure job (build throwaway image + run measurement) with timeout-minutes
CHANGELOG.md — [Unreleased] M6 entry
```

#### Deep file dependency analysis
- `docs/benchmarks/m6-columnar-vs-row.md` (NEW): records T2.1's measured output.
- `docs/analytics/columnar-htap.md` (NEW): the DoD-2 plan-choice doc + DoD-3 honesty.
- `.github/workflows/ci.yml` (invariant: existing jobs stay): additive `columnar-measure` job; `timeout-minutes` (heavy build).

#### Deep Dives
- **Report honesty:** report both timings + the speedup ratio if any; if columnar does not beat row on the fixture, say so plainly (rule 3). The EXPLAIN artifact (DuckDBScan vs SeqScan) is the load-bearing DoD-2 evidence regardless of timing.
- **Doc honesty (D2):** state lakehouse DuckDB+Iceberg on disk, NOT in-memory; competitive-different bet (ADR 0002).
- **CI:** the `columnar-measure` job builds `Dockerfile.columnar` (heavy → generous `timeout-minutes`), starts it with the preload GUCs, runs `pytest -m integration tests/test_columnar.py`.

#### Tasks
1. Run the measurement; write `docs/benchmarks/m6-columnar-vs-row.md`.
2. Write `docs/analytics/columnar-htap.md` (plan choice + D2 honesty + enable steps).
3. Add the `columnar-measure` CI job; add the CHANGELOG entry.

#### TDD
```
RED:     CI job missing before edit (yaml.safe_load assertion).
GREEN:   `python3 -c "import yaml; assert 'columnar-measure' in yaml.safe_load(open('.github/workflows/ci.yml'))['jobs']"` exits 0; both docs exist with measured numbers + the EXPLAIN artifact.
REFACTOR: none expected.
VERIFY:  python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))" && test -f docs/benchmarks/m6-columnar-vs-row.md && test -f docs/analytics/columnar-htap.md
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — markdown + YAML; no concurrent state.

#### Acceptance Criteria
- [ ] `docs/benchmarks/m6-columnar-vs-row.md` exists with measured row vs columnar timings + the DuckDBScan-vs-SeqScan EXPLAIN artifact + the correctness note — `grep -c -iE 'duckdbscan|seq scan|columnar' docs/benchmarks/m6-columnar-vs-row.md` returns `> 0`.
- [ ] `docs/analytics/columnar-htap.md` documents the row-vs-columnar plan choice + the D2 honesty (lakehouse, not in-memory) — `grep -c -iE 'lakehouse|in-memory|DuckDBScan' docs/analytics/columnar-htap.md` returns `> 0`.
- [ ] CI `columnar-measure` job parses + present with `timeout-minutes` — `python3 -c "import yaml; assert 'columnar-measure' in yaml.safe_load(open('.github/workflows/ci.yml'))['jobs']"` exits `0`.
- [ ] No unbenchmarked perf claim — `grep -ciE 'faster than|outperforms|[0-9]+x ' docs/benchmarks/m6-columnar-vs-row.md` returns `0` (measured numbers only, Rule 5); D2 honesty present.
- [ ] Pass: size — changed files `wc -l` within budget.

#### DoD
- [ ] All tasks completed and validated
- [ ] Report + analytics doc committed with measured numbers + plan artifact
- [ ] CI job parses + runs locally-validated steps
- [ ] CHANGELOG `[Unreleased]` updated
- [ ] File-size budget respected

## Coverage Matrix

| # | Gap / Requirement (ROADMAP M6 DoD + blueprint) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Columnar storage (pg_mooncake) enabled for selected tables, analytical query measured vs row-store (DoD-1) | T1.1, T2.1, T3.1 | throwaway pg17 image + columnstore mirror + measured aggregate vs row + report |
| 2 | Row vs columnar plan choice documented (DoD-2) | T2.1, T3.1 | EXPLAIN artifact: DuckDBScan (mirror) vs Seq Scan (row) + analytics doc |
| 3 | Honesty: lakehouse DuckDB+Iceberg, not in-memory (DoD-3 / D2) | D2, T3.1 | analytics doc + report state the honest delta |
| 4 | Permissive (MIT) — no AGPL | T1.1 | license-sweep pg_mooncake+pg_duckdb MIT verdicts + audit doc |
| 5 | PG17 support (risk #1) | T1.1 | Dockerfile.columnar builds pg_mooncake-pg17 (Makefile-supported) |
| 6 | Correctness (mirror == row) | T2.1 | `test_columnar_mirror_matches_row` |
| 7 | Distribution unchanged until measurement justifies (YAGNI) | T1.1 | pg_mooncake only in the throwaway `Dockerfile.columnar` (D1); shipped image untouched |
| 8 | Reproducible in CI | T3.1 | `columnar-measure` job |
| 9 | Sync-overhead honesty (risk #2) | T3.1 | report/doc note the sync model + `UNBENCHMARKED` sync cost |

**Coverage: 9/9 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] `bash packaging/license-sweep.sh` exits `0` with the pg_mooncake/pg_duckdb MIT verdict lines
- [ ] `docker build -f packaging/Dockerfile.columnar -t theo-db-columnar .` exits `0` (or the documented fallback substrate is used + recorded)
- [ ] Columnar measurement green against the throwaway image; skip green without it
- [ ] Measured report committed (`docs/benchmarks/m6-columnar-vs-row.md`) — numbers only, no unbenchmarked claim; the DuckDBScan-vs-SeqScan EXPLAIN artifact embedded
- [ ] Analytics doc committed (`docs/analytics/columnar-htap.md`) — row-vs-columnar plan + D2 honesty
- [ ] Zero lint warnings — `cd benchmarks && ruff check theodb_bench tests`
- [ ] File-size budget respected (per `rules/architecture.md`)
- [ ] CHANGELOG.md updated under `[Unreleased]` (Unbreakable Rule 6)
- [ ] Backward compatibility preserved — shipped `Dockerfile` unchanged; `VectorDB` existing methods intact
- [ ] Runtime-metric proof — the columnar harness is observed reporting both timings + the DuckDBScan plan against the real pg_mooncake build (not just compiling)
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge

## Failure scenarios (external I/O)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| pg_mooncake/pg_duckdb extension (in-DB, requires preload) | extension not loaded | run against an image without pg_mooncake | `test_columnar_skips_without_extension` skips with a clear reason (no silent green) |
| PostgreSQL (`psycopg`, throwaway container) | container not ready | run before healthy | `VectorDB.connect/ping` raises a clear error; CI waits on readiness |
| columnstore mirror sync | mirror queried immediately after insert | seed then query the mirror | the mirror reflects the data (sub-second freshness) OR the test waits for sync; correctness asserted |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate the columnar capability, measurement, and plan evidence end-to-end.

### Execution
```
bash packaging/license-sweep.sh                                       # pg_mooncake/pg_duckdb MIT + exit 0
docker build -f packaging/Dockerfile.columnar -t theo-db-columnar .   # pg_mooncake-pg17 build
docker run -d --name m6-it -e POSTGRES_PASSWORD=postgres -p <port>:5432 theo-db-columnar
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_columnar.py -q                     # mirror==row + DuckDBScan plan + timings
ruff check theodb_bench tests/test_columnar.py
# no regression on the shipped image:
docker build -t theo-db:dev . && PGPORT=<port2> bash smoke.sh
```

### Acceptance Criteria
- [ ] license-sweep prints the columnar MIT verdicts + exits 0
- [ ] Columnar image builds (or fallback substrate documented) — `docker images theo-db-columnar` present OR report records the fallback; `pytest -m integration tests/test_columnar.py -k matches` exits `0` (mirror == row).
- [ ] EXPLAIN proves DuckDBScan (mirror) vs Seq Scan (row) — `pytest -m integration tests/test_columnar.py -k plan` exits `0` (DoD-2).
- [ ] Both timings reported — `pytest -m integration tests/test_columnar.py -k timings` exits `0`; the report states measured numbers only + the D2 honesty.
- [ ] skip test green when pg_mooncake absent — `pytest -m integration tests/test_columnar.py -k skips` exits `0` (clean skip, no silent pass).
- [ ] Shipped image smoke still green — `docker build -t theo-db:dev . && PGPORT=<p> bash smoke.sh` exits `0` (no regression; distribution unchanged).
