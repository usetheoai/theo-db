---
type: Measurement
title: mínimo e máximo por caminho rápido de zone-map — veredito
description: O maior ganho medido do pilar colunar (~1300–1400×), porque a resposta já está no diretório de metadados e nenhum dado precisa ser lido.
resource: git:f7c7b93:docs/benchmarks/columnar-minmax-zonemap-verdict.md
tags: [benchmark, columnar, zone-map, minmax, fast-path, mvcc]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: cmm
    resource: git:f7c7b93:docs/benchmarks/columnar-minmax-zonemap-verdict.md
    title: Verdict — columnar min/max + zone-map fast-path
    last_modified: 2026-07-19
---

**O maior ganho medido do pilar colunar: ~1300–1400×.**

# Por que o ganho é dessa ordem

Porque **nenhum dado é lido**. O diretório de mínimo e máximo por bloco, que o formato já mantinha para
poder pular blocos, **contém a resposta** para `min(col)` e `max(col)`: basta agregar os extremos dos
blocos.

**Um ganho de três ordens de grandeza não vem de executar mais rápido — vem de não executar.** É o mesmo
tipo de salto que o filtro inline obteve ao **evitar** trabalho em vez de acelerá-lo
([m90](/benchmarks/m90-inline-filter.md)).

# A parte difícil não é o ganho, é a correção

Um caminho rápido que responde a partir de **metadados** precisa respeitar **visibilidade**: linhas
apagadas ou não visíveis ao snapshot do leitor **não podem** influenciar o extremo reportado.

Por isso o documento registra que **a correção de MVCC foi verificada em revisão dedicada** — e não
apenas que o número é grande.

**Um atalho que responde a partir de metadados é exatamente onde uma violação de MVCC passaria
despercebida**, porque o resultado parece plausível.

# Contexto

Faz par com os vereditos de [skip por zone-map](/benchmarks/columnar-zonemap-verdict.md) e sua
[variante temporal](/benchmarks/columnar-zonemap-temporal-verdict.md), e é um dos ganhos reportados em
[analítico colunar](/features/14-analitico-colunar.md).
