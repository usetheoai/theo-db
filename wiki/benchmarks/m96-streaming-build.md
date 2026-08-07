---
type: Measurement
title: m96 — build em streaming: memória independente de N
description: A rota via estrutura de ordenação do PostgreSQL, que o milestone anterior adiara por risco, entrega o limite que o streaming parcial não alcançava.
resource: git:f7c7b93:docs/benchmarks/m96-streaming-build.md
tags: [benchmark, build, memoria, tuplesort, escala, m96]
milestone: M96
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m96
    resource: git:f7c7b93:docs/benchmarks/m96-streaming-build.md
    title: M96 — tuplesort-streaming ambuild
    last_modified: 2026-07-13
---

Fecha o limite que o [m89](/benchmarks/m89-ambuild-streaming.md) deixara explícito: **o pico ainda
carregava uma cópia do corpus**, então a escala seguinte não caberia.

# O desenho

O build **nunca materializa o corpus**. São **duas varreduras do heap**: uma para treinar por amostragem,
outra para atribuir em streaming para uma estrutura de ordenação **que derrama para disco** quando excede
a memória de manutenção configurada. A leitura de volta vem **agrupada por lista**, com **uma lista em
voo por vez**.

**O alvo é memória proporcional à configuração mais a amostra — independente de N.**

Essa é a diferença qualitativa: o milestone anterior reduziu a constante; este **remove a dependência de
N**.

# O que se mede

Pico de residência do backend, amostrado durante a criação do índice, contra o tamanho da base — em N
crescente. **A forma da curva é o resultado**, não um ponto isolado: uma linha plana prova a
independência; uma linha crescente a refutaria.

# Sobre a rota

É exatamente a rota via FFI que o [m89](/benchmarks/m89-ambuild-streaming.md) **adiara**, por ser risco
alto sem necessidade medida naquela escala. **Quando a necessidade apareceu, a rota foi tomada.**

Adiar por risco e retomar quando a evidência exige é o funcionamento correto de um gate — o oposto tanto
de construir cedo demais quanto de nunca construir.
