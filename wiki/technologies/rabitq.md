---
type: Technology
title: RaBitQ
description: O quantizador vetorial permissivo do estado da arte — 1 bit, sem treino de codebook, com erro provado; e a alavanca que o projeto mediu como ganho de memória, não de QPS.
resource: https://arxiv.org/abs/2405.12497
tags: [tecnologia, quantizacao, memoria, algoritmo, honest-negative]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: rabitq-paper
    resource: https://arxiv.org/abs/2405.12497
    title: Gao & Long, RaBitQ
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O RaBitQ é um esquema de quantização vetorial com três propriedades incomuns juntas: **1 bit por
dimensão**, **sem treino de codebook** — o que elimina a etapa cara e dependente de dados — e com
**bound de erro provado**, permitindo em alguns regimes dispensar o refinamento exato.[^rabitq-paper] Uma
rotação aleatória prévia distribui a informação entre as dimensões.

# Papel neste acervo

Era a alavanca **não-refutada** que restava, depois que duas tentativas de quantização sobre o carrier de
grafo caíram por medição. O core foi **vendorizado** de uma implementação permissiva
([ADR 0032](/decisions/0032-vendor-rabitq-rs-core.md)) — copiando **apenas o núcleo algorítmico** e
descartando a camada de storage, que era incompatível com um access method do PostgreSQL.

# O veredito

**A alavanca é viável, e o ganho medido é memória, não QPS**
([ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md)).

A 1M vetores de alta dimensão, ele mede **competitivo** com precisão plena em latência — **não** os ~25×
que separavam o projeto do [ScaNN](/technologies/scann.md) —, com **5,3 MB residentes** na variante em
disco. Dentro do banco, mede **3,28× menor a paridade de recall**
([e1](/benchmarks/e1-rabitq-inpg-verdict.md)).

**A consequência foi não construir o access method completo**: fazê-lo só para igualar a latência já
existente seria esforço sem retorno no eixo perseguido.

# O que ele estabeleceu sobre o pilar

Como **o melhor quantizador permissivo do estado da arte não reproduz o gap**, o veredito de que superar
o adversário em QPS é inalcançável por extensão permissiva deixa de ser sobre uma implementação e passa a
ser sobre **paradigma** — o argumento central do
[ADR 0035](/decisions/0035-m73-northstar-vector-verdict.md).

# Situação do código

O algoritmo existe como **reimplementação própria**; a árvore vendorizada original, que nunca chegou a
compilar dentro do projeto, foi objeto do [ADR 0046](/decisions/0046-rabitq-vendor-tree-deleted.md). A
validação matemática do estimador está em
[validação do estimador](/benchmarks/archive/rabitq-estimator-validation.md).

[^rabitq-paper]: Gao & Long, RaBitQ
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
