---
type: Measurement
title: m150 — filtragem por zone-map: pular blocos sem descomprimir
description: Usa o diretório de mínimo e máximo que o formato já mantinha para descartar blocos inteiros antes de descomprimir, sobre dados deliberadamente clusterizados.
resource: git:f7c7b93:docs/benchmarks/m150-chunk-group-filtering.md
tags: [benchmark, columnar, zone-map, skip, m150]
milestone: M150
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m150
    resource: git:f7c7b93:docs/benchmarks/m150-chunk-group-filtering.md
    title: M150 — Chunk-group filtering no scan colunar
    last_modified: 2026-07-25
---

**Pular blocos inteiros por mínimo e máximo, sem descomprimir** — consumindo o diretório que o
[formato](/benchmarks/m99-columnar-tam.md) já mantinha e que ainda não era usado no scan geral.

# Por que o dataset é clusterizado de propósito

A coluna de filtro é **monotônica**, portanto **clusterizada**.

Isso não é escolher dados favoráveis: é escolher o **regime em que o mecanismo pode funcionar**. Um
zone-map só descarta blocos quando os valores de um bloco são **coesos** — em dados aleatórios, cada
bloco contém quase todo o intervalo, e nenhum é descartável.

**Medir num regime onde o mecanismo é estruturalmente inoperante não diria nada sobre ele.** O que a
honestidade exige é **declarar** o regime — que é o que o documento faz —, para que ninguém extrapole o
ganho para dados não ordenados.

É o mesmo cuidado que [m40](/benchmarks/m40-carrier.md) exerce ao declarar que gaussiano aleatório é o
pior caso para índice de grafo.

# Ordem de grandeza

Os verdicts dedicados do mecanismo medem **7,29× de ganho por skip** e até **~1300–1400×** no caminho
rápido de mínimo e máximo — ver [zone-map](/benchmarks/columnar-zonemap-verdict.md) e
[min/max](/benchmarks/columnar-minmax-zonemap-verdict.md).

# Contexto

Faz parte da série destravada pelo [profile](/benchmarks/m148-flamegraph-scan.md).
