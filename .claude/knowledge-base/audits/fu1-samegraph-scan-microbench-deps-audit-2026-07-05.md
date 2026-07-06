# Deps Audit: fu1-samegraph-scan-microbench

**Date:** 2026-07-05
**Mode:** plan-bound:fu1-samegraph-scan-microbench
**Verdict:** PASS
**Hard caps triggered:** [] (none)

## Summary
- Ecosystems detected: Rust (`theodb_rs/Cargo.toml` + `Cargo.lock`)
- New deps declared by the plan: 1 — `criterion = "0.5.1"` (dev-only)
- Vulnerabilities found on declared/new deps: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 0 LOW
- Outdated: n/a (criterion 0.5.1 is the pgvectorscale-proven pin for pgrx 0.16.1)
- Auditor coverage: { cargo-audit 0.22.1: ran, osv-scanner 1.9.2: available, RustSec advisory-db: consulted }

## Vulnerabilities (sorted by severity)

None on the plan's declared dependency.

## Pre-existing tree note (NOT introduced by this plan)

`cargo audit` on the current `theodb_rs/Cargo.lock` reports ONE warning:
- `serde_cbor 0.11.2` — **unmaintained** (RUSTSEC-2021-0127), transitive via `pgrx 0.16.1` (`pgrx → serde_cbor`).
  It is an *unmaintained* advisory (not a CVE), already present before FU-1, and shows as "allowed" (2 allowed
  warnings). It is pgrx's transitive dep — out of FU-1's scope, not introduced by `criterion`.

## Outdated (non-vulnerable)

`criterion 0.5.1` is intentionally pinned to match `pgvectorscale`'s pin (the same-stack peer running pgrx
0.16.1 / Rust 1.91), per the blueprint. Newer (vectorchord uses 0.8.2) but 0.5.1 is the proven-compatible choice
for this exact pgrx toolchain — an ADR-backed pin, not an accidental staleness.

## Plan validation (Mode 2)

| Plan dep | Section | Manifest match | Audit clean? | Rule 9 OK? | Verdict |
|---|---|---|---|---|---|
| `criterion` `0.5.1` | NEW | n/a (to add as `[dev-dependencies]`) | yes — **no advisory in RustSec DB** (`~/.cargo/advisory-db/crates/criterion/` absent) | yes — "benchmark harness padrão; não reimplementar timing/CI/outlier (Regra 9)"; alt (hand-rolled timing) rejected; dev-only → zero cdylib | OK |

## Rule-9 + footprint check

- `criterion` is added to `[dev-dependencies]` ONLY → it never links into the released cdylib extension (zero
  runtime footprint). Confirmed by design (dev-deps are excluded from `cargo build --release` of the lib target).
- The blueprint's parsimony-rung-4 rationale holds: a dev-only benchmark dep is the minimal footprint; not
  reinventing criterion's statistical machinery (bootstrap CI, outlier classification) is Rule 9 compliance.

## Recommended next steps

1. Add `criterion = "0.5.1"` under `[dev-dependencies]` in `theodb_rs/Cargo.toml` at implement time (T3.1).
2. Proceed with `/plan-confidence` — the deps gate is PASS.
