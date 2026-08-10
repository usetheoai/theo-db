---
type: Measurement
title: m114 — completude de agregados colunares: cada forma admitida e cada forma recusada
description: Verifica que as formas aceitas dão resultado idêntico ao nativo E que as recusadas caem no plano nativo continuando corretas — as duas metades do contrato.
resource: git:f7c7b93:docs/benchmarks/m114-columnar-aggregate-verdict.md
tags: [benchmark, columnar, agregados, fail-safe, contrato, m114]
milestone: M114
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m114
    resource: git:f7c7b93:docs/benchmarks/m114-columnar-aggregate-verdict.md
    title: M114 — columnar aggregate completeness
    last_modified: 2026-07-19
---

# O contrato que este benchmark verifica

Numa tabela colunar de 1M linhas contra uma tabela heap idêntica:

- **cada forma de agregado ADMITIDA** tem o resultado comparado ao nativo — valor escalar, ou o conjunto
  agrupado completo quando há agrupamento e filtro;
- **cada forma RECUSADA** é asserida **não** ser o caminho acelerado, **e continuar correta**.

**Verificar as recusas é a metade que costuma faltar.** Um pushdown que aceita o que não deveria produz
resultado errado; um que recusa e **não** cai corretamente no plano nativo produz erro. Testar só as
aceitas cobre metade do risco.

Isso torna concreto o comportamento **fail-safe** que a feature declara: o que não é admitido **cai para
o plano nativo**, mantendo correção e perdendo apenas a aceleração.

# O ganho, medido de forma isolada

O speedup é medido **ligando e desligando o pushdown sobre a MESMA tabela** — o que isola o executor da
variável de storage, o mesmo cuidado do [m100](/benchmarks/m100-datafusion-executor.md).

# Contexto

A feature é [analítico colunar](/features/14-analitico-colunar.md); a composabilidade com expressões
envolventes é [m115](/benchmarks/m115-columnar-composability-verdict.md).
