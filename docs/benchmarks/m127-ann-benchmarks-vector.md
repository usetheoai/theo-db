# M127 — Official benchmark VECTOR pilot: ann-benchmarks BaseANN adapter + wrap layer (measured)

**Date:** 2026-07-20 · **Box:** DO droplet (theo-e2e-runner), pgrx-managed PG17.10, `theodb_rs` · **Dataset:** GloVe
(ann-benchmarks `glove-100-angular`, **PDDL / D1-safe**). Implements ADR-0050 (adopt-and-wrap) for the vector pillar.

**Verdict:** TheoDB's ann-benchmarks-shaped `BaseANN` adapter drives a real **recall@10 × QPS Pareto** through the
`theodb_hnsw` index, and the retained adopt-and-wrap layer (byte-identical regression + paired significance) — which
ann-benchmarks itself lacks (blueprint Q11) — works end-to-end.

## Measured recall@10 × QPS Pareto (single-thread, ann-benchmarks protocol)

GloVe-100-angular, **subsampled to n_corpus = 5,000 / n_queries = 200**, exact brute-force ground-truth (cosine),
build once (39.7 s), sweep `theodb_hnsw.ef_search`:

| ef_search | recall@10 | QPS (single-thread) |
|---:|---:|---:|
| 10 | 0.7155 | 202.1 |
| 40 | 0.8365 | 186.8 |
| 100 | 0.9315 | 99.1 |
| 200 | **1.0000** | 31.3 |
| 400 | 1.0000 | 32.7 |

Recall rises monotonically with `ef_search` and reaches **1.0 at ef≥200** (the sanity gate: HNSW converges to exact
NN as ef grows), trading QPS for recall — the canonical ann-benchmarks recall×QPS frontier. This is produced by
driving the `BaseANN` contract (`fit`/`set_query_arguments`/`query`), the exact interface a public ann-benchmarks
entry uses (`benchmarks/theodb_bench/ann_adapter.py`).

## Wrap layer — the capabilities the official tools lack (blueprint Q11)

Run on the SAME index at ef=400, twice:

- **Byte-identical regression A/B:** `identical=True, n=200, diverged=0` — re-querying the same index returns
  byte-identical rankings (determinism / regression detection working). ann-benchmarks/VectorDBBench ship no such
  check; this is the retained TheoDB capability (`benchmarks/theodb_bench/regression.py`).
- **Paired significance (run1 vs run2):** `mean_diff=0.0, p_permutation=1.0` → **deterministic (not significant)** —
  the M123 permutation test (`benchmarks/theodb_bench/significance.py`) wired over the per-query recall, confirming
  the two passes are statistically identical. The official tools ship no significance test.

Together these two modules ARE the reusable adopt-and-wrap layer that M128–M130 reuse (ADR-0050): the official
runner gives external comparability; our layer gives the significance + regression + correctness the runner lacks.

## Honest scope (ADR M127-2)

- **Self-hosted box, NOT the canonical AWS `c6a.4xlarge`** — the QPS numbers are not leaderboard-comparable; the
  public ann-benchmarks leaderboard PR (which normalizes on `c6a.4xlarge`) is a tracked operational follow-up.
- **Subsampled to n=5,000** (the shared droplet's load made a full-corpus run flaky) — the full 1.18M GloVe canonical
  run is the operational follow-up. The pilot's purpose is to prove the adapter + wrap pattern end-to-end on **real**
  GloVe data, which it does; the scale is honestly labeled, not inflated.
- **GloVe is D1-safe (PDDL)**; SIFT/GIST (TEXMEX) stays CI-download-only until its license is verified (blueprint
  MUST-VERIFY). Any ScaNN/AlloyDB QPS-gap magnitude cites `docs/benchmarks/m73-headtohead-verdict.md`, not this run.

## Reproduction

```
# self-hosted PG17 with theodb_rs; benchmarks deps (numpy, h5py, psycopg2)
PYTHONPATH=benchmarks python3 benchmarks/run_m127_ann_vector.py \
  --n 5000 --n-queries 200 --ef 10 40 100 200 400 \
  --hdf5 benchmarks/.cache/glove-100-angular.hdf5 --out docs/benchmarks/m127-ann-benchmarks-vector.json
```
GloVe auto-downloads from `https://ann-benchmarks.com/glove-100-angular.hdf5` (User-Agent required). No dataset →
status `UNBENCHMARKED` (no fabricated numbers). 9 unit tests cover the adapter contract + the regression module.
