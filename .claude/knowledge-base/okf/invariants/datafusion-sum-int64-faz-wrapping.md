---
type: Invariant
title: sum(Int64) do DataFusion faz add_wrapping — casar para Decimal128 antes de somar
description: Para saída numeric exata, o caminho é sum(cast(col AS Decimal128(38,0))) sobre i128; sum(Int64) silenciosamente dá a volta.
tags: [datafusion, aritmetica, colunar]
timestamp: 2026-07-30T00:00:00Z
---

# `sum(Int64)` do DataFusion faz `add_wrapping` — casar para `Decimal128` antes de somar

## O invariante

`sum(Int64)` no DataFusion usa **`add_wrapping`**: em overflow ele **dá a volta em silêncio**, sem erro. Para um
agregado que precisa ser byte-idêntico ao PostgreSQL — que promove para `numeric` — isso é resultado errado sem
sinal.

O caminho correto (M117 / ADR-N1):

```
sum(cast(col AS Decimal128(38,0)))   -- i128 exato
count(col)
```

e o datum PG-`numeric` é construído em Rust via `AnyNumeric` do pgrx. Para `avg`,
`AnyNumeric(sum) / AnyNumeric(count)` **é** literalmente o `numeric_div` do PostgreSQL — daí a byte-identidade.

## O corolário que economiza esforço

Saída `numeric` é **exata e associativa** (i128). Logo ela **não pode** divergir por tamanho de batch — ao
contrário de `float8`, onde a ordem de soma muda o último bit (IEEE 754). Numa revisão de tipos, não gaste
esforço procurando divergência em `numeric`; procure em `float`.

## Relacionados

- [invariant/chunk-group-e-a-unidade-de-tudo](chunk-group-e-a-unidade-de-tudo.md)
