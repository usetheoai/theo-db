---
type: Measurement
title: m31 — leitura parcial de páginas estruturadas
description: Fecha o gap algorítmico O(N)→O(probes) com 45× de ganho, e reporta honestamente o resíduo de fator constante — cujos números seriam depois retro-invalidados por dados degenerados.
resource: git:f7c7b93:docs/benchmarks/m31-am-latency.md
tags: [benchmark, ivfflat, latencia, partial-read, retro-invalidado, m31]
milestone: M31
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m31
    resource: git:f7c7b93:docs/benchmarks/m31-am-latency.md
    title: M31 — Index AM query latency
---

> ⚠️ **Os números de latência deste artefato foram retro-invalidados** como medições ANN pelo
> [ADR 0012](/decisions/0012-benchmark-data-degeneracy.md): os dados de teste tinham **todas as linhas
> com o vetor idêntico**, o que fazia o workload ser força bruta sobre empates fantasiada de busca
> aproximada. **A conquista estrutural — a mudança de complexidade — segue válida**; a comparação de
> fator constante é que estava sobre dados ruins. Os números corrigidos estão em
> [m31b](/benchmarks/m31b-simd-distance.md).

# O que mudou

O scan anterior **desserializava o blob inteiro por query**, O(N). Esta fatia reestrutura a persistência
em **página de meta com centroides e diretório de listas** mais **páginas de lista**, de modo que o scan
lê a meta e os centroides — proporcional ao número de listas — e depois **apenas as páginas das listas
sondadas**, pontuando direto sobre os bytes da página com buffer reaproveitado, **sem alocação por
entrada**.

# Medido

| Abordagem | p50 |
|---|---|
| blob O(N) por scan | ~1700 ms |
| **leitura parcial estruturada** | **~38 ms** |
| referência (SIMD em C) | ~14 ms |

**O gap de I/O está fechado:** o índice passa a ler aproximadamente o mesmo número de páginas que a
referência, e é **~45× mais rápido** que o caminho anterior.

**Resíduo honesto:** ~2,7× atrás da referência. **O gap algorítmico fechou; o que resta é fator
constante** — distância escalar auto-vetorizada contra SIMD de largura dupla com despacho de CPU em
runtime, em C e com anos de tuning.

# A resposta a esse resíduo

Em vez de afrouxar o critério, o milestone foi **re-escopado ao ganho medido**, e o fator constante virou
milestone próprio — [ADR 0011](/decisions/0011-m31-rescope-simd-followup.md). O documento registra
explicitamente que fechar o resíduo **não é escopo desta fatia**.
