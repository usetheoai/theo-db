# ADR 0012 — Benchmark data must be DISTINCT (the InitPlan-hoist degeneracy)

**Status:** Accepted · **Date:** 2026-07-01 · **Milestone:** M31b · **Supersedes latency claims in:** ADR 0011 / M31

## Context

While profiling the `theodb_ivfflat` scan for M31b (SIMD distance), the opt-in profiler `THEODB_SCAN_PROFILE=1`
reported, on a 100 000-row / dim-128 table:

```
theodb scan profile: cand=100000 nonempty_lists=1/100 probes=10 reads=13764us score=3191us sort=91us
```

The scan was scoring **all 100 000 rows** (not the ~10 000 expected from probing 10 of 100 lists) because the
k-means had placed **every vector into a single list** (`nonempty_lists=1/100`).

Root-cause investigation ruled out theodb: a standalone reproduction of the exact k-means (k-means++ + 10 Lloyd,
SplitMix64 seed 42, f32 distances) on genuinely-distinct uniform vectors produces **balanced** lists (max ≈ 1069,
10 probes → ≈ 10 035 candidates). The culprit was the **benchmark's data-generation SQL**:

```sql
INSERT INTO lat SELECT g, ('['||(SELECT string_agg((random())::text, ',')
                                 FROM generate_series(1,128))||']')::vector
FROM generate_series(1,100000) g;
```

The inner `(SELECT … FROM generate_series(1,128))` does not reference the outer row `g`, so PostgreSQL treats it as
a **non-correlated InitPlan and evaluates it exactly once** — the volatility of `random()` does not force per-row
re-evaluation of an uncorrelated sub-select. Result: **all 100 000 rows received the IDENTICAL vector**
(`SELECT COUNT(DISTINCT embedding::text) = 1`, verified). `LATERAL` did not help (still no reference to `g`).

Identical points collapse *any* correct k-means to one non-empty list, and recall is trivially 10/10 (every row is
an exact tie). The degeneracy was therefore invisible to the recall gate and produced a brute-force-on-ties
workload masquerading as ANN. **Every latency number recorded before M31b was measured on this degenerate data**,
including M31's `~38 ms` / `~2.7× behind pgvector` (`docs/benchmarks/m31-am-latency.md`).

## Decision

1. **Benchmark data MUST be distinct.** Seed vectors from Python (`random.Random(seed)`) and load via `COPY`; never
   the non-correlated `string_agg((random())…)` SQL idiom. The data-gen asserts `COUNT(DISTINCT) == N` before use.
2. **Two regimes are measured:** uniform-random (IVFFlat worst case — recall parity + robust latency win) and
   gaussian-clustered (realistic embedding-like — high recall + p50 ≤ pgvector). The gate lives in
   `benchmarks/tests/test_index_am_latency.py`.
3. **The M31 latency figures are retro-invalidated** as ANN measurements (they measured identical-vector ties). The
   corrected M31b numbers (`docs/benchmarks/m31b-simd-distance.md`) supersede them. M31's *structural* achievement
   (O(N)→O(probes) partial reads) still stands — but its constant-factor comparison was on bad data.
4. **The profiler stays** (`THEODB_SCAN_PROFILE=1`, off by default) as permanent list-balance observability: a
   near-`1/100` `nonempty_lists` on distinct data is the tripwire for this class of bug.

## Consequences

- **Positive:** the true M31b result is far stronger than the pre-M31b picture — on distinct data theodb is 2.6×
  faster than pgvector (uniform, recall parity) and ≤ pgvector at full recall (clustered). The "2.7× behind"
  narrative was an artifact of bad data, now corrected. The k-means/IVFFlat build is validated as correct.
- **Negative / honest:** a released benchmark (M31) carried invalid numbers. This ADR records the correction rather
  than silently overwriting history (Unbreakable Rule 3). No code bug existed in the engine; the defect was in the
  test harness.
- **Follow-up (M32):** with pruning now proven correct on real data, the next lever for the P0 vector-superiority
  track is a configurable `lists`/`probes` reloption and higher-recall operating points — tracked under M32.

## Alternatives considered

- **Correlated SQL (`WHERE g = g` / `+ 0*g`)** to defeat the InitPlan hoist — rejected: fragile (the planner may
  constant-fold the correlation away) and opaque to a reviewer. Python `COPY` of explicitly-distinct data is
  unambiguous and reproducible (seeded).
- **Keep uniform-only data** — rejected: uniform random is the IVFFlat worst case and understates real recall;
  a clustered regime is needed to measure the realistic operating point.
