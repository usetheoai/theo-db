---
type: Measurement
title: m107 — travessia CSR contra CTE recursiva: o gate do pilar de grafo
description: 106–232× mesmo contra o baseline mais justo, com oráculo de correção passando em todos os trials — e o spike é conservador contra si mesmo por desenho.
resource: git:f7c7b93:docs/benchmarks/m107-graph-spike.md
tags: [benchmark, grafo, csr, bfs, gate, baseline-justo, m107]
milestone: M107
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m107
    resource: git:f7c7b93:docs/benchmarks/m107-graph-spike.md
    title: M107 Phase-0 spike — native CSR + BFS
---

**Veredito: GO**, com uma ressalva de custo de build que **ditou o desenho da fase seguinte**.

# O resultado, com dois baselines

| Escala | travessia nativa | CTE simples | CTE com deduplicação (**mais justo**) | ganho vs justo |
|---|---|---|---|---|
| 100k arestas | 0,25 ms | 181,6 ms | 55,2 ms | **232×** |
| 1M arestas | 1,38 ms | 222,5 ms | 139,2 ms | **106×** |

**Medir contra o baseline mais forte, e não só contra o ingênuo**, é o que faz a conclusão sobreviver a
escrutínio. Contra o baseline ingênuo o número seria 738× — e seria um espantalho.

# A correção, verificada

O oráculo de conjunto alcançável — **contagem, checksum e uma re-checagem independente por hash** —
casou com **ambas** as variantes de CTE **em todos os trials**. Um ganho de 200× sem verificação de
correção não vale nada.

# O escopo, e por que ele é conservador

O spike mede **a expansão do conjunto alcançável**, que é o custo dominante do baseline. Ele **não** é a
query completa do consumidor real, que tem ainda uma cauda de pontuação.

**Essa cauda é trabalho adicional do lado da CTE** — logo o custo real ponta a ponta do baseline é
**maior** que o medido. **O spike é conservador contra si mesmo**, e diz isso.

Isolar a primitiva é deliberado: **é ela a pergunta do gate**.

# A ressalva que moldou o desenho

Construir a estrutura na hora **domina** o custo a 1M — o que colapsaria o ganho ponta a ponta para ~8×.
**Portanto a fase seguinte DEVE persistir a estrutura**, e o número operativo passa a ser o da travessia
isolada.

Uma ressalva que muda a arquitetura da implementação, descoberta no spike e não depois, é o retorno
máximo que um gate pode dar. A decisão é o [ADR 0048](/decisions/0048-m107-native-graph-engine-go.md), e
a feature entregue é [grafo nativo](/features/13-grafo-nativo.md).
