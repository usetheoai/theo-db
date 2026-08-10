---
type: Measurement
title: m72 — QPS multi-cliente a 1M: o regime de produção que faltava
description: Mede throughput agregado sob 8 conexões concorrentes contra o mesmo índice, e é onde o índice próprio supera a referência — num regime declaradamente favorável a ele.
resource: git:f7c7b93:docs/benchmarks/m72-qps-multiclient.md
tags: [benchmark, concorrencia, throughput, regime, m72]
milestone: M72
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m72
    resource: git:f7c7b93:docs/benchmarks/m72-qps-multiclient.md
    title: M72 — QPS multi-cliente a 1M
    last_modified: 2026-07-10
---

# O que faltava

As medições anteriores davam **p50 com um cliente**. Este mede o regime que a produção realmente tem:
**throughput agregado sob N conexões concorrentes martelando o MESMO índice**.

Um índice pode ganhar com um cliente e perder sob concorrência — são propriedades diferentes, e medir só
a primeira e falar da segunda seria extrapolação.

# Resultado

A 1M × 128d, com 8 clientes concorrentes, 3 runs por ponto, a recall casado de ~0,91:

| Índice | recall | QPS | p50 |
|---|---|---|---|
| **próprio** | 0,917 | **597,7** | **13,6 ms** |
| referência | 0,9095 | 539,5 | 16,5 ms |

**+11% de QPS a recall casado.** E o índice próprio alcança um recall (0,97 a 354 QPS) em que a
referência **platôa antes**, por volta de 0,914.

O build também é **~3× mais rápido**, fruto da [linhagem de otimização](/benchmarks/m44-parallel-build.md).

# A ressalva que enquadra o resultado

**Este é o regime favorável ao índice próprio** — 128 dimensões, dados clusterizados, que é exatamente o
regime-alvo do fix de navegabilidade do
[ADR 0034](/decisions/0034-hnsw-extend-candidates-navigability.md).

**A fronteira de alta dimensão e alto recall permanece da referência.** Declarar o regime em que o
resultado vale é o que impede que ele vire claim universal — e é assim que ele entra no
[veredito consolidado](/decisions/0035-m73-northstar-vector-verdict.md): como
"competitivo-a-superior no regime 128d clusterizado", com essa qualificação colada.
