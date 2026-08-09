---
type: Measurement
title: m175 — o planner escolhe um plano 91× mais lento porque o custo do índice HNSW está superestimado em 94×
description: A 20 mil linhas o índice responde em 2 ms e o seq scan em 182 ms, mas o modelo de custo estima o índice como 94× mais caro — então ele nunca é escolhido sem intervenção manual.
resource: benchmarks/artifacts/m175/planner-cost-inversion.json
tags: [benchmark, m175, planner, cost-model, hnsw, defeito, dogfood, bloqueia-migracao]
milestone: M175
generated: { by: claude-code/opus-5, at: 2026-08-09T12:00:00Z }
sources:
  - id: inv
    resource: benchmarks/artifacts/m175/planner-cost-inversion.json
    title: EXPLAIN ANALYZE dos dois planos, 20k linhas, vector(1536)
---

Achado ao verificar o drop-in do [dogfood](/benchmarks/m184-pilares-superficie-medida-verdict.md) — o
`theo-rag` migrando do pgvector para o TheoDB. **Não era o que se procurava, e é mais grave que o que se
procurava.**

# A medição

20 000 linhas, `vector(1536)`, índice criado por sintaxe pgvector
(`USING hnsw (vector vector_cosine_ops)`), `ANALYZE` rodado:

| plano | custo estimado | **tempo real** |
|---|---|---|
| default — `Sort` + `Seq Scan` | 830,19..880,19 | **182,117 ms** |
| forçado — `Index Scan using chunks_vector_idx` | **3 404,25..83 080,00** | **1,994 ms** |

**O índice é 91× mais rápido e é estimado como 94× mais caro.** O modelo de custo está invertido, e o
planner escolhe sistematicamente o plano pior.

# Por que isto importa mais que um número de benchmark

**Todo usuário que criar um índice vetorial recebe um índice que nunca é usado** — a menos que saiba
executar `SET enable_seqscan=off; SET enable_sort=off`, que não está em nenhum caminho documentado de
uso normal.

O [runbook de diagnóstico](/runbooks/vector-scan-diagnostics.md) abre dizendo exatamente isto:

> **A causa nº 1 de "recall ou latência ruim" NÃO é o `ef` — é o planner não escolher o índice.**

O runbook trata isso como erro de configuração do usuário. **A medição mostra que é o comportamento
default do produto** em escala onde o índice é decisivamente melhor.

**Bloqueia o dogfood.** O PR [usetheoai/theo-rag#206](https://github.com/usetheoai/theo-rag/pull/206)
migra o `theo-rag` para o TheoDB, e o drop-in funciona — mas o índice que ele criaria não seria usado. A
migração entregaria buscas 91× mais lentas que o esperado, sem erro nenhum que denunciasse.

**Explica um tropeço anterior.** Duas tentativas de perfilar a busca do SymQG não produziram amostra
([m184](/benchmarks/m184-symqg-profile-simbolos-verdict.md)) porque o planner não usava o índice — eu
tratei como detalhe de bancada. Era este defeito.

# O que NÃO foi testado

- **Se ocorre com `theodb_ivfflat`.** Só o `theodb_hnsw` foi medido.
- **Outras dimensões e escalas.** Um ponto: 20k × 1536d. A 500 linhas o `Seq Scan` é a escolha
  *correta*, então existe um cruzamento entre os dois — **onde ele está não foi medido**.
- **A causa no código.** `am/cost.rs` existe e não foi lido. Este artefato mede o sintoma, não a origem.
- **Se o pgvector real acerta no mesmo cenário.** Sem esse controle, não se pode afirmar que o
  comportamento diverge do que o ecossistema espera — só que diverge do que a física da consulta manda.

# Relacionados

- O runbook que trata o sintoma como erro do usuário: [diagnóstico do query vetorial](/runbooks/vector-scan-diagnostics.md)
- O dogfood que este defeito bloqueia: manifesto em `.claude/knowledge-base/dogfood/manifest.md`
- O perfil cujo tropeço isto explica: [SymQG com símbolos](/benchmarks/m184-symqg-profile-simbolos-verdict.md)
