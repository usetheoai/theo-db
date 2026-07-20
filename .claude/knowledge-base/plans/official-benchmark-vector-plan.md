---
slug: official-benchmark-vector
milestone_id: M127
created_at: 2026-07-20
goal: Ship TheoDB's ann-benchmarks BaseANN adapter + the reusable adopt-and-wrap layer, proven by a measured recall×QPS Pareto on a real D1-safe dataset (GloVe)
---

# Plan — M127 Official benchmark: VECTOR pillar (ann-benchmarks BaseANN + wrap layer)

## Goal

Deliver TheoDB's first-class **ann-benchmarks `BaseANN` adapter** (the exact interface a public ann-benchmarks
entry uses) + extract the reusable **adopt-and-wrap analysis layer** (paired significance + byte-identical
regression A/B over per-query output), proven by a **measured recall@10 × QPS Pareto** on a real, D1-safe dataset
(GloVe, PDDL) against a self-hosted TheoDB — following the ann-benchmarks single-thread protocol.

**Single metric:** a reproducible `recall@10 × QPS` Pareto (≥3 ef points) for TheoDB's `theodb_hnsw` on GloVe,
produced by driving the `BaseANN` adapter, with the wrap layer emitting a paired-significance verdict on a
repeated run and a byte-identical A/B pass — all recorded in `docs/benchmarks/m127-ann-benchmarks-vector.md`.

## Context

Implements ADR-0050 (adopt-and-wrap) for the vector pillar — the de-risking pilot that establishes the pattern
M128–M130 reuse. The discovery blueprint
(`knowledge-base/discoveries/blueprints/official-db-benchmark-harness-blueprint.md`) selected ann-benchmarks
(algorithm recall×QPS Pareto) + VectorDBBench (PG-compat comparability) as canonical, and found the official tools
provide NO significance/regression/correctness layer — so the wrap layer is retained, not dropped. The public
leaderboard PR needs the canonical AWS `c6a.4xlarge`; this milestone delivers the adapter + a self-hosted-box
measured run (the public-box submission is the honest operational follow-up, like M124's wired-vs-running).

## Baseline Context

Repo state: git sha `eb5718a`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `benchmarks/theodb_bench/ann_adapter.py` | — | (NEW) | The `BaseANN`-shaped adapter wrapping TheoDB SQL (`fit`/`query`/`set_query_arguments`). |
| `benchmarks/theodb_bench/regression.py` | — | (NEW) | Byte-identical A/B over per-query neighbor lists (the retained wrap capability ClickBench/ann-benchmarks lack). |
| `benchmarks/run_m127_ann_vector.py` | — | (NEW) | Driver: build once, sweep ef, measure recall@10×QPS Pareto, emit per-query output + the wrap verdict. |
| `benchmarks/theodb_bench/test_ann_adapter.py` | — | (NEW) | Unit tests for the adapter contract + regression module. |
| `benchmarks/theodb_bench/significance.py` | 93 | `paired_significance` (M123) | Reused UNCHANGED (the significance half of the wrap layer). |
| `benchmarks/theodb_bench/recall.py` | 138 | recall@k vs ground-truth | Reused (recall metric). |
| `benchmarks/theodb_bench/db.py` | 278 | `VectorDB` connect/DDL | Reused (DB boundary). |

### Current callers / dependents (verified `file:line`)

- `benchmarks/theodb_bench/significance.py:22` — `paired_significance(a, b, *, seed, n_resamples)` — the wrap layer's significance half; the driver calls it on repeated runs.
- `benchmarks/theodb_bench/recall.py` — recall@k vs ground-truth (the ann-benchmarks recall metric).
- `benchmarks/theodb_bench/db.py:*` — `VectorDB` (psycopg connect + `theodb_hnsw` DDL) — the adapter's DB boundary.
- `theodb_rs/src/am/mod.rs:70` — `CREATE ACCESS METHOD theodb_hnsw` + `theodb_hnsw.ef_search` GUC — the index the adapter drives.
- Retire candidates (redundant bespoke comparative): `benchmarks/run_m32_sift1m.py`, `benchmarks/run_m33_scann.py` — superseded by the official adapter + wrap; deletion deferred to a follow-up to keep this slice additive.

### Domain glossary

- **BaseANN** — the ann-benchmarks algorithm interface: `fit(X)` (build), `query(v, n)` (top-n NN), optional `set_query_arguments(...)` (per-run search params). One point on the Pareto = one param set.
- **recall@10** — fraction of the true top-10 NN returned, averaged over queries (ann-benchmarks metric).
- **QPS-at-recall** — throughput read off the recall–QPS Pareto frontier at a fixed recall; ann-benchmarks enforces single-thread.
- **wrap layer** — the retained TheoDB analysis the official tools lack: paired significance + byte-identical regression + correctness.

### Architecture boundaries affected

Per `rules/architecture.md`: pure benchmark tooling (Python, `benchmarks/`) — NO production Rust change. The
adapter is an interface-shaped wrapper over the existing `VectorDB` boundary; the wrap layer is a leaf analysis
module. No layer direction changes; no on-disk/API change to the engine.

## Prior Art & Related Work

- Blueprint (web-evidenced, 2026-07-20): ann-benchmarks `BaseANN` contract (`fit`/`query`/`set_query_arguments`,
  HDF5 per-query output, single-thread), VectorDBBench pgvector/pgvectorscale/alloydb drivers as the copy template,
  and the unanimous Q11 finding (no official significance/regression → retain the wrap layer). Sources:
  github.com/erikbern/ann-benchmarks + arXiv:1807.05614 + github.com/zilliztech/VectorDBBench.
- Internal: `theodb_bench/significance.py` (M123 paired permutation) is the significance half already shipped;
  `recall.py` is the recall metric. This milestone adds the adapter + the regression half + the driver.

## ADRs

### ADR M127-1 — a BaseANN-shaped adapter over SQL, not a fork of ann-benchmarks

**Decision:** implement `ann_adapter.py` as a standalone class matching the ann-benchmarks `BaseANN` signature
(`fit`/`query`/`set_query_arguments`) wrapping `VectorDB` (psycopg + `theodb_hnsw` DDL), driven by our own
`run_m127` harness that follows the ann-benchmarks protocol (build once, sweep ef, single-thread recall×QPS). We
do NOT vendor/fork the ann-benchmarks framework.

**Rationale (cites blueprint + `rules/parsimony-ladder.md` rung 4):** the adapter is the only artifact a public
entry needs; the ann-benchmarks framework is a heavy Docker harness whose protocol we can honor with a thin driver
(YAGNI on the full framework). The class is signature-compatible, so a later public-box PR reuses it verbatim.

**Alternatives rejected:**
- **Vendor/fork the whole ann-benchmarks repo** — REJECTED: heavy, and its data harness overlaps our `dataset.py`
  (Rule 9 — don't add a redundant dependency).
- **Reuse run_m32/m33 as-is** — REJECTED: they are bespoke comparative scripts, not the `BaseANN` contract; they
  are the redundant thing this milestone supersedes.

### ADR M127-2 — measure on a D1-safe self-hosted box; public leaderboard PR is a follow-up

**Decision:** measure recall×QPS on **GloVe** (PDDL, D1-safe) on the self-hosted droplet, honestly labeled
"self-hosted box, not the canonical AWS `c6a.4xlarge`"; the public ann-benchmarks leaderboard PR (which requires
the canonical box) is a tracked operational follow-up.

**Rationale (cites blueprint license table + Rule 3):** GloVe is the only cleanly D1-safe-to-bundle dataset; SIFT/
GIST TEXMEX license is UNCONFIRMED (MUST-VERIFY) so it is CI-download-only, not part of this slice's committed
run. The self-hosted number proves the adapter + wrap; the canonical-box submission is a separate operational step
(mirrors M124 wired-vs-running honesty).

**Alternatives rejected:** claiming a leaderboard position from a self-hosted box — REJECTED (dishonest; the
leaderboard normalizes on `c6a.4xlarge`).

## Dependencies

`## Dependencies`: **none new** — reuses `numpy` (already in `benchmarks/requirements.txt`, used by
`significance.py`), `psycopg`/`h5py` (already used by `theodb_bench`). No crate/pip added. GloVe dataset is
CI-downloaded (PDDL), not vendored.

## Coverage Matrix

| Goal claim | Task |
|---|---|
| BaseANN-shaped adapter over TheoDB SQL | T1 (ann_adapter.py) |
| Reusable wrap layer: byte-identical regression (significance already exists) | T2 (regression.py) |
| Measured recall@10×QPS Pareto on GloVe (self-hosted) + significance + A/B | T3 (run_m127 driver + droplet run) |

## Phase 1 — the adapter + the wrap half

### T1.1 — `ann_adapter.py` (BaseANN contract over TheoDB SQL)

#### Why this step
The adapter is the artifact a public ann-benchmarks entry uses; it is the reusable seam ADR M127-1 defines.
Reasoning: a class with `fit(X)` (CREATE TABLE + COPY vectors + CREATE INDEX theodb_hnsw), `query(v, n)`
(`SET theodb_hnsw.ef_search`; `SELECT id ORDER BY e <-> v LIMIT n`), and `set_query_arguments(ef)` — matching the
ann-benchmarks signature exactly so a later public PR reuses it verbatim.

#### Files to edit
- `benchmarks/theodb_bench/ann_adapter.py` (NEW); `benchmarks/theodb_bench/test_ann_adapter.py` (NEW).

#### TDD
- RED: `test_ann_adapter_contract` asserts the class exposes `fit`, `query`, `set_query_arguments` and that
  `query` returns exactly `n` integer ids for a tiny in-memory fixture (a 5-vector table on a test DB).
- GREEN: implement the adapter over `VectorDB`; `query` returns the id list.
- REFACTOR: share the DDL with `db.py` (no duplicate CREATE INDEX logic).

#### Concurrency tests
(none — single-threaded) — ann-benchmarks enforces single-thread query; the adapter issues one SQL query per
`query()` call with no shared mutable state.

#### Failure scenarios
- DB connection refused / query error → the adapter raises a typed error (psycopg exception surfaced), not a
  silent empty result; the driver records it, never posts a fabricated recall.

#### Acceptance criteria
- `test_ann_adapter.py` passes: the adapter matches the BaseANN signature and `query(v,10)` returns 10 ids on the
  fixture; no production Rust changed.

#### DoD
- `python3 -m pytest benchmarks/theodb_bench/test_ann_adapter.py` green; the class is signature-compatible with ann-benchmarks BaseANN.

### T2.1 — `regression.py` (byte-identical A/B — the retained capability)

#### Why this step
The blueprint's unanimous finding: no official tool has byte-identical regression. Reasoning: a small module that,
given two per-query neighbor-id lists (baseline vs candidate), asserts they are identical and reports the first
diverging query — the reusable regression half of the wrap layer (significance is already `significance.py`).

#### Files to edit
- `benchmarks/theodb_bench/regression.py` (NEW); tests in `test_ann_adapter.py`.

#### TDD
- RED: `test_regression_detects_reorder` — two per-query result sets that differ on one query → the module reports
  NOT byte-identical + the offending qid; identical sets → byte-identical PASS.
- GREEN: implement the set-diff (aligned by qid, like the M125 `_align`).
- REFACTOR: reuse the qid-align helper shape from `run_m53_hybrid_beir.py`.

#### Concurrency tests
(none — single-threaded) — pure comparison of two in-memory result dicts.

#### Failure scenarios
- Mismatched qid sets between baseline/candidate → typed error (not a silent partial compare), so a dropped query
  can't masquerade as byte-identical.

#### Acceptance criteria
- `test_ann_adapter.py` regression tests pass: reorder detected with the offending qid; identical → PASS; mismatched qids → typed error.

#### DoD
- The regression module + significance.py together form the reusable wrap layer, unit-tested green.

## Phase 2 — measured Pareto + wrap verdict

### T3.1 — `run_m127` driver + measured GloVe run (self-hosted)

#### Why this step
The single metric: prove the adapter + wrap on a real dataset. Reasoning: the driver builds the index once via the
adapter's `fit`, sweeps ≥3 `ef_search` points, measures recall@10 (vs GloVe ground-truth) × QPS single-thread per
point (the Pareto), runs the query pass twice to feed `paired_significance`, and runs the byte-identical A/B
(re-query the same index → `regression.py` must report identical).

#### Files to edit
- `benchmarks/run_m127_ann_vector.py` (NEW); `docs/benchmarks/m127-ann-benchmarks-vector.md` + `.json` (NEW).

#### TDD
- RED: on a fresh index, a too-low `ef_search` yields recall@10 < 1.0 and higher `ef` raises it — the driver
  asserts the Pareto is monotonic-ish (recall non-decreasing with ef) and that QPS is measured (>0), else FAIL.
- GREEN: the driver produces the Pareto + the significance verdict (repeated run) + the byte-identical A/B PASS on
  GloVe on the self-hosted droplet.

#### Concurrency tests
(none — single-threaded) — ann-benchmarks single-thread protocol; QPS is measured single-client by design.

#### Failure scenarios
- **GloVe download fails / HTTP 5xx** → the driver exits UNBENCHMARKED (no fabricated numbers), mirroring the BEIR harness's OPENAI-absent path.
- **DB unreachable mid-run** → typed error surfaced; partial results discarded, not reported.
- **Non-deterministic rankings across the A/B re-query** → the byte-identical A/B FAILS loudly (that is the regression capability working).

#### Acceptance criteria
- `docs/benchmarks/m127-ann-benchmarks-vector.md` records a real recall@10×QPS Pareto (≥3 ef points) on GloVe on
  the self-hosted box, a paired-significance verdict on the repeated run, and a byte-identical A/B PASS; labeled
  "self-hosted box, not canonical c6a" (ADR M127-2).

#### DoD
- The measured artifact exists with the Pareto + wrap verdicts; the run is re-runnable (`run_m127_ann_vector.py`).

## Failure scenarios

- **GloVe dataset download HTTP failure / 5xx** — the driver exits `UNBENCHMARKED` cleanly (no fabricated recall/QPS), exactly like the BEIR harness's missing-key path.
- **TheoDB (psycopg) connection refused or query error mid-run** — a typed error is surfaced and partial results are discarded, never reported as a number.
- **Ranking non-determinism on the same index across the A/B re-query** — the byte-identical regression check FAILS loudly (the retained capability doing its job), rather than silently averaging it away.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| The measured run is on a self-hosted box, not the canonical AWS `c6a.4xlarge` → not a leaderboard-comparable number | MEDIUM | Honestly labeled (ADR M127-2); the public-box PR is a tracked operational follow-up (M124 wired-vs-running precedent) | implementer |
| GloVe ground-truth must match the ann-benchmarks recall definition or recall numbers are wrong | MEDIUM | Use the ann-benchmarks GloVe HDF5 (bundled ground-truth top-100) OR compute exact top-10 via a seqscan oracle; assert recall=1.0 at high ef as the sanity gate | implementer |
| `cargo pgrx` / droplet build gotchas (as in M124/M126) | LOW | Reuse the proven droplet setup (pgtest non-root, shared_preload); no production Rust change here so no rebuild needed beyond the installed .so | implementer |

## Unresolved Questions

- Should the redundant `run_m32_sift1m.py` / `run_m33_scann.py` be deleted in this slice or a follow-up? Resolved
  at plan time: **follow-up** — keep this slice additive (adapter + wrap + measured run); retiring the bespoke
  comparatives is a separate cleanup once M128–M130 also land (avoids a big-delete mid-pilot).
- (none other — every in-scope decision is resolved at plan time.)

## Global DoD

- `ann_adapter.py` (BaseANN contract) + `regression.py` (byte-identical A/B) + `run_m127_ann_vector.py`, unit
  tests green (`test_ann_adapter.py`).
- A MEASURED recall@10×QPS Pareto (≥3 ef points) on GloVe on the self-hosted droplet + a paired-significance
  verdict + a byte-identical A/B PASS, in `docs/benchmarks/m127-ann-benchmarks-vector.md` — honestly labeled
  self-hosted-box.
- No production Rust change; no new dependency. CHANGELOG `[Unreleased]`. Honest positioning: any ScaNN/AlloyDB
  reference cites `docs/benchmarks/m73-headtohead-verdict.md` for magnitude. `/code-quality` ∉ {FAIL_HARD, INVALID}.
