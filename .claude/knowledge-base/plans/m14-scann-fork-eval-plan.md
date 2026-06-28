---
slug: m14-scann-fork-eval
milestone_id: M14
created_at: 2026-06-28
goal: Measure DiskANN vs the ScaNN-quality target on the recall harness and decide fork/no-fork by ADR, closing docs/features 05
---

# Plan: ScaNN-quality / fork-trigger evaluation (feature 05) — measurement-gated

> **Version 1.0** — spec 05 documents a literal `theodb_scann` index (the AlloyDB ScaNN AM). Per the
> measurement-first doctrine (the PRD fork-gate policy / ADR 0002) and anti-sunk-cost (CLAUDE.md), M14 does NOT build a native
> ScaNN access method — it **measures** whether DiskANN (the shipped permissive StreamingDiskANN, M2) already
> reaches ScaNN-quality recall, and **decides fork/no-fork by ADR on that evidence**. Building a native AM is
> exactly what the fork-gate policy gates on a reproducible benchmark; this slice produces that benchmark + the decision.
> Honesty (Rule 3/5): no "ScaNN delivered" claim — DiskANN is documented as the delivered permissive
> equivalent; the literal `theodb_scann` AM stays gated.

## Goal

> Produce a reproducible recall@k benchmark of DiskANN vs the ScaNN-quality target (+ HNSW/IVFFlat baselines)
> and a fork/no-fork ADR anchored on it, measured by an integration test asserting DiskANN reaches the
> ScaNN-quality recall bar (recall@10 ≥ 0.90) and the ADR + report recording the decision with real numbers.

## Context

`docs/features/05-indice-scann.md` documents the literal `theodb_scann` extension (AlloyDB's ScaNN AM). TheoDB
ships **StreamingDiskANN** via `pgvectorscale` (M2) as the permissive ANN — already benchmarked in the
recall@k harness (M2/M9; DiskANN reaches recall ≥ 0.90 at high `query_search_list_size`). the PRD fork-gate policy makes any
fork/native-AM **conditional on a reproducible benchmark**; ADR 0002 is measurement-first; CLAUDE.md forbids
sunk-cost forks. M14 is therefore a **measurement + decision** milestone, not a code feature: run the
DiskANN-vs-ScaNN-quality comparison, cite published ScaNN/StreamingDiskANN numbers as the reference target,
and write the fork/no-fork ADR. No new index, no new dependency.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `benchmarks/theodb_bench/__main__.py` | ~190 | M11/M13 | recall harness CLI (`--index diskann/hnsw/ivfflat`) | UNCHANGED — M14 only RUNS it; no harness code change |
| `benchmarks/tests/test_integration.py` | ~369 | M13 | harness integration tests (incl. `test_harness_measures_diskann`) | existing tests green; a ScaNN-quality-bar test appended |
| `benchmarks/scann_fork_eval.sh` (NEW) | 0 | — | (to be created) one-command reproducible DiskANN-vs-baselines comparison | — |
| `docs/benchmarks/m14-scann-fork-decision.md` (NEW) | 0 | — | measured comparison + cited published ScaNN numbers | — |
| `docs/adr/0004-scann-fork-decision.md` (NEW) | 0 | — | the fork/no-fork DECISION (the PRD fork-gate policy / anti-sunk-cost) | — |
| `docs/features/05-indice-scann.md` | (exists) | — | the `theodb_scann` spec | add an honest "DiskANN delivered; theodb_scann gated" note |
| `CHANGELOG.md` | (exists) | — | public contract | `[Unreleased]` gets the M14 entry |

Every file in any `#### Files to edit` below appears in this table.

### Current callers / dependents

- **Symbol:** `_diskann_spec` / `run_benchmark` in `benchmarks/theodb_bench` (`__main__.py:94`, `harness.py`). M14 invokes the existing CLI (`--index diskann/hnsw/ivfflat`) — no code change. The new `scann_fork_eval.sh` is a caller (orchestrates 3 CLI runs).
- **Symbol:** `test_harness_measures_diskann` (`test_integration.py`) already asserts DiskANN recall ≥ 0.90. M14 adds a sibling that frames it as the ScaNN-quality bar.
- Enumerated via `grep -nE '_diskann_spec|test_harness_measures_diskann|--index' benchmarks/`.

### Domain glossary

- **ScaNN** — Google's SOTA ANN library (anisotropic quantization). The literal AM spec 05 documents (`theodb_scann`). Apache-2.0 algorithm; no permissive Postgres AM exists in-tree.
- **StreamingDiskANN (DiskANN)** — pgvectorscale's ANN index (SBQ quantization + streaming graph); TheoDB's shipped permissive ANN (M2). The candidate ScaNN-quality substitute.
- **ScaNN-quality bar** — recall@10 ≥ 0.90 at usable QPS (the recall band ScaNN/StreamingDiskANN reach on ann-benchmarks). The measurable target for the fork decision.
- **fork-trigger (the PRD fork-gate policy)** — a native AM / extension fork is authorized ONLY when a reproducible benchmark shows the permissive substitute insufficient. Anti-sunk-cost (CLAUDE.md): do not build what the substitute already covers.

### Architecture boundaries affected

Per `rules/architecture.md`: M14 touches only the **dev-only benchmark tooling** (run + a wrapper script) +
**docs** (report, ADR, spec note). No product code, no DB object, no new dependency. The ADR is an
architecture-decision record under `docs/adr/` (alongside 0001-no-engine-fork / 0002-north-star / 0003-bm25).

## Prior Art & Related Work

- **Internal:** `benchmarks/theodb_bench/__main__.py` (`_diskann_spec`, the measured DiskANN sweep); `docs/benchmarks/m9-ivfflat.md` (the recall-report format); `knowledge-base/discoveries/blueprints/vector-recall-benchmark-harness-blueprint.md` (the recall methodology); ADRs `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (measurement-first) + `docs/adr/0001-no-engine-fork.md` (fork posture).
- **External (the ScaNN-quality reference target):** ann-benchmarks published recall for ScaNN + pgvectorscale StreamingDiskANN (`https://ann-benchmarks.com`) and the pgvectorscale benchmark post (`https://github.com/timescale/pgvectorscale`); ScaNN paper (Guo et al. 2020). Cited as the target band; numbers carry their source per Rule 5.
- **Reference:** `.claude/knowledge-base/references/pgvectorscale/` (when present).

## Objective

- [ ] `benchmarks/scann_fork_eval.sh` — one command runs DiskANN + HNSW + IVFFlat on the harness and prints a recall×QPS comparison (reproducible evidence). Harness code UNCHANGED.
- [ ] Real measured DiskANN recall captured (run live against `theo-db:dev` + vectorscale).
- [ ] `docs/benchmarks/m14-scann-fork-decision.md` — the measured comparison + cited published ScaNN/StreamingDiskANN target numbers + honest synthetic-data caveat.
- [ ] `docs/adr/0004-scann-fork-decision.md` — the fork/no-fork DECISION anchored on the evidence (the PRD fork-gate policy / anti-sunk-cost), with the gate that would re-open it.
- [ ] `docs/features/05-indice-scann.md` — honest note: DiskANN is the delivered permissive ScaNN-quality equivalent; literal `theodb_scann` AM gated (not built).
- [ ] NO native ScaNN AM is built (anti-sunk-cost; the measurement shows the substitute suffices).

## ADRs

### D1 — Measure + decide; do NOT build a native ScaNN AM (the PRD fork-gate policy / anti-sunk-cost)

**Decision:** M14 delivers a reproducible DiskANN-vs-ScaNN-quality benchmark + a fork/no-fork ADR. It does
NOT implement a `theodb_scann` access method.

**Rationale:** the PRD fork-gate policy makes a native-AM/fork conditional on a reproducible benchmark proving the permissive
substitute insufficient. DiskANN (StreamingDiskANN) is already shipped and reaches recall ≥ 0.90 on the
harness (M9). Building a native ScaNN AM before the benchmark justifies it violates measurement-first (ADR
0002) and anti-sunk-cost (CLAUDE.md). The honest deliverable is the evidence + the decision; the AM is built
only if/when the evidence flips.

**Alternatives considered:** *Build `theodb_scann` now (literal spec parity)* — rejected: massive fork
(C++/pgrx ScaNN binding), unbenchmarked need, the M6 rustc/MSRV build blocker precedent; directly violates
the fork-gate policy + anti-sunk-cost. *Skip the milestone (DiskANN already shipped)* — rejected: spec 05 + the fork-trigger
need an explicit, evidence-backed, auditable decision, not silence.

**Consequences:** spec 05's literal `theodb_scann` is documented as gated (not delivered); DiskANN is the
delivered permissive equivalent; a future benchmark showing DiskANN insufficient re-opens the fork.

### D2 — ScaNN-quality bar = recall@10 ≥ 0.90; evidence = measured DiskANN + cited published ScaNN/DiskANN

**Decision:** Define the ScaNN-quality bar as recall@10 ≥ 0.90 at usable QPS. Support the decision with (a)
the harness's MEASURED DiskANN recall and (b) CITED published ann-benchmarks/pgvectorscale numbers for ScaNN
and StreamingDiskANN on real datasets.

**Rationale:** recall ≥ 0.90 is the band ScaNN/StreamingDiskANN occupy on ann-benchmarks; it is the honest,
checkable bar. Our harness uses synthetic gaussian (not real embeddings) — so the in-repo measurement is
paired with the published real-dataset numbers (Rule 5: every perf number carries its source; the synthetic
caveat is stated, the real-dataset claim is cited, not fabricated).

**Alternatives considered:** *Run real ScaNN in-process for a head-to-head* — rejected: requires the very
fork being evaluated (circular); published numbers are the honest reference. *Download a real glove HDF5 in
CI* — rejected: external-network dependency in the gate; the harness supports `--hdf5` for an opt-in real run,
documented as a follow-up.

**Consequences:** the decision rests on measured-DiskANN + cited-published-ScaNN; the synthetic-vs-real gap is
disclosed; a real-dataset `--hdf5` run is an honest documented follow-up.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Synthetic gaussian overstates DiskANN recall vs real embeddings | Medium | pair measured-synthetic with CITED published real-dataset ScaNN/DiskANN numbers; disclose the caveat; `--hdf5` real run documented as follow-up | Bench |
| "No-fork" decision later proves wrong as scale/recall needs grow | Medium | ADR 0004 names the explicit re-open gate (a reproducible benchmark showing DiskANN insufficient); decision is revisitable, not permanent | Bench |
| Reader mistakes M14 for "ScaNN shipped" | Low | spec 05 note + ADR + CHANGELOG state plainly: DiskANN is the substitute; `theodb_scann` AM is gated/not built | Bench |
| Citing published numbers without the artifact | Low | cite source URLs in the same paragraph (public-copy.md); mark the in-repo measurement as the only first-party number | Bench |

## Unresolved Questions

- Q1 — Run a real glove/sift `--hdf5` benchmark in-repo? Resolved at plan time: not in M14 (external-network in the gate); the harness supports it; documented as an honest follow-up. The synthetic measurement + cited published numbers support the decision.
- Q2 — Will the no-fork decision ever flip? Resolved: only if a reproducible benchmark shows DiskANN below the ScaNN-quality bar on a representative dataset (the ADR 0004 re-open gate).

## Dependencies

M14 adds **no new dependency** (Unbreakable Rule 9). It runs the existing harness (DiskANN via the already
shipped `pgvectorscale`) and writes docs.

| Dependency | Version | Role | License | Status / CVE |
|---|---|---|---|---|
| `pgvectorscale` StreamingDiskANN | shipped (M2) | the measured permissive ScaNN-quality substitute | PostgreSQL License | already shipped; no change |
| recall harness (`benchmarks/theodb_bench`) | shipped (M2/M9) | the measurement tool | BSD/LGPL deps | UNCHANGED |

No CVE audit delta: zero new declared dependencies.

## Dependency Graph

```
Phase 1 (scann_fork_eval.sh + run + ScaNN-quality-bar test) ──▶ Phase 2 (report + ADR 0004 + spec 05 note + CHANGELOG)
```

## Phase 1: Reproducible DiskANN-vs-ScaNN-quality benchmark

**Objective:** Produce the measured evidence: a one-command comparison + a ScaNN-quality-bar test.

### T1.1 — `scann_fork_eval.sh` (reproducible runner) + ScaNN-quality-bar test

#### Objective
Add a one-command comparison runner (DiskANN + HNSW + IVFFlat) and a test asserting DiskANN reaches the ScaNN-quality bar.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — adds `benchmarks/scann_fork_eval.sh` that runs `python3 -m theodb_bench --index
   diskann|hnsw|ivfflat` (same dataset) and prints the recall×QPS comparison; appends
   `test_diskann_reaches_scann_quality_recall` asserting DiskANN recall@10 ≥ 0.90 (the ScaNN-quality bar) —
   the measurable evidence the fork decision rests on. The harness itself is unchanged.

2. **Why it is necessary now** — the fork/no-fork ADR must be anchored on a reproducible benchmark (the PRD fork-gate policy);
   this produces it. The test makes "DiskANN meets the ScaNN-quality bar" a checkable gate, not a claim.

#### Evidence
- DiskANN sweep: `benchmarks/theodb_bench/__main__.py:94-122` (`_diskann_spec`, recall ≥0.90 at high sls).
- Existing diskann test: `benchmarks/tests/test_integration.py::test_harness_measures_diskann`.
- Recall metric: `benchmarks/theodb_bench/recall.py` (distance-thresholded recall).

#### Files to edit
```
benchmarks/scann_fork_eval.sh (NEW) — runs diskann+hnsw+ivfflat; prints recall×QPS comparison
benchmarks/tests/test_integration.py — test_diskann_reaches_scann_quality_recall (recall@10 >= 0.90)
```

#### Deep file dependency analysis
- `scann_fork_eval.sh` (NEW): orchestrates 3 existing CLI invocations; reads PG* env; no harness change.
- `test_integration.py` (Baseline row, invariant: existing tests green): appends one test using the existing `build_config`/`run_benchmark` (mirrors `test_harness_measures_diskann`).

#### Deep Dives
- **Script:** `set -euo pipefail`; for idx in diskann hnsw ivfflat: `python3 -m theodb_bench --index $idx --n 5000 --dim 32 --n-queries 100 --k 10 --runs 2 --metric cosine --out docs/benchmarks`; then print a one-line summary per index from the JSON. Honest: prints measured numbers only.
- **Test:** `build_config(--index diskann, n>=2000, dim 32)`, `run_benchmark`; assert `max(recall_at_k for diskann) >= 0.90` (the ScaNN-quality bar) + recall ∈ [0,1] + qps>0.
- **Edge cases:** vectorscale missing → the harness already errors clearly (diskann requires vectorscale); the test runs on `theo-db:dev` which has it.

#### Pseudo-code / Signatures
```bash
#!/usr/bin/env bash
set -euo pipefail
for idx in diskann hnsw ivfflat; do
  python3 -m theodb_bench --index "$idx" --n "${N:-5000}" --dim "${DIM:-32}" \
    --n-queries 100 --k 10 --runs 2 --metric cosine --out "${OUT:-docs/benchmarks}"
done
# then tabulate recall@10 / qps / build / size per index from the emitted JSON
```

#### Tasks
1. Write `benchmarks/scann_fork_eval.sh` (executable).
2. Append `test_diskann_reaches_scann_quality_recall` to `test_integration.py`.
3. Run the script live against `theo-db:dev`; capture the numbers.

#### TDD
```
RED:     test_diskann_reaches_scann_quality_recall() [integration] — run_benchmark(--index diskann, n>=2000, dim 32, cosine); assert max(recall@10) >= 0.90 (ScaNN-quality bar) AND recall in [0,1] AND qps>0. (RED if the bar were unmet; reuses the proven diskann path.)
GREEN:   The diskann sweep already reaches >=0.90 (M9) — the test passes; the script tabulates it.
REFACTOR: none expected (no harness change).
VERIFY:  cd benchmarks && PG*=... pytest -m integration tests/test_integration.py -k scann_quality -q ; bash scann_fork_eval.sh
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — sequential benchmark runs; no shared mutable state, no locks/async.

#### Acceptance Criteria
- [ ] `scann_fork_eval.sh` runs the 3-index comparison — `cd benchmarks && PGHOST=... bash scann_fork_eval.sh` exits `0` and prints a diskann/hnsw/ivfflat recall×QPS line each.
- [ ] DiskANN meets the ScaNN-quality bar — `cd benchmarks && PGHOST=... pytest -m integration tests/test_integration.py -k scann_quality -q` exits `0` (recall@10 ≥ 0.90).
- [ ] No harness regression — `cd benchmarks && PGHOST=... pytest -m integration tests/test_integration.py -k 'diskann or hnsw or ivfflat' -q` exits `0`.
- [ ] Pass: lint + script syntax — `cd benchmarks && ruff check tests` exits `0`; `bash -n scann_fork_eval.sh` exits `0`.
- [ ] Pass: size — changed files `wc -l` < `500`.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] DiskANN ScaNN-quality bar test green; comparison script runs.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'ScaNN\|scann_fork' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Phase 2: Report + ADR 0004 (fork decision) + spec 05 note

**Objective:** Record the measured comparison + the evidence-anchored fork/no-fork decision honestly.

### T2.1 — `docs/benchmarks/m14-scann-fork-decision.md` + `docs/adr/0004` + spec 05 note + CHANGELOG

#### Objective
Write the measured comparison report, the fork/no-fork ADR, the honest spec 05 note, and the CHANGELOG entry.

#### Why this step (action + reasoning — ReAct discipline)

1. **What this step does** — writes `docs/benchmarks/m14-scann-fork-decision.md` (measured DiskANN/HNSW/IVFFlat
   recall×QPS from T1.1 + cited published ScaNN/StreamingDiskANN target numbers + synthetic caveat);
   `docs/adr/0004-scann-fork-decision.md` (the fork/no-fork DECISION + re-open gate, the PRD fork-gate policy / anti-sunk-cost);
   an honest note on `docs/features/05-indice-scann.md`; the CHANGELOG entry.

2. **Why it is necessary now** — the decision must be auditable + evidence-anchored (the PRD fork-gate policy); the spec note
   prevents anyone mistaking M14 for "ScaNN shipped" (Rule 3).

#### Evidence
- Measured numbers: T1.1's harness output.
- Published target: ann-benchmarks/pgvectorscale (cited with source URLs).
- ADR format: `docs/adr/0001-no-engine-fork.md`, `0002`, `0003`.
- Honesty rules: `rules/public-copy.md`, CLAUDE.md (Rule 5/7, anti-sunk-cost).

#### Files to edit
```
docs/benchmarks/m14-scann-fork-decision.md (NEW) — measured comparison + cited target + caveat
docs/adr/0004-scann-fork-decision.md (NEW) — fork/no-fork DECISION + re-open gate
docs/features/05-indice-scann.md — honest note (DiskANN delivered; theodb_scann gated)
CHANGELOG.md — [Unreleased] M14 entry
```

#### Deep file dependency analysis
- report/ADR (NEW): record T1.1 output + the decision.
- `05` spec (Baseline row): additive note; the spec body (API-target) stays.

#### Deep Dives
- **Decision honesty:** the ADR states the DECISION token (NO-FORK: DiskANN is the delivered permissive ScaNN-quality substitute), the measured + cited evidence, and the explicit re-open gate (a reproducible benchmark showing DiskANN < bar on a representative dataset). No "ScaNN done" claim anywhere.

#### Tasks
1. Write the report with measured numbers + cited target.
2. Write ADR 0004 (decision + re-open gate).
3. Add the spec 05 note + CHANGELOG entry.

#### TDD
```
RED:     report/ADR absent before the run.
GREEN:   `test -f docs/benchmarks/m14-scann-fork-decision.md && test -f docs/adr/0004-scann-fork-decision.md`; report contains measured diskann recall; ADR contains the decision token.
REFACTOR: none expected.
VERIFY:  grep -ciE 'diskann|recall@10|no-fork|gated' docs/benchmarks/m14-scann-fork-decision.md docs/adr/0004-scann-fork-decision.md
```

#### Concurrency tests

**Concurrency posture: (none — single-threaded)** — docs only; no concurrent state.

#### Acceptance Criteria
- [ ] Report exists with measured DiskANN recall + cited target — `grep -ciE 'diskann|recall@10|scann' docs/benchmarks/m14-scann-fork-decision.md` returns `> 0`.
- [ ] ADR 0004 records the decision + re-open gate — `grep -ciE 'no-fork|do not build|re-open|gate' docs/adr/0004-scann-fork-decision.md` returns `> 0`.
- [ ] spec 05 honest note present — `grep -ci 'diskann\|gated\|0004' docs/features/05-indice-scann.md` returns `> 0`.
- [ ] No unbenchmarked perf claim / no "ScaNN delivered" overclaim — `grep -ciE 'scann (delivered|shipped|done|implemented)' docs/benchmarks/m14-scann-fork-decision.md docs/adr/0004-scann-fork-decision.md` returns `0`.
- [ ] Pass: size — changed files `wc -l` < `500`.

#### DoD
- [ ] All tasks completed and validated — every Acceptance Criteria above exits `0`.
- [ ] Report + ADR committed with real measured numbers + cited target; spec 05 noted.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -c 'ScaNN\|0004' CHANGELOG.md` returns `> 0`.
- [ ] File-size budget respected — changed files `wc -l` < `500`.

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | feature 05 — reproducible DiskANN vs ScaNN-quality benchmark | T1.1, T2.1 | `scann_fork_eval.sh` + measured report |
| 2 | DiskANN reaches the ScaNN-quality bar (measured) | T1.1 | `test_diskann_reaches_scann_quality_recall` (≥0.90) |
| 3 | fork/no-fork ADR anchored on evidence (the PRD fork-gate policy) | T2.1 | `docs/adr/0004-scann-fork-decision.md` |
| 4 | honesty: DiskANN is the substitute; theodb_scann gated | T2.1 | spec 05 note + ADR + CHANGELOG; no "ScaNN done" claim |
| 5 | NO native AM built (anti-sunk-cost) | T2.1 | decision documented in ADR 0004 (per ADR D1); zero AM code |
| 6 | no new dependency; harness unchanged | T1.1 | runs existing DiskANN; harness code untouched |
| 7 | synthetic-vs-real caveat disclosed (Rule 5) | T2.1 | report pairs measured-synthetic with cited published numbers |

**Coverage: 7/7 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed — every phase DoD above exits `0`.
- [ ] DiskANN ScaNN-quality bar test green — `cd benchmarks && PGHOST=... pytest -m integration tests/test_integration.py -k scann_quality -q` exits `0`.
- [ ] Reproducible comparison script runs — `cd benchmarks && bash scann_fork_eval.sh` exits `0` with measured numbers.
- [ ] ADR 0004 records the fork/no-fork decision + re-open gate; report has real measured numbers + cited target.
- [ ] spec 05 honest note (DiskANN delivered; `theodb_scann` gated); NO "ScaNN shipped" overclaim anywhere.
- [ ] File-size budget respected — changed files `wc -l` < `500` (per `rules/architecture.md`).
- [ ] CHANGELOG.md updated under `[Unreleased]` — `grep -c 'ScaNN\|0004' CHANGELOG.md` returns `> 0` (Unbreakable Rule 6).
- [ ] No new dependency (Rule 9); harness code unchanged; no native AM built.
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merge.

## Failure scenarios (external I/O)

M14 makes NO HTTP/LLM call; the only external I/O is the local DB connection (psycopg → `theo-db:dev`) the
harness already owns.

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| PostgreSQL (`psycopg`, container) | container not ready / unreachable | run the harness before the container is healthy | `VectorDB.connect/ping` raises a clear typed error (existing harness behavior) |
| `pgvectorscale` (DiskANN AM) | extension/AM missing | run `--index diskann` on an image without vectorscale | harness errors clearly ("diskann requires the vectorscale extension") — never a silent skip |
| planner | DiskANN index not used (seqscan on small N) | the sweep forces `enable_seqscan=off` | the index is used; recall reflects DiskANN, not a seqscan |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** Validate the measurement + decision end-to-end against a real container.

### Execution
```
docker run -d --name m14-it -e POSTGRES_PASSWORD=postgres -p <port>:5432 theo-db:dev   # has vectorscale
cd benchmarks && pip install -r requirements.txt
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres \
  pytest -m integration tests/test_integration.py -k 'scann_quality or diskann' -q
PGHOST=localhost PGPORT=<port> PGUSER=postgres PGPASSWORD=postgres PGDATABASE=postgres bash scann_fork_eval.sh
ruff check tests && bash -n scann_fork_eval.sh
```

### Acceptance Criteria
- [ ] DiskANN ScaNN-quality bar met (recall@10 ≥ 0.90, measured); comparison script runs — `cd benchmarks && PGHOST=... pytest -m integration tests/test_integration.py -k scann_quality -q` exits `0`.
- [ ] ADR 0004 + report committed with real numbers + cited target; spec 05 noted — `test -f docs/adr/0004-scann-fork-decision.md && grep -ciE 'diskann|recall' docs/benchmarks/m14-scann-fork-decision.md` returns `> 0`.
- [ ] No native AM built; harness unchanged; no "ScaNN shipped" claim — `git diff --name-only origin/main..HEAD | grep -c '^benchmarks/theodb_bench/__main__.py$'` returns `0` (harness untouched).
- [ ] No regression to the harness suites — `cd benchmarks && PGHOST=... pytest -m integration tests/test_integration.py -k 'diskann or hnsw or ivfflat' -q` exits `0`.
