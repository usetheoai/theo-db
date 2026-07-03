# Blueprint — Rigorous mean±std Pareto claim: theodb_hnsw vs pgvector hnsw on SIFT1M

**Slug:** m45-rigorous-pareto-claim
**Date:** 2026-07-03
**Type:** Discovery blueprint (internal prior-art consolidation — the "prior art" is OUR own M32/M40/M42 harness, not a third-party project).
**Verdict target:** SHIPPABLE_WITH_CAVEATS — converts the M42 single-run superiority *signal* into a rigorous, reproducible *claim* per `public-copy.md` §4.

## Context — why now

The M41→M44 arc made `theodb_hnsw` competitive on build + scan + recall×QPS. The strongest superiority
evidence is M42 (`docs/benchmarks/sift1m-carrier-verdict.md`): on real SIFT1M, theodb_hnsw beats pgvector
hnsw ~1.7–2.8× at matched recall. **But that number is not yet a defensible public claim** — the M42 doc's
own honest caveats state it:

- theodb_hnsw Pareto sweep was **single-run** (no mean±std).
- pgvector had only **2 operating points** (ef=40, ef=100) from the 4-way, not a full matched ef-sweep.
- The ~1.7–2.8× multiplier "should be confirmed with mean±std before a hard public claim".
- Query sample was capped at 200.

`public-copy.md` §4 requires, for a comparative performance claim: **(1)** a reproducible benchmark artifact
under `docs/benchmarks/`, **(2)** independent third-party reproduction, **(3)** the benchmark linked in the
same paragraph. This milestone delivers **(1)** rigorously (the achievable half) and produces the artifact
**ready for (2)**; (2) itself is out of our reach (needs a third party) and is honestly declared open.

## The rigor gap (what "rigorous" adds over M42)

| Axis | M42 (signal) | This milestone (claim) |
|---|---|---|
| QPS statistic | best-of-N / single-run | **mean ± std over ≥ 3 timed runs** per operating point |
| ef grid | theodb full sweep; pgvector 2 points | **shared ef grid on BOTH** (e.g. 40/64/100/200/400) |
| Margin | eyeballed "~1.7–2.8×" | **interpolated QPS at matched recall**, effect vs variance |
| Build params | matched (implicit) | **explicitly matched + asserted** (m=16, ef_construction=64) |
| Query sample | 200 (cap) | raised (scan is O(ef·M) since M35+M41) |
| Verdict | "competitive-to-superior" | honest number OR `UNBENCHMARKED`/no-claim if effect ≤ variance |

## Coverage Corner 1 — Integration tests (how we validate the boundary)

The measurement path is the **index-AM** path (`CREATE INDEX … USING theodb_hnsw` vs `USING hnsw`) — the
real planner query path, same as M32/M42, NOT the `theodb.hnsw_knn` SQL-function path of `bench_ann_index.py`.
Validation: the pure post-processing logic (interpolation, verdict) gets **unit tests** with hand-computed
oracles; the harness structure gets an **integration test** (tiny scale, two containers) mirroring
`benchmarks/tests/test_run_m44_parallel_build.py`; the real SIFT1M run is the D-deliverable.

## Coverage Corner 2 — Dependencies (reuse — parsimony rung 4)

Zero new dependency. Reused, already-installed:
- `theodb_bench.dataset.load_hdf5_full` — full 1M train + **exact GT** from the HDF5 `neighbors` (neighbors-GT, no 10¹⁰ brute force). `benchmarks/theodb_bench/dataset.py:56`.
- `theodb_bench.db.VectorDB` — `create_table/load_vectors/build_index/set_session/query_topk/assert_index_used/index_size_bytes`. `benchmarks/theodb_bench/db.py`.
- `theodb_bench.recall.recall_at_k` — recall vs GT distances (ANN-Benchmarks distance-thresholded). `benchmarks/theodb_bench/recall.py:117`.
- `statistics.mean` / `statistics.pstdev` — mean±std (stdlib; `metrics.py` only has `qps_best_of_n`, which is the very thing we must NOT use here).

## Coverage Corner 3 — Tools

`numpy` + `psycopg2` (present), the two prebuilt images `theo-db:m44` (the current carrier) for theodb_hnsw
and pgvector hnsw (same image ships pgvector). SIFT1M at `benchmarks/.datasets/sift-128-euclidean.hdf5`
(train 1M×128, test 10k×128, neighbors GT 10k×100). Same-image, same-machine, same session GUC discipline
(`max_parallel_maintenance_workers=0` on the build to keep build-time fair — though build-time is not the
claim here; recall×QPS is).

## Coverage Corner 4 — Techniques (SOTA anchoring — `discover-phd-rigor.md` R1)

- **ANN-Benchmarks methodology (SOTA standard):** the recall×QPS Pareto frontier is THE field-standard axis
  for ANN comparison (Aumüller et al., ann-benchmarks.com). A single operating point is meaningless; the
  curve is the comparison. Both indexes are swept over a **shared ef grid** and the frontier compared.
- **pgvector hnsw = the SOTA permissive baseline** — the most-adopted Apache/PostgreSQL-license ANN index,
  the honest yardstick for an OSS-permissive competitor (AlloyDB's ScaNN is not permissively runnable here;
  pgvector is the reachable SOTA — `CLAUDE.md` TheoDB rule 1, North Star ADR 0002).
- **Matched-recall interpolation** — because two indexes rarely land on the exact same recall, the honest
  margin is QPS(theodb) / QPS(pgvector) *at a common recall level*, obtained by **linear interpolation on the
  Pareto frontier** (the ann-benchmarks "QPS at recall=R" convention). This is the one genuinely new piece of
  logic and is unit-tested against hand-computed oracles.
- **Effect vs variance gate (PRD D3, anti-sunk-cost):** a superiority claim is licensed ONLY when the QPS
  margin at matched recall exceeds the combined std bands. If it does not, the honest output is "parity /
  no-claim" (a negative is a valid, publishable result — the measurement-first discipline of M36/M38/M39/M40).

## ADR D1 — index-AM path, not the SQL-function path

**Decision:** measure `CREATE INDEX … USING theodb_hnsw` vs `USING hnsw`.
**Alternatives rejected:** (a) `theodb.hnsw_knn` SQL function (`bench_ann_index.py`) — a different, non-planner
path; not what a user runs and not the M42 comparison. (b) In-memory `ann/hnsw.rs` micro-bench — bypasses the
page-native layout + scan kernel that ARE the product. The AM path is the only one that measures the shipped
carrier end-to-end.

## ADR D2 — mean±std over best-of-N

**Decision:** report per-operating-point QPS as mean ± std over ≥ 3 timed runs; keep the best-of-N only as a
secondary column for continuity with M32.
**Alternatives rejected:** best-of-N alone (M42's flaw — hides variance, the exact thing `public-copy.md` §4
and the M42 caveat call out). Median (less standard in ANN-Benchmarks than mean±std for the QPS axis).

## ADR D3 — new self-contained driver, do NOT modify the shared harness

**Decision:** add `benchmarks/run_m45_pareto.py` + a small pure module for the interpolation/verdict; reuse
`theodb_bench.{dataset,db,recall}` by import. Do NOT change `theodb_bench/harness.py` (it reports best-of-N and
is consumed by many `run_m*.py` — changing it risks regressions in shipped artifacts).
**Alternatives rejected:** extend `harness.py` to emit per-run distributions (blast radius across M32–M44
drivers; YAGNI for them). A thin driver isolates the rigor to this milestone.

## Honest caveats (declared up-front, per Rule 3)

- **Single machine, no independent reproduction** — this delivers `public-copy.md` §4 half (1), not (2). The
  artifact is *built for* independent repro (fixed seed, pinned images, exact command) but (2) is out of scope.
- **Local dev CPU, warm cache** — the *direction* + *matched-recall margin with variance* is the deliverable;
  absolute QPS is machine-specific and labeled as such.
- If the rigorous margin comes out **smaller** than M42's single-run 2.8× (as M41's rigor shrank 2.4-3.0× →
  1.2-1.5×), that is the honest number and it is what we publish — no cherry-picking.

## Prior art (internal)

- `docs/benchmarks/sift1m-carrier-verdict.md` (M42) — the signal this makes rigorous.
- `docs/benchmarks/m32-scale-sift1m.md` + `benchmarks/run_m32_sift1m.py` — the 4-way harness + AM path.
- `benchmarks/run_m44_parallel_build.py` + its structure test — the driver + mean±std pattern to mirror.
- `.claude/rules/discover-phd-rigor.md` — R1 SOTA-anchoring, R3 benchmark-evidence, applied here (P2 pillar).
- `.claude/rules/public-copy.md` §4 — the comparative-claim contract this milestone satisfies (half 1).
