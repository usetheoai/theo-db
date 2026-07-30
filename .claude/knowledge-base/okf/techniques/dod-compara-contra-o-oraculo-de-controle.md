---
type: Technique
title: Uma DoD é um número RELATIVO ao oráculo de controle — nunca um absoluto que ninguém demonstrou
description: Duas DoDs deste projeto tiveram de ser reescritas mid-flight porque pediam um absoluto que nem o SOTA permissivo atinge no mesmo dado.
resource: docs/adr/0030-m60-recall-parity-not-absolute-099.md
tags: [dod, planejamento, medicao, metodo]
timestamp: 2026-07-30T00:00:00Z
---

# Uma DoD é um número **relativo ao oráculo de controle** — nunca um absoluto que ninguém demonstrou

## As duas vezes que isto custou uma reescrita mid-flight

**M60 (ADR-0030).** A DoD dizia `recall@10 ≥ 0.99 a 500k×768d`. A medição head-to-head, mesmo corpus, GT exato:

| ef=1000, 500k×768d | recall@10 |
|---|---|
| **pgvector hnsw** (o oráculo) | **0.988** |
| theodb SBQ | 0.986 |
| theodb f32 | 0.974 |

> *"O gate 0.99 é um artefato do dado — **o próprio pgvector só chega a 0.988**."*

256 clusters gaussianos apertados em 768d produzem 10-vizinhos quase-equidistantes: o teto de recall@10 da classe
HNSW **naquela distribuição** fica abaixo de 0.99. A DoD pedia algo que o SOTA permissivo não entrega — e o
caminho SBQ já estava em **paridade** (0,986 vs 0,988, dentro do ruído de 1 slot de GT sobre 500).

**M71 (ADR-0031).** A DoD dizia `p50 ≤ pgvector a recall ≥ 0.99` — superioridade iso-recall. Medido: a iso-recall
0,996 a 100k, pgvector **2,13 ms (ef=100)** vs theodb **3,16 ms (ef=200)** — precisa **~2× o `ef`** a 100k e
**~5× a 500k**. A DoD foi reescrita para "melhoria de latência medida" (o entregável real: multi-entry `ep←W`,
**+29% QPS**, recall-neutral).

## A técnica

1. **Meça o oráculo de controle no MESMO dado primeiro.** Se o pgvector só faz 0,988 ali, `≥ 0,99` não é uma barra
   alta — é uma barra **fora da escala do experimento**.
2. **Escreva a DoD como uma relação** — "paridade com X medida no mesmo corpus", "≥ X% acima da baseline" — não
   como uma constante.
3. **Um absoluto só é legítimo quando é um requisito externo** (um SLA, um limite de formato, um contrato). Aí ele
   não é uma aposta: é uma restrição.
4. Reescrever a DoD por medição **não é afrouxar o gate** — é corrigir um instrumento. O que seria desonesto é
   declarar `0,99 ATINGIDO` mudando o dado até caber.

## O detalhe que fecha a cadeia de citação

O ADR-0031 diz corretamente "**~2× o `ef`** a 100k; **~5×** a 500k". O ADR-0035, mais tarde, escreveu "~1,8× o
`ef`" — que é o multiplicador de **latência**, não de `ef`. Dois ADRs do mesmo pilar, um certo e um comprimido.
Ver [numero-comprimido-na-cadeia-de-citacao](../failure-modes/numero-comprimido-na-cadeia-de-citacao.md).

## Relacionados

- [failure-mode/dados-sinteticos-degenerados](../failure-modes/dados-sinteticos-degenerados.md) — por que 0,99 era inalcançável ali
- [honest-negative/superioridade-vetorial-vs-scann](../honest-negatives/superioridade-vetorial-vs-scann.md)
- [technique/braco-de-controle-inalterado](braco-de-controle-inalterado.md)
