---
type: Decision
title: ADR 0020 — Embarcar pg_duckdb na distribuição (M61)
description: A peça columnar embarcada é pg_duckdb (MIT), não pg_mooncake; o ganho medido é sobre Parquet (~9× a 5M), e sobre o heap row-store o DuckDB perde.
resource: git:f7c7b93:docs/adr/0020-m61-embed-pgduckdb.md
tags: [adr, columnar, htap, pg-duckdb, parquet, lakehouse, m61]
adr_id: "0020"
adr_status: Accepted (revertido pelo ADR 0057)
decision_date: 2026-07-09
owner: human:paulohenriquevn
milestone: M61
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0020
    resource: git:f7c7b93:docs/adr/0020-m61-embed-pgduckdb.md
    title: ADR 0020 — Embarcar pg_duckdb
    last_modified: 2026-07-09
---

A adoção que o [ADR 0013](/decisions/0013-v1-legacy-columnar-bm25-scope.md) deixara gated — e que,
mais tarde, seria integralmente revertida pelo
[ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md).

# Decisão

Embarcar o **[pg_duckdb](/technologies/pg-duckdb.md)** (MIT), **não** o `pg_mooncake`.

O pg_duckdb é GA, suporta PG14–18 nativamente (incluindo o PG17 da imagem), e o `pg_mooncake` é na
verdade uma **camada sobre** o pg_duckdb (`requires='pg_duckdb'`) com default em PG18 — exatamente
a rota que travara o build do PG17 no ADR 0013. Adotar a base é mais direto que adotar o wrapper.

O build usa um estágio multi-stage com `DUCKDB_BUILD=ReleaseStatic`, gerando o bundle DuckDB
estático num único `.so`, com `shared_preload_libraries='pg_duckdb'` acrescentado de forma
idempotente.

# Alternativas rejeitadas

**pg_mooncake** — camada sobre o pg_duckdb, com default PG18 que já travara antes. **Reescrever
columnar próprio** — nível PhD, anos de trabalho. **Citus columnar e Hydra** — AGPLv3, barrados.
**DuckDB com link dinâmico** — exigiria `libduckdb.so` avulso e conviveria com version-skew; o
estático é um artefato só.

# Evidência medida

Benchmark de adoção a 5M linhas, ≥3 runs com média ± desvio
([m61](/benchmarks/m61-columnar-adoption.md)):

- **Sobre o heap row-store, via `force_execution`: honest-negative.** O DuckDB **perde**
  (0,63–0,89×). Ler dados em formato de linha através do DuckDB adiciona overhead; a vantagem
  vetorizada exige dados **já colunares**.
- **Sobre Parquet colunar: o DuckDB vence e escala — ~9× a 5M** (de 1,56× a 8,78×), com checksum
  correto.

# Consequências

**O valor entregue é analytics colunar sobre arquivos** — [Parquet](/technologies/parquet.md),
Iceberg, CSV —, uma capacidade de data lake, **não** um acelerador transparente do heap PostgreSQL.
Sem MotherDuck não há columnstore nativo persistente, e isso foi medido, não suposto.

Honestidade: o pg_duckdb é **exceção permissiva adotada**, não código próprio, e o número de
vantagem é o medido aqui, não herdado do mooncake.

**Custo:** +170 MB de imagem (bundle DuckDB estático), levando-a a 813 MB, mais a dependência de
runtime `libcurl4`, que o `.so` do pg_duckdb linka via httpfs. O tiering numa imagem separada ficou
como follow-up.

**Segurança:** `duckdb.allow_community_extensions=off` por padrão, verificado — nenhuma extensão
DuckDB não-auditada carrega. O `shared_preload_libraries` é fail-closed, e o smoke test assere o
load.[^adr0020]

# Como esta decisão terminou

A superfície construída sobre ela está no
[ADR 0021](/decisions/0021-m62-htap-codegen-surface.md); o tiering, no
[ADR 0056](/decisions/0056-m142-pgduckdb-htap-tiering.md); e a remoção total do pg_duckdb — o
último componente C++ do projeto — no
[ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md), substituído por colunar próprio
([ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md)).

[^adr0020]: ADR 0020 — Embarcar pg_duckdb (columnar/HTAP) na distribuição TheoDB
