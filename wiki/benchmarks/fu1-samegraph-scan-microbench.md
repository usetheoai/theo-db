---
type: Measurement
title: fu1 — micro-benchmark de alocação sobre o MESMO grafo
description: Isola a estratégia de alocação construindo o grafo uma vez e compartilhando-o entre os dois braços, e declara que o grafo sintético representa o workload de alocação, não o de recall.
resource: git:f7c7b93:docs/benchmarks/fu1-samegraph-scan-microbench.md
tags: [benchmark, micro-benchmark, alocacao, representatividade, isolamento]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: fu1
    resource: git:f7c7b93:docs/benchmarks/fu1-samegraph-scan-microbench.md
    title: M47 / FU-1 — same-graph scan-allocation micro-benchmark
---

**Caracterização, não competição**, do custo de **alocação** no laço quente do scan.

# O isolamento

O grafo é construído **uma vez e compartilhado pelos dois braços** — de modo que **a única variável é a
estratégia de alocação**.

**Mesmo grafo, mesma semente, mesma máquina.** Qualquer diferença medida só pode vir do que se pretendia
medir.

# A declaração de representatividade — o ponto do artefato

> O grafo é sintético. É representativo do **workload de ALOCAÇÃO** — o custo escala com o pool de busca,
> o grau dos nós e a contagem de visitas, todos reproduzidos —, **NÃO de recall**.
>
> A correção de recall do refactor é provada **à parte**, sobre um grafo REAL, por um oráculo dedicado.

Isto é exatamente o tipo de raciocínio que falta na maioria dos micro-benchmarks: **dizer para qual
propriedade o dado sintético é representativo, e onde a outra propriedade é verificada.**

Um grafo aleatório não tem a estrutura que determina recall — mas tem exatamente as dimensões que
determinam quantas alocações acontecem. **Usá-lo para o primeiro seria erro; para o segundo, é correto.**

# Ressalva de ambiente

A máquina tinha onze containers concorrentes — **não havia máquina quieta disponível** —, e o container
foi fixado em núcleos específicos como mitigação. A limitação é declarada, não contornada silenciosamente.

# Contexto

Mede o custo que [m46](/benchmarks/m46-highrecall-qps.md) removeu, e cujo ganho de QPS aquele milestone
**não conseguiu estabelecer** por contenção de ambiente.
