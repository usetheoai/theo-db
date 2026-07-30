---
type: Failure Mode
title: Dados sintéticos degenerados produzem recall absurdo — para cima ou para baixo
description: Vetores uniformes de alta dimensão saturam recall em 1.0 mesmo com probes=1; sem clusters, o recall despenca a 0.033. Nenhum dos dois mede o algoritmo.
tags: [benchmark, dataset, vetorial, recall]
timestamp: 2026-07-30T00:00:00Z
---

# Dados sintéticos degenerados produzem recall absurdo — para cima **ou** para baixo

## Os dois extremos, ambos medidos neste projeto

| Dataset sintético | Recall observado | Por quê |
|---|---|---|
| uniforme, alta dimensão | **satura em 1.0 mesmo com `probes=1`** | os vizinhos mais próximos ficam co-localizados; qualquer probe acerta |
| sem estrutura de cluster | **0.033** | pura degeneração — não havia vizinhança a encontrar |

Com clusters reais a 5k, **todos** atingiram recall 1.0 e o gate (≥0.99) passou. O 0.033 anterior não media
qualidade de índice; media o dataset.

## A armadilha de leitura

Os dois desfechos parecem informativos e não são:

- recall 1.0 vira "nosso índice é perfeito" — quando na verdade **nenhum** índice seria distinguível ali;
- recall 0.033 vira "nosso índice está quebrado" — e leva a caçar bug onde não há.

O sinal que importa não é o valor absoluto, e sim o **diferencial entre braços** sobre o **mesmo** dataset.

## Corolário medido — SBQ (M51)

"SBQ é mais rápido" também depende do regime: **in-RAM**, SBQ 1480 qps vs f32 1582 vs pgvector 1641 — **sem
vantagem**. A vantagem do SBQ só aparece **sob pressão de RAM**. Medir no regime errado responde a pergunta
errada com precisão.

## Como evitar

- Datasets sintéticos precisam de **estrutura declarada** (clusters, distribuição) e de uma sanidade: se o gate
  passa com o parâmetro mínimo (`probes=1`), o dataset é degenerado, não o índice é ótimo.
- Prefira dataset real (SIFT, ClickBench, BEIR) para qualquer número publicável.

## Relacionados

- [failure-mode/medicao-vacuosa-aceita](medicao-vacuosa-aceita.md)
- [honest-negative/sbq-sem-vantagem-in-ram](../honest-negatives/sbq-sem-vantagem-in-ram.md)
