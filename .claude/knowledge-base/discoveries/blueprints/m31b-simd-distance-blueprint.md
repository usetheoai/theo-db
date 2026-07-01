# Blueprint: M31b — SIMD vector distance (AVX2+FMA + runtime dispatch)

> **Discovery verdict:** SHIPPABLE_WITH_CAVEATS — grounded in pgvector's SIMD dispatch (cloned reference) + the
> M31 latency measurement (own). The design below is the M31b plan contract.

**Slug:** `m31b-simd-distance` · **Owner:** paulohenriquevn · **Created:** 2026-07-01

## Context

M31 closed the O(N) algorithmic gap (structured partial reads) but `theodb_ivfflat` is still ~2.7× behind pgvector
(~38 ms vs ~14 ms at 100k×128, `docs/benchmarks/m31-am-latency.md`). The residual is the **constant factor**: the
scan's per-candidate distance is a scalar/SSE2 loop; pgvector uses **AVX+FMA** SIMD. M31b (P0 track,
`memory: goto-p0-vector-superiority`) closes that residual to chase **p50 ≤ pgvector**.

## Coverage Corner 1 — Integration Tests

Reuse `benchmarks/tests/test_index_am_latency.py` (M31) — the same gate, tightened: assert `theodb_ivfflat` Index
Scan p50 **≤ pgvector** (not just within-band) at n ≥ 100k, recall@10 preserved. Plus the M20/M21/M22 parity
suites MUST stay green (the SQL-callable distance ops are untouched — see ADR-2).

## Coverage Corner 2 — Dependencies

**No new dependency** (parsimony rung 2 — stdlib). SIMD via `std::arch` intrinsics + `std::is_x86_feature_detected!`.
Rejected: `wide` / `multiversion` crates (a `/deps-audit` + license gate for something stdlib provides).

## Coverage Corner 3 — Tools

`std::arch::x86_64` AVX2/FMA intrinsics (`_mm256_loadu_ps`, `_mm256_sub_ps`, `_mm256_fmadd_ps`, horizontal reduce);
`#[target_feature(enable = "avx2,fma")]` (unsafe fns, callable only after runtime detect).

## Coverage Corner 4 — Techniques

**pgvector's approach (cited):** `vector.c:37` `#define VECTOR_TARGET_CLONES __attribute__((target_clones("default",
"fma")))` — GCC function multi-versioning: the compiler emits a `default` (SSE2) and an `fma` (AVX+FMA) clone of the
distance functions; the dynamic loader (`ifunc`) picks the best for the running CPU. That FMA 8-wide path is the ~2×
advantage over a scalar/SSE2 4-wide loop.

**The Rust equivalent (no stable auto-multiversioning without a crate):** hand-write the hot loop with `std::arch`
AVX2+FMA intrinsics in an `#[target_feature(enable="avx2,fma")]` unsafe fn, and dispatch at runtime:
`if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") { unsafe { l2_avx2(a,b) } } else { l2_scalar(a,b) }`.
The detect is cheap (cached by std) but we cache it once per process in an `AtomicU8` to avoid the macro cost in
the 65k-iteration hot loop. On non-x86 / non-AVX2 CPUs the scalar fallback runs (portability preserved).

**Numeric parity (honest):** the AVX2 loop accumulates in 8 partial f32 lanes then horizontally reduces — a
DIFFERENT summation order than the sequential scalar sum, so f32 results differ by ~1 ULP·√dim. This is
**recall-preserving** (neighbor ranking is unchanged within the M21 tolerance eps) but NOT bit-identical to the M20
scalar. pgvector has the SAME property (its FMA path ≠ a pure-scalar sum). So the SIMD path is validated by
**recall parity**, not bit-parity.

## Cross-cutting Comparison

| | M31 scan | pgvector | M31b scan |
|---|---|---|---|
| Distance | scalar/SSE2 (auto-vec 4-wide) | AVX+FMA (target_clones) | AVX2+FMA (std::arch) + scalar fallback |
| Dispatch | n/a | ifunc (compiler) | is_x86_feature_detected! (cached) |
| New dep | — | — | none (stdlib) |

## ADRs

### D1 — std::arch AVX2+FMA + runtime dispatch (no crate)

Chosen over `wide`/`multiversion` — stdlib provides intrinsics + feature-detect; a crate would add a deps-audit
surface for zero benefit. Scalar fallback preserves portability.

### D2 — SIMD only in the AM scan hot-loop; M20 SQL ops + M21 ann search stay scalar

The scan's per-candidate distance (65k/query) is where latency lives. The SQL-callable `theodb.l2_distance` etc.
(M20 byte-parity contract vs pgvector's TEXT) and the in-memory `ann` search (M21 recall parity) are NOT in the
hot path and keep the scalar impl — so M31b touches ZERO parity tests. The scan calls a new fast-distance path.

### D3 — Measurement-first: p50 ≤ pgvector or honest residual

If AVX2+FMA still doesn't reach ≤ pgvector, report the residual honestly (e.g. dim-tail handling, or pgvector's
AVX-512) and the next lever — never fake the number (`public-copy.md`).

### D4 — Profile-before-optimize (Phase 0): confirm the distance IS the bottleneck

Measurement-first applied to the optimization itself (per the flamegraph tip): BEFORE writing AVX2 intrinsics,
profile the scan hot-loop (read list bytes → decode into scratch → distance → sort) to confirm the distance
dominates the ~38 ms. If byte-decode / sort / page-reads are a large share, AVX2 on the distance alone won't reach
≤ pgvector and the effort would be mis-targeted (re-work). **Tooling (dev-only, not a product dep):** a standalone
`criterion` micro-bench of the hot-loop in `theodb_rs` (flamegraph-able natively, avoiding perf-inside-postgres)
and/or an A/B measurement (distance replaced by a no-op) to get the distance's share. `flamegraph`/`criterion` are
dev tools — no deps-audit on the shipped artifact.

## Recommendations

1. **Phase 0 — profile** the scan hot-loop (criterion micro-bench + flamegraph and/or A/B no-op) to quantify the
   distance's share of the latency; only then commit to the AVX2 target (M31b T0).
2. `vec.rs` (or `vec_simd.rs`): `l2_distance_simd` / `inner_product_simd` with AVX2+FMA + scalar fallback, cached
   dispatch (M31b T1) — plus any co-bottleneck Phase 0 surfaces (e.g. aligned/SIMD byte-decode).
3. `Metric::dist_fast` routing to the SIMD path; `scan_ivf_structured` uses it in the hot loop (M31b T2).
4. Tighten `test_index_am_latency.py` to assert p50 ≤ pgvector; recall preserved; M20/M21/M22 green (M31b T3).
5. Fallback proven: a unit test asserts the SIMD result ≈ scalar within eps (numeric sanity).
