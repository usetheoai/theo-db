---
type: Measurement
title: comparação por query contra o ClickHouse — a tabela do primeiro baseline
description: A razão por query, com a coluna de pushdown e a de verificação A/B ao lado — porque uma razão sem saber se a otimização disparou não informa.
resource: git:f7c7b93:docs/benchmarks/m159-artifacts/per-query-comparison.md
tags: [benchmark, dados-brutos, clickhouse, por-query, pushdown]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m159pq
    resource: git:f7c7b93:docs/benchmarks/m159-artifacts/per-query-comparison.md
    title: M159 — per-query comparison
---

A tabela por query que sustenta o
[primeiro baseline real contra o adversário](/benchmarks/m159-clickhouse-gap-verdict.md).

# As quatro colunas que importam

Cada linha traz o tempo dos dois lados, **a razão**, **se a query passou pelo caminho acelerado**, e **se
a verificação A/B de resultado passou**.

**As duas últimas colunas são o que tornam a razão interpretável.** Uma razão alta pode significar duas
coisas completamente diferentes:

- a query **passou** pelo caminho acelerado e ainda assim é mais lenta — é um gap **de performance**;
- a query **não passou** — é um gap **de cobertura**, e a solução é rotear, não otimizar.

Sem a coluna de pushdown, as duas se confundem. E foi exatamente por não distinguir isso que
[m165](/benchmarks/m165-const-out-verdict.md) encontrou uma query com razão de 152× cuja causa era
**uma limitação pequena de roteamento** — que, corrigida, a levou a 10×.

# A dispersão que a tabela revela

As razões variam de cerca de 2× a mais de 20× entre queries. **Uma média sobre isso seria pouco
informativa** — a distribuição é o resultado, e é ela que permite o julgamento de "para quais classes de
query o alvo é alcançável" que o veredito emite.

# Evolução

A mesma tabela aparece em momentos posteriores, permitindo comparação direta:
[m165](/benchmarks/m165-artifacts/comparison-m165.md) e
[medição fresca](/benchmarks/clickbench-fresh-2026-07-27-artifacts/comparison-fresh.md).
