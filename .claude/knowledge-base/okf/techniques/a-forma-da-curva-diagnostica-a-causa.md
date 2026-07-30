---
type: Technique
title: Um gap que é multiplicador ~constante ao longo do knob é custo por-candidato, não diferença algorítmica
description: A forma da curva separa custo fixo por candidato de diferença de algoritmo antes de qualquer profiler.
resource: docs/benchmarks/m50-sota-ruler.md
tags: [diagnostico, benchmark, performance]
timestamp: 2026-07-30T00:00:00Z
---

# Um gap que é multiplicador **~constante** ao longo do knob é custo por-candidato

## O padrão de leitura

Varra o knob (`ef_search`, `probes`, `work_mem`) e olhe a **forma** da razão entre os dois braços:

| Forma da razão | Diagnóstico |
|---|---|
| **~constante** ao longo de todo o knob | **custo fixo por candidato** — cada candidato custa mais, e o knob só muda quantos são |
| **cresce** com o knob | diferença **algorítmica** — o braço pior escala pior |
| **cai** com o knob | custo fixo de **setup** sendo amortizado |

## O caso — M50

`theodb_hnsw` vs `pgvector_hnsw`: razão de latência **1,64× ± 0,35**, medida **same-run** nos três runs e estável
ao longo do sweep. A leitura correta é *custo por-candidato fixo* — e ela apontou o trabalho (kernel de scoring,
layout de leitura) **antes** de qualquer profiler rodar.

O contraste vem do M71: o gap **iso-recall** vs pgvector **piora com a escala** (~2× o `ef` a 100k → ~5× a 500k) —
forma diferente, causa diferente (**navegabilidade do grafo**, não custo por candidato). E de fato: cortes de
custo por-candidato reduziram o p50 absoluto e **não moveram** a razão iso-recall.

> Otimizar o eixo errado melhora o número e não move o veredito.

## Por que a razão same-run é robusta

Ela é medida **dentro** do mesmo run, então o ruído do ambiente afeta os dois braços igualmente e se cancela — ao
contrário dos absolutos, que o M50 declara carregarem ruído de contenção (load 7,87 → 12,66 durante a coleta).

## Relacionados

- [technique/braco-de-controle-inalterado](braco-de-controle-inalterado.md)
- [honest-negative/superioridade-vetorial-vs-scann](../honest-negatives/superioridade-vetorial-vs-scann.md)
