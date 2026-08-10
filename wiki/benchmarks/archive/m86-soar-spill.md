---
type: Measurement
title: m86 — SOAR spill: honest-negative em QPS de cache quente
description: Implementa uma técnica publicada em ~40 linhas para atacar o gargalo de sondagem de centroides, e mede que ela não paga no regime testado.
resource: git:f7c7b93:docs/benchmarks/archive/m86-soar-spill.md
tags: [benchmark, soar, ivf, honest-negative, arquivo, m86]
dataset: SIFT1M
milestone: M86
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m86
    resource: git:f7c7b93:docs/benchmarks/archive/m86-soar-spill.md
    title: M86 — pg_scann SOAR spill
    last_modified: 2026-07-12
---

**Veredito: honest-negative** no regime medido — QPS em cache quente sobre dataset real.

# A técnica

Cada vetor é **derramado para uma segunda lista**, escolhida por uma função de perda que penaliza a
componente do resíduo alinhada ao primeiro centroide:

$$ L(c') = \lVert v - c' \rVert^2 + \lambda \cdot \frac{\langle v - c', r \rangle^2}{\lVert r \rVert^2}, \quad r = v - c_1 $$

A intuição: uma query que sonda **menos** listas ainda encontra o vetor, porque ele está em duas — o que
ataca diretamente o gargalo de **sondagem de centroides** que a linhagem vinha identificando.

# O custo de testar

**~40 linhas** mais uma reloption, reusando a deduplicação por identificador que o scan já fazia —
**sem mudança no scan**.

**Testar uma hipótese publicada por 40 linhas** é o cenário ideal para um gate: o custo de descobrir que
não paga é quase zero.

# O resultado

Não entrega ganho no regime medido. Como os outros negativos da série, a implementação fica registrada e
o eixo é descartado como caminho de performance.

Vale notar que a mesma técnica é referenciada no
[dossiê de pesquisa](/references/scann-storage-separation-2026-07.md) como **alavanca ortogonal** de
recall por probe — ou seja, ela pode pagar noutro eixo que não o testado aqui, e isso está dito em vez de
ser esquecido.
