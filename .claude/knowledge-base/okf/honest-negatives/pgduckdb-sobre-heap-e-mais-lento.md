---
type: Honest Negative
title: pg_duckdb force_execution sobre heap é 0,52-0,78× do row-executor do PostgreSQL
description: Resultados corretos e plano usando DuckDB — e ainda assim mais lento que o executor nativo, em todas as escalas.
tags: [htap, duckdb, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# `pg_duckdb` `force_execution` sobre heap é **0,52-0,78×** do row-executor do PostgreSQL

## O veredito

Forçar o DuckDB a executar sobre o **heap** do PostgreSQL é **mais lento que o executor nativo** em **todas** as
escalas medidas. E o resultado não é artefato de configuração:

- resultados **corretos** (match ✓);
- o plano de fato usa DuckDB (`duckdb_plan=True`).

## A explicação, e por que ela generaliza

Um motor vetorizado só ganha quando lê **dados no layout dele**. Sobre um heap row-major, ele paga a conversão
linha→coluna e não colhe nenhuma das vantagens (compressão, skip por zone-map, leitura de coluna isolada). A
vantagem do DuckDB é **do formato**, não do executor.

Isso é o que sustenta a decisão de investir no `theodb_columnar` own-code em vez de terceirizar execução: sem
storage colunar próprio, o motor vetorizado não tem do que se alimentar.

## Consequência histórica

Junto com a licença e o peso (118 MB de bundle C++ estático), este número embasou o tier-out do `pg_duckdb` para
imagem opcional (M142) e depois a remoção total (M143).

## Relacionados

- [honest-negative/topn-columnar](topn-columnar.md)
