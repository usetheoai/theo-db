---
type: Technique
title: Meça um braço que NÃO mudou junto com o experimento
description: Se o binário inalterado lê +122% mais rápido entre dois runs, a box domina o sinal e nenhum veredito é possível.
resource: docs/benchmarks/m46-highrecall-qps.md
tags: [benchmark, controle, rigor]
timestamp: 2026-07-30T00:00:00Z
---

# Meça um braço que **não mudou** junto com o experimento

## O padrão

Além do braço A e do braço B, rode um **terceiro braço cujo código não mudou** — tipicamente a baseline externa
(pgvector, o motor de referência). Ele mede o **piso de ruído do ambiente**.

## O caso que ensinou — M46

O experimento media o efeito de uma otimização de scan. O **controle pgvector, binário inalterado**, derivou
entre os dois runs:

| `ef` | run 1 | run 2 | deriva do **controle** |
|---|---|---|---|
| 100 | 790,1 ± 104,5 | 1016,0 ± 99,8 | **+29%** |
| 200 | 287,5 ± 26,0 | 638,1 ± 138,1 | **+122%** |
| 300 | 210,8 ± 11,2 | 519,4 ± 16,8 | **+146%** |
| 400 | 198,2 ± 18,8 | 439,5 ± 30,8 | **+122%** |

Os deltas do braço experimental eram `+21% / +6% / −12% / +18%` — **todos dentro do ruído que o controle
revelou**. Sem o controle, o `+21%` teria virado claim. Com ele, o milestone declarou honest-negative de QPS e
publicou só o que não depende de relógio (recall-neutro **provado**).

Contexto: load average **18–36 numa box de 12 cores**, 11 containers competindo.

## O segundo requisito, do mesmo caso

Para atribuir uma mudança **só de alocação**, os dois braços precisam do **grafo byte-idêntico**. Dois containers
com build **paralelo racy** constroem grafos diferentes — e o gate automático de recall reportou
`RECALL_REGRESSION` para uma diferença de **−0,0005** que era a corrida do build, não regressão de scan. O gate
não distingue as duas coisas.

## Distinto do controle positivo

| | Prova |
|---|---|
| [controle-positivo](controle-positivo.md) | que o **oráculo** consegue reprovar |
| **braço de controle inalterado** | que o **ambiente** não está produzindo o efeito |

## Relacionados

- [measurement/deriva-de-box-m168](../measurements/deriva-de-box-m168.md) — o mesmo mecanismo, medido depois
- [technique/ablacao-mesmo-indice](ablacao-mesmo-indice.md)
