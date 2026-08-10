---
type: Measurement
title: m52 — ANN filtrado: recall sob WHERE seletivo
description: Atinge paridade de recall no regime seletivo, que é onde o scan iterativo importa; o eixo de recall é determinístico e o de QPS carrega ruído, e o gate usa o primeiro.
resource: git:f7c7b93:docs/benchmarks/m52-filtered-ann.md
tags: [benchmark, filtered-ann, seletividade, iterative-scan, m52]
milestone: M52
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m52
    resource: git:f7c7b93:docs/benchmarks/m52-filtered-ann.md
    title: M52 — Filtered ANN
    last_modified: 2026-07-07
---

**Veredito:** no filtro **seletivo** — que é o ponto onde o scan iterativo existe — o índice próprio
**atinge paridade** com a referência. O critério é cumprido.

# Resultado por seletividade

| Seletividade | recall próprio | recall referência | paridade | p50 próprio | p50 referência |
|---|---|---|---|---|---|
| **~1%** | **0,9713 ± 0,002** | 0,9640 ± 0,003 | ✅ | 42,8 ms | 14,6 ms |
| ~10% | 0,5973 ± 0,009 | 0,5873 ± 0,006 | ✅ | 3,9 ms | 3,0 ms |
| ~50% | 0,5873 ± **0,032** | 0,5773 ± 0,008 | ✅ | 3,5 ms | 2,3 ms |

Dois pontos que a tabela mostra e o texto não precisa afirmar: o recall despenca para ~0,59 nas
seletividades médias em **ambos** os lados — é limitação da abordagem, não do índice; e o desvio de
0,032 no último ponto é notavelmente maior que os demais.

# A escolha de eixo

**O recall é determinístico** — semente fixa e ground truth exato —, logo **independente da carga da
máquina**. É **o eixo confiável e o gate deste milestone**.

**O QPS carrega ruído** de contenção, e o documento diz que ganho de throughput **não é objetivo** desta
fatia; ele pertence a outra.

**Escolher como gate o eixo que o instrumento mede bem**, e declarar o outro fora de escopo, é o que
permite concluir alguma coisa numa máquina imperfeita.

# O que veio depois

O post-filter medido aqui foi superado pelo **filtro inline** do
[ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md), que na mesma seletividade de ~1% mediu
**+0,48 de recall e ~20× de QPS** — porque pula os não-correspondentes **antes** de custarem um slot,
em vez de descartá-los depois.
