---
type: Measurement
title: m35 — scan HNSW page-native com leitura estruturada
description: Troca o scan O(N) sobre blob único por travessia sob demanda O(ef·M), medindo ~61× a recall preservado; a prova da complexidade é contagem de páginas, não relógio.
resource: git:f7c7b93:docs/benchmarks/m35-hnsw-structured-scan.md
tags: [benchmark, hnsw, page-native, complexidade, sift1m]
dataset: SIFT1M
milestone: M35
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m35
    resource: git:f7c7b93:docs/benchmarks/m35-hnsw-structured-scan.md
    title: M35 — theodb_hnsw page-native structured scan
---

**Objetivo:** trocar o scan de blob inteiro, O(N), por travessia estruturada sob demanda, O(ef·M).

**Método:** SIFT1M, n=1M, dim=128, k=10, 1000 queries semeadas, 3 runs.

# Fronteira a 1M

| `ef_search` | recall@10 | QPS | p50 |
|---|---|---|---|
| 40 | 0,9272 | 318,9 | 3,15 ms |
| 100 | 0,9789 | 100,4 | 10,06 ms |
| 200 | 0,9926 | 60,4 | 17,56 ms |

# Contra o scan O(N) anterior — a recall casado

O blob fazia **1,6 QPS a recall 0,9640**, porque o scan O(N) levava ~4 s por query.

No ponto de **recall casado** (`ef_search=100`, recall 0,9789 ≥ 0,964), o scan estruturado faz **100,4
QPS** — **~61× mais rápido a recall preservado**. Aceitando recall 0,927, sobe para ~194×.

**Casar o recall antes de comparar QPS** é o que impede a comparação de ser inflada — sem isso, bastaria
baixar o `ef` para "ganhar".

# A prova de complexidade — páginas, não relógio

Esta é a parte metodologicamente mais interessante. A prova de que a complexidade mudou **não** é o
tempo de parede: é a **contagem de páginas lidas**.

Medido em escala pequena, com `ef_search` fixo: **2742 páginas a 50.000 vetores** contra **2962 páginas
a 200.000** — uma razão de **1,08× enquanto N cresceu 4×**. **Plano em N**, que é a assinatura de
O(ef·M).

O tempo de parede **cresce sublinearmente** com N, por causa de mais cache miss num índice maior — mas
a **contagem de páginas**, que é sobre o que a discussão O(N) contra O(ef·M) versa, fica constante.

Ressalva declarada: essa demonstração roda numa dimensão e escala menores, onde builds são baratos — ela
**não é revalidada a 1M**.

# Trade-off honesto

O build estruturado leva **~17,5 minutos a 1M**, single-thread. **O build ficou mais lento; o scan é o
ganho de 61×.** É explicitamente um trade de construir-uma-vez para consultar-muitas.

# Relacionados

O desenho de layout e travessia está descrito no
[capítulo do handbook](/references/handbook-19-hnsw.md), e a feature correspondente é o
[índice HNSW](/features/02-indice-hnsw.md).
