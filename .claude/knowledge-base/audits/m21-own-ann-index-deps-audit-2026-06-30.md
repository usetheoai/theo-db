# Deps Audit: m21-own-ann-index

**Date:** 2026-06-30
**Mode:** plan-bound:m21-own-ann-index
**Verdict:** PASS_WITH_CAVEATS
**Hard caps triggered:** _none_ (no CVE on any declared dep; no plan-deps structural defect)

## Summary
- Ecosystems detected: rust (`theodb_rs/Cargo.lock`, 258 crates), python (`benchmarks/requirements.txt`)
- New deps introduced by M21: **0** (ADR D3 — `pgrx` + `std` + M20 `vec.rs` only)
- Vulnerabilities found: 0 CRITICAL, 0 HIGH, 0 MEDIUM, 0 LOW
- Unmaintained advisories (NOT CVEs): 2 — both transitive via the EXISTING `pgrx 0.16.1` framework dep, not introduced by M21
- Outdated: n/a (no new dep; pgrx pinned at the theodb_rs current version per ADR D4)
- Auditor coverage: { cargo-audit: ran (1146 advisories loaded), pip-audit: ran, osv-scanner: available (cargo-audit authoritative for Rust), govulncheck: n/a (no Go) }

## Vulnerabilities (sorted by severity)

_None._ `cargo audit` found 0 vulnerabilities across 258 crates; `pip-audit -r benchmarks/requirements.txt` → "No known vulnerabilities found".

## Advisories (informational — unmaintained, NOT vulnerabilities)

### RUSTSEC-2024-0436 — `paste@1.0.15` — unmaintained
- **Path:** `paste → pgrx-tests 0.16.1 → theodb_rs` (DEV/TEST scope only — `pgrx-tests`)
- **Severity:** warning (unmaintained), no CVE, no fix required
- **M21 relevance:** none — transitive via the existing test framework; not introduced by M21; no runtime exposure.

### RUSTSEC-2021-0127 — `serde_cbor@0.11.2` — unmaintained
- **Path:** `serde_cbor → pgrx 0.16.1 → theodb_rs`
- **Severity:** warning (unmaintained), no CVE, no fix required
- **M21 relevance:** none — transitive via the existing `pgrx` framework (ADR D4 keeps pgrx 0.16.1); not introduced by M21. Resolving it would require a pgrx upstream bump — out of M21 scope (anti-scope-creep), tracked as ecosystem hygiene.

## Plan validation (Mode 2)

| Plan dep | Section | Manifest match | Audit clean? | Rule 9 OK? | Verdict |
|---|---|---|---|---|---|
| `pgrx 0.16.1` | Existing | yes (`theodb_rs/Cargo.toml`) | yes (0 CVE; 2 transitive unmaintained warnings) | n/a (existing) | OK |
| `serde_json 1` | Existing | yes (`theodb_rs/Cargo.toml:30`) | yes | n/a | OK (not used by M21) |
| `psycopg2`/`numpy` | Existing (harness) | yes (`benchmarks/requirements.txt`) | yes (pip-audit clean) | n/a | OK |
| (no NEW deps) | New | n/a | n/a | yes (Rule 9 eval present: rand/simdeez/bincode all rejected) | OK |

## Caveats (why PASS_WITH_CAVEATS, not PASS)

Two transitive **unmaintained** advisories exist in the dependency tree (via the existing `pgrx`/`pgrx-tests`).
They are NOT CVEs and NOT introduced by M21, so they trigger no golden-rule hard cap — but they are logged here as
caveats for honesty (Rule 3). They do not block `/plan-confidence`. Both proceed.

## Recommended next steps

1. No manifest change required for M21 (zero new deps — the design goal).
2. Track the `pgrx`-transitive unmaintained advisories as ecosystem hygiene (resolved by a future pgrx bump, not M21).
3. Proceed to `/plan-confidence`.
