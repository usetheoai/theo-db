---
type: Measurement
title: m57 — superioridade do SBQ falsificada em escala com pressão de RAM
description: Mede o que a régua anterior não podia — o regime com pressão de memória — e derruba a tese; inclui o detalhe metodológico de constranger a RAM ENTRE o build e a medição.
resource: git:f7c7b93:docs/benchmarks/m57-sbq-superiority.md
tags: [benchmark, sbq, pressao-de-ram, honest-negative, metodologia, m57]
milestone: M57
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m57
    resource: git:f7c7b93:docs/benchmarks/m57-sbq-superiority.md
    title: M57 — SBQ-inline superiority verdict
---

**Veredito: HONEST-NEGATIVE.** A tese de que o access method próprio se justificava porque a quantização
entregaria **≥2× de QPS** sob pressão de memória está **FALSIFICADA por medição**. O caminho quantizado é
recall-neutro mas **consistentemente mais lento — nunca mais rápido** —, in-RAM e sob pressão.

# O detalhe metodológico que torna a medição possível

**A RAM é constrangida ENTRE o build e a medição**, com o harness dividido em duas fases.

A razão é concreta: **um build de grafo precisa de memória de manutenção que o estado constrangido não
daria**. Sem a divisão, o experimento mediria a impossibilidade de construir o índice, não o
comportamento dele sob pressão.

Depois do build, a memória do container é reduzida e os caches são limpos — só então a medição roda.

# Duas outras escolhas que mudam o resultado

**Os dados são gaussianos de mistura, e não gaussianos puros** — porque **o gaussiano puro é degenerado
para busca aproximada**, dando recall arbitrário. Escolher a distribuição que não trivializa o problema é
pré-requisito de qualquer conclusão.

**A máquina é dedicada e limpa**, com carga verificada abaixo de um limiar — a lição do
[m46](/benchmarks/m46-highrecall-qps.md) mecanizada.

# O mecanismo por trás do negativo

Os números e a explicação — que o grafo tem **localidade de acesso**, de modo que a precisão plena **não
thrasha** mesmo excedendo a RAM, e que o read path quantizado é **mais caro por query** e **piora com a
escala** — estão no [ADR 0018](/decisions/0018-m57-sbq-inline-not-superior.md).

**O mecanismo é o que faz o resultado generalizar** além do dataset medido, e é por isso que ele
reenquadrou o trabalho seguinte para o eixo anisotrópico ([m59](/benchmarks/m59-anisotropic-ah.md)).
