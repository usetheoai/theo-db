---
type: Measurement
title: m166 — matriz de cobertura de tipos, execução
description: A tabela caso a caso da matriz, em que cada linha declara o comportamento esperado antes do observado — o que torna uma recusa correta um sucesso, não uma falha.
resource: git:f7c7b93:docs/benchmarks/m166-type-coverage.md
tags: [benchmark, tipos, matriz, fail-closed, m166]
milestone: M166
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m166tc
    resource: git:f7c7b93:docs/benchmarks/m166-type-coverage.md
    title: M166 — type coverage
---

Execução da matriz de tipos estabelecida em [m163](/benchmarks/m163-type-coverage-verdict.md), agora
sobre a cobertura ampliada.

# A forma da tabela é o método

Cada linha traz **quatro campos**: o caso, **o que se espera**, o que aconteceu, e a divergência.

| caso | esperado | obtido | divergência |
|---|---|---|---|
| contagem | rotear | ok | 0 |
| soma de inteiro | rotear | ok | 0 |
| lista com nulo | **recusar** | **recusou** | — |
| chave inteira estreita | rotear | ok | 0 |

**Declarar a expectativa antes do resultado** é o que transforma uma **recusa em sucesso**. Sem a coluna
de expectativa, "recusou" pareceria falha de cobertura; com ela, é o **contrato fail-closed funcionando**.

E note a linha de lista com nulo: **a divergência não é zero, é vazia** — porque não houve comparação a
fazer. Um harness que registrasse zero ali estaria mentindo por preenchimento automático.

# Por que essa matriz existe

Porque o benchmark padrão **não contém** esses valores. A cobertura de queries e a cobertura do **espaço
de entrada** são coisas diferentes, e confundi-las foi o que deixou bugs de classe de tipo sobreviverem
repetidamente — o diagnóstico do m163.
