---
type: Measurement
title: corrida em glove-25-angular — o dataset real que decidiu o índice default
description: A primeira medição sobre embeddings reais do projeto; foi ela que inverteu a leitura do baseline sintético e escolheu o índice default por evidência.
resource: git:f7c7b93:docs/benchmarks/archive/2026-06-27-glove-25-angular.md
tags: [benchmark, dataset-real, glove, decisao-de-indice, arquivo]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: glove
    resource: git:f7c7b93:docs/benchmarks/archive/2026-06-27-glove-25-angular.md
    title: TheoDB vector benchmark — glove-25-angular
    last_modified: 2026-06-27
---

A primeira corrida do projeto sobre **embeddings reais** — uma distribuição de palavras, clusterizada, e
não gaussiana uniforme.

# Por que ela foi decisiva

O [baseline sintético](/benchmarks/archive/2026-06-27-pgvector-cosine.md) sugerira uma leitura que **esta
corrida inverteu**.

Sobre dados reais, o índice de grafo **domina em todos os eixos** — recall, throughput, tempo de build e,
crucialmente, **tamanho**. A suposta vantagem de 42% em tamanho da alternativa **desapareceu**, revelando
que ela era **artefato de alta dimensionalidade**, não propriedade do algoritmo.

O raciocínio completo, incluindo por que **nenhum dos dois benchmarks estava no envelope de projeto** da
alternativa, está na [decisão de índice](/decisions/m2-index-decision.md).

# O que ela estabeleceu como método

**Que uma decisão de índice não se toma sobre dados sintéticos.** O baseline sintético provou que o
harness funcionava e **sinalizou o próprio dataset como inadequado**; a decisão esperou o dataset real.

Foi o primeiro caso do padrão que se repetiria: **declarar o regime em que um resultado vale**, em vez de
generalizá-lo.

# Método

Semente fixa, recall com limiar de distância na semântica das suítes públicas, ground truth exato por
força bruta, melhor-de-N sobre 3 execuções.
