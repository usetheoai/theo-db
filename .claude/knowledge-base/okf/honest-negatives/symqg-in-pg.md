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

## A lição — e a causa correta do residual

> **CORRIGIDO 2026-07-30 após review.** Este conceito creditava o gap residual ao "page tax". A fonte separa duas
> coisas: o page tax foi **real, dominante e MITIGADO** (v1 dava 8,5× de perda; o fix v2 — rows contíguos,
> endereçamento aritmético — deu índice 5,66× menor, coube em `shared_buffers` e rendeu **+2,3× QPS**). O
> residual de 2,6-3,9× é **assimetria de maturidade**: scan symqg first-cut (`Vec`/hop, `HashSet` visited, heaps
> `f64`) contra um `theodb_hnsw` otimizado ao longo de M35-M46 (`with_page_item` copy-free, SIMD). Os levers
> estão nomeados e não foram perseguidos.

O **page tax** do PostgreSQL é real e caro — mas é **mitigável**, e foi mitigado. Um spike off-PG mede o
**algoritmo**; ele não prevê o in-PG, porque o in-PG carrega tanto a plataforma quanto a maturidade da
implementação.

Corolário operacional: um spike off-PG favorável **não** é gate de decisão para um AM. O gate é in-PG.

## Nota de licença

O SymphonyQG C++ de referência é **study-only**; a implementação foi own-code.

## Relacionados

- [technique/ablacao-mesmo-indice](../techniques/ablacao-mesmo-indice.md)
- [invariant/licenca-agpl-e-study-only](../invariants/licenca-agpl-e-study-only.md)
