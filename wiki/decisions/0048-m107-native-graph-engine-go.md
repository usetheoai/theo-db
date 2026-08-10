---
type: Decision
title: ADR 0048 — Motor de grafo nativo (CSR + BFS vetorizado) sobre o substrato colunar+vetorial: GO
description: A travessia nativa mede 106–232× mais rápida que CTE recursiva mesmo contra o baseline mais justo; o gate anti-sunk-cost autoriza o pilar de grafo.
resource: git:f7c7b93:docs/adr/0048-m107-native-graph-engine-go.md
tags: [adr, grafo, graphrag, csr, msbfs, sql-pgq, measurement-first, m107]
adr_id: "0048"
adr_status: Accepted
decision_date: 2026-07-16
milestone: M107
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0048
    resource: git:f7c7b93:docs/adr/0048-m107-native-graph-engine-go.md
    title: ADR-0048 — Native graph engine
    last_modified: 2026-07-16
---

O gate measurement-first de um pilar novo, espelhando o padrão que o pilar vetorial usou: um spike
decide se as milestones seguintes são autorizadas.

# Contexto

Grafo é capacidade recorrente e transversal, e o mandato do produto é ser um banco AI-native. A
convergência da SOTA — DuckPGQ, Kùzu — é que grafo e vetor eficientes vivem numa engine só, pareando
**storage colunar + adjacência CSR + travessia vetorizada (MS-BFS) + join worst-case-optimal**, expostos
via **SQL/PGQ**.

O TheoDB já possui três dos quatro ingredientes: o colunar
([ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md),
[ADR 0044](/decisions/0044-m103-vector-columnar-coresidence.md)), o access method vetorial próprio
com kernels SIMD, e a superfície `ai.*`. **Falta a travessia nativa.** O GraphRAG do ecossistema roda
hoje sobre **CTE recursiva** em tabelas relacionais — o baseline a bater.

# Decisão

Construir um motor de grafo nativo como **código próprio**: adjacência CSR mais operadores de
travessia vetorizada por fronteira, **fundidos** com o substrato colunar e vetorial existente.
Adotam-se as *técnicas* dos papers públicos, **sem vendorizar** as engines, **sem** CTE recursiva e
**sem** Apache AGE. **Veredito do gate: GO.**

# Evidência

Spike reproduzível, 4 trials por escala, com oráculo de correção passando em todos os 8 contra
**ambos** os baselines ([m107](/benchmarks/m107-graph-spike.md)):

| Escala | Travessia nativa | CTE `UNION ALL` | CTE `UNION` (dedup, mais justo) | vs `UNION ALL` | vs dedup |
|---|---|---|---|---|---|
| 100k arestas | 0,25 ms | 181,6 ms | 55,2 ms | **738×** | **232×** |
| 1M arestas | 1,38 ms | 222,5 ms | 139,2 ms | **169×** | **106×** |

O oráculo de conjunto alcançável — contagem, checksum e uma re-checagem independente por hash de
conjunto — casou com **ambas** as variantes de CTE em todos os trials. A travessia nativa vence
**106–232×** mesmo contra o baseline dedup, então **a conclusão sobrevive a um baseline
não-espantalho**. E o spike isola a expansão do conjunto alcançável, que é o custo dominante da
CTE — sendo portanto **conservador**.

# Alternativas consideradas

**CTE recursiva** — join com bitmap-OR por hop e explosão de intermediários; medida 170–732× mais
lenta na operação central. **Apache AGE** (permissivo, passa no portão de licença) — compila Cypher
para joins relacionais recursivos, isto é, **o mesmo imposto por hop**, mais um silo de query
separado. **Rejeitado por arquitetura, não por licença.** **Empacotar Kùzu ou DuckPGQ** — engines
excelentes, mas são stores separados de nó único; empacotá-las bifurcaria o substrato de storage. A
literatura mostra que operadores de travessia nativos **sobre o storage existente** batem bancos de
grafo nativos — o gap é o modelo de execução, não o storage.[^adr0048]

# Consequências

**Positiva:** um pilar de grafo reusando o investimento colunar, vetorial e SIMD; o fluxo de GraphRAG
— entrada por vetor, travessia limitada, rerank — roda zero-copy numa engine só, que é a vitória
AI-native.

**Ressalva que molda o desenho, vinda do próprio spike:** construir a CSR na hora **domina** a 1M —
38 ms de build contra 1 ms de travessia, o que colapsa o ganho ponta a ponta para 8,3×. **Portanto a
primeira fase de implementação DEVE persistir a CSR** como índice, construída uma vez e mantida
incrementalmente, para que o número operativo seja o da travessia isolada.

**Escopo:** este gate provou a primitiva. As fases da engine — índice CSR persistido, operador MS-BFS
vetorizado, superfície de expansão de grafo, vetor sobre nós, e ranqueamento por comunidade — são
milestones seguintes, cada uma com seu gate. A **qualidade** do grafo (extração) é avaliação
separada, que a engine não resolve.

[^adr0048]: ADR-0048 — Native graph engine over the columnar+vector substrate
