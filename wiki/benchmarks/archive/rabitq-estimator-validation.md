---
type: Measurement
title: validação do estimador RaBitQ por Monte Carlo hermético
description: Valida o núcleo matemático sem banco e sem máquina remota — um teste determinístico que compara a estimativa a partir do código contra a distância verdadeira, por profundidade de bits.
resource: git:f7c7b93:docs/benchmarks/archive/rabitq-estimator-validation.md
tags: [benchmark, estimador, monte-carlo, hermetico, quantizacao, arquivo]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: rbqest
    resource: git:f7c7b93:docs/benchmarks/archive/rabitq-estimator-validation.md
    title: Extended multi-bit RaBitQ — estimator validation
    last_modified: 2026-07-17
---

Validação do **núcleo matemático** do quantizador, isolada de tudo o mais.

# O que "hermético" compra

O teste roda **sem banco de dados e sem máquina remota** — apenas o núcleo, num teste da própria
linguagem.

Isso significa **zero variância de ambiente**: o resultado é determinístico e reprodutível por qualquer
pessoa, a qualquer momento, sem provisionar nada.

**Para validar uma propriedade matemática, o ambiente é ruído puro.** Rodar isso dentro do banco
adicionaria variáveis sem adicionar informação — e é o inverso do que a linhagem aprendeu sobre
**performance**, onde medir fora do banco engana ([m75](/benchmarks/m75-ivf-aqah-spike.md),
[e2 spike](/benchmarks/e2-symqg-spike.md)).

**A escolha do ambiente segue a natureza da propriedade: correção matemática valida-se isolada;
desempenho valida-se no lugar real.**

# O que é medido

O **erro do estimador contra a distância verdadeira, por profundidade de bits** — comparando a estimativa
computada **a partir do código quantizado, sem tocar o vetor original**, com o valor exato.

A curva de erro por profundidade é o que permite escolher a profundidade certa para um alvo de recall, em
vez de descobrir empiricamente depois.

# Nota

O algoritmo é **reimplementação própria** de trabalho permissivo; a árvore vendorizada original foi
**deletada**, conforme o [ADR 0046](/decisions/0046-rabitq-vendor-tree-deleted.md). O veredito dentro do
banco está em [e1](/benchmarks/e1-rabitq-inpg-verdict.md).
