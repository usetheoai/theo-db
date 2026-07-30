---
type: Failure Mode
title: Comparar dois retrievers cujos candidate-sets têm semânticas diferentes mede o filtro, não o ranker
description: Três instâncias pagas: corpus menor que o top-k, filtro booleano que dropa 93% dos relevantes, e parser AND vs OR.
resource: docs/benchmarks/m53-hybrid-beir.md
tags: [retrieval, benchmark, metrica]
timestamp: 2026-07-30T00:00:00Z
---

# Comparar dois retrievers cujos candidate-sets têm semânticas diferentes mede o **filtro**, não o ranker

## Três instâncias pagas, mesma classe

| Caso | O que parecia | O que era |
|---|---|---|
| **M7** — `Recall@100` 1,0000 vs 0,3125 | BM25 muito superior | o corpus tem **12 documentos**, menor que o `top-k=100` → o BM25 devolve tudo trivialmente |
| **M53** — perna `fts` com nDCG 0,0703 vs BM25 0,6881 (9,8×) | ranker lexical 9,8× melhor | o filtro `@@` do FTS **dropa ~93% dos relevantes** antes de ranquear — é candidate-set, não ranking |
| **M140.1** — `m=5`: 0,991 vs 0,202 | ganho enorme | `websearch_to_tsquery` usa **AND**, o parser do Tantivy usa **OR** — semântica de query e tokenização, não score |

Nas três, a leitura ingênua inflava o ganho em **ordem de grandeza**.

## A distinção que resolve

| Métrica | Mede | Justa quando |
|---|---|---|
| `Recall@k` | quantos relevantes **entraram** no candidate-set | os dois braços têm a mesma política de admissão |
| `nDCG@k` (truncado no topo) | a **ordem** dos que entraram | quase sempre |

**Só a métrica truncada no topo é comparação justa entre retrievers com admissão diferente.** O próprio artefato
do M7 escreve: *"do not cite the Recall@100 delta as evidence of BM25 superiority"*.

## Regra derivada

Antes de atribuir uma diferença ao ranker, pergunte: **os dois braços viram o mesmo conjunto de candidatos?** Se
um filtra e o outro não, a diferença é do filtro até prova em contrário.

## Relacionados

- [failure-mode/dados-sinteticos-degenerados](dados-sinteticos-degenerados.md) — corpus menor que `k` é a mesma família
- [honest-negative/bm25-na-fusao-rrf](../honest-negatives/bm25-na-fusao-rrf.md)
