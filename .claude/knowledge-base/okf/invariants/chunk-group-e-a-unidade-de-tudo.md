---
type: Invariant
title: CHUNK_GROUP_ROWS = 10.000 é a unidade de decode, skip e memória do colunar
description: Todo termo O(N) no colunar tem uma versão O(chunk-group); quando um caminho não tem, é defeito de escala esperando a escala.
resource: theodb_rs/src/am/columnar_codec.rs
tags: [colunar, arquitetura, escala]
timestamp: 2026-07-30T00:00:00Z
---

# `CHUNK_GROUP_ROWS = 10.000` é a unidade de decode, skip e memória do colunar

## O invariante

`columnar_codec.rs:24` fixa a unidade. 1M linhas = 100 chunk-groups; 100M = 10.000. Toda estrutura do caminho
colunar **deveria** ser O(chunk-group):

| Caminho | Estado |
|---|---|
| decode do scan (M168) | O(chunk-group) ✔ |
| top-k (M168) | O(chunk-group + k) ✔ |
| zone-map skip (M150) | por chunk-group ✔ |
| **plano do scan** (`ScanPlan`) | **O(N/10.000 × natts)** ✘ — 48,1 MiB a 100M |
| **`flush_pending`** (escrita) | **O(mwm)** com multiplicador **×8** ✘ — [medição](../measurements/amplificacao-maintenance-work-mem.md), issue #221 |
| **resultado agrupado** (`rows: Vec<Vec<>>`) | **O(grupos)** ✘ — invisível à MemoryPool |

## A heurística que isso gera

Ao ler qualquer caminho colunar: **"o que aqui é proporcional a N em vez de ao chunk-group?"** Os três defeitos
de escala do M169 foram achados assim, e o padrão de fix é sempre o mesmo — mover a transposição/materialização
para dentro do laço por chunk-group.

## Nuance que gera "correção" errada

O doc-comment do `ChunkDirEntry` diz "**fixed 44 bytes**" — esse é o **serializado**. Em memória, com dois `u64` e
alinhamento 8, são **48 B**. Os dois números estão certos para coisas diferentes.

## Relacionados

- [measurement/amplificacao-maintenance-work-mem](../measurements/amplificacao-maintenance-work-mem.md)
- [measurement/scanplan-e-on](../measurements/scanplan-e-on.md)
