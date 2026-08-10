---
type: Measurement
title: previsão de orçamento para o ClickBench oficial
description: Preços consultados por API dos provedores, não estimados — a regra de evidência primária aplicada a custo, e o pré-requisito para o número publicável.
resource: git:f7c7b93:docs/benchmarks/clickbench-official-budget.md
tags: [benchmark, custo, infraestrutura, evidencia-primaria, planejamento]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: cbbudget
    resource: git:f7c7b93:docs/benchmarks/clickbench-official-budget.md
    title: Previsão de budget — ClickBench oficial
    last_modified: 2026-07-24
---

# A regra aplicada a dinheiro

> **Preços consultados via API**, **não estimados**.

O repositório exige que números de performance sejam medidos e não estimados. **Este artefato aplica a
mesma regra a custo** — consultando as APIs de faturamento e de preços dos provedores em vez de usar
valores de memória ou de tabelas desatualizadas.

É consistente: um orçamento errado leva a uma decisão de infraestrutura errada, do mesmo modo que um
número de latência inventado leva a uma decisão técnica errada.

# Por que este artefato existe

Todos os benchmarks da série carregam a mesma ressalva — **não é o hardware canônico, então os tempos
não são comparáveis a leaderboard**.

**Este documento é o que torna o número canônico alcançável:** ele dimensiona o custo de rodar na
máquina de referência, que é o que falta para produzir o **número publicável**.

É também a segunda metade do requisito de claim comparativo que
[m45](/benchmarks/m45-pareto-sift1m.md) declarara em aberto — a comparabilidade externa que o
[ADR 0050](/decisions/0050-official-benchmark-adopt-and-wrap.md) foi criado para endereçar.

# A disciplina de não deixar máquina ociosa

O planejamento acompanha a prática visível nos demais artefatos: máquinas **efêmeras, criadas para a
medição e destruídas ao final**.
