---
slug: m45-rigorous-pareto-claim
created_at: 2026-07-03
goal: Produce a rigorous mean±std recall×QPS Pareto benchmark of theodb_hnsw vs pgvector hnsw on SIFT1M with an honest matched-recall margin verdict.
---

# Plan: Rigorous mean±std Pareto claim — theodb_hnsw vs pgvector hnsw on SIFT1M

> **Version 1.1** — (v1.1: absorbed edge-case EC-2 single-point-frontier + EC-3 zero-latency-guard RED tests; EC-1 no-overlap confirmed covered.) Convert the M42 single-run superiority *signal* into a defensible, reproducible *claim*. Build both HNSW indexes on real SIFT1M with matched build params, sweep a shared `ef_search` grid, run ≥3 timed samples per operating point to get **mean ± std** QPS + recall against exact GT, then compute the **honest QPS margin at matched recall** by Pareto interpolation. If the margin exceeds combined variance → a licensed superiority claim (`public-copy.md` §4 half 1); if not → an honest parity/no-claim result. No new dependency; reuses the `theodb_bench` harness pieces.

## Goal

> Enable TheoDB to make a defensible vector-superiority claim by producing a reproducible **mean±std recall×QPS Pareto** benchmark of `theodb_hnsw` vs `pgvector hnsw` on SIFT1M, measured by `benchmarks/tests/test_run_m45_pareto.py` passing AND `docs/benchmarks/m45-pareto-sift1m.json` containing ≥3-run mean±std per shared-`ef` operating point for both indexes plus a matched-recall margin verdict.

## Context

The M41→M44 arc (v0.33.2–v0.33.5) made `theodb_hnsw` competitive on build, scan, and recall×QPS. The strongest superiority evidence is M42 (`docs/benchmarks/sift1m-carrier-verdict.md`): on real SIFT1M, `theodb_hnsw` beats `pgvector hnsw` ~1.7–2.8× at matched recall. **That number is not yet a defensible public claim** — the M42 doc's own caveats say so: the theodb Pareto sweep was single-run, pgvector had only 2 operating points, the ~1.7–2.8× was eyeballed, and the sample was capped at 200. `public-copy.md` §4 requires a reproducible artifact (half 1) + independent reproduction (half 2) for a comparative claim. This plan delivers half 1 rigorously and produces the artifact ready for half 2. It closes the still-open M32 DoD bullet (`ROADMAP.md:313` — "mean±std ≥3 runs table", `[ ]`).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `benchmarks/m45_pareto.py` (NEW) | 0 | — | (pure post-processing: interpolation + margin verdict) | — |
| `benchmarks/run_m45_pareto.py` (NEW) | 0 | — | (driver: build both indexes, shared ef sweep, mean±std, write doc) | — |
| `benchmarks/tests/test_run_m45_pareto.py` (NEW) | 0 | — | (unit tests for pure logic + integration structure test) | — |
| `benchmarks/theodb_bench/dataset.py` | 109 | `5887132` (2026-07-02) | `load_hdf5_full` — full 1M train + exact neighbors-GT | READ-ONLY (import only; do not modify) |
| `benchmarks/theodb_bench/db.py` | 264 | `16421b2` (2026-06-28) | `VectorDB` — build/query/session/index_size | READ-ONLY (import only) |
| `benchmarks/theodb_bench/recall.py` | 138 | `5887132` (2026-07-02) | `recall_at_k` vs GT distances | READ-ONLY (import only) |
| `docs/benchmarks/m45-pareto-sift1m.md` (NEW) | 0 | — | (the human-readable artifact) | — |
| `docs/benchmarks/m45-pareto-sift1m.json` (NEW) | 0 | — | (the machine artifact for reproduction) | — |
| `CHANGELOG.md` | — | — | public contract | `[Unreleased]` discipline (Rule 6) |

### Current callers / dependents

- **Symbol:** `run_m45_pareto.run()` in `benchmarks/run_m45_pareto.py` (NEW) — **Callers:** its own `main()` + `test_run_m45_pareto.py`. No production caller (a benchmark driver, like `run_m44_parallel_build.run`). **External:** no.
- **Symbol:** `interpolate_qps_at_recall()`, `pareto_margin_verdict()` in `benchmarks/m45_pareto.py` (NEW) — **Callers:** `run_m45_pareto.py` + the unit tests. **External:** no.
- **Reused (unmodified):** `theodb_bench.dataset.load_hdf5_full` (`dataset.py:56`), `theodb_bench.db.VectorDB` (`db.py:30`), `theodb_bench.recall.recall_at_k` (`recall.py:117`) — all imported; zero changes to their callers.

### Domain glossary

- **recall×QPS Pareto frontier** — the ANN-Benchmarks standard: for a range of `ef_search`, plot recall (x) vs throughput QPS (y); the curve, not one point, is the comparison.
- **matched-recall margin** — QPS(theodb)/QPS(pgvector) at a common recall level, obtained by linear interpolation on each frontier (the ann-benchmarks "QPS@recall=R" convention).
- **`ef_search`** — HNSW scan-time candidate-list size (session GUC): `SET theodb_hnsw.ef_search = N` (guc.rs:19; default 64, max 1000) / `SET hnsw.ef_search = N` (pgvector). Higher = more recall, lower QPS. Sweeping it needs NO rebuild.
- **neighbors-GT** — exact ground truth from the HDF5 `neighbors` ids (ANN-Benchmarks), computed in 10⁶ ops not 10¹⁰ brute force (`dataset.py:56`).
- **effect > variance gate (PRD D3)** — a superiority claim is licensed only when the margin exceeds combined std bands; otherwise the honest output is parity/no-claim (anti-sunk-cost, measurement-first).

### Architecture boundaries affected

None in the product (`theodb_rs`). This is a **benchmark-only** change under `benchmarks/` — the DIP boundary it respects is `theodb_bench`'s `VectorDB` interface (blueprint ADR D1: the harness accepts an injected `db`, testable without a container). No product code, no schema, no public API. The `benchmarks/` tree is outside the `theodb_rs/src` file-size budget domain but the 500-LoC guideline still applies per file.

## Prior Art & Related Work

- **Internal blueprint:** `knowledge-base/discoveries/blueprints/m45-rigorous-pareto-claim-blueprint.md` — Coverage Corners (Techniques: ANN-Benchmarks Pareto, matched-recall interpolation; ADR D1/D2/D3).
- **Internal benchmark:** `docs/benchmarks/sift1m-carrier-verdict.md` (M42) — the signal made rigorous here.
- **Internal driver pattern:** `benchmarks/run_m44_parallel_build.py` + `benchmarks/tests/test_run_m44_parallel_build.py` — the mean±std driver + structure-test shape mirrored.
- **Reference harness:** `benchmarks/theodb_bench/{dataset,db,recall}.py` — reused by import (Rule 9, parsimony rung 4).
- **Rules:** `.claude/rules/discover-phd-rigor.md` R1 (SOTA-anchor: pgvector hnsw = SOTA permissive baseline) + R3 (benchmark-evidence); `.claude/rules/public-copy.md` §4 (comparative-claim contract).
- **External:** ANN-Benchmarks methodology (Aumüller, Bernhardsson, Faithfull — the recall×QPS Pareto standard). Relevance: the field-standard way to compare ANN indexes; a single operating point is not a comparison.

## Objective

- [ ] Sub-goal 1 — a pure, unit-tested `interpolate_qps_at_recall(points, target_recall)` that returns interpolated QPS on a frontier (and `None` when the target recall is out of the measured range).
- [ ] Sub-goal 2 — a pure, unit-tested `pareto_margin_verdict(theodb_pts, pgvector_pts)` returning the matched-recall margin(s) + an honest verdict token (`SUPERIOR` / `PARITY` / `INFERIOR`) gated on effect > variance.
- [ ] Sub-goal 3 — a driver `run_m45_pareto.py` that builds both indexes on SIFT1M, sweeps a shared ef grid, runs ≥3 timed samples per point → mean±std QPS + recall vs exact GT, and writes `docs/benchmarks/m45-pareto-sift1m.{md,json}`.
- [ ] Sub-goal 4 — the real SIFT1M artifact produced, with the honest verdict recorded (superiority OR parity — no cherry-picking), CHANGELOG updated.

## ADRs

### D1 — Measure the index-AM path (`USING theodb_hnsw` vs `USING hnsw`)
- **Decision:** build via `CREATE INDEX … USING theodb_hnsw (embedding theodb_hnsw_l2_ops)` and `USING hnsw (embedding vector_l2_ops)`; query through the planner (`SET enable_seqscan=off`, assert index used).
- **Rationale:** this is the shipped carrier end-to-end (page-native layout + M41 scan kernel) and the exact M42 comparison; it is what a user runs.
- **Alternatives considered:** (a) `theodb.hnsw_knn` SQL function (`bench_ann_index.py`) — a different, non-planner path, rejected as unrepresentative; (b) in-memory `ann/hnsw.rs` micro-bench — bypasses the page layout that IS the product, rejected.
- **Consequences:** requires a running container (integration), and the theodb scan sample may be capped for tractability (recorded honestly).

### D2 — Report mean±std over ≥3 runs, not best-of-N
- **Decision:** per operating point, run ≥3 timed passes; report QPS as `mean ± pstdev`. Keep best-of-N only as a secondary continuity column.
- **Rationale:** best-of-N hides variance — the exact flaw `public-copy.md` §4 and the M42 caveat call out. The claim must show the spread.
- **Alternatives considered:** best-of-N alone (M42's flaw), rejected; median (less standard than mean±std on the QPS axis in ANN-Benchmarks), rejected.
- **Consequences:** ~3× the query wall-clock vs a single run; acceptable (rigor is the point — Esforço≠Complexidade).

### D3 — New self-contained driver; do NOT modify the shared harness
- **Decision:** add `run_m45_pareto.py` + pure `m45_pareto.py`; reuse `theodb_bench.{dataset,db,recall}` by import; do NOT touch `theodb_bench/harness.py`.
- **Rationale:** `harness.py` reports best-of-N and is consumed by M32–M44 drivers; changing it risks regressions in shipped artifacts (blast radius). A thin driver isolates the mean±std rigor.
- **Alternatives considered:** extend `harness.py` to emit per-run distributions — rejected (YAGNI for the other drivers + regression risk).
- **Consequences:** small duplication of the measurement loop (query→time→recall) in the driver; justified by isolation (DRY applies to business logic, not to an intentionally-isolated benchmark loop).

### D4 — Matched build params, asserted
- **Decision:** both indexes built at `m=16, ef_construction=64` (theodb: `HNSW_M`/`HNSW_EF_CONSTRUCTION` in `am/build.rs:16-17`; pgvector: `WITH (m=16, ef_construction=64)`), single-threaded build (`max_parallel_maintenance_workers=0`) on both.
- **Rationale:** a fair Pareto requires identical graph-construction budget; otherwise the frontier comparison is confounded.
- **Alternatives considered:** each index at its own default — rejected (pgvector default IS m=16/efc=64, so matching is free and removes a confound).
- **Consequences:** the theodb build param is a fixed Rust constant (not a reloption), so the match is asserted in the doc, not set via DDL for theodb.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Single machine, no independent reproduction — delivers only `public-copy.md` §4 half 1 | Medium | Declare openly in the doc + Goal; produce fixed-seed, pinned-image, exact-command artifact ready for half 2 | bench |
| The rigorous margin may be SMALLER than M42's 2.8× (as M41 rigor shrank 2.4-3.0→1.2-1.5×) | Medium | Publish the honest number; the plan's success is a rigorous measurement, NOT a target multiplier (anti-sunk-cost) | bench |
| Full ef-sweep ×3 runs ×2 indexes at 1M is expensive wall-clock | Low | ef_search sweeps need no rebuild (GUC); cap theodb query sample if needed (recorded); build once per index | bench |
| theodb query-sample cap could bias recall vs pgvector's full sample | Low | Use the SAME query subset for both indexes at each ef; record the sample size in the artifact | bench |
| Container flakiness (initdb restart timing) mid-run | Low | Wait-for-ready loop with retries (mirror run_m44); fail loud if the DB never becomes ready | bench |

## Unresolved Questions

- Q1 — What shared `ef_search` grid best covers the frontier overlap? (Plan resolves: `[40, 64, 100, 200, 400]` — the M42 grid; both indexes' recall spans the interesting 0.92–0.999 band there.)
- Q2 — Do the two frontiers overlap in recall enough to interpolate a matched-recall margin at all? (If they do not overlap, `interpolate_qps_at_recall` returns `None` and the verdict is `PARITY` with reason "no recall overlap" — a handled negative case, not a crash.)
- Q3 — What query sample size keeps the run tractable while giving a stable recall/QPS? (Plan resolves: default 500 queries; raise-able via `--nq`. The M35+M41 scan is O(ef·M), not O(N), so 500 is tractable — larger than M42's 200 cap.)

## Dependency Graph

```
Phase 1 (pure logic, TDD) ──▶ Phase 2 (driver + structure test) ──▶ Phase 3 (real SIFT1M run + doc)
                                                                          │
                                                                          ▼
                                                              Final Phase: Integration Validation
```

Phase 1 is a hard blocker for Phase 2 (the driver imports the pure logic). Phase 2 blocks Phase 3 (the run uses the driver). Sequential — no parallelism.

---

## Phase 1: Pure post-processing logic (interpolation + margin verdict)

**Objective:** a dependency-free, unit-tested module that turns raw (recall, qps_mean, qps_std) points into a matched-recall margin + honest verdict.

### T1.1 — `interpolate_qps_at_recall` + `pareto_margin_verdict`

#### Objective
Implement the two pure functions that compute the honest margin, fully covered by unit tests with hand-computed oracles.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — introduces `benchmarks/m45_pareto.py` with `interpolate_qps_at_recall(points, target_recall) -> float | None` (linear interpolation of QPS at a target recall on a frontier) and `pareto_margin_verdict(theodb_pts, pgvector_pts, ...) -> dict` (matched-recall margins at shared recall levels + `SUPERIOR/PARITY/INFERIOR` gated on effect>variance).
2. **Why it is necessary now** — this is the one genuinely new piece of logic (ADR D2/D3 blueprint Techniques corner) and the ONLY part that can be unit-tested without a container. TDD it first so the driver (Phase 2) consumes a proven core; a wrong interpolation silently fabricates the claim.

#### Evidence
- Blueprint `m45-rigorous-pareto-claim-blueprint.md` § "Coverage Corner 4 — Techniques" (matched-recall interpolation, effect-vs-variance gate).
- ANN-Benchmarks "QPS@recall=R" convention (blueprint Prior art).

#### Files to edit
```
benchmarks/m45_pareto.py — NEW: interpolate_qps_at_recall + pareto_margin_verdict (pure)
benchmarks/tests/test_run_m45_pareto.py — NEW: RED unit tests with hand-computed oracles (this file also holds Phase 2 structure test)
```

#### Deep file dependency analysis
- `benchmarks/m45_pareto.py` — NEW; stdlib only (`statistics` already used across benchmarks). No import of `theodb_bench` (keeps it container-free and fast). Downstream: imported by `run_m45_pareto.py` (Phase 2).
- `benchmarks/tests/test_run_m45_pareto.py` — NEW; unit tests import `m45_pareto` directly (no DB). Mirrors `test_run_m44_parallel_build.py` structure.

#### Deep Dives
- **`interpolate_qps_at_recall(points, target_recall)`**: `points` = list of `{"recall": r, "qps_mean": q}` (a frontier). Sort by recall. If `target_recall` < min recall or > max recall → return `None` (out of range — handled negative case, Q2). Else find the bracketing pair `(r0,q0),(r1,q1)` and return `q0 + (q1-q0)*(target_recall-r0)/(r1-r0)`. Monotonicity: as recall rises, ef rises, QPS falls — but the function does NOT assume monotonic QPS; it interpolates on whatever bracket contains the target.
- **`pareto_margin_verdict(theodb_pts, pgvector_pts, margin_tol=0.05)`**: pick shared recall levels = the recall values where BOTH frontiers have coverage (within each other's range). For each, `margin = qps_theodb / qps_pgvector`. Verdict: `SUPERIOR` if the min margin across shared levels > 1 AND the QPS gap at each level exceeds combined std (effect>variance); `INFERIOR` if max margin < 1 with the same variance gate; else `PARITY`. Return `{"shared_levels": [...], "margins": [...], "verdict": ..., "reason": ...}`. If no shared recall overlap → `verdict=PARITY, reason="no recall overlap"`.
- **Invariants:** pure (no I/O, no global state); deterministic; empty input → `PARITY` with reason, never a crash.
- **Edge cases:** single-point frontier (cannot interpolate → treat as covering only its exact recall); identical recall on two adjacent points (avoid div-by-zero: if `r1==r0` return `q0`); target exactly on a measured recall (return that point's qps).

#### Pseudo-code / Signatures
```pseudocode
function interpolate_qps_at_recall(points, target_recall) -> float | None
  pts = sort points by recall
  if target < pts[0].recall or target > pts[-1].recall: return None
  for (a, b) in consecutive_pairs(pts):
    if a.recall <= target <= b.recall:
      if b.recall == a.recall: return a.qps_mean
      return a.qps_mean + (b.qps_mean - a.qps_mean) * (target - a.recall) / (b.recall - a.recall)

# Example
input:  points=[{recall:0.94, qps_mean:230}, {recall:0.99, qps_mean:110}], target=0.965
output: 230 + (110-230)*(0.965-0.94)/(0.99-0.94) = 230 - 60 = 170.0
```

#### Tasks
1. Create `benchmarks/m45_pareto.py` with the two functions (stdlib only).
2. Write RED unit tests: interpolation midpoint, exact-recall hit, out-of-range→None, div-by-zero guard, verdict SUPERIOR/PARITY/INFERIOR, no-overlap→PARITY, empty→PARITY.
3. Implement minimal code to green all RED tests.

#### TDD
```
RED: test_interpolate_midpoint_linear() — points (0.94,230),(0.99,110), target 0.965 → 170.0 (±1e-6)
RED: test_interpolate_exact_recall_returns_point_qps() — target == a measured recall → that qps
RED: test_interpolate_out_of_range_returns_none() — target below min / above max → None
RED: test_interpolate_equal_recall_no_div_by_zero() — two points same recall → lower-bound qps, no ZeroDivisionError
RED: test_interpolate_single_point_frontier_only_covers_its_recall() — 1-point frontier returns qps only at its exact recall, None elsewhere (EC-2)
RED: test_verdict_superior_when_margin_gt_1_and_effect_exceeds_variance() — theodb faster at every shared recall, gap > std → SUPERIOR
RED: test_verdict_parity_when_gap_within_variance() — margins ~1 or gap < combined std → PARITY
RED: test_verdict_inferior_when_theodb_slower() — theodb slower at shared recall, gap > std → INFERIOR
RED: test_verdict_parity_when_no_recall_overlap() — disjoint recall ranges → PARITY reason "no recall overlap"
RED: test_verdict_empty_inputs_parity() — [] , [] → PARITY, no crash
GREEN: Implement interpolate_qps_at_recall + pareto_margin_verdict minimally.
REFACTOR: extract a `_shared_recall_levels` helper if the verdict body exceeds ~20 lines. Else none.
VERIFY: cd benchmarks && python3 -m pytest tests/test_run_m45_pareto.py -k "interpolate or verdict" -v
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] All 9 RED unit tests green — `python3 -m pytest benchmarks/tests/test_run_m45_pareto.py -k "interpolate or verdict"` exits 0.
- [ ] `interpolate_qps_at_recall` returns `None` (not a crash) out of range.
- [ ] `pareto_margin_verdict` never raises on empty/disjoint input.
- [ ] Pass: size — `m45_pareto.py` ≤ 120 lines.
- [ ] Pass: lint — `python3 -m pyflakes benchmarks/m45_pareto.py` clean.

#### DoD (Definition of Done)
- [ ] Unit tests green — `python3 -m pytest benchmarks/tests/test_run_m45_pareto.py -k "interpolate or verdict"`.
- [ ] File ≤ 120 lines.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -q m45 CHANGELOG.md` exits 0.

---

## Phase 2: Driver + structure integration test

**Objective:** a driver that reuses the harness pieces, sweeps a shared ef grid on both indexes with mean±std, and a tiny-scale integration test proving its output shape.

### T2.1 — `run_m45_pareto.py` driver

#### Objective
Build both HNSW indexes, sweep `[40,64,100,200,400]` on both with ≥3 timed samples/point → per-point mean±std QPS + recall, assemble the report + margin verdict, write `docs/benchmarks/m45-pareto-sift1m.{md,json}`.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — introduces `run_m45_pareto.py`: loads the dataset (reuse `load_hdf5_full`), for each index builds it and sweeps the ef grid, measuring per-run QPS (mean±std) + recall via `recall_at_k`, then calls `pareto_margin_verdict` and writes the doc.
2. **Why it is necessary now** — the measurement must exist before the real run (Phase 3); building on the proven pure logic (Phase 1) keeps the claim-bearing arithmetic tested. Reusing `theodb_bench.{dataset,db,recall}` (ADR D3) is Rule 9 — no harness rebuild.

#### Evidence
- `benchmarks/run_m44_parallel_build.py:60-88` — the mean±std driver pattern (per-run timing → `statistics.mean/pstdev`).
- `benchmarks/theodb_bench/harness.py:56-108` — the build→sweep→measure loop reused conceptually (query_topk timing, recall_at_k, index isolation via DROP).
- `theodb_rs/src/am/guc.rs:19` — `theodb_hnsw.ef_search` GUC (default 64, max 1000); pgvector `hnsw.ef_search`.

#### Files to edit
```
benchmarks/run_m45_pareto.py — NEW: run(seq_port_unused...)/main(); build both, shared ef sweep, mean±std, write doc
benchmarks/tests/test_run_m45_pareto.py — ADD: integration structure test (tiny n, one container) mirroring test_run_m44
```

#### Deep file dependency analysis
- `run_m45_pareto.py` — NEW; imports `numpy`, `psycopg2` (present), `theodb_bench.dataset.load_hdf5_full`, `theodb_bench.db.VectorDB`, `theodb_bench.recall.recall_at_k`, and `m45_pareto` (Phase 1). Downstream: `main()` + the structure test.
- `test_run_m45_pareto.py` — ADD an integration test gated `@pytest.mark.integration` needing a container (like `test_run_m44`). Tiny n so it runs fast.

#### Deep Dives
- **Measurement loop per (index, ef):** warmup pass (untimed), then `runs` timed passes over the SAME query subset; each pass records mean per-query latency → `qps = 1/mean_latency`; collect `runs` QPS values → `qps_mean = statistics.mean`, `qps_std = statistics.pstdev`. Recall from the last pass's distances via `recall_at_k` vs exact GT.
- **Index isolation:** build theodb_hnsw and pgvector hnsw on the same column; before measuring one, `DROP INDEX` the other so the planner cannot cross-use (harness.py:63 pattern). Assert the intended index is used (`EXPLAIN` contains the index name) before timing.
- **Matched build (D4):** pgvector DDL `WITH (m=16, ef_construction=64)`; theodb build is the fixed Rust constant (assert-documented). `SET max_parallel_maintenance_workers=0` on both.
- **Shared query subset:** both indexes measured on `queries[:nq]` (default 500) at each ef — identical sample (mitigation for the cap-bias risk).
- **Invariants:** both indexes measured on identical corpus + query set + GT; ef grid identical; the doc records nq, runs, seed, image tags, host.
- **Edge cases:** container not ready (retry loop, fail loud); `ef_search` below k rejected by theodb (`ann_query.rs:133` requires ef≥k=10 — grid min is 40, safe); recall 0 (degenerate build) → verdict handles as data, doc flags it.

#### Pseudo-code / Signatures
```pseudocode
function run(port, hdf5, nq=500, runs=3, ef_grid=[40,64,100,200,400], seed=2026) -> report_dict
  corpus, queries, gt_dist = load_hdf5_full(hdf5, nq, seed, k=10, metric="l2")
  db = VectorDB(dsn(port)).connect(); db.ensure_extension(); db.create_table(...); db.load_vectors(...)
  frontier = {}
  for index in ("theodb_hnsw", "pgvector_hnsw"):
     drop other index; build this index (matched params); assert_index_used
     pts = []
     for ef in ef_grid:
        db.set_session(SET <index>.ef_search = ef)
        warmup(queries[:nq])
        run_qps = []
        for _ in range(runs):
           lat, dists = time_queries(queries[:nq])
           run_qps.append(1.0 / mean(lat))
        recall = recall_at_k(gt_dist[:nq], dists, 10)
        pts.append({recall, qps_mean: mean(run_qps), qps_std: pstdev(run_qps), ef, p50})
     frontier[index] = pts
  verdict = pareto_margin_verdict(frontier["theodb_hnsw"], frontier["pgvector_hnsw"])
  return {params, frontier, verdict}
```

#### Tasks
1. Create `run_m45_pareto.py` with `run()` + `main()` (argparse: `--hdf5 --port --nq --runs --seed --ef-grid --write-doc`).
2. Reuse `load_hdf5_full`, `VectorDB`, `recall_at_k`, `m45_pareto.pareto_margin_verdict`.
3. Write the integration structure test (tiny n, one container) asserting the report has both frontiers with `qps_mean/qps_std/recall` per point + a `verdict`.
4. `--write-doc` renders `docs/benchmarks/m45-pareto-sift1m.{md,json}`.

#### TDD
```
RED: test_run_m45_emits_two_frontiers_with_mean_std() [integration] — tiny n on one container: report has frontier["theodb_hnsw"] and ["pgvector_hnsw"], each a list of points with qps_mean, qps_std, recall in [0,1]; verdict in {SUPERIOR,PARITY,INFERIOR}
RED: test_qps_guards_zero_latency() [unit] — a mean-latency of 0 (clock granularity) does not raise ZeroDivisionError; qps clamps via epsilon (EC-3)
GREEN: Implement run()/main().
REFACTOR: extract `_measure_frontier(db, index, ef_grid, queries, gt, runs)` if run() exceeds ~40 lines.
VERIFY: cd benchmarks && PORT=<c> python3 -m pytest tests/test_run_m45_pareto.py -k emits_two_frontiers -v
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded; the driver runs queries sequentially, one psycopg2 connection)
```

#### Acceptance Criteria
- [ ] Integration structure test green against a container — `python3 -m pytest benchmarks/tests/test_run_m45_pareto.py -k emits_two_frontiers` exits 0.
- [ ] Report contains both frontiers with `qps_mean`, `qps_std`, `recall` per operating point + a `verdict`.
- [ ] Both indexes measured on the identical query subset + GT — asserted by the structure test comparing the recorded `nq` value across both frontiers equals the input `nq`.
- [ ] Pass: size — `run_m45_pareto.py` ≤ 200 lines.
- [ ] Pass: lint — `pyflakes` clean.

#### DoD (Definition of Done)
- [ ] Structure test green.
- [ ] File ≤ 200 lines.
- [ ] CHANGELOG `[Unreleased]` updated — `grep -q m45 CHANGELOG.md` exits 0.

---

## Phase 3: Real SIFT1M run + honest verdict doc

**Objective:** run the driver on real SIFT1M for both indexes and record the honest artifact + verdict.

### T3.1 — Produce `docs/benchmarks/m45-pareto-sift1m.{md,json}`

#### Objective
Execute the rigorous run at 1M (or a documented large subsample if 1M×5ef×3runs×2idx is intractable on the dev box), write the mean±std Pareto tables for both indexes, the matched-recall margin, and the honest verdict.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — runs `run_m45_pareto.py --write-doc` on `theo-db:m44` with SIFT1M, capturing mean±std per point for both indexes and the interpolated matched-recall margin.
2. **Why it is necessary now** — the artifact IS the deliverable (the Goal metric: the json exists with the required shape). Without the real run there is no claim, only a harness.

#### Evidence
- `benchmarks/.datasets/sift-128-euclidean.hdf5` (train 1M×128, test 10k×128, neighbors GT).
- `docs/benchmarks/sift1m-carrier-verdict.md` (M42) — the numbers this run makes rigorous.

#### Files to edit
```
docs/benchmarks/m45-pareto-sift1m.md — NEW: mean±std Pareto tables + margin + honest verdict + reproduce command
docs/benchmarks/m45-pareto-sift1m.json — NEW: machine artifact (frontiers + verdict + params)
CHANGELOG.md — [Unreleased] entry (Added: rigorous mean±std Pareto claim)
```

#### Deep file dependency analysis
- The docs are generated by `run_m45_pareto.py --write-doc`; the md is human-curated after (honest verdict prose, caveats). No code depends on the docs.
- CHANGELOG: append under `[Unreleased] § Added`.

#### Deep Dives
- **Scale decision:** attempt full 1M. If the total wall-clock is impractical, use the largest tractable subsample of the SIFT1M train (recorded honestly, e.g. 200k) — still real structured SIFT vectors with valid neighbors-GT via `load_hdf5_subsample` OR the full-train path. The scale used is recorded in the artifact; the M42 caveat about scale is preserved.
- **Verdict prose:** state the matched-recall margin with mean±std bands; if effect > variance → licensed superiority claim wording (with the benchmark link in the same paragraph, per `public-copy.md` §4); if not → honest parity/no-claim. Declare half-2 (independent repro) open.
- **Invariants:** every number carries units + methodology (runs, nq, seed, host, image); no bare "faster than" without the artifact link.

#### Tasks
1. Ensure `theo-db:m44` container up with SIFT1M loaded (or let the driver load it).
2. Run `python3 benchmarks/run_m45_pareto.py --hdf5 ... --nq 500 --runs 3 --write-doc`.
3. Curate `docs/benchmarks/m45-pareto-sift1m.md` (NEW): mean±std tables, margin verdict, honest caveats, reproduce command.
4. CHANGELOG `[Unreleased] § Added`.

#### TDD
```
RED: (data deliverable — no new unit test; covered by Phase 1+2 tests) The artifact's SHAPE is asserted by test_run_m45 (both frontiers + verdict).
GREEN: Run produces the json; md curated.
REFACTOR: None.
VERIFY: test -f docs/benchmarks/m45-pareto-sift1m.json && python3 -c "import json;d=json.load(open('docs/benchmarks/m45-pareto-sift1m.json'));assert d['frontier']['theodb_hnsw'] and d['frontier']['pgvector_hnsw'] and d['verdict']"
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `docs/benchmarks/m45-pareto-sift1m.json` exists with both frontiers (mean±std per point) + verdict.
- [ ] `docs/benchmarks/m45-pareto-sift1m.md` states the matched-recall margin with variance + honest verdict + reproduce command + half-2-open caveat.
- [ ] No performance claim in the md without the artifact link in the same paragraph (`public-copy.md` §4).
- [ ] CHANGELOG updated.

#### DoD (Definition of Done)
- [ ] Artifact json + md present and consistent.
- [ ] Verdict is honest (superiority OR parity — matches the data).
- [ ] CHANGELOG `[Unreleased]` updated — `grep -q m45 CHANGELOG.md` exits 0.

---

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Matched-recall margin arithmetic (interpolation) proven | T1.1 | Pure `interpolate_qps_at_recall` + 9 unit tests |
| 2 | Honest effect>variance verdict | T1.1 | `pareto_margin_verdict` + verdict unit tests |
| 3 | mean±std per operating point (not best-of-N) | T2.1 | driver measures `runs` timed passes → mean/pstdev |
| 4 | Shared ef grid on BOTH indexes | T2.1 | ef `[40,64,100,200,400]` swept on theodb + pgvector |
| 5 | index-AM path, matched build params | T2.1 (D1,D4) | `USING theodb_hnsw`/`USING hnsw`, m=16/efc=64, single-thread build |
| 6 | Reproducible SIFT1M artifact + honest verdict | T3.1 | `docs/benchmarks/m45-pareto-sift1m.{md,json}` |
| 7 | Larger query sample than M42's 200 | T2.1/T3.1 | default nq=500 |
| 8 | `public-copy.md` §4 half-1 (artifact) + half-2 declared open | T3.1 | doc records reproduce cmd; half-2 caveat explicit |

**Coverage: 8/8 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `cd benchmarks && python3 -m pytest tests/test_run_m45_pareto.py -v` green (unit; integration gated by container)
- [ ] Zero lint warnings — `pyflakes benchmarks/m45_pareto.py benchmarks/run_m45_pareto.py`
- [ ] File-size budget respected (`m45_pareto.py` ≤120, `run_m45_pareto.py` ≤200)
- [ ] CHANGELOG.md updated under `[Unreleased]` (Rule 6)
- [ ] Backward compatibility — zero changes to `theodb_bench/*` or product code (import-only reuse)
- [ ] **Data-deliverable proof** — `docs/benchmarks/m45-pareto-sift1m.json` exists, contains both frontiers with mean±std per point + a verdict; the md's claim (if any) is honest and links the artifact
- [ ] Plan archived after `/review` READY_TO_MERGE + PR merged

## Failure scenarios (when I/O external)

The driver touches the PostgreSQL container (psycopg2 — external I/O).

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `theo-db:m44` container (psycopg2) | DB not ready (initdb restart timing) | start container, connect immediately | driver's wait-for-ready loop retries N times; fails LOUD with a clear error if never ready (no silent empty result) |
| `theo-db:m44` (build) | index build fails / OOM at scale | run at a scale exceeding RAM | exception propagates (fail-fast); driver does NOT write a partial/degenerate artifact |
| `theo-db:m44` (query) | connection drop mid-sweep | (documented; not force-injected — single local container) | psycopg2 raises; the run aborts with the error surfaced, no half-written json |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** validate the harness end-to-end against a real container and confirm the artifact shape.

### Execution
```
cd benchmarks && python3 -m pytest tests/test_run_m45_pareto.py -v      # unit (Phase 1) + integration structure (Phase 2)
pyflakes benchmarks/m45_pareto.py benchmarks/run_m45_pareto.py           # zero warnings
# Real run (Phase 3):
python3 benchmarks/run_m45_pareto.py --hdf5 benchmarks/.datasets/sift-128-euclidean.hdf5 --nq 500 --runs 3 --write-doc
test -f docs/benchmarks/m45-pareto-sift1m.json
```

### Acceptance Criteria
- [ ] Unit tests green (Phase 1); integration structure test green against a container (Phase 2) — `python3 -m pytest benchmarks/tests/test_run_m45_pareto.py -v` exits 0.
- [ ] Zero lint warnings — `pyflakes benchmarks/m45_pareto.py benchmarks/run_m45_pareto.py` prints nothing.
- [ ] `docs/benchmarks/m45-pareto-sift1m.json` present with both frontiers (mean±std) + verdict
- [ ] The md verdict matches the json data (honest — superiority OR parity) — `python3 -c "import json; d=json.load(open('docs/benchmarks/m45-pareto-sift1m.json')); print(d['verdict'])"` value appears verbatim in the md.
- [ ] Failure scenarios: wait-for-ready retry exercised (container timing) — no silent empty artifact; verified by the run aborting with a raised error (non-zero exit) when the container is unreachable, never writing a partial json.

### If Validation Fails
1. Separate plan-caused failures from pre-existing.
2. Fix all plan-caused failures.
3. Re-run the chain.
4. Log pre-existing issues in the PR description.
