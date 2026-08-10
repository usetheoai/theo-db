---
type: Measurement
title: m110 — extração de grafo no banco: paridade entre linguagens como gate
description: Provar identidade byte a byte com o extrator do consumidor prova não-regressão de recall por construção, sem precisar de uma avaliação de qualidade separada.
resource: git:f7c7b93:docs/benchmarks/archive/m110-extraction.md
tags: [benchmark, grafo, extracao, paridade, gate, arquivo, m110]
milestone: M110
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m110
    resource: git:f7c7b93:docs/benchmarks/archive/m110-extraction.md
    title: M110 — in-DB graph extraction
    last_modified: 2026-07-16
---

# O gate, e por que ele é elegante

O baseline de qualidade de grafo do consumidor **é** o extrator heurístico dele. Portanto:

**provar que a extração dentro do banco é byte-idêntica à do consumidor** — somada à prova de que a
travessia devolve o mesmo conjunto alcançável — **prova que o recall a jusante não regride, por
construção**, **sem precisar de uma avaliação de qualidade separada**.

Isso é economia real de esforço com **mais** garantia, não menos: uma avaliação de qualidade daria um
número com barra de erro; a identidade byte a byte dá certeza.

O padrão só funciona porque o baseline é **determinístico**. Se o extrator do consumidor usasse um
modelo, a identidade seria impossível e a avaliação seria inevitável — e é exatamente por isso que a
qualidade da extração **por LLM** permaneceu declarada como avaliação separada no
[ADR 0048](/decisions/0048-m107-native-graph-engine-go.md).

# O que também é medido

Throughput da extração — porque mover uma etapa para dentro do banco pode preservar o resultado e piorar
o custo, e as duas coisas precisam ser verificadas.

# Contexto

A feature resultante é [grafo nativo](/features/13-grafo-nativo.md), e o fluxo completo de recuperação é
avaliado em [m111/m112](/benchmarks/archive/m111-m112-graphrag-retrieval.md) — onde o veredito honesto é
bem menos confortável.
