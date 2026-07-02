# M31b — SIMD vector distance (AVX2+FMA): profile + latency

**Milestone:** M31b · **Date:** 2026-07-01 · **Plan:** `.claude/knowledge-base/plans/m31b-simd-distance-plan.md`

## Phase 0 — profile-before-optimize (the flamegraph/criterion tip)

Standalone micro-bench of the scan hot-loop compute (N=65 000 candidates, dim=128), portable build (SSE2 baseline,
matching the extension — no `target-cpu=native`), per query, mean over 60 iters:

| Component | Cost | Share |
|---|---:|---:|
| **decode** (page bytes → f32 scratch, `from_le_bytes`) | 1.96 ms | **45%** |
| **distance** (scalar l2) | 2.34 ms | **55%** |
| total hot-loop compute | 4.30 ms | 100% |

**Decisive finding:** the distance is only 55% of the compute — **AVX2 on the distance alone would NOT reach
≤ pgvector** (halving 55% ≈ 26% total win). The profile (thanks to the flamegraph tip — measurement-first applied
to the optimization) prevented mis-targeting the SIMD effort.

**Design (informed by the profile):** the AVX2 distance reads f32 DIRECTLY from the entry's page bytes via
`_mm256_loadu_ps` (unaligned load), **fusing decode + distance** into one SIMD pass — eliminating BOTH the 45%
decode and the 55% scalar distance (no scratch buffer). Repro: `benchmarks/micro/simd_hotloop_bench.rs`
(`rustc -O --edition 2021 simd_hotloop_bench.rs && ./simd_hotloop_bench`) — committed, portable (no PGRX_HOME).

## Phase 1 — the fused SIMD (implementation validated)

Standalone micro-bench (`benchmarks/micro/simd_hotloop_bench.rs`, portable SSE2 build): parity vs scalar oracle
across dims 1..129 (within eps — recall-preserving, not bit-identical, as designed); the fused AVX2+FMA hot-loop is
**1.62× faster** than scalar decode+distance at N=65k. AVX2+FMA present on the build host. The extension `#[pg_test]`
parity tests (`vec.rs`) lock the contract; the observable gate is the Python benchmark below.

## ⚠ Data-integrity finding (why every pre-M31b latency number was invalid)

The profiler (`THEODB_SCAN_PROFILE=1`, added in M31b) reported `cand=100000 nonempty_lists=1/100` — the scan was
scoring ALL 100k rows because the k-means put every vector into ONE list. Root cause is NOT theodb: the benchmark's
seed SQL `INSERT ... (SELECT string_agg((random())::text, ',') FROM generate_series(1,128))` is a **non-correlated
subquery** that PostgreSQL evaluates ONCE as an InitPlan → **all 100k vectors were IDENTICAL** (`COUNT(DISTINCT)=1`,
verified). Identical points collapse any correct k-means to one list; recall was trivially 10/10 (all ties),
masking it. So M31's recorded `~38 ms` / `~2.7× behind pgvector` measured brute-force-on-identical-ties, not ANN.
Fix: seed DISTINCT vectors from Python via `COPY` (seed=42). See `docs/adr/0012-benchmark-data-degeneracy.md`.

## Latency + recall — n = 100 000, dim = 128, probes = 10, lists = 100 (warm, DISTINCT data, seed=42)

The profiler on distinct data confirms the fix: `cand≈10039 nonempty_lists=100/100 probed_top=[1053,1037,…]` —
balanced lists, O(probes) reads (~10% of N), exactly like pgvector.

### Regime A — UNIFORM random (IVFFlat worst case: no cluster structure)

| Index | recall@10 (mean/30q) | p50 | ratio |
|---|---:|---:|---|
| **theodb_ivfflat (M31b)** | 3.9/10 | **1.71 ms** | **0.38× (2.6× faster)** |
| pgvector ivfflat | 3.8/10 | 4.53 ms | 1× |

Recall is low for BOTH (uniform random has no clusters to prune on) — theodb is at **recall parity** and
**2.6× faster** (SIMD-fused scan + tight loop vs pgvector's AVX C).

### Regime B — CLUSTERED gaussian (200 centers — realistic embedding-like operating point)

| Index | recall@10 (mean/30q) | p50 | ratio |
|---|---:|---:|---|
| **theodb_ivfflat (M31b)** | **10.0/10** | **5.29 ms** | **0.95× (≤ pgvector)** |
| pgvector ivfflat | 10.0/10 | 5.58 ms | 1× |

At the realistic point both reach full recall; theodb p50 **≤ pgvector**. **M31b DoD MET** (`p50 ≤ pgvector, recall
preserved`) in both regimes. Phase breakdown at this point (profiler): reads ≈ 1.3 ms (dominant), score ≈ 0.37 ms
(the SIMD distance is no longer the bottleneck), sort ≈ 1.0 ms.

## Before → after (the SIMD contribution, isolated)

Measured on the **degenerate identical-data** workload (the only apples-to-apples before/after available, since
that is what M31 shipped against), warm, 40 iters: M31 scalar p50 **24.69 ms** → M31b SIMD p50 **17.03 ms**
(**1.45× / −31%**), recall unchanged. On correct distinct data the SIMD contribution stacks on top of the
now-correct O(probes) pruning to land theodb ≤ pgvector (tables above).

## Reproduction

```bash
docker build -t theo-db:m31b .
docker run -d --name theo-db-m31b -e POSTGRES_PASSWORD=postgres -p 5432:5432 theo-db:m31b
PGPORT=5432 python3 -m pytest benchmarks/tests/test_index_am_latency.py -q   # both regimes, DISTINCT data
# profiler: add -e THEODB_SCAN_PROFILE=1 to `docker run`; grep "theodb scan profile" in the container log.
```
