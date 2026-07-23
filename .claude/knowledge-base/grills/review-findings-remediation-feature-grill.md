---
slug: review-findings-remediation
generated_by: roadmap-feature
date: 2026-07-23
status: completed
milestone_id: M144
---

# Grill — review-findings-remediation (M144)

Fonte dos achados: loop-code-review full de `theodb_rs/` (2026-07-23) — `.claude/knowledge-base/audits/theodb-rs-code-review-2026-07-23.md` + `code-review-output/code-review.db (local, gitignored)` (100 findings, 90/90 arquivos, 3 gates passed).

## Q1 — O que é e por que agora?

Remediação P0+P1 dos findings de correção/segurança do review. Por que agora: o loop achou 3 HIGH acionáveis que atingem usuários reais do binário shipado — (1) segurança: `symqg_spike_bench` executável por PUBLIC lê path arbitrário do servidor (`src/bench_symqg.rs:48`, `sql/theodb_rs--1.0.0--1.1.0.sql:340` sem REVOKE); (2) quebra de promessa: cadeia de upgrade congelada em 1.1.0 — superfície lakehouse M143 (`read_parquet`/`write_parquet`/`olap`) inalcançável via `ALTER EXTENSION theodb_rs UPDATE`; (3) PII: `_vectorizer_process_delete` engole o Result do SPI (`let _ =`, `src/vectorizer.rs:460`) — delete falho é marcado done e o embedding do dado apagado permanece pesquisável.

## Q2 — Dependências

M143 `[x]` — única dependência real (a superfície lakehouse que a cadeia de upgrade precisa expor já existe no binário). M141 (dogfood) NÃO bloqueia.

## Q3 — DoD

1. Upgrade 1.1.0→1.2.0 expõe `read_parquet`/`write_parquet`/`olap` via `ALTER EXTENSION theodb_rs UPDATE`, provado instalando 1.1.0 num PG limpo e fazendo upgrade (harness no droplet).
2. `symqg_spike_bench` com `REVOKE FROM PUBLIC` + teste negativo de role comum.
3. `_vectorizer_process_delete` propaga erro do SPI — teste RED primeiro provando que delete falho NÃO é marcado done.
4. MEDIUMs P1 fechados com teste cada: PRE_COMMIT flush vs DROP TABLE mesma-txn (`columnar.rs:193`), `sanitize_error_text` Unicode length-changing (`vectorizer.rs:742`), retry com backoff (`vectorizer.rs:285`), guard no cast u32 do CSR (`graph.rs:314`).
5. CHANGELOG + suíte verde. (Absorve os 6 test gaps da fase 4 por construção — TDD.)

## Q4 — Riscos novos

1. Script de upgrade sobre catálogos existentes é traiçoeiro (lição M137: pgrx não gera upgrade script; regex-anchoring; corrupção silenciosa de shell type). Mitigação: harness de upgrade real no droplet.
2. Propagar o erro do delete pode reter jobs em retry infinito se o erro for permanente. Mitigação: dead-letter após N tentativas (mecanismo já existe na fila M122).

## Cross-check fora-de-escopo

Sem overlap com `## Fora de escopo do v2` (remediação interna de código próprio).

## SOTA delta

Não — remediação de código próprio; referências existentes suficientes.
