---
slug: m32-scale-benchmark
milestone_id: M32
created_at: 2026-07-02
goal: Produce reproducible ≥1M-vector head-to-head scale evidence (QPS/p50-p95-p99/recall@10/build-time/index-bytes for theodb_ivfflat+theodb_hnsw vs pgvector ivfflat+hnsw) by extending theodb_bench, measured by benchmarks/tests/test_scale_benchmark.py passing + a committed docs/benchmarks/m32-scale-sift1m.{md,json} artifact.
---

# M32 — Scale benchmark harness (≥1M vectors, QPS head-to-head vs pgvector)

## Goal

Extend `theodb_bench` to run a 4-way (theodb_ivfflat, theodb_hnsw, pgvector ivfflat, pgvector hnsw) scale
head-to-head on SIFT1M (≥1M×128), emitting QPS + p50/p95/p99 + recall@10 + build-time + index-bytes with
mean±std over ≥3 runs and an honest per-knob verdict, **measured by** `benchmarks/tests/test_scale_benchmark.py`
passing (4-way harness at a CI-safe real N) **and** a committed `docs/benchmarks/m32-scale-sift1m.{md,json}`
artifact produced by the real ≥1M run.

## Context

Prior art is 80% in-repo: the mature `theodb_bench` harness already loads ANN-Benchmarks HDF5, computes
float32-matched exact ground truth, measures ANN-Benchmarks distance-thresholded recall, p50/p95/p99, best-of-N
QPS, index bytes (`pg_relation_size`), and build time, and writes JSON+MD. Discovery
(`.claude/knowledge-base/discoveries/blueprints/m32-scale-benchmark-blueprint.md`) found three gaps: (1) the harness
recomputes brute-force GT (infeasible at 1M×10k = 10¹⁰); (2) the CLI wires only pgvector index specs, not the M26
`theodb_ivfflat`/`theodb_hnsw` AMs; (3) host RAM is ~7 GB and the theodb AMs build the full corpus in-memory in one
PG backend — feasible but tight/slow. This plan closes those three gaps with minimal, reused code.

## Baseline Context

### Files that will be touched

| File | LoC | git sha (last) | Why |
|---|---|---|---|
| `benchmarks/theodb_bench/dataset.py` | 96 | `61e64db` | ADD `load_hdf5_full` (full train + GT distances from HDF5 `neighbors`) |
| `benchmarks/theodb_bench/harness.py` | 140 | `61e64db` | branch GT: use precomputed neighbors-GT when provided, else brute-force (unchanged small-N path) |
| `benchmarks/theodb_bench/__main__.py` | 176 | `61e64db` | ADD `_theodb_ivfflat_spec`/`_theodb_hnsw_spec`; `--index` gains `theodb_ivfflat`/`theodb_hnsw`/`4way`; `--full-train` flag |
| `benchmarks/tests/test_scale_benchmark.py` | (NEW) | — | 4-way harness gate at CI-safe real N + neighbors-GT unit test |
| `docs/benchmarks/m32-scale-sift1m.{md,json}` | (NEW) | — | the ≥1M artifact (produced by the operator run) |

### Current callers / dependents

- `theodb_bench.harness.run_benchmark` — called by `theodb_bench.__main__.main` and by `benchmarks/tests/test_integration.py:143`, `test_harness.py:34`. Signature/return unchanged by this plan (only an internal GT branch added).
- `theodb_bench.dataset.load_hdf5_subsample` / `make_dataset` — called by `run_benchmark` (`harness.py:31-34`). Unchanged; `load_hdf5_full` is additive.
- `theodb_bench.__main__.build_config` — called by `test_integration.py:82`, `test_harness.py:116`. New `--index` choices are additive (existing choices unchanged).
- `VectorDB.build_index` / `.index_size_bytes` / `.query_topk` / `.assert_index_used` (`db.py:100,131,110,122`) — reused as-is; already index-type-agnostic (any DDL / any index name).

### Domain glossary

- **SIFT1M** — ANN-Benchmarks `sift-128-euclidean` dataset: `train` 1M×128, `test` 10k×128, `neighbors` 10k×100 exact GT ids (Euclidean/l2).
- **neighbors-GT** — exact ground-truth distances derived by loading the true-neighbor vectors (from `train[neighbors[q]]`) and computing their distance to each query — 10k×100 distances, cheap; avoids the 10¹⁰ brute force.
- **op-point** — a single (recall, QPS) operating point. theodb AMs have NO query knob (fixed `SCAN_PROBES=10` / `SCAN_EF=64`), so each reports ONE op-point vs pgvector's swept curve.
- **best-of-N QPS** — `1/min(per-run mean latency)` (`metrics.qps_best_of_n`).

### Architecture boundaries affected

None in the Rust engine. All changes are in the Python `benchmarks/theodb_bench` harness (dev-only tooling, not the shipped extension). DIP preserved: `run_benchmark` still takes an injected `db` (`harness.py:29`).

## Prior Art & Related Work

- In-repo blueprint: `.claude/knowledge-base/discoveries/blueprints/m32-scale-benchmark-blueprint.md` (this cycle).
- In-repo harness (the thing extended): `benchmarks/theodb_bench/{db,dataset,harness,metrics,recall,__main__}.py`.
- Existing real-dataset artifact (the pattern to follow): `docs/benchmarks/2026-06-27-glove-25-angular.{md,json}` (GloVe-25 via the same harness).
- Reference (cloned): `.claude/knowledge-base/references/pgvector` (ivfflat/hnsw index semantics + recall-test methodology).
- ANN-Benchmarks semantics already implemented: `benchmarks/theodb_bench/recall.py:61` (`recall_at_k`, distance-thresholded).

## ADRs

### ADR-1 — Extend `theodb_bench`; do not build a new harness
**Decision:** add `load_hdf5_full` + two theodb specs + a GT branch to the existing harness.
**Rationale:** parsimony rung 4 (reuse installed) — the harness already does load/GT/build/query/recall/percentiles/
QPS/report (`harness.py`). Per `README.md`/architecture, do not reinvent (Unbreakable Rule 9).
**Alternatives rejected:** (a) standalone 1M script — duplicates the mature, tested harness; (b) ann-benchmarks
upstream runner — heavy Docker-in-Docker, does not know theodb's AMs.

### ADR-2 — neighbors-GT for 1M (not brute force)
**Decision:** when `--full-train`, compute GT distances from the HDF5 `neighbors` ids (load true-neighbor vectors,
distance to each query) instead of `brute_force_ground_truth`.
**Rationale:** 1M×10k brute force = 10¹⁰ f32 distances (hours + RSS); neighbors-GT is 10k×100 = 10⁶ (seconds). The
HDF5 ships exact GT; `recall_at_k` needs GT *distances* (`recall.py:61`), which the neighbor vectors yield exactly.
**Alternatives rejected:** (a) chunked brute-force subsample — still 10⁹ for 1000 queries, and reduces query count;
kept only as a fallback when `neighbors` is absent; (b) trust HDF5 `distances` dataset if present — not all files
ship it and precision/metric may differ from our float32 contract; recomputing from neighbor vectors is safest.

### ADR-3 — theodb at a fixed op-point vs pgvector sweep (honest)
**Decision:** theodb specs carry NO sweep (session = `SET enable_seqscan=off` only); pgvector keeps its ef/probes
sweep. The verdict states theodb is a single fixed op-point.
**Rationale:** no theodb query knob exists (fixed Rust constants). Fabricating a sweep would be dishonest
(Rule 3 / `public-copy.md`). **Alternatives rejected:** adding a GUC/reloption now — that is an engine change, out
of M32 (a measurement milestone); it is a named future milestone.

### ADR-4 — CI runs a scaled real N; ≥1M is an operator artifact
**Decision:** `test_scale_benchmark.py` runs the 4-way harness at a CI-safe N (≤ 50k) on real SIFT data; the ≥1M
run is operator-invoked and committed as `docs/benchmarks/m32-scale-sift1m.{md,json}` with its reproduction command.
**Rationale:** a ≥1M 4-way run (tens of min–hours, theodb single-thread builds) cannot gate every commit. Same
pattern as the committed GloVe-25 artifact. The DoD "≥1M" is met by the artifact + reproducer, not by CI.

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `h5py` | `3.16.0` (`>=3.10`) | Python | read ANN-Benchmarks HDF5 (`train`/`test`/`neighbors`); dev-only, BSD-3 |
| `numpy` | `1.26.4` | Python | vector arrays + GT distance math; dev-only, BSD-3 |
| `psycopg2` | (installed) | Python | container I/O via `VectorDB`; dev-only |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | | | the harness + h5py/numpy already cover load, GT, recall, percentiles, QPS, report | no new dep needed |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency graph

```
Phase 0 (acquire SIFT1M) ──▶ Phase 1 (neighbors-GT loader) ──▶ Phase 2 (theodb specs + CLI) ──▶ Phase 3 (scale test + ≥1M run) ──▶ Phase 4 (report + verdict)
                                     │                                                              ▲
                                     └──────────────────── (Phase 1 test does not need the container; Phase 3 does)
```

## Phase 1 — neighbors-GT dataset loader

### T1.1 — `load_hdf5_full` (full train + GT distances from HDF5 `neighbors`)

#### Why this step
The harness cannot compute 1M ground truth by brute force (10¹⁰). ADR-2: derive exact GT distances from the HDF5's
precomputed `neighbors` ids by loading the true-neighbor vectors and measuring their distance to each query
(10⁶ ops). This is the load-bearing scale unlock; without it the ≥1M DoD is infeasible on any host.

#### Files to edit
- `benchmarks/theodb_bench/dataset.py` (add `load_hdf5_full(path, n_queries, seed, k) -> (corpus, queries, gt_dist)`)
- `benchmarks/tests/test_scale_benchmark.py` (NEW — unit test, no container)

#### TDD
- RED: `test_load_hdf5_full_gt_matches_brute_force` — build a tiny synthetic HDF5 (train/test/neighbors) in a
  tmp file; assert `load_hdf5_full`'s returned `gt_dist[:, :k]` equals `brute_force_ground_truth(corpus, queries,
  k, "l2")[1]` within `1e-4` (same float32 contract). Given-When-Then: given an HDF5 with known neighbors, when
  loaded full, then the neighbors-GT distances equal the brute-force GT distances.
- RED: `test_load_hdf5_full_returns_full_train` — assert returned corpus row count == train size (no subsample).
- GREEN: implement `load_hdf5_full` — load full `train` (float32 to bound RSS), subsample `n_queries` from `test`
  (seeded), gather `neighbors[query_idx, :k]`, compute distances query→train[neighbor] in float32 (reuse the
  metric math shape from `recall.py`), return sorted-ascending gt_dist.
- REFACTOR: share the l2/cosine distance kernel with `recall.py` (no duplicated distance logic — DRY).

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `pytest benchmarks/tests/test_scale_benchmark.py -k gt_matches_brute_force` passes.
- `load_hdf5_full` returns `gt_dist` with shape `(n_queries, k)`, float64, ascending per row.
- corpus dtype is float32 (RSS bound); no subsample of train.

#### DoD
- `python3 -c "from theodb_bench.dataset import load_hdf5_full"` imports clean.
- The neighbors-GT test is green; the distance kernel is not duplicated (grep shows one l2 helper).

## Phase 2 — theodb AM specs + CLI wiring

### T2.1 — `_theodb_ivfflat_spec` + `_theodb_hnsw_spec` + `--index 4way` + `--full-train`

#### Why this step
The DoD is a 4-way head-to-head, but `build_config` wires only pgvector specs (`__main__.py:126`). ADR-3: theodb
specs carry no sweep (fixed op-point). This makes `run_benchmark` measure all four indexes in one invocation
(it already loops over `index_specs` — `harness.py:37`).

#### Files to edit
- `benchmarks/theodb_bench/__main__.py` (add two spec builders; extend `--index` choices + `build_config`; add
  `--full-train` flag that sets `config["full_train"]=True`)
- `benchmarks/theodb_bench/harness.py` (branch GT: if `config.get("full_train")` use `load_hdf5_full` + its gt_dist;
  else the existing path)
- `benchmarks/tests/test_scale_benchmark.py` (extend — build_config 4way assertions)

#### TDD
- RED: `test_build_config_4way_has_four_specs` — `build_config(parse(["--index","4way","--metric","l2"]))`
  yields specs named `{hnsw, ivfflat, theodb_ivfflat, theodb_hnsw}`; the two theodb DDLs are
  `CREATE INDEX ... USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops)` /
  `... USING theodb_hnsw (embedding theodb_hnsw_l2_ops)`; theodb specs have a single sweep entry with
  `session == ["SET enable_seqscan = off"]` (no ef/probes) — ADR-3.
- RED: `test_theodb_specs_l2_only` — requesting `--metric cosine --index 4way` raises/skips theodb specs with a
  clear message (theodb is l2-only — see ADR-2 and blueprint decision on l2-only), never emits a fabricated cosine opclass.
- GREEN: implement the spec builders + `build_config` branch + `--full-train`.
- REFACTOR: factor a `_theodb_spec(name, am, opclass, table)` helper (DRY across the two AMs).

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `python3 -m pytest benchmarks/tests/test_scale_benchmark.py -k build_config` green.
- `--index 4way` produces exactly 4 specs (l2); theodb specs have no ef/probes session statements.
- `--index theodb_ivfflat` / `theodb_hnsw` each produce their single spec.

#### DoD
- Existing `--index {hnsw,ivfflat,both,all}` behavior unchanged (regression: `test_integration.py`/`test_harness.py`
  still green).
- No fabricated symbols: DDL opclass names match the registered `theodb_ivfflat_l2_ops`/`theodb_hnsw_l2_ops`.

## Phase 3 — scale integration test + the ≥1M run

### T3.1 — 4-way harness gate at CI-safe real N (integration test)

#### Why this step
Prove the 4-way harness runs end-to-end against a real container on real SIFT data, at an N small enough for CI
(ADR-4). This is the committed, repeatable gate; the ≥1M run is the artifact.

#### Files to edit
- `benchmarks/tests/test_scale_benchmark.py` (integration test, `@pytest.mark.integration`)

#### TDD
- RED: `test_4way_scale_harness_runs_on_real_data` — `_seed`/load a small SIFT subsample (e.g. n=20k via
  `load_hdf5_subsample` on the cached HDF5; skip with a clear reason if the file is absent), build config
  `--index 4way`, run `run_benchmark`, assert: 4 index families present in results; every result has
  `0 ≤ recall_at_k ≤ 1`, `qps > 0`, `build_ms > 0`, `index_bytes > 0`; each theodb AM's `assert_index_used`
  passed (no seqscan fallback). Given-When-Then: given real SIFT vectors + 4 indexes, when the harness runs, then
  every AM yields a valid measured op-point via its own index.
- GREEN: the Phase-1/2 code already provides this; the test wires it.
- REFACTOR: none (test-only).

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- SIFT HDF5 absent → `pytest.skip("SIFT1M not cached; run scripts/fetch_sift1m or pass --hdf5")` (never a false pass).
- A theodb build OOMs at the CI N → test fails loud with the RSS/error (not silently skipped).

#### Acceptance criteria
- `PGPORT=<container> python3 -m pytest benchmarks/tests/test_scale_benchmark.py -m integration` green against
  `theo-db:m32` (the M31b image + this harness; no engine change so `theo-db:m31b` suffices).
- The 4 index families each produce a valid measured row through their own index.

#### DoD
- The scale test passes on the container; skips cleanly (with reason) when the dataset is missing.

### T3.2 — the ≥1M SIFT1M artifact run (operator)

#### Why this step
The DoD's core evidence: the real ≥1M head-to-head numbers. ADR-4: operator-invoked, committed as an artifact.

#### Files to edit
- `docs/benchmarks/m32-scale-sift1m.{md,json}` (produced by the run)

#### TDD
- Not a unit test — this is the measured artifact. Validation is the DoD checks below (the numbers exist, are
  reproducible, mean±std over ≥3 runs, hardware + peak RSS recorded).

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- theodb_hnsw build OOMs/exceeds time at 1M → per ADR-3/T3 of the blueprint, record the ceiling reached honestly
  (the exact N/RSS wall) and report theodb_ivfflat + both pgvector at 1M; NEVER fabricate the missing number.
  A partial-but-honest artifact + a documented resource wall satisfies Rule 3; the "≥1M" claim is then scoped to
  the indexes that built (with the wall documented).

#### Acceptance criteria
- `docs/benchmarks/m32-scale-sift1m.json` exists with `n >= 1_000_000`, `runs >= 3`, per-index
  `{recall_at_k, qps, p50, p95, p99, mean, std, build_ms, index_bytes}`, `host`, and a `peak_rss` note.
- The `.md` carries the per-knob honest verdict (parity/superior/inferior with the number) + the exact repro command.

#### DoD
- The artifact is committed; its reproduction command re-runs the harness at ≥1M.
- No cherry-pick: the full QPS-recall frontier (all pgvector sweep points + theodb op-points) is reported.

## Phase 4 — honest per-knob verdict + CHANGELOG

### T4.1 — verdict synthesis + CHANGELOG

#### Why this step
The DoD requires an honest per-knob verdict (ANN-Benchmarks ethos, no cherry-pick) and Unbreakable Rule 6 requires
a CHANGELOG entry.

#### Files to edit
- `docs/benchmarks/m32-scale-sift1m.md` (verdict section)
- `CHANGELOG.md` (`[Unreleased] § Added`)

#### TDD
(none — documentation synthesis over the measured artifact)

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- The verdict names, for each (index-family, operating-knob), one of `parity`/`superior`/`inferior` **with the
  number** and the comparison baseline; theodb's fixed-op-point limitation is stated (ADR-3).
- CHANGELOG `[Unreleased] § Added` has an M32 entry linking the artifact.

#### DoD
- `python3 skills/release/scripts/changelog_section_nonempty.py --section Unreleased` (or the fallback) shows the M32 entry.

## Coverage Matrix

| Goal claim / DoD item | Task(s) |
|---|---|
| Harness runs ≥1M vectors (SIFT1M) against the container; reuses theodb_bench recall | T1.1 (GT unlock), T3.2 (1M run) |
| Table QPS + p50/p95/p99 + recall@10 + build time + index bytes; theodb ivfflat/hnsw vs pgvector; mean±std ≥3 runs; hardware; docs/benchmarks + .json | T2.1 (4-way specs), T3.2 (artifact), reused `metrics`/`harness` |
| Honest per-knob verdict (parity/superior/inferior), no cherry-pick, ANN-Benchmarks semantics | T4.1 (verdict), reused `recall.recall_at_k` |
| CI-repeatable 4-way harness proof | T3.1 |
| No new dependency (Rule 9) | Dependencies section (none) |
| CHANGELOG (Rule 6) | T4.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| theodb_hnsw in-memory build OOMs at 1M on ~7 GB host | HIGH | float32 corpus; raise nothing for theodb (in-proc); if it walls, record the ceiling honestly (T3.2 failure scenario) + report the indexes that built — never fabricate | paulohenriquevn |
| ≥1M run is slow (single-thread scalar theodb builds; tens of min) | MEDIUM | operator artifact, not CI (ADR-4); "esforço alto é bem-vindo"; bound query count (≤10k) for tractable QPS | paulohenriquevn |
| QPS non-determinism under host contention (workspace containers) | MEDIUM | best-of-N QPS (`min` per-run) already de-noises; report mean±std; note host load | paulohenriquevn |
| pgvector hnsw build at 1M needs maintenance_work_mem | MEDIUM | `SET maintenance_work_mem` in the pgvector build session (harness spec) or document the setting used | paulohenriquevn |

## Unresolved Questions

- Does `theodb_hnsw` build complete at 1M×128 within the 7 GB RAM wall, or only theodb_ivfflat? — resolved
  empirically in T3.2 (probe 100k→250k→500k→1M); the artifact records the ceiling either way.
- Is the HDF5 `neighbors` GT in float32-comparable precision to our contract? — T1.1 recomputes distances from the
  neighbor VECTORS (not trusting a shipped `distances` array), so the contract holds by construction.

## Failure scenarios

- **SIFT HDF5 missing** → tests skip with an explicit reason; the artifact run refuses with a clear message. (T3.1)
- **theodb build OOM at 1M** → honest ceiling report, partial artifact, no fabricated number. (T3.2)
- **A theodb AM falls back to seqscan** → `assert_index_used` fails loud (harness `harness.py:44`). (T3.1)

## Final Phase — Integration Validation

- `pytest benchmarks/tests/test_scale_benchmark.py` (unit GT test + build_config) green (no container).
- `PGPORT=<c> pytest benchmarks/tests/test_scale_benchmark.py -m integration` green (4-way on real SIFT subsample).
- Coexistence: `test_integration.py` + `test_harness.py` still green (harness API unchanged).
- The ≥1M artifact committed with mean±std ≥3 runs + honest per-knob verdict + repro command.
