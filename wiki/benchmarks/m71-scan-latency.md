---
type: Measurement
title: m71 — multi-entry no build: +29% de QPS recall-neutro, e o veredito iso-recall honesto
description: Entrega uma melhoria real e reporta, no mesmo documento, que a iso-recall o índice ainda precisa de ~2× o ef — separando melhoria de superioridade.
resource: git:f7c7b93:docs/benchmarks/m71-scan-latency.md
tags: [benchmark, hnsw, multi-entry, qps, iso-recall, m71]
milestone: M71
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m71
    resource: git:f7c7b93:docs/benchmarks/m71-scan-latency.md
    title: M71 — Latência do scan do AM
    last_modified: 2026-07-10
---

# O que foi entregue

Carregar o **conjunto completo** da busca como entry-set entre camadas do build, seguindo o algoritmo
original de inserção, em vez de colapsá-lo a um único nó. Produz um **grafo melhor conectado**.

**Medido: +29% de QPS, recall-neutro**, a 500k × 768d.

# O veredito iso-recall, no mesmo documento

A recall casado, o índice **ainda precisa de ~2× o `ef`** da referência — e ~5× em escala maior. **Não é
superior nem está em paridade** de latência.

A causa é a mesma lacuna de **navegabilidade do grafo** que gateava o recall, com **sete alavancas já
refutadas**.

**Publicar a melhoria e o limite no mesmo artefato** é o que impede o +29% de ser lido como superioridade.
O reenquadramento formal do critério está no
[ADR 0031](/decisions/0031-m71-latency-improvement-not-superiority.md).

# O trade-off aceito

O build multi-entry é **mais lento** — mais trabalho por inserção. Troca deliberada num índice focado em
latência de query.

# Onde a causa-raiz foi finalmente atacada

Não por mais varredura de parâmetros, mas pela análise estrutural do grafo que levou ao
[ADR 0034](/decisions/0034-hnsw-extend-candidates-navigability.md).
