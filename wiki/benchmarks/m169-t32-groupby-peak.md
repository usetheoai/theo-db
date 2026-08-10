---
type: Measurement
title: m169 — o pico de GROUP BY que o streaming NÃO reduz
description: Caracteriza o limite que sobra: o pico é linear na cardinalidade dos grupos, e dois milhões deles consomem 95% da pool disponível.
resource: git:f7c7b93:docs/benchmarks/m169-t32-groupby-peak.md
tags: [benchmark, memoria, group-by, cardinalidade, limite, m169]
milestone: M169
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m169t32
    resource: git:f7c7b93:docs/benchmarks/m169-t32-groupby-peak.md
    title: M169 T3.2 — o pico do GROUP BY
---

**Veredito: o pico é LINEAR na cardinalidade, e 2 milhões de grupos consomem 95,4% da pool
disponível.**

# Por que este artefato existe

O título diz o essencial: **é o pico que o streaming NÃO reduz.**

O milestone removeu a materialização proporcional ao número de **linhas**. Mas a tabela de agrupamento é
proporcional ao número de **grupos** — e essa é uma dimensão diferente, que streaming nenhum resolve,
porque **os grupos precisam existir simultaneamente para serem agregados**.

**Caracterizar o limite que sobra depois de uma otimização** é o que impede que o ganho seja lido como
"o problema foi resolvido". A memória de linhas foi resolvida; a de grupos não, e ela tem forma
diferente.

# A forma do resultado

**Linear na cardinalidade** — o que permite prever: dobrar os grupos dobra o pico, e uma query com
cardinalidade alta o suficiente falha independentemente do tamanho da tabela.

Saber a **forma** da curva vale mais que o ponto medido, porque ela extrapola. É a mesma diferença que
[m35](/benchmarks/m35-hnsw-structured-scan.md) explorou ao medir contagem de páginas em N crescente para
provar complexidade, em vez de medir um ponto.

# Contexto

Complementa o [estado final](/benchmarks/m169-t41.md) explicando parte do que ainda não completa a 100M,
e alimenta a mesma discussão de bounds que o
[ADR 0047](/decisions/0047-m104-scaling-tradeoffs-deliberate.md) organizou.
