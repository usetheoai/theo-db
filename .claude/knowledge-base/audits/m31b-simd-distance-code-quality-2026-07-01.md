# Code-Quality Audit — m31b-simd-distance

**Date:** 2026-07-01 · **Verdict:** PASS_WITH_CAVEATS · **Milestone:** M31b

## Method

The Rust toolchain (pgrx/cargo) requires `PGRX_HOME` (a local PG install) that is not present on the dev host —
the extension builds only in the Docker image. The authoritative Rust code-quality signal is therefore the
`cargo pgrx install --release` compile inside `docker build -t theo-db:m31b`:

- **Symbol fabrication (D2):** a release compile fails on any undefined symbol. Build EXIT=0 → no fabrication.
- **Dead code (D1):** `grep -ciE '#N warning:'` over the full build log = **0** — no `never used` / `unused`
  warnings for the new `l2_dist_from_bytes` / `l2_sq_from_bytes_scalar` / `simd_x86::{available,l2_sq}` symbols.
- **Wiring (D3):** `l2_dist_from_bytes` caller = `scan.rs::scan_ivf_structured` (L2 path); integration test =
  `test_index_am_latency.py` (both regimes exercise the L2 Index Scan); runtime metric = `THEODB_SCAN_PROFILE`
  phase timing. Triad complete. Scalar fallback reachable on non-AVX2 CPUs + covered by `l2_from_bytes_*` pg_tests.

## Findings

| Severity | Finding |
|---|---|
| INFO | Rust dead-code/fabrication detectors (cargo-udeps) not run locally (no PGRX_HOME); substituted by the clean release build (0 warnings). Caveat logged per golden rule § 1. |
| INFO | `unsafe` AVX2 block: SAFETY documented; guarded by `simd_x86::available()` (AVX2+FMA) + `debug_assert_eq!(raw.len(), query.len()*4)`; unaligned loads use `_mm256_loadu_ps`. |

## Verdict

**PASS_WITH_CAVEATS** — no FAIL_HARD (no fabrication, no dead code); the single caveat is the local-toolchain
substitution (clean Docker release build). Proceeds to `/review`.
