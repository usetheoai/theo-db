---
type: Technology
title: DataFusion
description: O motor de query vetorizado em Rust sobre Arrow; é o executor analítico do TheoDB e a peça que tornou possível remover a dependência C++.
resource: https://datafusion.apache.org/
tags: [tecnologia, rust, columnar, executor, arrow]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: df-site
    resource: https://datafusion.apache.org/
    title: Apache DataFusion, site oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O DataFusion é um motor de execução de query **vetorizado, escrito em Rust**, construído sobre
[Arrow](/technologies/arrow.md), sob licença permissiva. Ele oferece execução em lotes colunares,
agregação por hash, filtros e leitura de [Parquet](/technologies/parquet.md), como biblioteca embutível
em vez de servidor.[^recalled]

# Papel neste acervo

**É o executor analítico.** As agregações, agrupamentos e filtros vetorizados do
[colunar próprio](/features/14-analitico-colunar.md) rodam por ele, com o projeto escrevendo **a cola** —
aceitação de forma de query, conversão de tipos e integração com o planner — e **adotando o algoritmo**.

Essa divisão é a regra de não reinventar aplicada corretamente, e está explícita em
[m100](/benchmarks/m100-datafusion-executor.md) e no
[verdict de agrupamento](/benchmarks/columnar-groupby-verdict.md).

# A decisão que ele destravou

O gate de viabilidade ([m98](/benchmarks/m98-coexistence.md)) provou que ele **coexiste com o framework
de extensão num único crate** e **executa dentro de um backend do PostgreSQL** — sem o quê o pilar
colunar próprio não existiria.

E, por já estar no binário, ele tornou possível a decisão mais econômica da linhagem: o
[spike de leitor Parquet](/benchmarks/parquet-reader-owncode-spike.md) mediu que ler Parquet com ele
custa **+9 MB** contra os 118 MB do bundle C++ — porque **faltava apenas ligar o leitor**, não adicionar
um motor.

# O limite que ele impõe

Ele é **outro motor**, com seus próprios tipos e sua própria semântica numérica. É por isso que toda
ampliação de cobertura do pilar colunar exige **prova de equivalência** — de collation
([m153](/benchmarks/m153-groupby-text.md)), de exatidão
([m154](/benchmarks/m154-count-distinct.md)), de ausência de overflow
([m166](/benchmarks/m166-wide-sum-verdict.md)) e de tipo de saída
([agregados numéricos](/benchmarks/numeric-output-aggregates-verdict.md)).

E é dele o comportamento de spill cuja falha, sob orçamento apertado de descritores, produziu a regressão
registrada no [ADR 0059](/decisions/0059-m169-fail-open-cobre-falha-de-spill.md).

[^df-site]: Apache DataFusion, site oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
