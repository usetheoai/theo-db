---
type: Measurement
title: m130 — pilar HTAP com carga mista real
description: A quarta e última aplicação do padrão; roda mistura transacional e analítica sobre UM schema numa fase única, com métrica dupla rotulada como derivada e um oráculo de resultado.
resource: git:f7c7b93:docs/benchmarks/m130-htap.md
tags: [benchmark, htap, ch-benchmark, benchbase, oraculo, m130]
milestone: M130
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m130
    resource: git:f7c7b93:docs/benchmarks/m130-htap.md
    title: M130 — Official-benchmark HTAP pillar
    last_modified: 2026-07-21
---

**A quarta e última aplicação** do padrão adotar-e-envolver.

# O que a carga mede

Uma mistura transacional somada a 22 queries analíticas **sobre UM schema, numa fase de trabalho única e
mista** — que é o que distingue HTAP de rodar dois benchmarks separados.

O driver oficial entra como **container externo, fora da árvore**, com **SHA fixado** — reprodutibilidade
por versão, não por nome de tag.

# O que a camada própria acrescenta

Três coisas que a ferramenta não dá:

- uma **métrica dupla derivada — e rotulada como derivada**, o que impede que ela seja lida como número
  primário do benchmark;
- **dispersão entre execuções**, com três sessões e artefato por sessão;
- um **oráculo para o lado analítico** — porque a ferramenta valida tempo, não resultado.

**Rotular uma métrica derivada como derivada** parece pedante e não é: uma composição de dois números
oficiais não é um número oficial, e tratá-la como tal é como claims incomparáveis nascem.

# Resultado relevante

Este é o run que mediu o pilar HTAP **com 0% de erro**, e é citado pela
[feature de lakehouse](/features/15-lakehouse-parquet.md) como evidência do caminho misto.

# Ressalva

Máquina compartilhada e **não canônica**.
