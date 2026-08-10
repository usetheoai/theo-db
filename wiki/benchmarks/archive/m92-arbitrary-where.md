---
type: Measurement
title: m92 — WHERE arbitrário por Custom Scan: inline domina o post-filter
description: Reusa o sub-plano bitmap nativo do planner para materializar os TIDs e pular não-membros dentro da travessia, em vez de reimplementar seleção escalar.
resource: git:f7c7b93:docs/benchmarks/archive/m92-arbitrary-where.md
tags: [benchmark, custom-scan, bitmap, filtered-ann, arquivo, m92]
dataset: SIFT1M
milestone: M92
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m92
    resource: git:f7c7b93:docs/benchmarks/archive/m92-arbitrary-where.md
    title: M92/M93 — arbitrary-WHERE filtered vector search
    last_modified: 2026-07-13
---

**Veredito: GO — inline domina o post-filter**, em **recall e QPS**, no regime seletivo em que o
post-filter passa fome.

# O mecanismo, e o que ele reusa

O nó empurra um `WHERE` escalar arbitrário para dentro do scan vetorial assim:

1. **roda o sub-plano bitmap nativo do planner** sobre o índice btree da coluna escalar;
2. materializa os TIDs correspondentes num conjunto de pertinência;
3. o primeiro estágio do scan vetorial **pula os não-membros inline**.

**A parte elegante é o passo 1: reusar o bitmap do próprio PostgreSQL** em vez de reimplementar seleção
escalar. O nó não precisa entender o predicado — só consumir o resultado de quem já entende.

# Contexto na linhagem

Isto completa o que o [ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md) declarara **fora de
escopo**: o filtro por label cobria apenas a coluna declarada, e `WHERE` arbitrário sobre coluna comum
continuava post-filtrando.

A investigação daquele milestone havia **movido** o Custom Scan para depois, por ser maquinaria pesada de
planner e executor que o critério não exigia — e aqui, quando o critério passou a exigir, ele foi
construído.

**Adiar corretamente é diferente de não fazer.**

# Complemento

O modelo de custo que decide **quando** o planner escolhe este nó sozinho é o
[m95](/benchmarks/m95-cost-model.md), e a sondagem adaptativa que ele incorpora vem do
[m91](/benchmarks/m91-adaptive-filter.md).
