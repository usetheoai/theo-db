---
type: Measurement
title: e1 — rerank RaBitQ sem precisão plena, dentro do PostgreSQL
description: A única variável entre os dois braços é o codec de rerank; tudo o mais — dados, índice, queries, ground truth — é idêntico.
resource: git:f7c7b93:docs/benchmarks/e1-rabitq-inpg-verdict.md
tags: [benchmark, rabitq, quantizacao, memoria, isolamento, sift1m]
dataset: SIFT1M
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: e1
    resource: git:f7c7b93:docs/benchmarks/e1-rabitq-inpg-verdict.md
    title: E1 — f32-free RaBitQ rerank in-PostgreSQL
    last_modified: 2026-07-17
---

# O isolamento, descrito com precisão

> Os **mesmos** vetores indexados de dois modos **no MESMO access method**, consultados com as **mesmas**
> queries oficiais e pontuados contra o **mesmo** ground truth oficial — **a única variável é o codec de
> rerank**.

Essa é a forma canônica de um A/B: enumerar tudo o que ficou constante e nomear a única coisa que mudou.
O mesmo rigor do delta em [m169](/benchmarks/m169-t41-delta.md).

# O resultado

O caminho quantizado é **3,28× menor a paridade de recall** — o ganho de **memória** que caracteriza o
[RaBitQ](/technologies/rabitq.md).

Isso é coerente com o veredito estratégico já estabelecido: **o ganho desta família é memória, não
QPS** ([ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md)). Aqui a propriedade é
confirmada **dentro do banco**, e não apenas num spike — o que importa, dada a lição repetida de que
ganhos in-memory não sobrevivem ao caminho de página.

# Licença

O algoritmo é **reimplementação própria** de trabalho permissivo, conforme a política de
[proveniência](/references/license-audit.md). O destino do core originalmente vendorizado está no
[ADR 0046](/decisions/0046-rabitq-vendor-tree-deleted.md).
