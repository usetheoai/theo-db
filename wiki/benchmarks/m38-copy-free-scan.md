---
type: Measurement
title: m38 — investigação do gargalo de leitura: uma medição, não um ganho
description: Fecha três hipóteses de uma vez, incluindo a de que a quantização escalar preservaria recall; a mudança de código é estritamente melhor mas o benchmark não sustenta claim.
resource: git:f7c7b93:docs/benchmarks/m38-copy-free-scan.md
tags: [benchmark, honest-negative, sbq, recall, variancia, m38]
milestone: M38
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m38
    resource: git:f7c7b93:docs/benchmarks/m38-copy-free-scan.md
    title: M38 — Investigação do gargalo reads
---

> **Resultado honesto declarado no topo:** este milestone entregou uma **MEDIÇÃO** que fechou três
> hipóteses, **não** um ganho de QPS. A mudança de código é recall-idêntica e estritamente melhor, mas
> **o benchmark não sustenta claim de performance**.

Declarar isso antes dos números é o que impede o leitor de extrair do artefato uma conclusão que ele não
tem.

# Hipótese 1 — a quantização escalar preserva recall? **Falsificado**

Contra o scan exato em SIFT real de 120k, com o baseline em recall 1,0000:

| Configuração | recall@10 |
|---|---|
| 1 bit | 0,774 |
| 2 bits | 0,854 |
| 4 bits | 0,947 |

**Mesmo na melhor configuração, fica abaixo de 1,0.** A explicação mecânica: quantização **escalar**
perde informação de ranking demais por byte — e é exatamente por isso que as implementações de referência
usam quantização **de produto** com distância assimétrica por tabela de lookup.

**O gate de recall preservado não é atingível por esse caminho**, e o achado generaliza além do dataset.

# Hipótese 2 — a cópia dupla é o gargalo ponta a ponta? **Falsificado**

Eliminá-la é correto e estritamente melhor, mas **o efeito medido é menor que a variância da máquina**.

# A ressalva de variância, e o que ela impede

A medição roda numa CPU móvel com throttling térmico **e sob carga pesada de containers durante a
corrida** — o documento declara que **o efeito medido é MENOR que essa variância**.

Reconhecer isso significa recusar-se a publicar um ganho que o instrumento não consegue distinguir de
ruído. A mudança fica no código porque é **correta**, não porque foi medida como mais rápida.

# Onde esta linha se encaixa

É o segundo de uma sequência de honest-negatives que reorientou o trabalho de performance —
[m36](/benchmarks/archive/m36-scan-optimization.md), este, e [m39](/benchmarks/m39-pq.md) — todos fechando
caminhos antes de investimento caro.
