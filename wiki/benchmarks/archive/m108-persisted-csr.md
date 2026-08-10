---
type: Measurement
title: m108 — CSR persistido: construir uma vez, consultar muitas
description: Fecha a ressalva do spike guardando a estrutura como bytea num catálogo, o que dá WAL, crash-safety e MVCC nativos sem código próprio.
resource: git:f7c7b93:docs/benchmarks/archive/m108-persisted-csr.md
tags: [benchmark, grafo, csr, persistencia, cache, arquivo, m108]
milestone: M108
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m108
    resource: git:f7c7b93:docs/benchmarks/archive/m108-persisted-csr.md
    title: M108 — Persisted-CSR
---

**Veredito: gate cumprido.** A estrutura persistida serve consultas a **16× a CTE recursiva** em queries
quentes, e 10× a frio — o ganho da travessia, **agora sem a reconstrução por query** —, com o oráculo de
correção passando.

# O que fecha

O [spike](/benchmarks/m107-graph-spike.md) provara a travessia 106–738× melhor, **mas o build na hora
dominava em escala**, colapsando o ganho ponta a ponta para ~7×.

**A ressalva do spike foi tratada como requisito de desenho, e não como nota de rodapé** — é para isso
que serve declará-la.

# A solução, e o que ela evita construir

A estrutura é persistida **uma vez, como `bytea` num catálogo** — e o PostgreSQL torna isso **WAL-logged,
crash-safe e MVCC nativamente**.

**Nenhum WAL próprio, nenhum access method de índice escrito à mão.** É o mesmo truque de composição que
o [colunar](/decisions/0042-m99-own-code-columnar-tam.md) e o
[lexical](/decisions/0052-m140-1-lexical-storage-decision.md) usam, e que repetidamente se mostrou mais
barato **e** mais seguro que a alternativa.

O cache por backend segue o padrão do [cache Arrow](/benchmarks/archive/m101-arrow-cache.md), **chaveado
pela época de construção**, de modo que uma reconstrução **invalida transparentemente**.

# Método

O **mesmo grafo** para as duas engines, com topologia deliberadamente concentrada em hubs — que é o caso
realista e também o mais difícil para travessia.
