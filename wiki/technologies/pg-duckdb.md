---
type: Technology
title: pg_duckdb
description: A extensão que embutia o motor analítico DuckDB no PostgreSQL; foi o pilar colunar do projeto por um tempo, e virou o último componente C++ a ser removido.
resource: https://github.com/duckdb/pg_duckdb
tags: [tecnologia, extensao, columnar, duckdb, removido]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: pgduckdb-repo
    resource: https://github.com/duckdb/pg_duckdb
    title: pg_duckdb, repositório oficial
  - id: recalled
    resource: conhecimento do produtor em 2026-08-07, não lido de fonte
    title: Conhecimento do produtor
---

O pg_duckdb embute o **DuckDB** — um motor analítico colunar vetorizado, in-process — dentro do
PostgreSQL, permitindo rotear queries analíticas para ele e ler formatos como
[Parquet](/technologies/parquet.md).[^recalled] Licença permissiva.

# A trajetória completa neste acervo

É a peça com o arco mais longo do repositório, e ele vale como estudo de caso:

**Adotada** ([ADR 0020](/decisions/0020-m61-embed-pgduckdb.md)) — escolhendo a **base** em vez do wrapper
que travara antes, com o ganho medido **sobre arquivos Parquet (~9×)** e **honest-negative sobre o heap**.

**Restringiu o desenho** ([ADR 0021](/decisions/0021-m62-htap-codegen-surface.md)) — ela **proíbe
execução dentro de função**, o que forçou uma superfície em que as funções geram SQL para o cliente
executar.

**Bloqueou uma capacidade** ([ADR 0023](/decisions/0023-m64-rag-unified-not-columnar-planner.md)) —
por serem **duas engines com dois planners**, um plano híbrido único é impossível, e isso corrigiu uma
premissa de critério de pronto.

**Tierada para fora do default** ([ADR 0056](/decisions/0056-m142-pgduckdb-htap-tiering.md)), quando o
colunar próprio amadureceu.

**Removida por completo** ([ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md)) — 118 MB de C++
trocados por 9 MB de Rust, com paridade provada.

# As duas lições

**Uma restrição de dependência vira restrição de arquitetura.** O desenho de codegen não era escolha —
era consequência. E quando a dependência saiu, **a restrição sumiu e o código simplificou**.

**Um componente de linguagem diferente carrega custo além do tamanho.** Ela era o único C++ numa stack
Rust mais PostgreSQL, com sua própria superfície de rede e uma biblioteca de sistema extra.

# O que a substituiu

[Lakehouse Parquet próprio](/features/15-lakehouse-parquet.md) sobre
[DataFusion](/technologies/datafusion.md) e [Arrow](/technologies/arrow.md), mais o
[colunar in-database próprio](/features/14-analitico-colunar.md).

[^pgduckdb-repo]: pg_duckdb, repositório oficial
[^recalled]: Conhecimento do produtor, não verificado contra fonte nesta redação
