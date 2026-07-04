# Deps Audit: m46-hnsw-highrecall-qps

**Date:** 2026-07-04
**Mode:** plan-bound:m46-hnsw-highrecall-qps
**Verdict:** PASS
**Hard caps triggered:** [] (nenhum)

## Summary
- Ecosystems detected: Rust (`theodb_rs/Cargo.toml` + `Cargo.lock`)
- Deps introduzidas pelo plano: **0** (§ Dependencies § New = none — parsimony rung 5, pre-size com
  `std::collections` hasher default; âncora pgvectorscale `graph/mod.rs:109-111`)
- Vulnerabilidades (CVE): 0 CRITICAL, 0 HIGH, 0 MEDIUM, 0 LOW
- Warnings (unmaintained, não-CVE): 2 — ambos pré-existentes e transitivos via `pgrx`
- Auditor coverage: { cargo-audit 0.22.1: ran, osv-scanner: available }

## Warnings (não-bloqueantes, pré-existentes, fora do escopo M46)

### RUSTSEC-2024-0436 — `paste 1.0.15` unmaintained
- **Tipo:** unmaintained (não é CVE de severidade; sem fix disponível — crate arquivado).
- **Path:** `paste` → `pgrx-tests 0.16.1` → `theodb_rs` (dev-dependency transitiva).
- **Escopo M46:** nenhum. O M46 não toca `Cargo.toml`; é dependência da framework de testes do pgrx.

### RUSTSEC-2021-0127 — `serde_cbor 0.11.2` unmaintained
- **Tipo:** unmaintained (não é CVE de severidade).
- **Path:** `serde_cbor` → `pgrx 0.16.1` → `theodb_rs` (transitiva da framework pgrx).
- **Escopo M46:** nenhum. Resolvido a montante quando o pgrx atualizar; fora do controle deste milestone.

## Plan validation (Mode 2)

| Plan dep | Section | Manifest match | Audit clean? | Rule 9 OK? | Verdict |
|---|---|---|---|---|---|
| (nenhuma) | New = none | n/a — plano usa `std::collections` | n/a | sim (FxHash/ahash/smallvec avaliados e rejeitados por parsimony) | OK |

## Recommended next steps

1. Nenhuma ação de dependência — o M46 não introduz nem atualiza dep.
2. Os 2 warnings de unmaintained são dívida transitiva do pgrx, pré-existente, não-bloqueante; endereçados
   quando o pgrx bumpar (fora do escopo M46). Não requerem allowlist (são warnings, não CVEs).
3. Prosseguir com `/plan-confidence`.
