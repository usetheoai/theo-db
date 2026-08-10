---
type: Measurement
title: m118 — retomar de descartados no ANN filtrado: honest-negative
description: Alcança recall perfeito e é ~7× mais lento que a referência; a medição é declarada direcional e de escala reduzida, longe do alvo do critério.
resource: git:f7c7b93:docs/benchmarks/m118-resume-discarded.md
tags: [benchmark, filtered-ann, honest-negative, escala-reduzida, m118]
milestone: M118
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m118
    resource: git:f7c7b93:docs/benchmarks/m118-resume-discarded.md
    title: M118 — Filtered ANN resume-from-discarded
    last_modified: 2026-07-20
---

**Veredito: honest-negative.**

| Engine | latência média | latência quente | recall@10 |
|---|---|---|---|
| próprio, com retomada | 16,03 ms | 14,70 ms | **1,0000** |
| referência, iterativo relaxado | 2,21 ms | 0,645 ms | 0,9300 |

**Recall perfeito, e ~7× mais lento** — e a leitura honesta é que o recall superior **não compensa** a
diferença de latência nesta forma.

# As ressalvas que enquadram

**A máquina NÃO estava quieta** — havia outro banco co-residente. **A escala é reduzida** e a medição é
declarada **direcional**, explicitamente **não** o alvo do critério, que pediria escala real em máquina
dedicada.

**Declarar que a medição não é a que o critério pede** é o que impede um negativo direcional de fechar
prematuramente uma linha de investigação. O resultado sugere; ele não decide.

# Contexto

O caminho que efetivamente resolveu filtro seletivo foi outro — o **filtro inline** de
[m90](/benchmarks/m90-inline-filter.md), que evita o problema em vez de remediá-lo, e o
[Custom Scan](/benchmarks/archive/m92-arbitrary-where.md) para predicados arbitrários.
