---
type: Measurement
title: m136 — gates mecânicos de qualidade e Postgres com asserções no CI
description: Retrofit honesto sobre ~30 mil linhas que nunca tiveram gate — o que dava para mecanizar de vez foi mecanizado, e o resto entrou em baseline com data de expiração.
resource: git:f7c7b93:docs/benchmarks/m136-quality-gates.md
tags: [benchmark, ci, gates, cassert, retrofit, divida-tecnica, m136]
milestone: M136
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m136
    resource: git:f7c7b93:docs/benchmarks/m136-quality-gates.md
    title: M136 — gates mecânicos de qualidade + Postgres cassert no CI
    last_modified: 2026-07-21
---

# O ponto de partida

Cerca de 30 mil linhas **nunca haviam tido gate mecânico** da linguagem: mais de mil avisos do linter,
código nunca formatado, e a restrição de licença dependendo **apenas de vigilância humana**.

# A decisão de retrofit, e por que ela é boa

**Seis gates, cada um verificado verde por medição direta.** E o tratamento da dívida existente é o
detalhe que vale copiar:

- **A formatação foi resolvida de vez** — é **um comando mecânico**, então não há razão para adiar.
- **O restante entrou em baseline permitido, com data de expiração e plano de queima** — porque mil
  avisos são mil decisões individuais, e resolvê-los de uma vez seria mudança gigante e arriscada.

**A decisão de "não ficar no meio" foi resolvida por viabilidade, não por preferência.** Um baseline com
sunset é dívida **declarada e datada**; um baseline sem sunset é dívida **permanente disfarçada de
processo**.

# O gate que pega o que linter não pega

Rodar o PostgreSQL **com asserções ativadas e randomização de memória alocada** no CI é o que expõe
violações de invariante que um build normal tolera silenciosamente — especialmente relevante numa
extensão que faz FFI e manipula páginas.

É a rede que o [ADR 0051](/decisions/0051-m139-tantivy-pg-page-directory-design.md) cita como cobertura
para a classe de código que ele desenha.

# Contexto

A restrição de licença que dependia de vigilância humana é a mesma que a
[auditoria de licenças](/references/license-audit.md) mecanizou por varredura determinística.
