# Deps Audit: pgrx-extension-foundation

**Date:** 2026-06-29
**Mode:** plan-bound:pgrx-extension-foundation
**Verdict:** PASS_WITH_CAVEATS
**Hard caps triggered:** [] (no CRITICAL/HIGH/MEDIUM CVE in the chosen dependency set after remediation)

## Summary

- Ecosystems detected: Rust (cargo) — the plan introduces a NEW `theodb_rs` crate (greenfield; no `Cargo.lock` exists yet, so the audit was run on a synthesized lockfile with the exact declared deps — measurement-first, ADR 0002).
- Auditor coverage: `cargo audit` 0.22.1 (RustSec advisory-db, 1139 advisories) RAN; `osv-scanner` available (cross-check). NOT fabricated.
- Total deps audited: 183–194 crates (full transitive tree of `pgrx =0.16.1` + HTTP crate + `pgvector` + `serde_json`).
- **Initial finding (declared `minreq` + `https`/rustls feature):** 3 vulnerabilities — all in `rustls-webpki 0.101.7` pulled transitively by `minreq 2.14.1`'s rustls-based `https` feature.
- **After remediation (`minreq` + `https-native`/OpenSSL feature):** 0 vulnerabilities; 1 informational warning (`serde_cbor` unmaintained, transitive via `pgrx`).

## Vulnerabilities (initial — before remediation)

The plan's v1.1 `## Dependencies` listed `minreq` with the rustls `https` feature. Measured on the synthesized lockfile:

### RUSTSEC-2026-0104 / 0099 / 0098 — `rustls-webpki 0.101.7` (transitive via `minreq 2.14.1` → `rustls 0.21.12`)
- **Titles:** "Name constraints were accepted for certificates asserting a wildcard name" (0104); "Name constraints for URI names were incorrectly accepted" (0099); name-constraint handling bypass (0098). Dated 2026-04-14.
- **Class:** TLS certificate name-constraint validation weakness — security-relevant for TheoDB's HTTPS embed call (and its SSRF posture).
- **Fixed in:** `rustls-webpki >=0.103.12` (a major bump of the rustls stack).
- **Root:** `minreq`'s rustls `https` feature pins an old `rustls 0.21` → old `rustls-webpki`.

## Remediation (measured — 4 candidates tested)

All four candidates were lockfile-generated + `cargo audit`-scanned. Result (rustls-webpki advisories eliminated in ALL four; only the `serde_cbor`-via-pgrx warning remains):

| Candidate | rustls-webpki CVEs | New system dep | Keeps blueprint D2 choice (minreq, ISC, minimal)? |
|---|---|---|---|
| **A — `minreq` 2.x + `https-native` (OpenSSL/native-tls)** ✅ CHOSEN | 0 | none (libssl-dev already in the builder, `Dockerfile:13`) | YES |
| B — `minreq` 3.x + `https` (newer rustls) | 0 | none | YES (but a major bump of an unreleased-in-plan version) |
| C — `ureq` 2.x | 0 | none | partial (ureq is the D2 fallback) |
| D — `ureq` 3.x | 0 | none | partial |

**Decision: Candidate A — `minreq 2.x` with the `https-native` (native-tls/OpenSSL) feature instead of the rustls `https` feature.**
- Eliminates all 3 `rustls-webpki` advisories (verified: `grep -c rustls-webpki Cargo.lock` → 0).
- OpenSSL is ALREADY a builder dependency (`libssl-dev`, scale-builder stage) — zero new system dependency.
- Keeps `minreq` (ISC, minimal) — the blueprint D2 choice — at the planned 2.x line.
- `ureq` remains the documented fallback (D2) if the no-redirect API confirmation fails.

## Outstanding caveat (PASS_WITH_CAVEATS)

### RUSTSEC-2021-0127 (`serde_cbor`) + `paste` unmaintained (WARNINGS, not CVEs)
- **Verified on the committed `theodb_rs/Cargo.lock`:** `cargo audit` → 0 vulnerabilities; 2 *allowed warnings* (`serde_cbor` unmaintained, `paste` no-longer-maintained) — both transitive via `pgrx` (the framework), in its proc-macro/test machinery, not TheoDB's request path. Same acceptance + rationale as below.
- **Path:** `serde_cbor 0.11.2 → pgrx 0.16.1 → theodb_rs`.
- **Class:** unmaintained crate advisory (informational) — NOT an exploitable CVE; cargo audit reports it as an *allowed warning*, not a vulnerability (audit exits 0 for vulns).
- **Why accepted:** `serde_cbor` is a transitive dependency of `pgrx =0.16.1` (our extension framework, pinned to match the image's `PGRX_VERSION=0.16.1` — ADR D3). It is in pgrx's schema/test machinery, not in TheoDB's runtime request path. Removing it requires upgrading/forking pgrx, which would drift from the image toolchain (rejected — ADR D3). Re-assess when pgrx upgrades its dependency tree.
- **Severity:** LOW / informational. Does not cap the verdict below PASS_WITH_CAVEATS.

## Plan validation (Mode 2)

| Plan dep | Section | On registry | Audit clean? | Rule 9 OK? | Verdict |
|---|---|---|---|---|---|
| `pgrx =0.16.1` | NEW | yes (0.16.1) | yes (only the serde_cbor warning, transitive, LOW) | yes (framework; alt = hand-rolled C ABI rejected) | OK |
| `minreq 2.x` (`https-native`) | NEW | yes (2.14.1) | **yes after remediation** (rustls→native-tls) | yes (alt reqwest/hand-rolled TCP rejected) | OK — plan must pin `https-native` feature |
| `pgvector 0.4` | NEW | yes (0.4.2, MIT) | yes | yes (alt float4[]-cast/hand-rolled protocol rejected) | OK |
| `serde_json 1.x` | NEW | yes (1.0.150) | yes | yes (alt hand-rolled JSON rejected) | OK (droppable if minreq json feature covers parsing) |
| `ureq 2.x` (fallback) | NEW (conditional) | yes (2.12.1) | yes | yes (D2 fallback) | OK as fallback |

All licenses D1-permissive: minreq ISC; pgvector MIT; pgrx Apache-2.0/MIT; serde_json Apache-2.0/MIT; ureq MIT/Apache.

## Recommended next steps

1. **Update the plan (v1.2):** in `## Dependencies` + ADR D2, pin `minreq` with the **`https-native`** feature (native-tls/OpenSSL), NOT the rustls `https` feature — with the RUSTSEC-2026-0098/0099/0104 rationale. (Applied below.)
2. At implement time (T1.1), confirm `minreq`'s no-redirect + timeout API on the native-tls path (D2 open item); `ureq` fallback if absent.
3. T5.1 re-runs `cargo audit` on the REAL committed `theodb_rs/Cargo.lock` to confirm 0 vulns before merge.
4. Proceed to `/plan-confidence`.
