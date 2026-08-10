---
type: Measurement
title: comparação por query após a correção de roteamento
description: A mesma tabela do baseline, no mesmo formato, permitindo comparação direta — e nela aparecem razões abaixo de 1, isto é, queries em que o sistema é mais rápido.
resource: git:f7c7b93:docs/benchmarks/m165-artifacts/comparison-m165.md
tags: [benchmark, dados-brutos, clickhouse, comparacao, evolucao]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m165cmp
    resource: git:f7c7b93:docs/benchmarks/m165-artifacts/comparison-m165.md
    title: M165 — comparison table
---

A mesma tabela por query do
[baseline anterior](/benchmarks/m159-artifacts/per-query-comparison.md), **no mesmo formato** — o que é
justamente o que permite comparar as duas diretamente.

**Manter o formato estável entre medições** é uma escolha simples com efeito grande: mudanças de layout
entre execuções tornam impossível ver evolução sem retrabalho.

# O que a tabela mostra

As razões caíram substancialmente em relação ao baseline — e há linhas com **razão abaixo de 1**, isto é,
queries em que o sistema é **mais rápido** que o adversário.

Essas linhas importam porque **contradizem uma leitura monolítica do gap**: não existe "somos N× mais
lentos"; existe uma **distribuição** em que algumas classes de query já estão à frente e outras muito
atrás.

É essa distribuição que sustenta um veredito por classe, e não um multiplicador único — a mesma razão
pela qual o [veredito vetorial](/decisions/0035-m73-northstar-vector-verdict.md) qualifica o resultado
por **regime** em vez de dar um número só.

# O ganho específico deste momento

A correção que este artefato acompanha levou **uma query de 152× para 10×** — ver
[m165](/benchmarks/m165-const-out-verdict.md). Uma única linha da tabela mudando de ordem de grandeza é
o que a distribuição, e não a média, deixa visível.
