---
type: Measurement
title: pushdown de GROUP BY no colunar — veredito
description: Ganho de 4,5 a 9,8× para chave INTEIRA. Chave de TEXTO recusa o pushdown, e a guarda está certa — nosso executor emite byte-wise e o PostgreSQL promete ordem de collation. O B-097 entregou a forma de plano que o B-095 previa como saída, e o agregado vetorizado continua ausente: a forma não era o bloqueio.
resource: git:f7c7b93:docs/benchmarks/columnar-groupby-verdict.md
tags: [benchmark, columnar, group-by, datafusion, reuso, b-095, b-097, limite-declarado]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: cgb
    resource: git:f7c7b93:docs/benchmarks/columnar-groupby-verdict.md
    title: theodb_columnar GROUP BY pushdown verdict
    last_modified: 2026-07-19
---

**Ganho medido: 4,53 a 9,75×**, numa tabela colunar de 1M linhas contra heap idêntico.

# O que é próprio e o que é adotado

A agregação por hash é **adotada** do motor vetorizado; **a cola é código próprio** — aceitação da forma
da query, análise da chave de agrupamento e do layout de saída, cursor de múltiplas linhas, e a conversão
reversa dos valores para o formato do PostgreSQL.

**Essa divisão é a regra de não reinventar aplicada corretamente:** o algoritmo difícil vem de uma
biblioteca madura e permissiva; o que é escrito é a integração, que ninguém mais poderia escrever.

E a conversão reversa é a parte não trivial da cola: **o resultado precisa voltar como dado do
PostgreSQL, com os tipos exatos**, sob pena de violar a garantia byte-idêntica.

# O contrato mantido

Como toda ampliação do pilar colunar, esta vem com verificação de que as formas aceitas dão resultado
idêntico ao nativo, e as recusadas caem para o plano nativo continuando corretas — o contrato de
[m114](/benchmarks/m114-columnar-aggregate-verdict.md).

# Contexto

É uma das capacidades listadas em [analítico colunar](/features/14-analitico-colunar.md), e um dos ganhos
que a feature reporta.

# O limite que este veredito não dizia: chave de TEXTO recusa o pushdown

Medido em 2026-08-21 pelo [[B-095]], e depois re-medido pelo [[B-097]]. O ganho acima é real para
chave **inteira**. Para chave de **texto** o pushdown **não engata**, e o usuário não tinha onde ler
isso:

```
GROUP BY <int>   → Custom Scan (theodb_columnar_agg)      ← pushdown
GROUP BY <text>  → Seq Scan → Sort → GroupAggregate       ← sem pushdown
```

Com `THEODB_ADMIT_TRACE=1` a razão aparece: `swap_sorted_text_group_not_resorted`
(`theodb_rs/src/am/columnar_agg.rs:2028-2033`).

## A guarda está CERTA, e é isso que torna o limite interessante

Não é bug de omissão. Nosso executor emite grupos em ordem **byte-wise**; o PostgreSQL promete ordem
de **collation**. Trocar o plano sem um `Sort` completo acima devolveria a ordem errada — resposta
incorreta, rápida. **Recusar é o comportamento certo.**

## O que o [[B-097]] mudou, e o que não mudou

O [[B-095]] previa duas saídas: emitir em ordem de collation, **ou** fazer o planner produzir a forma
com `Sort` acima. O [[B-097]] entregou a segunda — o planner passou a ver a contagem real e a forma do
plano mudou nos seis pontos do sweep, de `GroupAggregate` para `Sort` + `HashAggregate`.

**E o `theodb_columnar_agg` continua ausente.** Medido nos seis pontos, de 10K a 2M linhas: o portão de
caminho analítico reprova exatamente como antes. **A forma do plano não era o bloqueio, ou não era o
único** — o que refuta a saída nº 2 hipotetizada pelo próprio item. A saída nº 1, emitir em ordem de
collation, segue sem exploração.

## Para quem usa

Uma tabela `USING theodb_columnar` agregada por coluna de texto **não recebe o agregado vetorizado**,
e a diferença medida entre as duas formas é de ordem de grandeza (13× no docstring do adapter). Se a
carga agrupa por texto e o ganho colunar importa, a chave inteira é o caminho que hoje entrega — e
isto está escrito aqui em vez de ser descoberto lendo um `EXPLAIN`.

Evidência: [[b058-crossover-colunar]] § Re-medido em 2026-08-21.
