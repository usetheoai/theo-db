# Code Quality Audit: m146-hardening-remediation

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

---

## Nota de proveniência (M146, #175) — por que ESTE run é o autoritativo

Produzido no e2e-runner (165.227.121.20), o **único** ambiente onde os dois detectores executam de verdade.
A distinção importa porque três falso-verdes distintos foram medidos antes de chegar aqui:

| # | Sintoma | Causa | Estado |
|---|---|---|---|
| 1 | `PASS` com "Languages audited: none" | `code-quality-languages.txt` vazio | corrigido (rust habilitado) |
| 2 | `PASS`, mas D1 nunca rodou | `cargo +nightly udeps` executado na raiz, onde não há `Cargo.toml` (o workspace é `theodb_rs/`) | corrigido (roda no diretório do manifest) |
| 3 | `PASS`, mas D2 auditou **zero** símbolos | tree-sitter ausente; o extractor degrada para lista vazia **em silêncio**, e "nenhum símbolo" era lido como "nenhum problema" | corrigido (guard de vacuidade + parser instalado) |

Provas independentes de que os detectores rodaram neste run:

- **D1** — `cargo +nightly udeps --output json --all-targets` em `theodb_rs/`:
  `{"success":true,"unused_deps":{},"note":null}`.
- **D2** — `extract_imports_and_calls` sobre `theodb_rs/src/am/columnar.rs`: 10 símbolos, 10 imports
  (antes do parser: 0 — e o guard novo converte isso em `SOFT_CAP auditor_unavailable_tree-sitter-rust`
  em vez de PASS).

O único achado é honesto e não é defeito de código: em `am/columnar.rs`, `use pg_sys::XactEvent as XE`
aparece num arquivo com `use pgrx::prelude::*`. Um glob traz nomes que nenhuma varredura estática enumera,
então o detector **não consegue provar** se `pg_sys` é um crate ou um módulo trazido pelo glob — e reporta
"não verificável" (SOFT_FLOOR) em vez de afirmar fabricação. `pg_sys` é, de fato, o módulo re-exportado
pelo `pgrx`; o código está correto.
