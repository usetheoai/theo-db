---
type: Measurement
title: agregados de saída numérica byte-idênticos
description: Soma e média de inteiros produzem saída em tipo de precisão arbitrária no PostgreSQL, então rotear essas formas exige reproduzir a semântica exata, não uma aproximação em ponto flutuante.
resource: git:f7c7b93:docs/benchmarks/numeric-output-aggregates-verdict.md
tags: [benchmark, columnar, numeric, precisao, byte-identico]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: numagg
    resource: git:f7c7b93:docs/benchmarks/numeric-output-aggregates-verdict.md
    title: Verdict — byte-identical numeric-output integer aggregates
    last_modified: 2026-07-19
---

Rotear soma e média de inteiros pelo caminho acelerado, com **identidade byte a byte** contra o executor
nativo.

# Por que esta classe é mais difícil do que parece

No PostgreSQL, somar inteiros largos e tirar média de inteiros produzem resultado em **tipo de precisão
arbitrária** — não em ponto flutuante.

**Um motor vetorizado que calcule isso em ponto flutuante dá resultado quase certo e byte-divergente.**
"Quase certo" é exatamente o que a garantia do pilar colunar não aceita: o contrato é
[byte-idêntico ao heap](/benchmarks/m114-columnar-aggregate-verdict.md), não "próximo o suficiente".

Então rotear estas formas exige **reproduzir a semântica exata**, incluindo o tipo de saída e o
arredondamento — e é por isso que elas precisaram de milestone próprio em vez de virem junto com as somas
de ponto flutuante.

# O padrão que se repete

É a mesma decisão de [m154](/benchmarks/m154-count-distinct.md), que recusou contagem distinta
aproximada, e de [m166](/benchmarks/m166-wide-sum-verdict.md), que admitiu soma de expressão **apenas na
classe provadamente livre de overflow**.

**Toda ampliação de cobertura no pilar colunar custa uma prova de equivalência semântica** — nunca uma
tolerância numérica.

# Método

Tabela colunar de 1M linhas contra heap idêntico, com paralelismo desligado para tornar a comparação
determinística.
