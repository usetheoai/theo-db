# Code Quality Audit: m168-streaming-topk

**Date:** 2026-07-29
**Mode:** plan-bound
**Verdict:** PASS_WITH_CAVEATS
**Score cap:** 89
**Hard caps triggered:** symbol_fab_unverifiable_rust

## Summary

- Languages audited: rust
- Languages skipped: _none_
- Total findings: 2 (0 HARD, 0 SOFT_CAP, 2 SOFT_FLOOR, 0 INFO)

## Findings by detector

### D1 — Dead code
_No findings._

### D2 — Symbol fabrication
| File | Symbol | Severity | Message |
|---|---|---|---|
| `root/theo-db/theodb_rs/src/am/columnar_project.rs` | `use pg_sys::XactEvent as XE` | SOFT_FLOOR | Could not verify 'pg_sys': not on crates.io, but this file has a glob import, so it may be a glob-imported module |
| `root/theo-db/theodb_rs/src/am/columnar.rs` | `use pg_sys::XactEvent as XE` | SOFT_FLOOR | Could not verify 'pg_sys': not on crates.io, but this file has a glob import, so it may be a glob-imported module |

### D3 — Cross-package orphan exports

**NÃO EXECUTADO.** Este detector é contratado pelo golden rule § 5, mas não tem ponto de invocação neste
orquestrador (`detect_orphan_exports` levanta `NotImplementedError` em todos os detectores). A tabela vazia
abaixo significa "não medido", **não** "nada encontrado" — M146, review F-arch-4.

_No findings._

### D4 — Mutation testing

**NÃO EXECUTADO.** Idem D3: contratado pelo golden rule § 5, sem ponto de invocação. Para Rust e Go o próprio
golden rule declara o detector DEFERIDO (ADR T4.3); para Python e TypeScript ele deveria rodar e não roda.
Tabela vazia = não medido — M146, review F-arch-4.

_No findings._

## Related

- Golden rule: [`.claude/rules/code-quality-golden-rule.md`](../../rules/code-quality-golden-rule.md)
- Allowlist: [`.claude/rules/code-quality-allowlist.txt`](../../rules/code-quality-allowlist.txt)
- Thresholds: [`.claude/rules/code-quality-thresholds.txt`](../../rules/code-quality-thresholds.txt)
