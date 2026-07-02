---
slug: m33-scann-headtohead
milestone_id: M33
created_at: 2026-07-02
goal: Measure theodb_ivfflat vs ScaNN OSS (the AlloyDB vector-index algorithm) on SIFT1M and emit an honest per-dimension SUPERIOR/PARITY/GAP verdict, measured by a committed docs/benchmarks/m33-scann-headtohead.{md,json} artifact + the North Star public-copy status updated honestly.
---

# M33 — head-to-head vs AlloyDB/ScaNN (the North Star superiority claim)

## Goal

Produce the North Star pillar's honest measured answer: benchmark `theodb_ivfflat` (and pgvector ivfflat, reused
from M34) vs **ScaNN OSS** (the open-source algorithm behind AlloyDB's vector index — AlloyDB itself is
GCP-managed, not locally runnable) on SIFT1M (1M×128), and emit a per-dimension **SUPERIOR / PARITY / GAP** verdict
(recall@10, QPS, latency, memory) with the mandatory library-vs-database caveat — **measured by** a committed
`docs/benchmarks/m33-scann-headtohead.{md,json}` and the `public-copy.md` North Star status updated to reflect the
measured result (a qualified benchmark-linked statement, or an honest marked GAP).

## Context

M32 measured theodb ≥1M scale; M34 made theodb_ivfflat competitive with pgvector (p50 ≤ pgvector at 1M). M33 is the
capstone: vs the SOTA target. Discovery
(`.claude/knowledge-base/discoveries/blueprints/m33-scann-headtohead-blueprint.md`) confirmed EMPIRICALLY that
AlloyDB access is blocked (GCP-managed) so the DoD-sanctioned fallback is ScaNN OSS (`pip install scann`, verified
1.4.2 builds + searches on this AVX2 host), and that theodb/pgvector numbers are reusable from the committed M34
SIFT1M artifact (same dataset, hardware, neighbors-GT).

## Baseline Context

### Files that will be touched

| File | LoC | git sha (last) | Why |
|---|---|---|---|
| `benchmarks/run_m33_scann.py` | (NEW) | — | build ScaNN on SIFT1M full-train + sweep leaves-to-search; measure recall@10/QPS/p50-p95-p99/build-time/peak-RSS; consolidate with the M34 theodb+pgvector rows |
| `benchmarks/tests/test_m33_scann.py` | (NEW) | — | CI-safe: ScaNN recall helper matches `theodb_bench.recall_at_k` semantics on tiny data (both sides scored identically); skips full run |
| `benchmarks/requirements.txt` | (exists) | — | add `scann` (dev-only benchmark dep) |
| `docs/benchmarks/m33-scann-headtohead.{md,json}` | (NEW) | — | the head-to-head artifact + per-dimension verdict |
| `README.md` (North Star line) | (exists) | — | update the vector-superiority status honestly per the measured result (public-copy gate) |

### Current callers / dependents

- `theodb_bench.recall.recall_at_k` / `neighbors_ground_truth` (`benchmarks/theodb_bench/recall.py:61,84`) — reused so ScaNN + theodb are scored with IDENTICAL distance-thresholded (ANN-Benchmarks) recall semantics.
- `theodb_bench.dataset.load_hdf5_full` (`dataset.py`) — reused to load SIFT1M full train + neighbors-GT for the ScaNN measurement.
- `docs/benchmarks/m34-ivfflat-reloption.json` — the theodb_ivfflat + pgvector ivfflat frontier (n=1M) reused as the two non-ScaNN columns.

### Domain glossary

- **ScaNN** — Google Research's OSS ANN library (Apache-2.0; Guo et al. ICML 2020, arXiv:1908.10396): anisotropic vector quantization + SOAR partitioning + AVX asymmetric-hashing distance. The algorithm AlloyDB's vector index is built on.
- **AlloyDB** — Google Cloud managed PostgreSQL with a proprietary `alloydb_scann` index; GCP-only (no local/CI run).
- **num_leaves_to_search** — ScaNN's recall/speed knob (analogous to ivfflat `probes`).
- **library-vs-database caveat** — ScaNN is an in-memory ANN library (no persistence/txn/SQL); theodb is a transactional PG index. The verdict states both axes.

### Architecture boundaries affected

None in the engine. All new code is a dev-only benchmark script + test in `benchmarks/`. No Rust change. DIP: the ScaNN measurement reuses `theodb_bench.recall`/`dataset` (the same scoring path).

## Prior Art & Related Work

- Blueprint (this cycle): `.claude/knowledge-base/discoveries/blueprints/m33-scann-headtohead-blueprint.md`.
- In-repo reuse: `benchmarks/theodb_bench/{recall,dataset}.py`; the M34 artifact `docs/benchmarks/m34-ivfflat-reloption.json`; the M32 harness lineage (`run_m32_sift1m.py`).
- External (web, cited in the artifact not as a `references/` path): ScaNN paper arXiv:1908.10396; ann-benchmarks.com ScaNN SIFT results; AlloyDB ScaNN docs — used for methodology framing, not as fabricated in-repo citations.

## ADRs

### ADR-1 — ScaNN OSS is the AlloyDB proxy (access blocked)
**Decision:** benchmark vs ScaNN OSS (`pip install scann`), not a live AlloyDB.
**Rationale:** AlloyDB is GCP-managed — no local/CI run; the M33 DoD explicitly sanctions ScaNN standalone as the
fallback, and ScaNN IS the algorithm behind AlloyDB's index. Every prior milestone used the local-container
methodology; provisioning GCP AlloyDB is non-reproducible + out of that methodology.
**Alternatives rejected:** (a) provision AlloyDB on GCP (cost, credentials, non-reproducible, CI-hostile);
(b) skip the comparison + keep the claim `UNBENCHMARKED` (defeats the milestone's purpose).

### ADR-2 — reuse the M34 theodb/pgvector numbers (do NOT re-run)
**Decision:** read theodb_ivfflat + pgvector ivfflat rows from `docs/benchmarks/m34-ivfflat-reloption.json`.
**Rationale:** measured on the SAME SIFT1M, hardware, neighbors-GT; re-running (theodb lists=1000 build ~10 min
single-thread) only adds noise. M33 adds the ScaNN column + the consolidated verdict.
**Alternatives rejected:** a fresh 3-way run (no value; the M34 numbers are committed + reproducible).

### ADR-3 — an honest GAP is a valid, DoD-sanctioned outcome
**Decision:** if theodb shows a GAP vs ScaNN's raw algorithm, report it with the number + the library-vs-database
caveat; do NOT tune theodb to "win".
**Rationale:** Rule 3 + the DoD ("sustém OU refuta honestamente"). ScaNN is a specialized quantization-heavy library;
theodb_ivfflat is a straightforward full-precision IVFFlat inside a transactional DB — a GAP on raw QPS is a likely,
honest result and a complete milestone. Tuning-to-win would be cherry-picking / M35+ engine scope.
**Alternatives rejected:** quantization in theodb now (that is a future engine milestone, not a measurement).

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `numpy` | `2.2.6` | Python | ScaNN + recall math; verified compatible with theodb_bench (29 harness tests green) |
| `h5py` | `3.16.0` | Python | SIFT1M HDF5 load; dev-only |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale (libs evaluated) | Why this one |
|---|---|---|---|---|
| `scann` (NEW) | `1.4.2` | Python | Evaluated: FAISS (Meta — different algorithm, not the AlloyDB one); building our own ScaNN (PhD-level, months, defeats the "measure the SOTA" purpose). ScaNN OSS IS the AlloyDB algorithm — the correct baseline. | dev-only benchmark dep; Apache-2.0; the exact SOTA the milestone measures against |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency graph

```
Phase 0 (deps-audit scann) ──▶ Phase 1 (ScaNN SIFT1M measurement script + CI test) ──▶ Phase 2 (consolidated head-to-head report + verdict + public-copy status)
```

## Phase 1 — ScaNN SIFT1M measurement

### T1.1 — build + measure ScaNN on SIFT1M

#### Why this step
The milestone's evidence: ScaNN's real recall@10 / QPS / latency / memory on the SAME SIFT1M the M34 theodb numbers
used. Reusing `theodb_bench.recall`/`dataset` guarantees identical scoring (distance-thresholded recall vs the same
neighbors-GT), so the head-to-head is fair.

#### Files to edit
- `benchmarks/run_m33_scann.py` (NEW), `benchmarks/tests/test_m33_scann.py` (NEW), `benchmarks/requirements.txt`

#### TDD
- RED: `test_scann_recall_matches_theodb_recall_semantics` — on a tiny synthetic dataset, compute ScaNN neighbors +
  score them with the SAME `theodb_bench.recall.recall_at_k` (distance-thresholded) used for theodb; assert the
  helper produces a value in [0,1] and that an exact (brute-force-score) ScaNN config yields recall ≈ 1.0. Given a
  tiny corpus + queries, when ScaNN searches with brute-force scoring, then recall_at_k ≈ 1.0 (the scoring path is
  identical to theodb's). (No 1M run in CI — that is the operator artifact, ADR-4-style.)
- GREEN: `run_m33_scann.py` — load SIFT1M full train + neighbors-GT via `load_hdf5_full`; build ScaNN
  (`.tree(num_leaves≈√n).score_ah(2).reorder(...)`); sweep `num_leaves_to_search`; measure recall@10 (via
  `recall_at_k` against neighbors-GT distances), QPS (best-of-N), p50/p95/p99, build-time, peak RSS.
- REFACTOR: share the query/timing loop shape with the theodb_bench metrics (`latency_percentiles`, `qps_best_of_n`)
  — no duplicated percentile math.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- `scann` import fails / no AVX2 → the script exits with a clear message; the CI test `pytest.skip`s if scann absent.
- SIFT1M HDF5 absent → clear skip/refuse (same pattern as `run_m32_sift1m.py`).

#### Acceptance criteria
- `python3 -m pytest benchmarks/tests/test_m33_scann.py -q` green (or skips cleanly if scann absent).
- `run_m33_scann.py` produces ScaNN recall@10 within [0,1], QPS>0, p50>0, peak-RSS>0 across the sweep at n=1M.

#### DoD
- `scann` in `benchmarks/requirements.txt`; the ScaNN recall is scored by the SAME `recall_at_k` as theodb (grep: one recall function).
- ruff clean; no fabricated numbers.

## Phase 2 — consolidated head-to-head + honest verdict

### T2.1 — the artifact + per-dimension verdict + public-copy status

#### Why this step
The DoD's core: a per-dimension SUPERIOR/PARITY/GAP verdict with numbers + the mandatory caveat, and the North Star
status updated honestly (public-copy gate — no claim without a linked benchmark).

#### Files to edit
- `docs/benchmarks/m33-scann-headtohead.{md,json}` (NEW), `README.md` (North Star line), `CHANGELOG.md`

#### TDD
(none — documentation/verdict synthesis over the measured artifact + the reused M34 numbers)

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- ScaNN faster than theodb on QPS at matched recall → report the GAP honestly with the number + the
  library-vs-database caveat (ADR-3); NEVER spin it as parity.

#### Acceptance criteria
- `docs/benchmarks/m33-scann-headtohead.json` has, at matched recall points, ScaNN vs theodb_ivfflat vs pgvector
  numbers for recall@10 / QPS / p50 / memory, plus a `verdict` per dimension ∈ {SUPERIOR, PARITY, GAP}.
- `docs/benchmarks/m33-scann-headtohead.md` carries the library-vs-database caveat + hardware + reproduction command
  + the ScaNN version + arXiv reference.
- `README.md` North Star line reflects the measured result (a qualified benchmark-linked statement OR a marked
  `meta`/gap) — no unqualified superiority claim without the benchmark link (`public-copy.md`).

#### DoD
- `CHANGELOG.md [Unreleased]` has the M33 entry linking the artifact.
- `grep` shows the North Star/vector claim in README is either benchmark-linked or explicitly marked aspirational.

## Coverage Matrix

| Goal / DoD item | Task(s) |
|---|---|
| Benchmark vs ScaNN OSS on SIFT1M same dataset/hardware, methodology documented (caveats) | T1.1, T2.1 |
| Per-dimension SUPERIOR/PARITY/GAP verdict (recall@10, QPS, latency, memory) with numbers; docs/benchmarks + .json | T2.1 |
| public-copy gate: vector claim permitted only with benchmark link, else marked meta/gap | T2.1 |
| Identical recall scoring for both sides (fairness) | T1.1 (reuse recall_at_k) |
| No new SHIPPED dependency (scann is dev-only, deps-audit) | Dependencies + Phase 0 |
| CHANGELOG (Rule 6) | T2.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| ScaNN's raw algorithm beats theodb → North Star "superior" refuted | MEDIUM | ADR-3: an honest GAP is a valid outcome; the caveat (library vs transactional DB) frames it; the milestone delivers the measured answer, not a win | paulohenriquevn |
| ScaNN vs theodb is apples-to-oranges (lib vs DB) → misleading | HIGH | mandatory library-vs-database caveat in the verdict (analysis-golden-rule); report BOTH axes; never claim DB-parity from a lib number | paulohenriquevn |
| AlloyDB's managed ScaNN differs from OSS ScaNN → proxy imperfect | MEDIUM | disclose: OSS ScaNN is the algorithm, AlloyDB's is tuned/quantized differently; the artifact says "ScaNN OSS as the AlloyDB proxy" honestly | paulohenriquevn |
| numpy 2.x (scann) breaks theodb_bench | LOW | verified compatible (29 harness tests green under numpy 2.2.6) | paulohenriquevn |

## Unresolved Questions

- Does ScaNN's default `score_ah` (asymmetric hashing / quantized) config or `score_brute_force` best represent the AlloyDB operating point? — resolved in T1.1 by sweeping + reporting the quantized (AH) config (AlloyDB's ScaNN IS quantized) as the primary, with brute-force-scored as the recall ceiling.
- Should memory be RSS or index-structure bytes? — report ScaNN peak-RSS (in-memory) vs theodb index bytes, explicitly labeled as different measures (the caveat covers it).

## Failure scenarios

- **scann absent / no AVX2** → CI test skips; operator run refuses with a clear message. (T1.1)
- **theodb GAP vs ScaNN** → honest GAP verdict + caveat, no spin. (T2.1, ADR-3)
- **AlloyDB proxy imperfect** → disclosed (OSS ScaNN ≠ managed AlloyDB ScaNN). (T2.1)

## Final Phase — Integration Validation

- `pytest benchmarks/tests/test_m33_scann.py` green (or clean skip).
- `run_m33_scann.py` produces the ScaNN SIFT1M numbers (recall/QPS/p50/RSS) at n=1M.
- `docs/benchmarks/m33-scann-headtohead.{md,json}` committed: per-dimension verdict + mandatory caveat + hardware +
  repro + ScaNN version + arXiv ref.
- README North Star line honest (benchmark-linked or marked); CHANGELOG updated.
