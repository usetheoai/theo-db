---
slug: m31b-simd-distance
milestone_id: M31b
created_at: 2026-07-01
goal: Cut the theodb_ivfflat Index Scan distance cost with AVX2+FMA SIMD (runtime dispatch, scalar fallback) so the p50 reaches pgvector, proven by benchmarks/tests/test_index_am_latency.py asserting theodb p50 <= pgvector p50 on n>=100k dim=128 with recall preserved and M20-M22 parity suites green, all against the container.
---

# M31b — SIMD vector distance (AVX2+FMA + runtime dispatch)

## Goal

Cut the `theodb_ivfflat` Index Scan's per-candidate distance cost with **AVX2+FMA SIMD** (runtime-dispatched,
scalar fallback), so the Index Scan **p50 reaches pgvector**, measured by a single observable metric:
**`benchmarks/tests/test_index_am_latency.py` passing** — `theodb_ivfflat` Index Scan p50 **≤ pgvector `ivfflat`**
on n ≥ 100k dim 128 with recall@10 preserved AND the M20/M21/M22 parity suites green, all in the Docker image,
evidence in `docs/benchmarks/m31b-simd-distance.md`.

## Context

M31 closed the O(N) algorithmic gap (structured partial reads) but `theodb_ivfflat` is ~2.7× behind pgvector
(~38 ms vs ~14 ms at 100k×128, `docs/benchmarks/m31-am-latency.md`). The residual is the constant factor: the scan
scores 10s of thousands of candidates per query with a scalar/SSE2 distance loop (`vec.rs`), while pgvector uses
**AVX+FMA** (`vector.c:37` `target_clones("default","fma")`). M31b (P0 CTO GOTO — `memory: goto-p0-vector-superiority`)
closes that residual, chasing p50 ≤ pgvector — the North Star's measured-latency pillar.

## Baseline Context

### Files that will be touched

| File | LoC today | git sha (last touch) | Why it exists / role in M31b |
|---|---|---|---|
| `theodb_rs/src/vec.rs` | 133 | `cc6ecbf` | M20 scalar distance ops (l2/ip/cosine). Add AVX2+FMA SIMD variants + a cached runtime dispatcher; the scalar impls STAY (M20 byte-parity surface). |
| `theodb_rs/src/am/scan.rs` | ~160 | `cc6ecbf` | M31 structured scan hot-loop. Route the per-candidate distance through the SIMD dispatcher. |
| `theodb_rs/src/ann/mod.rs` | 269 | `cc6ecbf` | `Metric::dist` (scalar, used by M21 ann search + M22 sbq). Add `Metric::dist_fast` (SIMD-dispatched) used ONLY by the scan; `dist` stays scalar (M21/M22 untouched). |
| `benchmarks/tests/test_index_am_latency.py` | ~90 | `cc6ecbf` | M31 latency gate — tighten to assert p50 ≤ pgvector. |
| `docs/benchmarks/m31b-simd-distance.md` | 0 (NEW) | — | Reproducible before/after latency + the Phase-0 profile breakdown. |

### Current callers / dependents

- `scan_ivf_structured` (`am/scan.rs`) calls `metric.dist(query, &scratch)` per candidate — the hot path M31b
  reroutes to `metric.dist_fast`.
- `vec::{l2_distance,inner_product,cosine_distance}` are called by `Metric::dist`, the M20 SQL-callable ops, and
  the M21 `ann` search / M22 `sbq`. M31b ADDS SIMD variants; the scalar callers are unchanged.

### Domain glossary

- **AVX2+FMA:** 256-bit SIMD (8× f32) with fused multiply-add — the instruction set pgvector's fast clone uses.
- **runtime dispatch:** choose the SIMD or scalar impl based on `is_x86_feature_detected!` at run time (cached).
- **horizontal reduce:** summing the 8 SIMD lanes into one f32 at the end of the loop.
- **dist_fast:** the scan-only distance entry that routes to SIMD; `dist` stays scalar for the parity surface.

### Architecture boundaries affected

Per `rules/architecture.md`: `vec.rs`/`ann` stay pure domain (no pg types). The SIMD is `std::arch` (stdlib) — no
new dependency, no boundary change. `am/scan.rs` (infra) calls the pure `dist_fast`.

## Prior Art & Related Work

- Blueprint `m31b-simd-distance-blueprint.md` — pgvector's `target_clones` SIMD + the std::arch dispatch design + the profile-before-optimize step.
- pgvector `src/vector.c:37` (`VECTOR_TARGET_CLONES`) — the SIMD-dispatch reference.
- M31 (`am/scan.rs`, `docs/benchmarks/m31-am-latency.md`) — the ~38 ms baseline this optimizes.

## ADRs

### ADR-1 — std::arch AVX2+FMA + cached runtime dispatch (no crate)

**Decision:** hand-write the distance hot loop with `std::arch` AVX2+FMA intrinsics in an
`#[target_feature(enable="avx2,fma")]` unsafe fn, dispatched at runtime via `is_x86_feature_detected!` (cached once
in an atomic), with a scalar fallback for non-AVX2 CPUs. **Rationale:** stdlib provides intrinsics + feature-detect
(parsimony rung 2) — matches pgvector's `target_clones` approach with the Rust equivalent; portability preserved by
the fallback. **Rejected alternatives:** (a) `wide` / `multiversion` crates — a deps-audit + license-gate surface
for something stdlib does; (b) `RUSTFLAGS=-C target-feature=+avx2` global build — breaks portability (SIGILL on
non-AVX2 CPUs), no runtime fallback.

### ADR-2 — SIMD only in the scan hot-loop; M20 SQL ops + M21/M22 stay scalar

**Decision:** the SIMD path is used ONLY by `scan_ivf_structured` (via `Metric::dist_fast`). The SQL-callable
`theodb.l2_distance` etc. (M20 byte-parity contract vs pgvector's TEXT) and the in-memory `ann` search / `sbq`
rerank (M21/M22 recall parity) keep the scalar `Metric::dist`. **Rationale:** the scan's 65k-candidate loop is
where latency lives; the parity-contracted surfaces are single-call and must not change. This makes M31b touch
ZERO parity tests. **Rejected:** SIMD everywhere — risks the M20 exact-text-parity (SIMD summation order ≠ scalar)
for no latency benefit on the single-call surfaces.

### ADR-3 — Profile-before-optimize (Phase 0)

**Decision:** before writing AVX2, profile the scan hot-loop (read bytes → decode → distance → sort) to confirm
the distance dominates the ~38 ms; if a co-bottleneck (byte-decode / sort) is large, address it too. **Rationale:**
measurement-first applied to the optimization — avoids mis-targeting the SIMD effort (re-work). **Rejected:**
writing AVX2 blind on the assumption the distance is the bottleneck.

## Dependency Graph

```
Phase 0 (profile the hot-loop: criterion micro-bench + flamegraph / A/B — quantify the distance's share)
   ↓
Phase 1 (vec.rs: AVX2+FMA l2/ip SIMD + cached dispatch + scalar fallback; numeric-sanity unit test)  ← informed by 0
   ↓
Phase 2 (Metric::dist_fast; scan_ivf_structured routes the hot loop through it)                        ← depends on 1
   ↓
Phase 3 (benchmark p50 <= pgvector + recall preserved + M20-M22 green; doc) — Final Validation
```

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `std::arch` | (stdlib) | Rust | AVX2/FMA intrinsics + `is_x86_feature_detected!` — no new dependency. |

### New — to be introduced

(none for the shipped artifact — SIMD is stdlib. Dev-only profiling tools (`cargo-flamegraph`, `criterion` as a
dev-dependency) are NOT product dependencies and are not linked into the extension `.so`.)

### Removed

(none.)

## Phase 0 — Profile the hot-loop (profile-before-optimize)

### T0.1 — Quantify the distance's share of the scan latency
#### Why this step
Per ADR-3, confirm the scalar distance is the dominant cost BEFORE writing AVX2 — else the SIMD effort is
mis-targeted (byte-decode / sort could dominate). Uses dev-only tooling (criterion micro-bench + flamegraph, and/or
an A/B where the distance is replaced by a no-op), not a product dependency.
#### Files to edit
- `theodb_rs/benches/scan_hotloop.rs` (NEW, `#[cfg(bench)]` / criterion dev-dependency) — a micro-bench replaying
  the hot-loop (fixture list bytes → decode into scratch → distance → collect) with two variants: full vs
  distance-replaced-by-no-op, to attribute the cost.
#### TDD
- The bench itself is the measurement (not a pass/fail unit test); its output is recorded in the Phase-0 note.
  A cheap correctness assert: the no-op variant returns the same COUNT of candidates (only the distance is stubbed).
#### Concurrency tests
(none — single-threaded) — a micro-bench of pure compute.
#### Acceptance criteria
- The profile attributes the ~38 ms scan cost across {distance, byte-decode, sort, reads} with a number per bucket
  (e.g. "distance = X% via criterion A/B"); recorded in `docs/benchmarks/m31b-simd-distance.md § Profile`.
#### DoD
- The distance's share is quantified (a concrete %/ms), and Phase 1's target (AVX2 on distance, + any co-bottleneck)
  is set by it — documented before any SIMD code is written.

## Phase 1 — AVX2+FMA SIMD distance + dispatch

### T1.1 — `l2_distance_simd` / `inner_product_simd` (AVX2+FMA) + cached runtime dispatcher + scalar fallback
#### Why this step
The distance is the scan's inner cost (Phase 0 quantifies it). AVX2+FMA 8-wide closes the ~2× vs scalar/SSE2, with
a runtime-dispatched fallback so non-AVX2 CPUs still run (portability).
#### Files to edit
- `theodb_rs/src/vec.rs` — `#[target_feature(enable="avx2,fma")] unsafe fn l2_avx2(a,b)` / `ip_avx2(a,b)` (8-wide
  accumulate + horizontal reduce + scalar tail for `dim % 8`); safe `l2_distance_fast(a,b)` / `ip_fast` dispatching
  on a cached `is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma")` (an `AtomicU8`), falling back to
  the existing scalar `l2_distance`/`inner_product`.
#### TDD
- Rust `#[pg_test]` `l2_simd_matches_scalar_within_eps`: for random vectors (dims 8, 128, 130 — incl. a non-8
  tail), `(l2_distance_fast(a,b) - l2_distance(a,b)).abs() < 1e-3 * (1 + l2_distance(a,b))` — SIMD ≈ scalar
  (recall-preserving, not bit-identical). Same for ip.
- Negative: dim mismatch still fail-fast (reuse `check_dims`).
#### Concurrency tests
(none — single-threaded) — pure functions; the cached dispatch atomic is written idempotently (same value from any
thread), a benign data race on identical bytes.
#### Failure scenarios
- Non-AVX2 CPU → `l2_distance_fast` dispatches to the scalar fallback (portability; asserted by forcing the scalar
  path in a test build).
- `dim % 8 != 0` → the scalar tail handles the remainder (the eps test uses dim 130 to cover it).
#### Acceptance criteria
- `l2_distance_fast(a,b) ≈ l2_distance(a,b)` within eps for dims {8,128,130} (`assert` in the unit test); the AVX2
  fn is `#[target_feature]`-gated and only reached after the runtime detect.
#### DoD
- `cargo pgrx` builds; the SIMD-vs-scalar eps test compiles; clippy `-D warnings` clean. File ≤ 500 LoC.

## Phase 2 — Wire the SIMD path into the scan hot-loop

### T2.1 — `Metric::dist_fast`; `scan_ivf_structured` scores via SIMD
#### Why this step
Route the scan's per-candidate distance through the SIMD dispatcher, leaving `Metric::dist` (scalar) for M20/M21/M22.
#### Files to edit
- `theodb_rs/src/ann/mod.rs` — `pub(crate) fn dist_fast(self, a, b) -> f64` routing L2→`vec::l2_distance_fast`,
  Ip→`-vec::inner_product_fast`, Cosine→scalar `cosine_distance` (cosine's two extra norms make a full SIMD variant
  a Phase-1 stretch; use scalar unless Phase 0 shows cosine matters — the l2 opclass is the DEFAULT + benchmarked).
- `theodb_rs/src/am/scan.rs` — `scan_ivf_structured` scores each candidate with `metric.dist_fast(query, &scratch)`.
#### TDD
- `test_index_am.py::test_index_scan_returns_correct_neighbors` (kept) — recall@5 ≥ 4/5 via the SIMD scan path.
#### Concurrency tests
(none — single-threaded) — the scan is per-backend; the dispatch atomic is process-shared + idempotent.
#### Acceptance criteria
- Recall@10 preserved (≥ parity vs the M31 scalar scan on the same corpus); the scan compiles + runs with `dist_fast`.
#### DoD
- `pytest test_index_am.py` green (structured scan via SIMD, recall preserved).

## Phase 3 — Final Integration Validation + benchmark

### T3.1 — p50 ≤ pgvector + recall + M20-M22 parity; doc
#### Why this step
Measurement-first (ADR-3): the P0 latency-parity claim is asserted ONLY by the head-to-head vs pgvector.
#### Files to edit
- `benchmarks/tests/test_index_am_latency.py` — tighten the band to `theodb_p50 <= pgv_p50 * 1.15` (parity) at
  n ≥ 100k dim 128; recall preserved.
- `docs/benchmarks/m31b-simd-distance.{md,json}` — Phase-0 profile + before(M31 38ms)/after(SIMD)/pgvector, mean±std.
#### Concurrency tests
(none — single-threaded) — sequential benchmark.
#### Failure scenarios
- If AVX2+FMA still misses p50 ≤ pgvector, report the residual honestly (e.g. AVX-512 / decode-bound) + the next
  lever; do NOT fake the number (`public-copy.md`). The re-scoped honest outcome, if needed, is documented — not hidden.
#### Acceptance criteria
- `theodb_ivfflat` Index Scan p50 ≤ pgvector (within the 1.15× parity band) on n ≥ 100k dim 128; recall@10 ≥ parity;
  M20/M21/M22 suites green (the scalar parity surfaces are untouched); clippy `-D warnings` clean; CHANGELOG updated.
#### DoD
- `pytest test_index_am_latency.py` (tightened) + M20-M22 green in the image; `docs/benchmarks/m31b-simd-distance.{md,json}` written.

## Coverage Matrix

| # | M31b DoD item | Task(s) |
|---|---|---|
| 1 | Profile confirms the distance is the bottleneck (measurement-first) | T0.1 |
| 2 | AVX2+FMA SIMD distance + runtime dispatch + scalar fallback (no new dep) | T1.1 |
| 3 | Wired into the scan; recall preserved | T2.1 |
| 4 | Benchmark: Index Scan p50 ≤ pgvector (n ≥ 100k dim ≥ 128); M20/M21/M22 parity green | T2.1 (recall) + T3.1 (latency + parity) |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| `unsafe` AVX2 intrinsics (loads, tail handling) have bugs | HIGH | numeric-sanity eps test vs scalar (T1.1) covers dims incl. a non-8 tail; the fn is target-feature-gated + only reached after runtime detect | paulohenriquevn |
| AVX2+FMA still doesn't reach p50 ≤ pgvector (decode-bound / AVX-512) | MEDIUM | Phase 0 profile targets the real bottleneck first; if a gap remains, report it honestly + the next lever (do not fake) | paulohenriquevn |
| SIMD summation order changes f32 → recall regression | MEDIUM | eps test proves SIMD ≈ scalar; recall@10 gate on the scan (T2.1) proves ranking unchanged | paulohenriquevn |
| Non-AVX2 CPU path untested → crash on old hardware | HIGH | scalar fallback via runtime dispatch; a test forces the scalar path; the container CPU has AVX2 but the fallback is exercised | paulohenriquevn |

## Failure scenarios

- Non-AVX2 / non-x86 CPU → `is_x86_feature_detected!` false → scalar fallback (no SIGILL).
- `dim % 8 != 0` → scalar tail; eps test uses dim 130.
- Corrupt/short page in the scan → the M26/M31 bounds-checked readers already fail-fast (unchanged).
- NULL query vector → M26 `SK_ISNULL` guard (unchanged).

## Unresolved Questions

- Does AVX2+FMA alone reach p50 ≤ pgvector, or is the scan also byte-decode-bound? — **Resolved by the Phase-0
  profile (T0.1)**; Phase 1 targets whatever dominates. If a residual remains after SIMD, it is reported honestly.
- Is cosine worth a full SIMD variant, or does the l2 DEFAULT opclass cover the benchmark? — decided by Phase 0 +
  the benchmark (l2 is the default + the measured path).

## Global DoD

- [ ] Phase-0 profile recorded (distance's share quantified) before any SIMD code.
- [ ] `benchmarks/tests/test_index_am_latency.py` (tightened) green: p50 ≤ pgvector (1.15× band) + recall preserved.
- [ ] SIMD-vs-scalar eps unit test green (dims 8/128/130); scalar fallback exercised.
- [ ] M20/M21/M22 parity suites green (scalar surfaces untouched); `test_index_am.py` green.
- [ ] `docs/benchmarks/m31b-simd-distance.{md,json}` written; no new product dependency; clippy `-D warnings` clean.
- [ ] every changed file ≤ 500 LoC; CHANGELOG `[Unreleased]` updated; no `Co-Authored-By`.
