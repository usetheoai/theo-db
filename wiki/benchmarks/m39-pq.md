---
type: Measurement
title: m39 — quantização de produto contra escalar: honest-negative
description: A implementação própria de PQ funciona e é testada, mas mede ~5× menos QPS a recall casado; o gate anti-sunk-cost fez seu trabalho antes da integração cara.
resource: git:f7c7b93:docs/benchmarks/m39-pq.md
tags: [benchmark, pq, quantizacao, honest-negative, anti-sunk-cost, m39]
milestone: M39
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m39
    resource: git:f7c7b93:docs/benchmarks/m39-pq.md
    title: M39 — Product Quantization vs SBQ
    last_modified: 2026-07-03
---

**Veredito: manter a quantização escalar.** A de produto **não** vence no eixo perseguido.

# O resultado

| Método | recall@10 | QPS | bytes por vetor |
|---|---|---|---|
| **produto** | 0,770 | **352 ± 49** | **8** |
| **escalar** | 0,769 | **1828 ± 30** | 32 |
| precisão plena | 1,000 por construção | — | 256 |

**A recall casado, a quantização de produto é ~5× mais lenta.** Ela troca 4× menos memória por ~5× menos
throughput — **um trade de memória por latência, não o ganho de QPS que o objetivo exigia**.

# O valor do gate

O trabalho construiu uma implementação **funcional e testada** de quantização de produto com distância
assimétrica, mediu head-to-head contra ground truth exato, e **parou**.

**O gate anti-sunk-cost fez seu trabalho ANTES da integração cara** no formato de página do access
method. Descobrir que o caminho não paga **depois** de integrá-lo teria custado a integração inteira mais
a pressão de justificá-la.

# A sequência

O documento se identifica como **o terceiro honest-negative consecutivo** da série: a distância não era o
gargalo ([m36](/benchmarks/archive/m36-scan-optimization.md)), a quantização escalar regride recall e a cópia não
era o gargalo ([m38](/benchmarks/m38-copy-free-scan.md)), e agora a quantização de produto não é ganho de
QPS.

E — o detalhe que mais importa — **o blueprint antecipava explicitamente essa possibilidade**. Três
negativos seguidos num programa não são fracasso de execução; são a evidência de que o gate está
calibrado para dizer não.

O que veio depois foi a pergunta certa: se o quantizador não é o limitante,
[o que é?](/benchmarks/m40-ceiling-probe.md)
