---
type: Measurement
title: skip por zone-map no colunar — veredito
description: 7,29× pulando blocos que o predicado não pode satisfazer, com kill-switch por GUC e a nota de que a métrica de skip é observável.
resource: git:f7c7b93:docs/benchmarks/columnar-zonemap-verdict.md
tags: [benchmark, columnar, zone-map, skip, observabilidade]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: czm
    resource: git:f7c7b93:docs/benchmarks/columnar-zonemap-verdict.md
    title: theodb_columnar zone-map skip-pruning verdict
    last_modified: 2026-07-18
---

**Ganho medido: 7,29×** numa tabela clusterizada de 1M linhas.

# O mecanismo

O predicado do `WHERE` é extraído e comparado contra o **mínimo e máximo de cada bloco**. Blocos que
**não podem** conter linha satisfatória são **pulados sem descompressão**.

A função que decide isso é **pura** — recebe o predicado e os extremos, devolve se o bloco pode casar —
o que a torna testável isoladamente, sem banco.

# Os dois detalhes de engenharia

**Um kill-switch por GUC**, que permite medir o ganho **no mesmo binário** — o padrão que
[m160](/benchmarks/m160-decode-zerocopy-verdict.md) explicita como forma de evitar comparar builds
diferentes.

**Uma métrica de skip observável**, que torna verificável **quantos blocos foram efetivamente pulados** —
sem ela, um ganho poderia vir de outra causa e ninguém saberia se o mecanismo sequer disparou. É o mesmo
papel que `candidates_seen` cumpre no diagnóstico vetorial.

# A dependência de clusterização

O ganho existe **porque os dados são clusterizados** — em dados aleatórios cada bloco contém quase todo o
intervalo e nada é descartável. O regime está declarado, como em
[m150](/benchmarks/m150-chunk-group-filtering.md).

# Relacionados

O caminho rápido de extremos é [min/max](/benchmarks/columnar-minmax-zonemap-verdict.md); a extensão a
tipos temporais é [temporal](/benchmarks/columnar-zonemap-temporal-verdict.md).
