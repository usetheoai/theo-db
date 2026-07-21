---
slug: official-benchmark-oltp
milestone_id: M129
created_at: 2026-07-21
goal: Ship TheoDB's OLTP benchmark harness (pgbench TPS + HammerDB TPROC-C NOPM) with paired significance over repeated runs, measured against self-hosted TheoDB
---

# Plan — M129 Official benchmark: OLTP pillar (pgbench + HammerDB TPROC-C)

## Goal

Deliver TheoDB's OLTP benchmark harness running the two field-standard tools — **pgbench** (native TPC-B-like, TPS)
and **HammerDB TPROC-C** (the real TPC-C mix, NOPM) — as external out-of-tree drivers against a self-hosted TheoDB
PG17, with the retained wrap layer (**paired significance** over repeated runs) + an explicit link to the retained
ACID/crash-safety gate (throughput is not validity). Applies the ADR-0050 adopt-and-wrap pattern.

**Single metric:** a reproducible OLTP artifact with **measured pgbench TPS** over ≥2 repeated runs against
self-hosted TheoDB + a **paired-significance verdict** on the run-to-run TPS (the wrap layer wired), recorded in
`docs/benchmarks/m129-oltp.md`. HammerDB TPROC-C NOPM is measured when the Docker driver is available; otherwise
its recipe is the documented operational follow-up.

## Context

Implements ADR-0050 for the OLTP pillar — the third application of the pattern. The discovery blueprint
(`knowledge-base/discoveries/blueprints/official-db-benchmark-harness-blueprint.md`) selected **HammerDB TPROC-C**
(claim-grade NOPM, the real TPC-C 45/43/4/4/4 mix) + **pgbench** (ubiquitous TPС-B-like smoke, TPS), and found the
OLTP tools provide NO significance test and NO ACID/durability gate (they post NOPM/TPS with `fsync=off`) — so the
paired significance + the crash-safety gate are retained. HammerDB is **GPLv3** → external out-of-tree driver only,
never forked/linked (D1-safe rule). TheoDB's OLTP path IS PostgreSQL's (`theodb_rs` is an extension), so this proves the
100%-wire-compatible OLTP gate + a throughput baseline.

## Baseline Context

Repo state: git sha `c613a63`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `benchmarks/run_m129_oltp.py` | — | (NEW) | Driver: orchestrate `pgbench` (init + N timed runs → TPS) + optional HammerDB TPROC-C (Docker → NOPM), emit results + paired significance. |
| `benchmarks/oltp/hammerdb_tproc_c.tcl` | — | (NEW) | The HammerDB TPROC-C build+run Tcl script (our own, drives the GPLv3 tool via its CLI — no HammerDB code vendored). |
| `benchmarks/theodb_bench/test_oltp.py` | — | (NEW) | Unit tests for the driver's TPS/NOPM parsers + the significance wiring. |
| `benchmarks/theodb_bench/significance.py` | 93 | `paired_significance` (M123) | Reused UNCHANGED (the wrap-layer significance half). |

### Current callers / dependents (verified `file:line`)

- `benchmarks/theodb_bench/significance.py:22` — `paired_significance(a, b, *, seed, n_resamples)` — the wrap layer's significance, run over the repeated-run TPS samples.
- `theodb_rs/isolation/crash_fold.sh`, `theodb_rs/isolation/crash_unlogged.sh` — the retained ACID/crash-safety gate (#46/#47); throughput is paired with this, never reported alone as "valid".
- pgbench: ships with PostgreSQL (`/root/.pgrx/17.10/pgrx-install/bin/pgbench`) — PostgreSQL License (D1-clean).
- HammerDB: `tpcorg/hammerdb` Docker image (GPLv3) — external out-of-tree driver.

### Domain glossary

- **pgbench** — PostgreSQL's shipped TPC-B-like tool: one 7-statement transaction; metric = TPS; scale `-s`, clients `-c`, threads `-j`, duration `-T`.
- **HammerDB TPROC-C** — a TPC-C-derived OLTP workload (real 45/43/4/4/4 mix), metric = **NOPM** (New-Orders Per Minute); NOT audited TPC-C, cannot be called "tpmC".
- **paired significance** — the M123 permutation test over run-to-run throughput samples (the wrap-layer capability the OLTP tools lack).
- **retained crash gate** — TheoDB's crash-safety harnesses; a throughput number is only meaningful paired with durability (the OLTP tools post TPS/NOPM even with `fsync=off`).

### Architecture boundaries affected

Per `rules/architecture.md`: pure benchmark tooling (Python + a Tcl driver script, `benchmarks/`) — NO production
Rust change. The drivers talk to TheoDB over the wire; HammerDB (GPLv3) runs as a separate Docker process (never
linked). No engine/API change.

## Prior Art & Related Work

- Blueprint (web-evidenced, 2026-07-20): pgbench protocol (TPS, `-i/-s/-c/-j/-T/-r`), HammerDB TPROC-C protocol
  (NOPM, 45/43/4/4/4, rampup+duration, `tpcorg/hammerdb` Docker), the GPLv3 external-driver posture, and the "no
  significance / no ACID gate" finding. Sources: postgresql.org pgbench docs, hammerdb.com, tpc.org.
- Internal: `theodb_bench/significance.py` (M123) is the retained significance; `theodb_rs/isolation/*.sh` are the
  retained crash-safety gate. The M127/M128 driver shape (argparse, run()->dict, UNBENCHMARKED path) is reused.

## ADRs

### ADR M129-1 — pgbench is the primary measured path; HammerDB TPROC-C is opt-in Docker

**Decision:** the driver runs **pgbench** (native, D1-clean, always available) as the primary measured OLTP path
(N repeated timed runs → TPS + paired significance), and runs **HammerDB TPROC-C** (NOPM) via the `tpcorg/hammerdb`
Docker image when `--hammerdb` is set + Docker is present; otherwise HammerDB is the documented follow-up.

**Rationale (cites blueprint + `rules/parsimony-ladder.md` rung 3):** pgbench ships with PG (rung 3, no install,
no GPL) so it is the robust always-available path; HammerDB (GPLv3, Docker) is heavier + external. Both are
field-standard; pgbench guarantees a measured result, HammerDB adds the claim-grade NOPM when feasible.

**Alternatives rejected:**
- **HammerDB only** — REJECTED: GPLv3 + Docker dependency makes it fragile as the sole path; pgbench is the native guarantee.
- **A bespoke OLTP workload** — REJECTED: not field-standard (defeats the adopt purpose).

### ADR M129-2 — throughput is always paired with the retained crash-safety gate

**Decision:** every TPS/NOPM number in the artifact is explicitly paired with a pointer to the retained
crash-safety gate (`theodb_rs/isolation/crash_*.sh`, #46/#47) — the artifact states that a throughput number
without durability is meaningless, which the OLTP tools do NOT enforce.

**Rationale (cites blueprint Q10 + Rule 3):** pgbench/HammerDB post big TPS/NOPM with `fsync=off`; only audited
TPC-C runs ACID. The retained gate is the correctness half the OLTP tools lack (the wrap layer).

**Alternatives rejected:** reporting TPS alone — REJECTED (dishonest; invites a fast-but-non-durable number).

## Dependencies

`## Dependencies`: **none new** — reuses `numpy` (already in `benchmarks/requirements.txt`, via `significance.py`).
pgbench ships with PG (PostgreSQL License). HammerDB runs as the `tpcorg/hammerdb` Docker image (GPLv3, external,
never vendored/linked). No crate/pip added.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| pgbench TPS over repeated runs + paired significance (the wrap layer) | T1 (pgbench driver + significance) |
| HammerDB TPROC-C NOPM (claim-grade) via Docker | T2 (HammerDB TPROC-C driver) |
| Throughput paired with the retained crash-safety gate | T3 (artifact pairs TPS/NOPM with the gate) |

## Phase 1 — pgbench + the wrap layer

### T1.1 — pgbench driver (TPS over N repeated runs) + paired significance

#### Why this step
pgbench is the native, always-available OLTP path (ADR M129-1) and the significance wiring is the wrap-layer
capability. Reasoning: `pgbench -i -s S` to build, then N repeated `pgbench -c C -j J -T T` runs parsing the `tps`
line, then `paired_significance` over the per-run TPS to demonstrate run-to-run stability (the wrap layer the OLTP
tools lack).

#### Files to edit
- `benchmarks/run_m129_oltp.py` (NEW); `benchmarks/theodb_bench/test_oltp.py` (NEW).

#### TDD
- RED: `test_parse_pgbench_tps` asserts the parser extracts the TPS float from a real pgbench output block
  (`tps = 1234.56 (without initial connection time)`), and returns a typed error (not 0) on unparseable output.
- GREEN: the parser + the driver run pgbench N times and feed the TPS list to `paired_significance`.
- REFACTOR: reuse the driver shape (argparse, run()->dict, UNBENCHMARKED) from run_m127/m128.

#### Failure scenarios
- pgbench binary absent / DB unreachable → the driver exits UNBENCHMARKED (no fabricated TPS), like the M127/M128 dataset-absent paths.

#### Concurrency tests
(none — single-threaded) — pgbench itself drives concurrent clients (`-c`), but the DRIVER orchestrates runs sequentially; no shared mutable state in the harness.

#### Acceptance criteria
- `test_oltp.py` passes: the TPS parser extracts the float + errors typed on garbage; the driver produces ≥2 TPS
  samples + a paired-significance dict on the self-hosted droplet.

#### DoD
- `python3 -m pytest benchmarks/theodb_bench/test_oltp.py` green; a measured pgbench TPS + significance verdict exists.

## Phase 2 — HammerDB TPROC-C + the retained gate

### T2.1 — HammerDB TPROC-C driver (NOPM) via Docker

#### Why this step
HammerDB TPROC-C is the claim-grade OLTP tool (real TPC-C mix, NOPM). Reasoning: a Tcl script (`hammerdb_tproc_c.tcl`)
drives the `tpcorg/hammerdb` Docker image's CLI (`dbset db pg`, `buildschema`, `pg_driver timed`, `vurun`) against
self-hosted TheoDB; the driver parses NOPM from the output. HammerDB (GPLv3) runs as an external Docker process —
no HammerDB code is vendored or linked.

#### Files to edit
- `benchmarks/oltp/hammerdb_tproc_c.tcl` (NEW — our driver script); `benchmarks/run_m129_oltp.py` (the `--hammerdb` pass); tests in `test_oltp.py`.

#### TDD
- RED: `test_parse_hammerdb_nopm` extracts the NOPM integer from a HammerDB result line
  (`… System achieved NNN NOPM from MMM …`); typed error on unparseable output.
- GREEN: with Docker + `--hammerdb`, the driver runs TPROC-C against TheoDB and records NOPM; without Docker, it
  records `HAMMERDB_SKIPPED: docker unavailable` (honest, not a fake number).

#### Failure scenarios
- Docker absent OR the image pull fails → `HAMMERDB_SKIPPED` recorded (pgbench result still stands); never a fabricated NOPM.
- HammerDB build/run error → the typed HammerDB error is recorded, not silently swallowed.

#### Concurrency tests
(none — single-threaded) — HammerDB drives its own virtual users; the harness orchestrates one run.

#### Acceptance criteria
- With Docker: a measured NOPM recorded; without: `HAMMERDB_SKIPPED` with reason. The Tcl script vendors no
  HammerDB source (drives the GPLv3 tool via its CLI only).

#### DoD
- HammerDB TPROC-C NOPM measured (or honestly skipped) on the droplet; the GPLv3 external-driver posture holds.

### T3.1 — pair throughput with the retained crash-safety gate

#### Why this step
Blueprint Q10: pgbench/HammerDB post throughput with `fsync=off` — no ACID gate. Reasoning: the artifact MUST pair
every TPS/NOPM with the retained crash-safety gate (`theodb_rs/isolation/crash_*.sh`, #46/#47), stating a throughput
number is meaningless without durability — the correctness half the OLTP tools lack.

#### Files to edit
- `docs/benchmarks/m129-oltp.md` (NEW — the artifact pairing TPS/NOPM with the retained gate).

#### TDD
- RED: the artifact would be dishonest if it reported TPS/NOPM without the durability pairing (a review BLOCKER).
- GREEN: `docs/benchmarks/m129-oltp.md` reports the measured TPS (+ NOPM if run) AND explicitly cites the retained
  crash-safety gate + `fsync` posture used for the run.

#### Failure scenarios
- (none — a doc pairing.)

#### Concurrency tests
(none — a doc.)

#### Acceptance criteria
- The artifact pairs every throughput number with the crash-safety gate + the `fsync` setting; no throughput stands alone.

#### DoD
- `docs/benchmarks/m129-oltp.md` records the measured throughput + the retained-gate pairing + honest scope.

## Failure scenarios

- **pgbench binary absent / TheoDB unreachable** — the driver exits `UNBENCHMARKED` (no fabricated TPS), like the M127/M128 dataset-absent paths.
- **Docker absent / HammerDB image pull fails** — `HAMMERDB_SKIPPED` recorded with reason; the pgbench result still stands; never a fabricated NOPM.
- **A throughput number reported without its durability posture** — a review BLOCKER by ADR M129-2; the artifact always states the `fsync` setting + the retained crash gate.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Self-hosted box (not canonical hardware) → TPS/NOPM not comparable to published TPC-C | MEDIUM | Honestly labeled; the audited-TPC-C tpmC is a separate certified process (blueprint); we report NOPM/TPS as an internal baseline | implementer |
| HammerDB (GPLv3) licence — must never enter the distributed tree | MEDIUM | External Docker process only; our Tcl script drives its CLI, vendors no HammerDB source; documented (ADR M129-1) | implementer |
| Throughput without durability invites a misleading number | MEDIUM | ADR M129-2: every number paired with the retained crash-safety gate + the `fsync` posture | implementer |

## Unresolved Questions

- Will HammerDB TPROC-C run cleanly against the pgrx-managed PG on the shared droplet in-session? Resolved at plan
  time: **best-effort** — the driver measures NOPM if Docker + the image work, else records `HAMMERDB_SKIPPED`
  honestly; pgbench (native) is the guaranteed measured path (ADR M129-1). The DoD is met by pgbench + significance
  + the gate pairing; HammerDB NOPM is a bonus when feasible.
- (none other — every in-scope decision is resolved at plan time.)

## Global DoD

- `run_m129_oltp.py` (pgbench + optional HammerDB) + `oltp/hammerdb_tproc_c.tcl` + unit tests green.
- A MEASURED pgbench TPS over ≥2 repeated runs against self-hosted TheoDB + a paired-significance verdict, and a
  HammerDB TPROC-C NOPM (or honest `HAMMERDB_SKIPPED`), in `docs/benchmarks/m129-oltp.md` — every throughput number
  paired with the retained crash-safety gate + the `fsync` posture.
- No production Rust change; no new dependency; HammerDB never vendored/linked (GPLv3). CHANGELOG `[Unreleased]`.
  `/code-quality` ∉ {FAIL_HARD, INVALID}.
