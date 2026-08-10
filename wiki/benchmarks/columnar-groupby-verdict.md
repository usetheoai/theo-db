---
type: Measurement
title: pushdown de GROUP BY no colunar — veredito
description: Ganho de 4,5 a 9,8×, com a agregação vetorizada adotada e apenas a cola sendo código próprio — o degrau de reuso da escada de parcimônia.
resource: git:f7c7b93:docs/benchmarks/columnar-groupby-verdict.md
tags: [benchmark, columnar, group-by, datafusion, reuso]
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
