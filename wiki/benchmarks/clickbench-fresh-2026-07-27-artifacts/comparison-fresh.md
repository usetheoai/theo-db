---
type: Measurement
title: comparação por query — medição fresca, com roteamento asserido
description: A terceira instância da mesma tabela, agora produzida por um harness que assere o roteamento por query, removendo a dúvida de falso verde das anteriores.
resource: git:f7c7b93:docs/benchmarks/clickbench-fresh-2026-07-27-artifacts/comparison-fresh.md
tags: [benchmark, dados-brutos, clickhouse, harness-endurecido, serie]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: cbfreshcmp
    resource: git:f7c7b93:docs/benchmarks/clickbench-fresh-2026-07-27-artifacts/comparison-fresh.md
    title: ClickBench fresh — comparison table
    last_modified: 2026-07-27
---

A **terceira** instância da mesma tabela por query, depois do
[baseline](/benchmarks/m159-artifacts/per-query-comparison.md) e da
[medição pós-correção](/benchmarks/m165-artifacts/comparison-m165.md).

# O que mudou no instrumento, não só no sistema

Esta tabela é produzida por um **harness endurecido**, que **assere o roteamento por query** — de modo
que uma agregação **recusada não pode passar como verde trivial** por divergência zero.

**Isso torna esta medição mais confiável que as anteriores**, e vale dizer com todas as letras: parte da
melhora aparente entre as tabelas poderia, em princípio, vir de queries antes contadas como aceleradas
sem estar. **Com o roteamento asserido, essa dúvida some.**

Reconhecer que **o instrumento melhorou junto com o sistema** — e que isso afeta a comparabilidade da
série — é o tipo de ressalva que raramente acompanha uma sequência de números melhorando.

# O valor da série

Três instâncias do mesmo formato, em três momentos, com o instrumento evoluindo. Juntas, elas mostram
não só **quanto** o gap caiu, mas **quais classes de query** se moveram — que é a informação que orienta
o trabalho seguinte.

O veredito correspondente é
[a medição fresca](/benchmarks/clickbench-fresh-vs-clickhouse-2026-07-27.md), onde o gap é reportado como
tendo caído aproximadamente pela metade.
