---
type: Feature
title: Lakehouse Parquet — ler, escrever e agregar arquivos externos
description: I/O de Parquet 100% próprio via DataFusion e Arrow, sem DuckDB; as primitivas de arquivo são superuser-only e a escrita v1 cobre apenas tipos escalares.
resource: git:f7c7b93:docs/features/15-lakehouse-parquet.md
tags: [feature, lakehouse, parquet, datafusion, arrow, htap, seguranca]
feature_status: entregue
milestone: M62+M130+M143
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat15
    resource: git:f7c7b93:docs/features/15-lakehouse-parquet.md
    title: Lakehouse Parquet own-code
---

**Status: entregue, na imagem default.** Ler, escrever e agregar [Parquet](/technologies/parquet.md)
externo é **100% código próprio** sobre [DataFusion](/technologies/datafusion.md) e
[Arrow](/technologies/arrow.md) — **sem DuckDB**. O [pg_duckdb](/technologies/pg-duckdb.md), último
componente C++ do projeto, foi removido por completo no
[ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md), trocando 118 MB de bundle por +9 MB de Rust.

# As primitivas

```sql
public.read_parquet(path)      -- SETOF jsonb
public.write_parquet(rel, path) -- bigint (linhas escritas)
public.olap(path)              -- agregado tipado
```

A leitura devolve **`SETOF jsonb`** — uma linha Parquet vira um jsonb —, o que cobre **todos os tipos,
inclusive aninhados**, sem a complexidade de resultado dinâmico. Foi escolha deliberada sobre a
alternativa de retornar registros tipados.

# Segurança — superuser-only

```sql
REVOKE ALL ON FUNCTION public.write_parquet(text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.read_parquet(text)        FROM PUBLIC;
```

**São I/O de arquivo do lado do servidor**, então seguem a mesma postura do `COPY … TO file`: apenas
superusuário. A superfície de usuário também é revogada, de modo que um papel sem privilégio **não
contorna chamando a primitiva direto**.

# A superfície de usuário HTAP

Depois da remoção do pg_duckdb, as funções **fazem o trabalho dentro da própria função**:

```sql
SELECT theodb.htap_refresh('vendas'::regclass);   -- escreve e registra o snapshot
SELECT theodb.olap('vendas'::regclass);           -- lê e agrega
SELECT theodb.htap_freshness('vendas'::regclass); -- há quanto tempo o snapshot é de
```

Isso é uma **simplificação real**, não cosmética: o desenho anterior gerava SQL para o *cliente*
executar, e existia **só** porque o pg_duckdb proibia execução dentro de função
([ADR 0021](/decisions/0021-m62-htap-codegen-surface.md)). Removida a dependência, removeu-se a
restrição — e com ela a dança.

**A freshness é exposta, não escondida:** o operador decide quando fazer refresh. Um scheduler
automático é follow-up.

# Tipos suportados na escrita v1

A **leitura cobre tudo**. A **escrita v1 cobre os escalares comuns** — inteiros, ponto flutuante,
booleano e texto —, e um tipo não suportado gera **erro tipado fail-closed**, em vez de escrever algo
errado. O dado continua legível pela leitura; a escrita ampla é follow-on.

# O trade-off declarado

O lakehouse aqui é **em disco, sobre Parquet**, e **não** in-memory automático como o do
[AlloyDB](/technologies/alloydb.md). Isso é a aposta declarada do projeto, dita com todas as letras —
não uma limitação omitida.

A paridade byte a byte do agregado contra o antigo caminho por DuckDB foi **medida**
([spike](/benchmarks/parquet-reader-owncode-spike.md)), e o pilar HTAP misto foi medido com 0% de erro
([m130](/benchmarks/m130-htap.md)).

# Relacionados

Para analytics sobre tabelas PostgreSQL **vivas**, e não arquivos, o caminho é o
[analítico colunar](/features/14-analitico-colunar.md). A decisão que abriu o pilar está em
[ADR 0013](/decisions/0013-v1-legacy-columnar-bm25-scope.md) e a que o fechou em
[ADR 0041](/decisions/0041-m97-columnar-defer.md).
