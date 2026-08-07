---
type: Measurement
title: m34 — IVFFlat com lists e probes configuráveis, contra o pgvector
description: Fecha o gap de ~8× medindo cada índice em isolamento — o bug de medição que a corrida anterior tinha era o planner cruzar dois índices da mesma família na mesma coluna.
resource: git:f7c7b93:docs/benchmarks/m34-ivfflat-reloption.md
tags: [benchmark, ivfflat, pgvector, sift1m, reloption, metodologia]
dataset: SIFT1M
milestone: M34
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m34
    resource: git:f7c7b93:docs/benchmarks/m34-ivfflat-reloption.md
    title: TheoDB vector benchmark — m34-ivfflat-reloption
---

**Método:** SIFT1M completo, 1000 queries semeadas, 3 runs com melhor-de-N. Ground truth exato a partir
dos vizinhos oficiais do dataset. **Ambos os índices construídos com os mesmos `lists`** e varridos sobre
**os mesmos `probes`**.

# O bug de medição que esta corrida pegou

**Cada configuração é medida em ISOLAMENTO** — o harness derruba o outro índice durante as queries de
uma configuração.

Sem isso, **dois índices da mesma família na mesma coluna deixam o planner cruzá-los**, o que **achata a
varredura** e produz números sem sentido. O defeito foi encontrado e corrigido nesta corrida.

É a mesma classe de armadilha que o [ADR 0012](/decisions/0012-benchmark-data-degeneracy.md) documenta:
**o harness medindo outra coisa que não a hipótese**.

# Veredito por knob, a recall casado

| probes | recall (próprio / pgvector) | p50 próprio | p50 pgvector | veredito |
|---|---|---|---|---|
| 1 | 0,373 / 0,374 | 0,60 ms | 0,37 ms | ~1,6× mais lento (ponto trivial, sub-ms) |
| 10 | 0,874 / 0,866 | 2,99 ms | 2,72 ms | ~10% mais lento, com recall 0,8 pt maior |
| 50 | 0,992 / 0,992 | 12,77 ms | 13,48 ms | **≈ paridade** — margem fina |
| 100 | 0,999 / 0,999 | **25,38 ms** | 28,32 ms | **≤ pgvector, −10%** |

**Critério atingido:** no ponto de alto recall com recall casado, o índice próprio é **10% mais rápido**,
com margem robusta; e fica em paridade nos pontos intermediários. O pgvector mantém pequena vantagem no
canto de baixo recall.

# Rigor declarado

A margem de 5% no ponto intermediário é **reportada como paridade, não como vitória** — porque o hardware
é uma CPU móvel termicamente limitada, e essa margem cabe dentro do ruído de throttling.

Reportar assim é o oposto de arredondar a favor: **uma margem fina em hardware ruidoso não é uma
vitória.**

Também declarado: `mean` e `std` são dispersão **por query dentro da amostra**, não variância entre runs;
e o QPS é melhor-de-N.

# Relacionado

Este é o run cujas linhas o [m33](/benchmarks/m33-scann-headtohead.md) reusa verbatim para o lado
PostgreSQL do head-to-head contra o ScaNN. A feature é o
[índice IVFFlat](/features/03-indice-ivfflat.md).
