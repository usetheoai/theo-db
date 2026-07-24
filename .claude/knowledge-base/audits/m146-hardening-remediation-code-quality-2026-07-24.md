# Code Quality Audit: m146-hardening-remediation

**Date:** 2026-07-24
**Mode:** plan-bound
**Verdict:** FAIL_SOFT
**Score cap:** 70
**Hard caps triggered:** auditor_unavailable_cargo-udeps, symbol_fab_unverifiable_rust

## Summary

- Languages audited: rust
- Languages skipped: _none_
- Total findings: 2 (0 HARD, 1 SOFT_CAP, 1 SOFT_FLOOR, 0 INFO)

## Findings by detector

### D1 — Dead code
| File | Symbol | Severity | Message |
|---|---|---|---|
| `.` | `cargo-udeps` | SOFT_CAP | cargo-udeps auditor unavailable: cargo-udeps exit 101: error: no such command: `udeps`

help: view all installed commands with `cargo --list`
help: find a package to install `udeps` with `cargo search cargo-udeps` |

### D2 — Symbol fabrication
| File | Symbol | Severity | Message |
|---|---|---|---|
| `home/paulo/Projetos/usetheo/theo-data/theo-db/theodb_rs/src/am/columnar.rs` | `use pg_sys::XactEvent as XE` | SOFT_FLOOR | Could not verify 'pg_sys': not on crates.io, but this file has a glob import, so it may be a glob-imported module |

### D3 — Cross-package orphan exports
_No findings._

### D4 — Mutation testing
_No findings._

## Related

- Golden rule: [`.claude/rules/code-quality-golden-rule.md`](../../rules/code-quality-golden-rule.md)
- Allowlist: [`.claude/rules/code-quality-allowlist.txt`](../../rules/code-quality-allowlist.txt)
- Thresholds: [`.claude/rules/code-quality-thresholds.txt`](../../rules/code-quality-thresholds.txt)
