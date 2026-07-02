---
slug: m31b-simd-distance
milestone_id: M31b
created_at: 2026-07-01
goal: theodb_ivfflat Index Scan p50 <= pgvector (n>=100k, dim=128), recall preserved, via AVX2+FMA fused distance
verdict: IMPLEMENTATION_COMPLETE
---

# M31b — SIMD vector distance (AVX2+FMA fused decode+distance) — implementation summary

## Goal (met)

theodb_ivfflat Index Scan p50 **≤ pgvector** at n=100k / dim=128 / probes=10, recall preserved — via AVX2+FMA
SIMD distance with a runtime dispatch + scalar fallback. **MET** on DISTINCT data in both regimes (see Evidence).

## What shipped

| Task | Change | Files | Wiring triad |
|---|---|---|---|
| T0.1 | Phase-0 profile (flamegraph tip) — decode 45% / distance 55% of the hot-loop | `docs/benchmarks/m31b-simd-distance.md § Phase 0` | n/a (measurement) |
| T1.1 | `l2_dist_from_bytes` — fused AVX2+FMA reads f32 straight from page bytes via `_mm256_loadu_ps`; cached `is_x86_feature_detected!` dispatch (`AtomicU8`); scalar fallback (`l2_sq_from_bytes_scalar`); parity tests across dims incl. 8-lane tail | `theodb_rs/src/vec.rs` | caller: scan.rs · test: `l2_from_bytes_*` pg_tests + Python latency · metric: profiler |
| T2.1 | scan L2 path scores candidates DIRECTLY off page bytes (no scratch decode); other metrics keep scratch path | `theodb_rs/src/am/scan.rs` | caller: `amrescan`→`scan_ivf_structured` · test: latency+recall · metric: `THEODB_SCAN_PROFILE` phase timing |
| T3.1 | latency+recall gate on DISTINCT data (uniform + clustered), assert p50 ≤ pgvector + recall parity | `benchmarks/tests/test_index_am_latency.py` | — |
| fix | benchmark-data degeneracy (identical vectors) discovered via profiler + fixed | `test_index_am_latency.py`, ADR 0012 | — |

## Key finding (measurement-first, per the flamegraph tip)

The profiler (`THEODB_SCAN_PROFILE=1`) exposed `cand=100000 nonempty_lists=1/100`: the latency harness seeded 100k
IDENTICAL vectors (non-correlated `string_agg((random())…)` sub-select hoisted by PostgreSQL as a one-time
InitPlan), collapsing k-means to one list → brute-force-on-ties. This retro-invalidated M31's "~2.7× behind
pgvector". No engine bug — theodb's k-means was always correct (standalone repro balances). Fixed by seeding
DISTINCT vectors via Python `COPY`. ADR 0012.

## Evidence (real, DISTINCT data — n=100k, dim=128, probes=10, lists=100, warm)

| Regime | theodb recall | theodb p50 | pgvector recall | pgvector p50 | verdict |
|---|---|---|---|---|---|
| Uniform (worst case) | 3.9/10 | **1.71 ms** | 3.8/10 | 4.53 ms | theodb **2.6× faster**, recall parity |
| Clustered (realistic) | 10.0/10 | **5.29 ms** | 10.0/10 | 5.58 ms | theodb **≤ pgvector** at full recall |

SIMD contribution isolated (degenerate before/after): M31 scalar 24.69 ms → M31b SIMD 17.03 ms (−31%).

## Gate results (image `theo-db:m31b`, PG17)

- `test_index_am_latency.py` — 2 passed (both regimes; p50 ≤ pgvector + recall parity asserted on DISTINCT data).
- Coexistence M20-M22 + index AM — 84 passed (`test_vector_ops`, `test_ann_index`, `test_recall`, `test_sbq_index`,
  `test_index_am`).
- Standalone SIMD parity/bench (`/tmp/simd_check.rs`): parity within eps across dims 1..129; 1.62× on the hot-loop.

## Numeric contract

SIMD lane-summation ≠ bit-identical to the M20 scalar sum (~1 ULP·√dim) — recall-preserving, same property as
pgvector's FMA path. The M20 SQL-callable distance ops are untouched (byte-parity intact). ADR-2 (blueprint).

## Not in scope (honest)

- Absolute recall on uniform-random data is low for BOTH indexes (no cluster structure) — this is inherent to
  IVFFlat, not a theodb defect; the realistic (clustered) point reaches 10/10.
- `lists`/`probes` are fixed defaults (100/10). A configurable reloption + higher-recall operating points → M32.
- **ip/cosine SIMD deferred (honest deviation from plan T1.1):** T1.1 named `l2_distance_simd` AND `inner_product_simd`.
  Only L2 (`l2_dist_from_bytes`) shipped — `theodb_ivfflat` registers ONLY `theodb_ivfflat_l2_ops` today, so the
  scan's L2 branch is the sole SIMD hot path; ip/cosine candidates (no opclass yet) fall back to the scalar
  `metric.dist`. Adding ip/cosine SIMD before an ip/cosine opclass exists would be dead code (YAGNI) — deferred to
  M32 alongside those opclasses. The `Metric::dist_fast` name in plan T2.1 was simplified to a direct
  `l2_dist_from_bytes` call behind the scan's `is_l2` branch (KISS).
