---
type: Measurement
title: m9 — IVFFlat medido no harness, contra HNSW
description: Valida o índice por listas invertidas com um detalhe de rigor que importa: os probes são clampados ao número de listas antes da deduplicação, para que cada rótulo corresponda ao que de fato executou.
resource: git:f7c7b93:docs/benchmarks/m9-ivfflat.md
tags: [benchmark, ivfflat, hnsw, harness, rigor, m9]
milestone: M9
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m9
    resource: git:f7c7b93:docs/benchmarks/m9-ivfflat.md
    title: M9 — IVFFlat / IVF vector index
    last_modified: 2026-06-28
---

**O que valida:** o índice por listas invertidas existia mas **nunca havia sido exercitado pelo harness
de recall**, que só media HNSW e DiskANN. Este milestone o torna cidadão de primeira classe da medição.

**Fecha também a especificação genérica de "IVF"**, com um achado simples e útil: **o índice IVF do
ecossistema *é* o IVFFlat** — não existe um access method distinto a implementar.

# Método, e o detalhe de rigor

Dataset gaussiano sintético, n=5000, dim=16, k=10, 100 queries, 3 runs. Ground truth por força bruta
exata.

**O detalhe que evita um rótulo mentiroso:** os `probes` varridos são clampados ao número de listas
**antes** da deduplicação, de modo que **o rótulo de cada linha é igual ao valor que de fato executou**.
Sondar mais listas do que existem é no-op; sem o clamp, uma linha rotulada `probes=10` executaria
silenciosamente `probes=5`.

Isso é pequeno e é exatamente o tipo de coisa que corrompe uma tabela de resultados sem que ninguém
perceba.

O scan sequencial é desabilitado durante a medição — **mede-se o índice, não a escolha do planner** para
N pequeno.

# Postura

**Nenhuma alegação de superioridade de velocidade é feita** — os dados mostram um **trade-off**,
reportado como medido. Essa é a postura padrão dos artefatos do projeto, e aqui ela aparece cedo.

# Relacionados

A feature correspondente é o [índice IVFFlat](/features/03-indice-ivfflat.md); a medição em escala real,
com os knobs configuráveis, é o [m34](/benchmarks/m34-ivfflat-reloption.md); e a escolha do índice
default está na [decisão de índice](/decisions/m2-index-decision.md).
