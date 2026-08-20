# Code Quality Audit: b031-b030-uma-extensao

**Date:** 2026-08-12
**Mode:** plan-bound
**Verdict:** FAIL_SOFT
**Score cap:** 70
**Hard caps triggered:** auditor_unavailable_cargo-udeps

## Summary

- Languages audited: rust
- Languages skipped: _none_
- Total findings: 3 (0 HARD, 1 SOFT_CAP, 0 SOFT_FLOOR, 2 INFO)

## Findings by detector

### D1 — Dead code
| File | Symbol | Severity | Message |
|---|---|---|---|
| `.` | `cargo-udeps` | SOFT_CAP | cargo-udeps auditor unavailable: cargo-udeps exit 101: Checking stable_deref_trait v1.2.1
   Compiling zstd-sys v2.0.16+zstd.1.5.7
   Compiling zstd-safe v7.2.4
    Checking aho-corasick v1.1.4
    Checking uuid v1.23.4
   Compiling crunchy v0.2.4
    Che |

### D2 — Symbol fabrication
| File | Symbol | Severity | Message |
|---|---|---|---|
| `.` | `d2` | INFO | D2 disabled by --no-network flag |

### D3 — Cross-package orphan exports
_No findings._

### D4 — Mutation testing
_No findings._

### D5 — architecture

| File | Symbol | Severity | Message |
|---|---|---|---|
| `.` | `layered-crate` | INFO | no architecture rules declared (layered-crate config not found: Layerfile.toml). D5 skipped — a rule the repo did not declare is not a rule Squad may enforce. |

## Related

- Golden rule: [`.claude/rules/code-quality-golden-rule.md`](../../rules/code-quality-golden-rule.md)
- Allowlist: [`.claude/rules/code-quality-allowlist.txt`](../../rules/code-quality-allowlist.txt)
- Thresholds: [`.claude/rules/code-quality-thresholds.txt`](../../rules/code-quality-thresholds.txt)
