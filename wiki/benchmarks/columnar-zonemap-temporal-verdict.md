---
type: Measurement
title: skip por zone-map em tipos temporais — veredito
description: Estende o mecanismo a timestamp e data, o que exige mapear domínios de tipo e literais corretamente — e o caso temporal é o mais comum em dados analíticos reais.
resource: git:f7c7b93:docs/benchmarks/columnar-zonemap-temporal-verdict.md
tags: [benchmark, columnar, zone-map, temporal, tipos]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: czmt
    resource: git:f7c7b93:docs/benchmarks/columnar-zonemap-temporal-verdict.md
    title: theodb_columnar zone-map temporal verdict
    last_modified: 2026-07-19
---

Estende o skip por zone-map a **timestamp com fuso e data**.

# Por que o caso temporal merece milestone próprio

**É o caso mais comum em dados analíticos reais.** Tabelas de eventos são naturalmente ordenadas por
tempo, o que as torna **clusterizadas por construção** na coluna que quase toda query filtra.

Ou seja: é o cenário em que o zone-map paga melhor — e deixá-lo de fora significaria ter o mecanismo sem
o seu principal beneficiário.

# O que a extensão exige tecnicamente

Não é reuso trivial. Ela exige **mapear o identificador de tipo do PostgreSQL para o domínio correto** de
comparação, **construir os arrays temporais tipados** do lado vetorizado, e **produzir o literal temporal
com o tipo certo** na expressão de filtro.

**Comparar um timestamp como se fosse um inteiro qualquer produziria skip errado** — e skip errado não é
lentidão, é **resultado faltando**. É a mesma classe de risco que motivou os guards de collation em
[m153](/benchmarks/m153-groupby-text.md).

Por isso o milestone inclui um **teste de regressão do domínio temporal** na função pura de decisão.

# Contexto

Completa o trio com [zone-map](/benchmarks/columnar-zonemap-verdict.md) e
[min/max](/benchmarks/columnar-minmax-zonemap-verdict.md), todos parte de
[analítico colunar](/features/14-analitico-colunar.md).
