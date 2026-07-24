# Code Quality Audit: m147-scan-version-dispatch

**Date:** 2026-07-24
**Mode:** plan-bound
**Verdict:** PASS_WITH_CAVEATS
**Score cap:** 89
**Hard caps triggered:** symbol_fab_unverifiable_rust

## Summary

- Languages audited: rust
- Languages skipped: _none_
- Total findings: 1 (0 HARD, 0 SOFT_CAP, 1 SOFT_FLOOR, 0 INFO)

## Findings by detector

### D1 — Dead code
_No findings._

### D2 — Symbol fabrication
| File | Symbol | Severity | Message |
|---|---|---|---|
| `root/theo-db/theodb_rs/src/am/columnar.rs` | `use pg_sys::XactEvent as XE` | SOFT_FLOOR | Could not verify 'pg_sys': not on crates.io, but this file has a glob import, so it may be a glob-imported module |

### D3 — Cross-package orphan exports
_No findings._

### D4 — Mutation testing
_No findings._

## Related

- Golden rule: [`.claude/rules/code-quality-golden-rule.md`](../../rules/code-quality-golden-rule.md)
- Allowlist: [`.claude/rules/code-quality-allowlist.txt`](../../rules/code-quality-allowlist.txt)
- Thresholds: [`.claude/rules/code-quality-thresholds.txt`](../../rules/code-quality-thresholds.txt)
