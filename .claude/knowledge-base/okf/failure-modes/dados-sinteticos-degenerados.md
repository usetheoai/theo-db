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

## Corolário — e ele é sobre a hipótese de regime, não sobre o regime

> **CORRIGIDO 2026-07-30 após review.** Esta seção citava uma tripla de QPS que **não existe em artefato algum** e
> repetia a tese "a vantagem do SBQ só aparece sob pressão de RAM", que o M57 **falsificou**.

A hipótese "o regime errado responde a pergunta errada" é boa e vale — mas o SBQ é o **contraexemplo**, não o
exemplo. Ali o regime certo **foi** medido (pressão de 1,8 GB e 1,3 GB) e o resultado continuou negativo
(0,73× / 0,77×), porque o HNSW tem localidade de acesso e não expõe o gargalo de I/O que o SBQ atacaria.

É por isso que aquilo virou [honest-negative](../honest-negatives/sbq-nao-ganha-qps-em-regime-algum.md) em vez de
ressalva: a hipótese de regime era razoável, foi testada, e o teste a matou.

O que **fica** deste conceito são os dois extremos de degeneração acima — recall 1.0 com `probes=1` e recall
0,033 sem cluster —, que são fiéis e medidos.

## Como evitar

- Datasets sintéticos precisam de **estrutura declarada** (clusters, distribuição) e de uma sanidade: se o gate
  passa com o parâmetro mínimo (`probes=1`), o dataset é degenerado, não o índice é ótimo.
- Prefira dataset real (SIFT, ClickBench, BEIR) para qualquer número publicável.

## Relacionados

- [failure-mode/medicao-vacuosa-aceita](medicao-vacuosa-aceita.md)
- [honest-negative/sbq-nao-ganha-qps-em-regime-algum](../honest-negatives/sbq-nao-ganha-qps-em-regime-algum.md)
