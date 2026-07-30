---
type: Honest Negative
title: SymphonyQG in-PG: AM correto, gate não atingido
description: Off-PG o 1-bit co-locado dava paridade + 1,8-2,66×; dentro do PostgreSQL o hnsw continua 2,6-3,9× mais rápido em warm.
tags: [vetorial, index-am, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# SymphonyQG in-PG: AM correto, gate **não** atingido

## Os dois resultados, e a distância entre eles

| Contexto | Resultado |
|---|---|
| **off-PG** (spike, E2) | 1-bit co-locado dá **paridade de recall + 1,8-2,66×** de velocidade @ SIFT1M |
| **in-PG** (`theodb_symqg` AM) | AM correto, mas o `hnsw` é **2,6-3,9× mais rápido** em warm |

## A lição

O **page tax** do PostgreSQL — buffer manager, MVCC, layout de página — consome a vantagem que o algoritmo tem
fora dele. Um spike off-PG mede o **algoritmo**; ele não prevê o resultado in-PG.

Corolário operacional: um spike off-PG favorável **não** é gate de decisão para um AM. O gate é in-PG.

## Nota de licença

O SymphonyQG C++ de referência é **study-only**; a implementação foi own-code.

## Relacionados

- [technique/ablacao-mesmo-indice](../techniques/ablacao-mesmo-indice.md)
- [invariant/licenca-agpl-e-study-only](../invariants/licenca-agpl-e-study-only.md)
