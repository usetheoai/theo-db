---
type: Measurement
title: m140.1 — motor BM25 próprio contra as duas alternativas, medido
description: Passa o gate em dois eixos independentes e reproduz um resultado anterior, com a magnitude do ganho declarada como modesta.
resource: git:f7c7b93:docs/benchmarks/m140-1-lexical-measurement.md
tags: [benchmark, bm25, lexical, tantivy, storage, m140]
milestone: M140.1
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m1401
    resource: git:f7c7b93:docs/benchmarks/m140-1-lexical-measurement.md
    title: M140.1 — BM25 own-engine measured
    last_modified: 2026-07-22
---

**Manchete honest-positive, com magnitude honesta:** o gate **passa** — o motor próprio bate o baseline
embarcado em **retrieval lexical puro**, que é o caso de uso do consumidor real, **em dois eixos
independentes**, **reproduzindo** um resultado anterior.

# As três qualificações que importam

**"Retrieval lexical puro"** delimita o regime — e não é o regime em que o produto opera por padrão, que
é a fusão. É exatamente por isso que [m138](/benchmarks/m138-bm25-fusion.md) chega a conclusão diferente:
**o eixo isolado e a fusão medem coisas distintas.**

**"Dois eixos independentes"** — qualidade de ranking e tamanho de índice — é mais forte que um eixo só,
porque um ganho que aparece em duas dimensões não correlacionadas é menos provável de ser artefato.

**"Reproduzindo um resultado anterior"** é a checagem que raramente se faz: **a medição nova concorda com
a antiga**, o que valida o harness além do resultado.

# A decisão de storage que ela ancora

O achado de que o índice próprio é **menor** — e não maior — é o que derrubou o argumento clássico a
favor de construir um access method dedicado, levando ao
[ADR 0052](/decisions/0052-m140-1-lexical-storage-decision.md): **heap, não access method próprio**.

Um benchmark que muda uma decisão de arquitetura ao **falsificar a intuição dominante** vale mais que um
que confirma o esperado.

# Contexto

A engine de produção resultante é [m140.3](/benchmarks/m140-3-bm25-engine.md); a robustez,
[m140.4](/benchmarks/m140-4-robustness-consumer.md); e o estado embarcado — **fora do binário
default** — está em [motor lexical BM25](/features/18-motor-lexical-bm25.md).
