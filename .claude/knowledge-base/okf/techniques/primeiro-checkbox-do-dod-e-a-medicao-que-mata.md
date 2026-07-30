---
type: Technique
title: O PRIMEIRO checkbox do DoD é a medição que pode matar o milestone — e duas vezes ela matou
description: M36 e M38 gatearam explicitamente na medição da própria premissa; as duas premissas foram falsificadas antes de qualquer implementação. É o gate mais barato que existe.
resource: .claude/knowledge-base/discoveries/blueprints/m36-quantization-in-index-blueprint.md
tags: [dod, planejamento, medicao, parsimonia]
timestamp: 2026-07-30T00:00:00Z
---

# O **primeiro** checkbox do DoD é a medição que pode **matar** o milestone

## Duas vezes o gate salvou o milestone matando a premissa

**M36 — quantização-no-índice.** A hipótese: o custo por candidato é dominado pela **distância f32**, e quantizar
fecharia o gap de ~25× vs ScaNN. O DoD gateou nisso, literalmente no checkbox #1: *"`THEODB_SCAN_PROFILE=1`
confirma que `score_us` domina `reads_us`"*. Medido (200k×128, 5 runs, 3 pontos de probes):

| probes | reads (I/O) | sort | **score (distância)** |
|---|---|---|---|
| 10 | 51% | 35% | **14%** |
| 50 | 49% | 37% | **15%** |

A distância **não era o gargalo** — era ~15% do custo. O milestone inteiro atacava o lugar errado, e isso custou
uma medição, não uma implementação.

**M38 — quantização de I/O.** Escopado do split do M36 para atacar o `reads` (o gargalo real). O DoD gateou em
"recall preservado ≥ baseline", **com cláusula de escalada explícita** escrita antes de medir. Em SIFT real
120k×128, o baseline f32 dava recall **1,0000**; o SBQ chegava a **0,947** (bits=4, over_fetch=40) e **~0,77** a
1 bit. O gate não era atingível com SBQ — e o próprio DoD já dizia o que fazer nesse caso.

## A técnica

1. **Identifique a premissa da qual o milestone depende.** Quase todo milestone tem uma: "X é o gargalo", "Y
   preserva a qualidade", "Z cabe na memória".
2. **Transforme-a no checkbox #1 do DoD**, escrito como uma medição com um oráculo nomeado — não como uma
   afirmação de contexto.
3. **Escreva a cláusula de escalada junto**, antes de saber o resultado. O M38 fez isso, e por isso a falsificação
   virou uma decisão preparada em vez de uma crise.
4. Uma premissa marcada `UNBENCHMARKED` numa análise **é o primeiro número a levantar** — foi exatamente o que o
   `council-vector-ann` disse no M36, e estava certo.

> Um milestone que falsifica a própria premissa na primeira semana **não fracassou** — ele economizou o resto.

## Relacionados

- [technique/dod-compara-contra-o-oraculo-de-controle](dod-compara-contra-o-oraculo-de-controle.md) — a outra metade: contra **o quê** o número é comparado
- [measurement/custo-do-scan-vetorial-nao-e-a-distancia](../measurements/custo-do-scan-vetorial-nao-e-a-distancia.md) — o número do M36
- [technique/medir-o-incremento-isolado-antes-de-pagar-o-caro](medir-o-incremento-isolado-antes-de-pagar-o-caro.md)
