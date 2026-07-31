---
type: Measurement
title: Consumir por chunk-group NÃO mudou resultado algum — float8 bit a bit, e 35/35 do espaço de tipos
description: Medido 2026-07-31 no M169: float8 idêntico bit a bit sobre dado adversarial (0.1 + 1e17 esparso), e o harness de tipos deu 35/35 com positive_control diverged=2 sobre o caminho streaming. Formas medidas, não prova para toda entrada.
resource: benchmarks/m169_float_assoc.sql
tags: [float, ieee754, streaming, colunar, agregado, associatividade]
timestamp: 2026-07-31T00:00:00Z
---

# Consumir por chunk-group não mudou `sum`/`avg` de `float8`

## A pergunta que obrigava a medir

O M169 fez os agregados colunares consumirem a relação **um chunk-group por vez** em vez de um `RecordBatch`
único. Isso muda a **ordem** em que os valores entram no acumulador — e adição em IEEE-754 **não é associativa**.
Se `sum(float8)` dependesse do tamanho do chunk-group, o milestone teria trocado um defeito barulhento (o
`byte array offset overflow`) por um silencioso, que é estritamente pior.

## O dado, escolhido para expor a ordem

25.000 linhas, 3 chunk-groups (o último parcial):

```sql
CASE WHEN g % 10000 = 0 THEN 1e17 ELSE 0.1 END
```

`0.1` não tem representação binária exata, e `ulp(1e17) = 16`: somar o `1e17` **primeiro** faz cada `0.1`
seguinte desaparecer no arredondamento; somar os `0.1` primeiro preserva a soma parcial. Dado de magnitude
uniforme esconderia a diferença — este a expõe.

## O resultado

| métrica | eager | streaming |
|---|---|---|
| `sum(f)` | `2.00000000000001e+17` | `2.00000000000001e+17` |
| `avg(f)` | `8000000000000.04` | `8000000000000.04` |

Comparados como texto com `extra_float_digits = 3`: no PG ≥ 12 `float8out` emite a representação **mais curta que
faz round-trip exato**, então igualdade de texto é igualdade de bits.

**Controle positivo:** corrompendo o braço streaming em **1 ULP** (`…02e+17`), o gate aborta com `rc=3`. Sem esse
braço, o "idêntico" seria um verde que não prova nada — divergência de associatividade tem exatamente essa ordem
de grandeza.

## O limite, dito antes de alguém generalizar

Isto é **uma forma medida**, não uma prova de independência de ordem para toda entrada. O que foi verificado:
uma distribuição adversarial, 3 chunk-groups, `sum` e `avg`. Não foram medidos `stddev`/`variance`, nem
cardinalidades onde o agrupamento reordena, nem tamanhos de chunk-group diferentes de 10.000 (a constante é fixa,
então o eixo nem é variável hoje).

O gate `benchmarks/m169_float_assoc.sql` fica no repositório justamente porque a resposta pode mudar com uma
versão do DataFusion — uma observação de uma vez só não sobrevive a upgrade.

## O espaço de tipos INTEIRO, medido no mesmo binário (2026-07-31)

O gate acima responde uma pergunta estreita (`float8`). A larga — *o streaming mudou algum resultado, em qualquer
tipo?* — foi respondida rodando `benchmarks/columnar_type_ab.py` com a GUC no default (**on**), de modo que todo
caso passou pelo caminho novo:

**35/35 casos como esperado**, `positive_control diverged=2`, 2000 linhas
(`docs/benchmarks/m169-type-coverage.md`). Cobre int2/int4/int8, float4/float8, bool, texto, timestamp/date/
timestamptz, colação nomeada, `IN`-list, `const_out`, group-expr e top-k — cada um ou **byte-idêntico quando
roteia** ou **declinando corretamente**.

O `positive_control` é o que impede esse "35/35" de ser um verde vazio: sem ele, um harness quebrado reportaria
35/35 exatamente igual.

**Armadilha operacional do harness, para não custar uma terceira vez:** ele **DROPa e recria `hits`** com um
schema sintético de 2000 linhas. Rodá-lo contra a base que tem o ClickBench destrói a tabela — aconteceu duas
vezes no M167. Existe um guard que recusa quando o `hits` da base parece ClickBench; obedeça-o e aponte
`PGDATABASE` para uma base scratch.

## Por que o ClickBench não responderia isto

Todas as colunas de `SUM`/`AVG` do ClickBench são **inteiras** (`AdvEngineID`, `IsRefresh`, `ResolutionWidth` são
`SMALLINT`; `UserID` é `BIGINT`). O A/B do benchmark prova o espaço de **dados** e é cego ao espaço de **tipos** —
ver [o A/B prova o espaço de dados, não o de tipos](../failure-modes/ab-prova-o-espaco-de-dados-nao-o-de-tipos.md).

## Relacionados

- [failure-mode/ab-prova-o-espaco-de-dados-nao-o-de-tipos](../failure-modes/ab-prova-o-espaco-de-dados-nao-o-de-tipos.md)
- [invariant/datafusion-sum-int64-faz-wrapping](../invariants/datafusion-sum-int64-faz-wrapping.md) — a armadilha vizinha, em inteiro
- [invariant/chunk-group-e-a-unidade-de-tudo](../invariants/chunk-group-e-a-unidade-de-tudo.md)
- [technique/controle-positivo](../techniques/controle-positivo.md)
