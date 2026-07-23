---
slug: cc-hotspots-refactor
generated_by: roadmap-feature
date: 2026-07-23
status: completed
milestone_id: M145
---

# Grill — cc-hotspots-refactor (M145)

Fonte: loop-code-review full de `theodb_rs/` (2026-07-23) — lizard 1.23.0 sobre 1359 fns; `code-review-output/audit/lizard_rust.csv (local, gitignored)`.

## Q1 — O que é e por que agora?

Refactor dos 4 hotspots de complexidade ciclomática julgados refactor-worthy pelo relatório (dos 15 com CC>25; o resto é complexidade essencial de engine, aceita): `admit` CC=59 (`columnar_agg.rs:250`), `theodb_embed_worker_main` CC=41 (`vectorizer.rs:797`), `write_parquet_impl` CC=35 (`parquet.rs:174`), `main_index_pages` CC=34 (`page/mod.rs:562`). Por que agora: o M144 mexe exatamente nesses arquivos (vectorizer, parquet) — refatorar DEPOIS dos fixes, com os testes novos no lugar, é mais seguro. Esforço≠Complexidade: só os alvos com ganho real de legibilidade.

## Q2 — Dependências

M144 — os fixes e seus testes de regressão precisam estar no lugar antes do refactor tocar os mesmos arquivos (evita conflito; anti-pattern do cycle-roadmap de dois milestones no mesmo módulo).

## Q3 — DoD

1. Os 4 alvos decompostos com CC ≤ 25 medido por re-run do lizard (mesmo comando do audit).
2. Comportamento preservado: suíte verde + A/B byte-idêntico in-PG para o caminho Agg-swap do `admit` (como M115).
3. Zero mudança de superfície SQL (mesmas assinaturas pg_extern).
4. Válvula honest-negative: se um alvo não ganhar legibilidade real ao decompor, registrar honest-negative e aceitar a CC (anti-sunk-cost), com justificativa no implementation log.
5. CHANGELOG.

## Q4 — Riscos novos

1. Refactor do `admit` pode regredir o caminho Agg-swap byte-idêntico do M115. Mitigação: A/B in-PG obrigatório no DoD.
2. Churn sem valor se a CC for essencial (só mover complexidade de lugar). Mitigação: válvula honest-negative no DoD.

## Cross-check fora-de-escopo

Sem overlap (refactor interno, comportamento preservado).

## SOTA delta

Não.
