---
type: Measurement
title: No scan vetorial o custo é I/O (~50%) e sort (~36%) — a distância f32 é ~15%
description: Medido com profiler em 200k×128, estável em 5 runs e 3 pontos de probes; falsificou a premissa do M36 e reescopou o milestone para o gargalo real.
resource: .claude/knowledge-base/discoveries/blueprints/m36-quantization-in-index-blueprint.md
tags: [vetorial, perfil, gargalo, scan]
timestamp: 2026-07-30T00:00:00Z
---

# No scan vetorial o custo é **I/O e sort** — a distância f32 é ~15%

## O número (M36, `THEODB_SCAN_PROFILE=1`)

`theodb_ivfflat` sobre 200k×128 (vetores distintos, seed 42). As três fases do scan, **estáveis em 5 runs e em 3
pontos de probes**:

| probes | candidatos | **reads (I/O)** | **sort** | **score (distância f32)** |
|---|---|---|---|---|
| 10 | 10.216 | **51%** | 35% | **14%** |
| 50 | 50.332 | **49%** | 37% | **15%** |

A estabilidade em duas dimensões independentes (repetição **e** carga) é o que torna o número decision-grade:
não é uma amostra afortunada, é a forma do custo.

## O que isso implica, e o que não implica

**Implica:** qualquer lever que ataque só o **cálculo da distância** tem teto de ~15%, mesmo que o torne
instantâneo. Foi por isso que o M36 foi re-escopado e o M38 nasceu mirando o `reads`.

**Não implica** que quantizar seja inútil: quantizar reduz **bytes lidos** (ataca os 50%), não o tempo de
cálculo. A confusão entre "quantização acelera o score" e "quantização reduz o I/O" é o que produziu o escopo
errado do M36 — e é a mesma distinção que reaparece em
[codigos-quantizados-co-locados-nao-reduzem-io](../honest-negatives/codigos-quantizados-co-locados-nao-reduzem-io.md):
o ganho existe se, e só se, o layout permitir ler menos.

**Não generaliza** para outras formas de índice sem re-medir: é IVF com esta razão candidatos/páginas. Num grafo
HNSW em RAM quente a divisão é outra. O profiler existe — meça, não extrapole.

## Relacionados

- [technique/primeiro-checkbox-do-dod-e-a-medicao-que-mata](../techniques/primeiro-checkbox-do-dod-e-a-medicao-que-mata.md)
- [technique/instrumentar-em-vez-de-adivinhar](../techniques/instrumentar-em-vez-de-adivinhar.md)
- [honest-negative/codigos-quantizados-co-locados-nao-reduzem-io](../honest-negatives/codigos-quantizados-co-locados-nao-reduzem-io.md)
