---
type: Measurement
title: m103 — substrato unificado vetor + colunar: o controle isolado que quantifica o pruning
description: A busca ponta a ponta é dominada pelo rerank e não consegue medir o ganho; um controle isolado mostra 77,4% de economia no decode.
resource: git:f7c7b93:docs/benchmarks/m103-vector-columnar.md
tags: [benchmark, co-residencia, column-pruning, controle-isolado, m103]
milestone: M103
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m103
    resource: git:f7c7b93:docs/benchmarks/m103-vector-columnar.md
    title: M103 — vector + columnar unified substrate
    last_modified: 2026-07-16
---

# O problema de medição, e a solução

A busca ponta a ponta é **dominada pelo rerank** — que percorre todas as linhas em probe total,
independentemente da largura do payload. Portanto **ela não consegue quantificar o ganho de column
pruning**; ela mostra apenas que o pruning **não adiciona custo dependente da largura** (razão 1,014,
dentro de um desvio).

**Um controle isolado é o que quantifica:** decodificar só as 4 colunas de índice leva **49,6 ms ± 0,3**,
contra **219,8 ms ± 1,8** decodificando todas — **77,4% do tempo de decode economizado**, bem acima do
piso de ruído.

**Quando a medição ponta a ponta é dominada por outro custo, medir o componente isoladamente é a única
forma honesta de quantificá-lo** — e reportar as duas coisas mostra por que uma não substitui a outra.

# A garantia arquitetural

O pruning é **arquiteturalmente garantido**, não medido por acaso: a projeção **pula a descompressão das
colunas não projetadas**. E é provado por teste, além de quantificado por benchmark.

# O que NÃO é reivindicado

**Recall é igual por construção — é o gate, nunca um ganho.** A prova é identidade **byte a byte** com a
busca exata filtrada em probe total, obtida reusando **o mesmo kernel e o mesmo desempate** — porque um
kernel diferente mudaria a ordem de somatório, o último bit, e portanto o desempate.

**Nenhum claim de QPS contra o adversário externo.** Co-residência **não fecha o gap de paradigma**.

E o tamanho em disco maior é reportado **separadamente**, com a nota de que **tamanho em disco não é
custo de decode** — duas grandezas que a intuição funde e que aqui são mantidas apartadas.

A decisão é o [ADR 0044](/decisions/0044-m103-vector-columnar-coresidence.md).
