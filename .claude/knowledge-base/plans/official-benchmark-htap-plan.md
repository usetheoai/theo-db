---
slug: official-benchmark-htap
milestone_id: M130
created_at: 2026-07-21
goal: Ship TheoDB's HTAP benchmark harness (BenchBase CH-benCHmark — TPC-C + 22 OLAP queries in one phase) as an external Docker driver against self-hosted TheoDB, deriving the tpmC/QphH dual metric from summary.json with the retained wrap layer (OLAP result validation + run-to-run dispersion)
---

# Plan — M130 Official benchmark: HTAP pillar (CH-benCHmark via BenchBase)

## Goal

Deliver TheoDB's HTAP benchmark harness running the field-standard **CH-benCHmark** (TPC-C's transactional mix +
22 TPC-H-style analytical queries against the SAME schema, in one mixed work-phase) via **BenchBase** (cmu-db,
Apache-2.0) as an external out-of-tree Docker driver against self-hosted TheoDB PG17, deriving the **dual tpmC/QphH
metric** from BenchBase's `summary.json`, with the retained wrap layer (OLAP result validation under contention +
run-to-run dispersion) that BenchBase lacks. Applies the ADR-0050 adopt-and-wrap pattern.

**Single metric:** a reproducible HTAP artifact with a **MEASURED CH-benCHmark run** against self-hosted TheoDB
(BenchBase `summary.json` parsed → derived tpmC-proxy + OLAP throughput/QphH-proxy) + a wrap-layer **OLAP
result-consistency check** across the analytical queries, recorded in `docs/benchmarks/m130-htap.md`. If BenchBase
cannot build/run on the box, the recipe is the documented operational follow-up and the artifact is `UNBENCHMARKED`
(no fabricated numbers).

## Context

Implements ADR-0050 for the HTAP pillar — the fourth and final application of the pattern. The discovery blueprint
(`knowledge-base/discoveries/blueprints/official-db-benchmark-harness-blueprint.md`) selected **CH-benCHmark via
BenchBase** (the canonical mixed OLTP+OLAP benchmark; TPC-C 45/43/4/4/4 mix + Q1–Q22 on one schema), and found the
HTAP tool provides NO significance test, NO byte-identical regression, and NO OLAP result oracle (it validates
timing + completion only) — so the run-to-run dispersion + the OLAP result-consistency check are retained. BenchBase
is **Apache-2.0** but ships **Java 23** (non-LTS, `2023-SNAPSHOT`, no release tags) → pin a git SHA and run it inside
a **Java-23 Docker container** (no host toolchain liability), as an external out-of-tree driver (never vendored into
the TheoDB tree). TheoDB's HTAP path IS PostgreSQL's + theodb_columnar (extension), so this proves the mixed
OLTP+OLAP wire-compatible gate.

## Baseline Context

Repo state: git sha `9823125`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `benchmarks/run_m130_htap.py` | — | (NEW) | Driver: orchestrate BenchBase CH-benCHmark (Docker → `summary.json`), parse per-txn throughput + latency, derive tpmC/QphH proxies, run the OLAP result-consistency check, emit results + run-to-run dispersion (CV). |
| `benchmarks/htap/benchbase_chbenchmark.sh` | — | (NEW) | Our container-entry script: clone BenchBase at a pinned SHA inside a Java-23 image, `mvnw package -P postgres`, run `-b tpcc,chbenchmark` against host TheoDB — no BenchBase source vendored. |
| `benchmarks/htap/chbenchmark_config.xml` | — | (NEW) | Our BenchBase config (scale, terminals, work-phase weights, PG JDBC to host TheoDB). |
| `benchmarks/theodb_bench/test_htap.py` | — | (NEW) | Unit tests for the driver's summary.json parser + tpmC/QphH derivation + the OLAP-consistency check + the CV wiring. |
| `benchmarks/run_m129_oltp.py` | 148 | `coefficient_of_variation` (M129) | Reused — CV imported for run-to-run dispersion (the wrap-layer dispersion half). |

### Current callers / dependents (verified `file:line`)

- `benchmarks/run_m129_oltp.py:38` — `coefficient_of_variation(samples)` — the single-system run-to-run dispersion metric (M129), reused for the CH-benCHmark repeated-run throughput.
- `benchmarks/theodb_bench/significance.py:22` — `paired_significance(a, b, *, seed, n_resamples)` — the A/B wrap-layer significance, available for a TheoDB-vs-baseline comparison if a second engine is run (not required for the single-system MEASURED gate).
- Docker: `benchmarks/run_m129_oltp.py:77` established the external-Docker-driver pattern (`docker run --rm --network host`) reused here for the Java-23 BenchBase container.
- theodb_columnar TableAM (M99+) — the analytical side of the mixed workload may target columnar storage; the OLTP side is the PG heap path.

### Domain glossary

- **CH-benCHmark** — the DBTest-2011 mixed HTAP benchmark: TPC-C's transactional schema+mix runs concurrently with 22 TPC-H-derived analytical queries (Q1–Q22) over the same tables.
- **BenchBase** — cmu-db's multi-DBMS benchmark framework (OLTP-Bench successor, Apache-2.0); runs the workload and emits `summary.json` (per-txn throughput + latency percentiles).
- **tpmC / QphH (proxy)** — the canonical TPC-C throughput / TPC-H analytical-power dual metric; BenchBase does not emit them directly, so they are **derived** (new-order txn rate; analytical-query completion rate). Labeled "proxy", never audited tpmC/QphH.
- **OLAP result-consistency check** — the retained wrap-layer capability: assert the 22 analytical queries return internally consistent, non-empty, deterministic-shape results under transactional contention (BenchBase validates only timing + completion).
- **run-to-run dispersion (CV)** — the M129 coefficient of variation over repeated throughput samples (the single-system stability metric the HTAP tool lacks).

### Architecture boundaries affected

Per `rules/architecture.md`: pure benchmark tooling (Python driver + a shell container-entry + an XML config,
`benchmarks/`) — NO production Rust change. BenchBase (Apache-2.0, Java 23) runs as a separate Docker process
(never linked); the driver talks to TheoDB over the wire. No engine/API change.

## Prior Art & Related Work

- Blueprint (web-evidenced, 2026-07-20): CH-benCHmark protocol (TPC-C + Q1–Q22 one phase), BenchBase invocation
  (`./mvnw clean package -P postgres`, `java -jar benchbase.jar -b tpcc,chbenchmark -c config.xml`), `summary.json`
  shape, the Apache-2.0/Java-23 posture (pin SHA; Java 23 build liability), and the "no significance / no OLAP result
  oracle / seed-determinism unconfirmed" finding. Sources: cmu-db/benchbase README, CH-benCHmark DBTest-2011.
- Internal: the M129 driver shape (`benchmarks/run_m129_oltp.py` — argparse, `run()->dict`, UNBENCHMARKED path,
  external-Docker driver, `coefficient_of_variation`) is the direct template; `theodb_bench/significance.py` (M123)
  is the retained A/B significance.

## ADRs

### ADR M130-1 — BenchBase runs inside a pinned Java-23 Docker container (external out-of-tree driver)

**Decision:** the driver runs BenchBase CH-benCHmark inside a **Java-23 Docker container** that clones BenchBase at
a **pinned git SHA**, builds it (`mvnw package -P postgres`), and runs `-b tpcc,chbenchmark` against the host
TheoDB. No BenchBase source is vendored into the TheoDB tree; no Java toolchain is installed on the host.

**Rationale (cites the blueprint license-handling decision + `rules/parsimony-ladder.md` rung 3):** BenchBase is Apache-2.0 (permissive, license-safe
to *use*) but ships Java 23 (non-LTS build liability) with no release tags. Running it in a container isolates the
Java-23 liability (rung 3 — use the container platform feature, don't install Java 23 on the box) and the pinned SHA
gives reproducibility despite the `2023-SNAPSHOT` versioning. External-Docker-driver mirrors the M129 HammerDB
pattern.

**Alternatives rejected:**
- **Install Java 23 + Maven on the host** — REJECTED: pollutes the box with a non-LTS toolchain; the container isolates it.
- **Vendor a prebuilt benchbase.jar** — REJECTED: opaque provenance + no SHA reproducibility; build from the pinned source in-container.
- **A bespoke HTAP workload** — REJECTED: not field-standard (defeats the adopt purpose).

### ADR M130-2 — the dual metric is a DERIVED proxy; OLAP result-consistency is the retained oracle

**Decision:** the tpmC/QphH dual metric is **derived** from BenchBase's `summary.json` (TPC-C new-order rate →
tpmC-proxy; analytical-query completion rate → QphH-proxy) and **labeled "proxy"**, never audited tpmC/QphH.
Additionally, the driver runs a wrap-layer **OLAP result-consistency check** — the 22 analytical queries must return
internally consistent, non-empty, deterministic-shape results under contention — because BenchBase validates only
timing + completion, not OLAP result values.

**Rationale (cites blueprint Cross-cutting table + Rule 3):** BenchBase emits per-txn throughput/latency, not the
canonical dual metric, and has no OLAP result oracle; seed-level deterministic replay is unconfirmed. Deriving a
labeled proxy + adding the result-consistency check is the honest correctness half the tool lacks.

**Alternatives rejected:** reporting BenchBase throughput as "tpmC/QphH" unqualified — REJECTED (dishonest; those are
audited-TPC terms). Trusting BenchBase's timing-only validation as correctness — REJECTED (it never checks OLAP result values).

## Dependencies

`## Dependencies`: **none new** in the TheoDB tree — reuses `numpy` (already in `benchmarks/requirements.txt` via
`significance.py`) and the Python stdlib (`coefficient_of_variation`). BenchBase (Apache-2.0) + its Java-23
toolchain run entirely inside the Docker container (`eclipse-temurin:23` base or equivalent), pinned by SHA, never
vendored/linked into the shipped artifact. No crate/pip added.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| BenchBase CH-benCHmark running against self-hosted TheoDB via pinned Java-23 Docker | T1 (container-entry + driver orchestration) |
| Dual tpmC/QphH proxy derived from summary.json (MEASURED) | T2 (summary.json parser + proxy derivation) |
| Retained wrap layer — OLAP result-consistency check + run-to-run dispersion (CV) | T3 (result-consistency check + CV wiring) |
| Java 23 liability documented + seed-determinism honestly marked; UNBENCHMARKED clean-exit if BenchBase fails | T4 (honesty rails + evidence doc) |

## Phase 1 — BenchBase CH-benCHmark driver + summary.json → proxy

### T1.1 — container-entry script + driver orchestration (BenchBase in Java-23 Docker)

#### Why this step
BenchBase must run against host TheoDB without a host Java toolchain (ADR M130-1). Reasoning: a shell entry clones
BenchBase at a pinned SHA inside `eclipse-temurin:23`, builds with `mvnw package -P postgres`, and runs
`-b tpcc,chbenchmark -c config.xml`; the Python driver orchestrates the `docker run` (mirroring the M129 HammerDB
external-driver pattern at `run_m129_oltp.py:77`) and captures `summary.json`. If Docker/BenchBase is unavailable,
the driver clean-exits UNBENCHMARKED (no fabricated numbers).

#### Files to edit
- `benchmarks/run_m130_htap.py` (NEW); `benchmarks/htap/benchbase_chbenchmark.sh` (NEW); `benchmarks/htap/chbenchmark_config.xml` (NEW); `benchmarks/theodb_bench/test_htap.py` (NEW).

#### TDD
- RED: `test_run_benchbase_skips_cleanly_without_docker` — monkeypatch `shutil.which` → None; assert `run_benchbase(...)` returns `{"status": "BENCHBASE_SKIPPED", ...}` with "docker" in the reason.
- GREEN: implement `run_benchbase` guarding on `shutil.which("docker")`, else `docker run` the Java-23 image with the pinned-SHA entry script.
- REFACTOR: extract the `docker run` argv builder shared with the parse step.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `run_benchbase` returns `BENCHBASE_SKIPPED` (no docker) or `BENCHBASE_ERRORED` (build/run failure) or a dict with the parsed `summary.json` path — never a fabricated number.
- No BenchBase source committed to the tree (only our `.sh` + `.xml`).

#### DoD
- `python3 -m pytest benchmarks/theodb_bench/test_htap.py -q` green; `git ls-files benchmarks/htap/` shows only our two files.

### T2.1 — summary.json parser + tpmC/QphH proxy derivation

#### Why this step
BenchBase emits per-txn throughput/latency, not the canonical dual metric (ADR M130-2). Reasoning: parse
`summary.json`, extract the TPC-C new-order transaction rate → tpmC-proxy and the analytical-query completion rate →
QphH-proxy, both labeled "proxy". A malformed/empty summary raises a typed error (fail-fast), never a silent zero.

#### Files to edit
- `benchmarks/run_m130_htap.py`; `benchmarks/theodb_bench/test_htap.py`.

#### TDD
- RED: `test_parse_summary_derives_tpmc_qphh_proxy` — feed a fixture `summary.json` dict; assert the derived tpmc_proxy + qphh_proxy match the expected computation. `test_parse_summary_errors_on_empty` — assert a `ValueError` on a summary with zero transactions.
- GREEN: implement `parse_benchbase_summary(summary)` + `derive_dual_metric(summary)`.
- REFACTOR: keep the derivation formula in one documented function.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- BenchBase produces a truncated/empty `summary.json` (build succeeded, run crashed mid-phase): the parser raises a typed `ValueError` with context; `run()` maps it to `BENCHBASE_ERRORED` (honest), never a zero-metric.

#### Acceptance criteria
- Derived metrics are labeled "proxy" in the output dict; empty/malformed summary → typed error, not a magic zero.

#### DoD
- Parser unit tests green; the proxy labeling is asserted in a test.

## Phase 2 — the wrap layer (OLAP result-consistency + dispersion) + honesty rails

### T3.1 — OLAP result-consistency check + run-to-run dispersion (CV)

#### Why this step
BenchBase validates only timing/completion, not OLAP result values (ADR M130-2). Reasoning: after the run, execute
the 22 analytical queries once more directly against TheoDB and assert internal consistency (non-empty, expected
column shape, deterministic aggregate where applicable); compute the coefficient of variation
(`run_m129_oltp.coefficient_of_variation`) over the repeated per-phase throughput samples for run-to-run stability.

#### Files to edit
- `benchmarks/run_m130_htap.py`; `benchmarks/theodb_bench/test_htap.py`.

#### TDD
- RED: `test_olap_consistency_flags_empty_result` — a stub query executor returning an empty result for an analytical query → the check reports `INCONSISTENT` for that query. `test_cv_wiring_over_phase_throughput` — CV over stable phase samples is low.
- GREEN: implement `olap_result_consistency(queries, executor)` + wire `coefficient_of_variation`.
- REFACTOR: reuse the M129 CV function (no re-implementation — parsimony rung 4).

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- A theodb_columnar analytical query returns an empty/degenerate result under contention: the consistency check reports `INCONSISTENT` with the query id (fail-clear), surfaced in the artifact — not silently passed.

#### Acceptance criteria
- The consistency check reports per-query PASS/INCONSISTENT; CV computed over ≥2 phase throughput samples; the M129 CV function is reused, not re-implemented.

#### DoD
- Consistency + CV unit tests green; no duplicate CV implementation (`grep` shows one definition).

### T4.1 — honesty rails + evidence doc (UNBENCHMARKED clean-exit; Java-23 + seed caveats)

#### Why this step
Rule 5 + ADR M130-2: no HTAP claim without a real BenchBase run; Java-23 liability + seed-determinism must be stated
honestly. Reasoning: `run()` returns UNBENCHMARKED (clean exit, no fabricated dual metric) when BenchBase
skips/errors; the evidence doc labels the dual metric a "proxy", records the pinned BenchBase SHA, documents the
Java-23 build liability, and marks seed-level deterministic replay as unconfirmed.

#### Files to edit
- `benchmarks/run_m130_htap.py`; `docs/benchmarks/m130-htap.md` (NEW); `docs/benchmarks/m130-htap.json` (NEW, measured or UNBENCHMARKED); `CHANGELOG.md`.

#### TDD
- RED: `test_run_unbenchmarked_when_benchbase_skipped` — with docker absent, `run(args)` returns `status="UNBENCHMARKED"` and no `dual_metric` key.
- GREEN: implement the `run()` UNBENCHMARKED path.
- REFACTOR: share the base result dict (box, engine, pinned SHA, caveats) with the OK path.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- UNBENCHMARKED path never emits a `dual_metric`; the evidence doc labels every number "proxy", cites the pinned SHA, and states the Java-23 + seed caveats.
- Self-hosted box labeled NOT canonical hardware; no unqualified "faster than X" (`rules/public-copy.md § 4`).

#### DoD
- `docs/benchmarks/m130-htap.md` exists with a reproduction section + honesty caveats; CHANGELOG `[Unreleased]` updated.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| BenchBase Java-23 build fails/times out in-container on the shared droplet | HIGH | Pin a known-good SHA; if the build fails, clean-exit UNBENCHMARKED (honest) and document the recipe as operational follow-up — never fabricate a dual metric | benchmarks |
| CH-benCHmark analytical queries hit the theodb_columnar planner-hang (#135 class) on wide tables | MEDIUM | Run the analytical side against the heap path first; if columnar pushdown hangs, document it (reference #135) and measure the heap HTAP baseline; do not block on the pushdown bug | benchmarks |
| Derived tpmC/QphH proxy mistaken for audited TPC metrics | MEDIUM | Label every number "proxy" in the artifact + doc; never write "tpmC"/"QphH" unqualified (ADR M130-2) | benchmarks |
| BenchBase seed-determinism unconfirmed → run-to-run not bit-reproducible | LOW | Report dispersion (CV) over repeated runs instead of asserting determinism; mark seed-replay unconfirmed honestly | benchmarks |

## Unresolved Questions

- Does the shared droplet have enough headroom (8 GB free) for a Java-23 BenchBase build + a concurrent OLTP+OLAP run? If not, reduce terminals/scale and document the reduced config. (Resolved at run time; UNBENCHMARKED if it cannot fit.)
- Will the analytical side target theodb_columnar or the heap? Default: heap for the MEASURED gate (avoids #135); columnar as a documented stretch.

## Failure scenarios

- **BenchBase container build failure (Java-23/mvnw):** `run_benchbase` catches the non-zero exit → `BENCHBASE_ERRORED` with the first error line; `run()` → UNBENCHMARKED. Reproduced in a test by a stub that raises on the build step.
- **Truncated `summary.json`:** parser raises a typed `ValueError`; `run()` → BENCHBASE_ERRORED. Reproduced by a fixture with zero transactions.
- **OLAP query empty/degenerate under contention:** the consistency check reports `INCONSISTENT` with the query id; surfaced in the artifact. Reproduced by a stub executor returning empty.

## Global Definition of Done

- [ ] BenchBase CH-benCHmark runs against self-hosted TheoDB via the pinned Java-23 Docker driver (or clean-exit UNBENCHMARKED with the recipe documented — no fabricated numbers).
- [ ] Dual tpmC/QphH **proxy** derived from `summary.json` and MEASURED (labeled "proxy", never audited TPC).
- [ ] Wrap layer wired: OLAP result-consistency check (per-query PASS/INCONSISTENT) + run-to-run dispersion (CV, reusing the M129 function).
- [ ] `benchmarks/theodb_bench/test_htap.py` green (parsers + derivation + consistency + CV + skip path).
- [ ] No BenchBase source vendored/linked (only our `.sh` + `.xml`); Apache-2.0 usage via pinned-SHA container.
- [ ] `docs/benchmarks/m130-htap.md` + `.json` written; CHANGELOG `[Unreleased]` updated; Java-23 liability + seed-determinism-unconfirmed stated honestly.
- [ ] Self-hosted box labeled NOT canonical hardware; no unqualified comparative claim.

## Final Phase — Integration Validation

- Run `python3 -m pytest benchmarks/theodb_bench/test_htap.py -q` — all green.
- Run the driver end-to-end on the droplet (or record UNBENCHMARKED honestly with the reason).
- council-benchmark review: real measurement vs supposition, license posture (Apache-2.0 usage, no vendored source), proxy labeling, honest scope.
