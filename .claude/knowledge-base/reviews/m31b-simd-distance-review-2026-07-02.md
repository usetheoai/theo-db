# Review — m31b-simd-distance

**Date:** 2026-07-02 · **Verdict:** READY_TO_MERGE · **Milestone:** M31b
**Method:** 3 parallel specialist agents (architecture+wiring+unsafe · tests+benchmark · cross-validation) over `git diff cc6ecbf..HEAD`.

## Verdict path

Initial multi-agent verdict: **NEEDS_FIXES** (no BLOCKERs; 3 HIGH, several MEDIUM/LOW). All findings resolved in commits `61e64db` + `<de-flake>`; re-validated green. Final: **READY_TO_MERGE**.

## Findings & resolutions

| # | Sev | Finding | Resolution |
|---|---|---|---|
| 1 | HIGH | `unsafe` AVX2 length invariant only `debug_assert`ed → release-mode OOB risk if a future caller violates it | `l2_dist_from_bytes` now `assert_eq!`s `raw.len()==query.len()*4` ALWAYS (release too); SAFETY comment states both halves. `vec.rs` |
| 2 | HIGH | clustered latency band `1.15` lets a 15% regression pass green (DoD is `≤`) | tightened to `1.10`; uniform test documented as the STRICT `≤` proof (0.38× margin) |
| 3 | HIGH | theodb/pgvector param-parity assumed, not pinned | documented DEFAULT_LISTS=100 / SCAN_PROBES=10 match pgvector's lists=100/probes=10 (fixed Rust constants; reloption=M32); `_assert_index_scan` pins each side actually uses its IVFFlat index |
| 4 | MED | profiler at WARNING clutters clients / trips warn-as-error | → `pgrx::log!` (server LOG) |
| 5 | MED | test never asserts the intended index was used | added `_assert_index_scan` (EXPLAIN plan contains `Index Scan using <index>`) |
| 6 | MED | "bit-identical" scalar test passed by Pythagorean-fixture coincidence (sqrt roundtrip) | rewrote against an independent f32 Σd² oracle + non-perfect-square input |
| 7 | MED | no connect/statement timeout → CI hang risk | `connect_timeout=30` + `SET statement_timeout='120s'` |
| 8 | MED | scalar dispatch branch never exercised on x86 (AVX always taken) | `simd_x86::force_for_test`/`reset_for_test` hooks + a test covering BOTH dispatch arms |
| 9 | MED | `m31b-simd-distance.json` (Global DoD) missing | emitted — raw regression baseline |
| 10 | MED | Phase-0/SIMD micro-bench in `/tmp` (not reproducible) | committed `benchmarks/micro/simd_hotloop_bench.rs`; doc refs updated |
| 11 | LOW | stale `Metric::dist_fast` comment | corrected to reference the `is_l2` scan branch |
| 12 | LOW | ip/cosine SIMD (plan T1.1) not shipped | honest deviation noted in impl summary — deferred to M32 (no ip/cosine opclass yet → would be dead code, YAGNI) |
| — | (flaky) | latency gate flaked once under host contention | `_p50_floor` = min-of-3-rounds (uncontended floor); 2 consecutive green runs |

## Confirmed positives (independently verified by the agents)

- **DoD MET, honestly:** uniform 1.71 vs 4.53 ms (0.38×, strict `≤`) at recall parity (3.9 vs 3.8); clustered 5.29 vs 5.58 ms (0.95×) at recall 10/10 both. On DISTINCT data.
- **Data-degeneracy honesty sound:** `COUNT(DISTINCT)=1` independently verifiable + now guarded by a test assert; no engine bug (standalone k-means repro balances); no silent history rewrite (ADR 0012 supersedes; M31 artifacts untouched).
- **Unsafe soundness:** `_mm256_loadu_ps` unaligned load of page bytes is correct; SIMD range + scalar tail in-bounds; per-process atomic dispatch with `Relaxed` is a sound one-time-detect.
- **Clean:** 0 Rust build warnings (no dead code / fabrication); DIP/layering correct (am → vec, not reverse); wiring triad complete; all commits on `develop`, ZERO `Co-Authored-By`; files ≤ 500 LoC; no public-copy violations.

## Gate results (image `theo-db:m31b`, PG17)

- `test_index_am_latency.py` — 2 passed (both regimes; Index-Scan asserts; robust floor; 2 consecutive green).
- Coexistence M20-M22 + index AM — 84 passed.
- Code-quality — PASS_WITH_CAVEATS (clean release build; toolchain-substitution caveat).
