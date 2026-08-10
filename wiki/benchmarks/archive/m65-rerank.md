---
type: Measurement
title: m65 — rerank por cross-encoder em BEIR: honest-negative
description: Degrada o nDCG em 3,8% ao custo de ~2 s por query, exatamente como a literatura previa para corpora fora de distribuição — e a superfície embarca mesmo assim, por razão declarada.
resource: git:f7c7b93:docs/benchmarks/archive/m65-rerank.md
tags: [benchmark, rerank, beir, honest-negative, arquivo, m65]
dataset: BEIR SciFact
milestone: M65
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m65
    resource: git:f7c7b93:docs/benchmarks/archive/m65-rerank.md
    title: M65 — ai.rerank benchmark
---

**Veredito: honest-negative.** O rerank **degradou o nDCG@10 em −3,8%** — de 0,7327 para 0,6947 — ao
custo de **~1,96 s de p50 por query**. O recall a 50 ficou conservado, o que serve de sanidade.

# O gate que este resultado exercita

O critério do milestone era explícito: a superfície **só é aceita se melhorar a qualidade
mensuravelmente**, com **honest-negative caso não melhore**.

**O gate previa o resultado negativo, e ele veio.**

# Por que a superfície embarca assim mesmo

Porque o valor declarado **não é ganho universal de qualidade**: é **fechar o ciclo de recuperação e
rerank de forma medível e independente de modelo**.

A literatura já dizia que o ganho **não é universal** — cross-encoders prontos degradam em corpora fora
da distribuição de treino, e o corpus medido é justamente um domínio especializado.

As consequências práticas ficam registradas: **rerank é opt-in, nunca default**, e **um reranker
in-domain pode ganhar onde este perdeu — mas isso exige o próprio benchmark, não extrapolação**.

**Embarcar uma superfície declarando que ela não entrega ganho garantido** é diferente de embarcá-la
alegando que entrega. A decisão completa é o
[ADR 0024](/decisions/0024-m65-ai-rerank-cross-encoder.md), e a feature é
[ranquear resultados](/features/09-ranquear-resultados.md).
