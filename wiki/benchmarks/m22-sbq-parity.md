---
type: Measurement
title: m22 — quantização SBQ própria: paridade de recall e de memória
description: 16× de redução contra precisão plena, com memória em paridade — e o registro insiste que paridade de memória não é vitória de memória.
resource: git:f7c7b93:docs/benchmarks/m22-sbq-parity.md
tags: [benchmark, sbq, quantizacao, memoria, paridade, m22]
milestone: M22
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m22
    resource: git:f7c7b93:docs/benchmarks/m22-sbq-parity.md
    title: M22 — Own SBQ scalar quantization parity
---

**Veredito: paridade alcançada.**

# Memória

| Representação | bytes por vetor | contra f32 |
|---|---|---|
| f32 (baseline) | 128 | 1× |
| **SBQ próprio (1 bit)** | **8** | **16× menor** |
| SBQ da referência (1 bit) | 8 | 16× menor |

**A memória é paridade com a referência, não vitória sobre ela** — a fórmula de tamanho é idêntica. O
documento diz isso literalmente, e o ganho substantivo é a **redução de 16× contra precisão plena**.

Distinguir "empatei com o concorrente" de "ganhei do baseline" é elementar e frequentemente omitido.

# Recall

O baseline da referência mede 0,6278 ± 0,0044. O caminho próprio, varrendo `over_fetch` e `probes`:

| over_fetch | probes | recall próprio | paridade |
|---|---|---|---|
| 8 | 8–32 | 0,50–0,55 | ❌ falha |
| 16 | 8 | 0,6250 | ✅ |
| 16 | 16 | **0,7033** | ✅ |
| 32 | 8 | 0,6717 | ✅ |

**As falhas são reportadas junto com os sucessos**, e elas ensinam o contrato de uso: com `over_fetch=8`
o pool de rerank é pequeno demais e o recall não chega ao baseline. **A quantização exige pool adequado**
— a mesma lição que reaparece no [ADR 0015](/decisions/0015-sbq-inline-keep-kill.md), onde uma
configuração de poucos bits topa em recall 0,52.

# Onde esta linha terminou

O SBQ é correto e economiza memória. Mas a tese de que ele traria **QPS** foi
[falsificada por medição](/decisions/0018-m57-sbq-inline-not-superior.md) — ele mede 0,35–0,77× do
throughput em precisão plena. O panorama completo das técnicas de quantização está em
[quantização vetorial](/features/19-quantizacao-vetorial.md).
