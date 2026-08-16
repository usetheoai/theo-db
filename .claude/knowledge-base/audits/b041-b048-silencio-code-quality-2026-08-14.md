# Code Quality Audit: b041-b048-silencio

**Date:** 2026-08-14
**Mode:** plan-bound
**Verdict:** PASS_WITH_CAVEATS
**Score cap:** 89
**Hard caps triggered:** symbol_fab_unverifiable_rust

## Summary

- Languages audited: rust
- Languages skipped: _none_
- Total findings: 3 (0 HARD, 0 SOFT_CAP, 2 SOFT_FLOOR, 1 INFO)

## Findings by detector

### D1 — Dead code
_No findings._

### D2 — Symbol fabrication
| File | Symbol | Severity | Message |
|---|---|---|---|
| `home/paulo/Projetos/theo/theo-platform/theo-db/theodb_rs/src/am/columnar.rs` | `use pg_sys::XactEvent as XE` | SOFT_FLOOR | Could not verify 'pg_sys': not on crates.io, but this file has a glob import, so it may be a glob-imported module |
| `home/paulo/Projetos/theo/theo-platform/theo-db/theodb_rs/src/am/columnar_project.rs` | `use pg_sys::XactEvent as XE` | SOFT_FLOOR | Could not verify 'pg_sys': not on crates.io, but this file has a glob import, so it may be a glob-imported module |

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
