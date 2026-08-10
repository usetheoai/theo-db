---
type: Feature
title: Columnar/HTAP via pg_mooncake (histórico — superseded)
description: A primeira exploração do pilar colunar, por espelho columnstore em DuckDB e Iceberg; nunca foi embarcada e hoje está superseded pelo colunar próprio.
resource: git:f7c7b93:docs/analytics/columnar-htap.md
tags: [feature, columnar, htap, historico, pg-mooncake, iceberg, superseded]
feature_status: histórico — nunca embarcado, superseded
status: deprecated
milestone: M6
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: analytics-htap
    resource: git:f7c7b93:docs/analytics/columnar-htap.md
    title: Columnar / HTAP analytics on TheoDB (pg_mooncake) — M6
---

**Este documento é histórico.** Ele descreve a **primeira** exploração do pilar colunar, por meio de uma
extensão que mantinha um **espelho columnstore** das tabelas de linha em DuckDB e Iceberg. Essa rota
**nunca foi embarcada** e hoje está superseded — o caminho atual é o
[analítico colunar próprio](/features/14-analitico-colunar.md) mais o
[lakehouse Parquet próprio](/features/15-lakehouse-parquet.md).

Vale ler pelo que ele registra sobre o **raciocínio**, não pela API.

# O que era

```sql
CREATE EXTENSION pg_mooncake CASCADE;
CREATE TABLE trades(id bigint PRIMARY KEY, symbol text, time timestamp, price real);
CALL mooncake.create_table('trades_iceberg', 'trades');   -- espelho auto-sincronizado
SELECT avg(price) FROM trades_iceberg WHERE symbol='AMZN'; -- roda no motor colunar
```

O planner roteava a query sobre o espelho por um custom scan vetorizado, enquanto a mesma query sobre a
tabela de linha usava scan de heap — e a escolha do caminho era do usuário, ao decidir qual objeto
consultar.

# O que continua verdadeiro

**A honestidade sobre o trade-off**, que sobreviveu a todas as reescritas: o colunar do TheoDB é
**lakehouse em disco**, uma aposta **deliberadamente diferente** do colunar **in-memory** do
[AlloyDB](/technologies/alloydb.md). Os pares in-memory do ecossistema PostgreSQL são **AGPL, barrados**
pelo portão de licença. Não se reivindica paridade in-memory.

**Quando o colunar ganha**, também medido já aqui: em agregações grandes, largas e pesadas de scan. Em
dados pequenos ou estreitos, o row-store pode ser mais rápido, porque o overhead de setup domina — e o
[M6](/benchmarks/m6-columnar-vs-row.md) mediu exatamente isso a 100k linhas.

Esse ponto de 100k depois se mostrou instável entre versões, e o
[ADR 0013](/decisions/0013-v1-legacy-columnar-bm25-scope.md) o declarou **não load-bearing**,
ancorando a decisão no ganho robusto a partir de 1M.

# Por que esta rota morreu

A adoção estava **gated** num build que travou por incompatibilidade de toolchain. Quando o pilar foi
retomado, a escolha foi adotar a **base** em vez do wrapper
([ADR 0020](/decisions/0020-m61-embed-pgduckdb.md)) — e essa base, por sua vez, acabou **removida por
completo** ([ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md)) quando o código próprio provou
entregar o mesmo por 1/13 do tamanho.

A trajetória completa dessa decisão — manter, tierar, remover — está registrada nos ADRs
[0013](/decisions/0013-v1-legacy-columnar-bm25-scope.md),
[0041](/decisions/0041-m97-columnar-defer.md), [0042](/decisions/0042-m99-own-code-columnar-tam.md),
[0056](/decisions/0056-m142-pgduckdb-htap-tiering.md) e
[0057](/decisions/0057-m143-pgduckdb-total-removal.md).
