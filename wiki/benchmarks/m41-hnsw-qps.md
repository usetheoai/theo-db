---
type: Measurement
title: m41 — otimização de QPS do scan HNSW, com a correção do próprio número
description: Uma corrida única sugeriu 2,4–3,0×; o A/B alternado de 4 amostras na mesma janela térmica corrigiu para 1,2–1,5× — e o documento explica por que o primeiro número estava errado.
resource: git:f7c7b93:docs/benchmarks/m41-hnsw-qps.md
tags: [benchmark, hnsw, qps, variancia, ab-test, autocorrecao, m41]
milestone: M41
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m41
    resource: git:f7c7b93:docs/benchmarks/m41-hnsw-qps.md
    title: M41 — theodb_hnsw scan QPS optimization
    last_modified: 2026-07-03
---

**Veredito: ganho modesto e real** — QPS de scan **1,2–1,5×** a **recall byte-idêntico**, com o ganho
crescendo com `ef_search` (1,46× no ponto mais alto, com significância).

# A nota de honestidade que abre o documento

**Uma comparação inicial, de corrida única entre sessões, mostrou 2,4–3,0×.** Esse número estava
**inflado por variância de throttling** — a mesma armadilha que os artefatos anteriores documentaram:
**uma corrida favorável não é evidência**.

Um **A/B alternado de 4 amostras**, com baseline e otimizado medidos costas com costas **na mesma janela
térmica**, dá o número real.

**O gate de recall não foi afetado — apenas o multiplicador de QPS foi corrigido para baixo.** Separar o
que a correção atinge do que ela não atinge é o que impede a revisão de virar desconfiança geral.

# A mudança

A travessia passa a decodificar e pontuar cada nó **dentro do escopo da página fixada**, com liberação
garantida por RAII, em vez de copiar os bytes do item para fora a cada nó. E a contagem de blocos passa a
ser cacheada uma vez por query, em vez de duas vezes por nó.

**A motivação veio de medição:** o [m40](/benchmarks/m40-carrier.md) mediu o grafo mais lento que as
listas invertidas **porque as listas amortizam o custo de fixar e travar a página sobre muitos vetores**,
enquanto o grafo pagava custo fixo por nó.

Ou seja: uma medição de carrier explicou **por que** um era mais lento, e essa explicação virou a
otimização.
