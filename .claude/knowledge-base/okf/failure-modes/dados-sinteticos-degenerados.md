---
type: Failure Mode
title: Dados sintéticos degenerados produzem recall absurdo — para cima ou para baixo, e nenhum dos dois mede o índice
description: Vetores uniformes de alta dimensão saturam recall em 1.0 mesmo com probes=1 — qualquer índice é indistinguível ali. O valor absoluto não mede o algoritmo; só o diferencial entre braços sobre o MESMO dataset mede.
tags: [benchmark, dataset, vetorial, recall]
timestamp: 2026-07-30T00:00:00Z
---

# Dados sintéticos degenerados produzem recall absurdo — para cima **ou** para baixo

## Os dois extremos, ambos medidos neste projeto

| Dataset sintético | Recall observado | Por quê |
|---|---|---|
| uniforme, alta dimensão | **satura em 1.0 mesmo com `probes=1`** | os vizinhos mais próximos ficam co-localizados; qualquer probe acerta |
| sem estrutura de cluster | recall **despenca** | pura degeneração — não havia vizinhança a encontrar |

Com clusters reais a 5k, **todos** atingiram recall 1.0 e o gate (≥0.99) passou.

> **CORRIGIDO 2026-07-30 (round 3).** Este conceito publicava **`0.033`** como o recall do caso sem cluster, em
> quatro lugares, e a seção de baixo o certificava como "fiel e medido". **Esse número não existe em artefato
> algum do repositório** — busca exaustiva em `docs/` e `benchmarks/` só devolve `0.0333` como **tempo** (cold
> seconds) em artefatos ClickBench, e o menor recall de qualquer artefato vetorial é `0.0634`. Veio de
> transcript, sem âncora. O **fenômeno** é real e a lição vale; o número foi removido em vez de inventar fonte.

## A armadilha de leitura

Os dois desfechos parecem informativos e não são:

- recall 1.0 vira "nosso índice é perfeito" — quando na verdade **nenhum** índice seria distinguível ali;
- recall quase-zero vira "nosso índice está quebrado" — e leva a caçar bug onde não há.

O sinal que importa não é o valor absoluto, e sim o **diferencial entre braços** sobre o **mesmo** dataset.

## Corolário — e ele é sobre a hipótese de regime, não sobre o regime

> **CORRIGIDO 2026-07-30 após review.** Esta seção citava uma tripla de QPS que **não existe em artefato algum** e
> repetia a tese "a vantagem do SBQ só aparece sob pressão de RAM", que o M57 **falsificou**.

A hipótese "o regime errado responde a pergunta errada" é boa e vale — mas o SBQ é o **contraexemplo**, não o
exemplo. Ali o regime certo **foi** medido (pressão de 1,8 GB e 1,3 GB) e o resultado continuou negativo
(0,73× / 0,77×), porque o HNSW tem localidade de acesso e não expõe o gargalo de I/O que o SBQ atacaria.

É por isso que aquilo virou [honest-negative](../honest-negatives/sbq-nao-ganha-qps-em-regime-algum.md) em vez de
ressalva: a hipótese de regime era razoável, foi testada, e o teste a matou.

O que **fica** deste conceito é o extremo com âncora — recall 1.0 com `probes=1` sobre dado uniforme de alta
dimensão — e a regra que ele sustenta: **o valor absoluto não mede o algoritmo; só o diferencial entre braços
sobre o mesmo dataset mede.**

## Como evitar

- Datasets sintéticos precisam de **estrutura declarada** (clusters, distribuição) e de uma sanidade: se o gate
  passa com o parâmetro mínimo (`probes=1`), o dataset é degenerado, não o índice é ótimo.
- Prefira dataset real (SIFT, ClickBench, BEIR) para qualquer número publicável.

## Relacionados

- [failure-mode/medicao-vacuosa-aceita](medicao-vacuosa-aceita.md)
- [honest-negative/sbq-nao-ganha-qps-em-regime-algum](../honest-negatives/sbq-nao-ganha-qps-em-regime-algum.md)
