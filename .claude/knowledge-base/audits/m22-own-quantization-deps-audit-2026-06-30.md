# Deps Audit: m22-own-quantization

**Date:** 2026-06-30
**Mode:** plan-bound:m22-own-quantization
**Verdict:** PASS_WITH_CAVEATS
**Hard caps triggered:** _none_ (no CVE on any declared dep; no plan-deps structural defect)

## Summary
- Ecosystems: rust (`theodb_rs/Cargo.lock`, 258 crates), python (`benchmarks/requirements.txt`)
- New deps introduced by M22: **0** (ADR D1/D4 — pure `std` bit ops + reuse of M20 `vec.rs` + M21 `ann/`)
- Vulnerabilities: 0 CRITICAL/HIGH/MEDIUM/LOW (cargo audit clean of CVEs; pip-audit clean — same harness as M21)
- Unmaintained advisories (NOT CVEs): 2 — `paste` (via `pgrx-tests`, dev-only) + `serde_cbor` (via `pgrx`), both
  transitive through the EXISTING pgrx framework, not introduced by M22 (identical to M20/M21)

## Critical license finding (D1 — the whole point of the M22 dep decision)

The discovery (`blueprint Q6`) found **vectorchord/RaBitQ is AGPL-3.0 / ELv2** — **forbidden** in the TheoDB
distribution (CLAUDE.md rule 2 / PRD D1). M22 therefore implements an **own SBQ-style quantizer** (independent,
permissive code learned from pgvectorscale's PostgreSQL-licensed SBQ) and **does NOT borrow RaBitQ**. The plan's
`## Dependencies › New` is `(none)` with the Rule-9 evaluation recording the AGPL rejection explicitly. Result:
**zero AGPL contamination**; the only quantization code shipped is TheoDB's own std-only implementation.

## Plan validation (Mode 2)

| Plan dep | Section | Manifest match | Audit clean? | Rule 9 OK? | Verdict |
|---|---|---|---|---|---|
| `pgrx 0.16.1` | Existing | yes | yes (0 CVE; 2 transitive unmaintained) | n/a | OK |
| `psycopg2`/`numpy` | Existing (harness) | yes | yes | n/a | OK |
| pgvectorscale `diskann` | Existing (image) | yes (theo-db image) | n/a (the SBQ baseline) | n/a | OK |
| (no NEW deps) | New | n/a | n/a | yes (Rule 9: rabitq AGPL-rejected, simdeez/rkyv rejected) | OK |

## Caveats (why PASS_WITH_CAVEATS)

Two transitive **unmaintained** advisories via the existing pgrx (not CVEs, not introduced by M22) — logged for
honesty, do not block. Proceed to `/plan-confidence`.
