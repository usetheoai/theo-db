---
type: Measurement
title: m56 — custo do DELETE por tombstone contra o muro do fold
description: Na mesma escala, o caminho de DELETE custa 2,7 s e 1,6 MB de pico, contra 117 s e 1,16 GB do fold — a validação do desenho híbrido.
resource: git:f7c7b93:docs/benchmarks/m56-inplace-maintenance.md
tags: [benchmark, vacuum, tombstone, memoria, manutencao, m56]
milestone: M56
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m56inp
    resource: git:f7c7b93:docs/benchmarks/m56-inplace-maintenance.md
    title: M56 — DELETE-path in-place tombstone cost
---

**Caracterização** do custo do caminho de DELETE após a mudança para tombstone in-place, contra o fold do
índice inteiro, **na mesma escala** — que é o que torna a comparação válida.

# O contraste

A 100k × 768d, com 10% das linhas deletadas:

| Caminho | VACUUM (wall) | pico de RSS privado | lock exclusivo | WAL |
|---|---|---|---|---|
| **tombstone** | **2,7 s** | **1,6 MB** | — | ~25 MB |
| compaction (fold, raro) | 117 s | 1159 MB | ~107 s | ~314 MB |

**Três ordens de grandeza em memória de pico, e nenhum lock exclusivo no caminho comum.**

Isso valida diretamente o desenho decidido no
[ADR 0017](/decisions/0017-m55-index-maintenance-at-scale.md): **tombstone in-place no caminho de DELETE,
que é o caso comum, e fold apenas para compaction, que passa a ser raro**.

# O que a medição não esconde

O desvio do tombstone é grande em relação à média (1367 sobre 2685 ms), e o WAL varia por mais da
metade — a dev box não é quieta. **Mas o efeito é de ordens de grandeza**, então sobrevive
folgadamente à variância. É a mesma lógica do [m30](/benchmarks/m30-columnar-scale.md): quando o efeito é
muito maior que o ruído, o ruído não decide.

O guard de carga no pré-voo continua ativo, herdado da lição do [m46](/benchmarks/m46-highrecall-qps.md).

# Baseline

O muro que este resultado ataca está caracterizado em [m55](/benchmarks/m55-vacuum-wall.md).
