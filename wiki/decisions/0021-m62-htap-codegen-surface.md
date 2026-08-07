---
type: Decision
title: ADR 0021 — Superfície HTAP unificada via codegen statement-level (M62)
description: O pg_duckdb proíbe execução DuckDB dentro de funções, então a superfície HTAP gera o SQL para o cliente executar no nível de conexão, mantendo catálogo e freshness como SQL puro.
resource: git:f7c7b93:docs/adr/0021-m62-htap-codegen-surface.md
tags: [adr, htap, codegen, lakehouse, parquet, freshness, m62]
adr_id: "0021"
adr_status: Accepted
decision_date: 2026-07-09
owner: human:paulohenriquevn
milestone: M62
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0021
    resource: git:f7c7b93:docs/adr/0021-m62-htap-codegen-surface.md
    title: ADR 0021 — Superfície HTAP unificada via codegen
    last_modified: 2026-07-09
---

Um caso em que uma **restrição da dependência**, descoberta por medição, redesenhou a superfície —
e o registro insiste que o resultado é desenho correto, não workaround.

# O bloqueador medido

A implementação inicial fez `theodb.htap_refresh` e `theodb.olap` como funções plpgsql que
chamavam `COPY TO parquet` e `duckdb.query` **internamente**. Medido:

```
ERROR: DuckDB execution is not supported inside functions
```

O [pg_duckdb](/technologies/pg-duckdb.md) **proíbe execução DuckDB dentro de funções**, e não há
GUC que permita — verificado, só existem `duckdb.max_memory`, `workers` e os de MotherDuck. No
nível de **statement** (conexão), porém, o `COPY`→Parquet e o `read_parquet` funcionam.

# Decisão

Pivotar para **codegen no nível de statement**:

- `theodb.htap_refresh_sql(rel) → text` e `theodb.olap_sql(rel) → text` **geram** o SQL que o
  **cliente executa na conexão**, onde o pg_duckdb permite DuckDB.
- `theodb.htap_register(rel, path)` e `theodb.htap_freshness(rel) → interval` são **SQL puro**
  sobre o catálogo `theodb._htap_snapshots`, e portanto funcionam dentro de função.
- **Nenhuma função chama `duckdb.query` internamente.**

# Alternativas rejeitadas

**Funções que executam DuckDB internamente** — rejeitada por medição, com erro reprodutível.
**Background worker rodando o COPY** — over-engineering; o cliente já roda statements.
**MotherDuck** (columnstore nativo sincronizado, chamada única transparente) — SaaS proprietário.
**Iceberg-mirror do pg_mooncake** — depende do moonlink, sob BSL 1.1, barrado. **Só documentar o
padrão sem funções** — perderia a rastreabilidade de freshness, que é justamente o valor de código
próprio compondo sobre a peça adotada.

# Evidência

A 5M linhas ([m62](/benchmarks/m62-htap.md)), em três eixos:

- **OLAP colunar ~31×** — `olap_sql` via `read_parquet` em 15,9 ms contra `GROUP BY` sobre heap em
  492,9 ms, com checksum casado.
- **Não-interferência:** o p95 de OLTP **não degrada** sob OLAP concorrente, porque o snapshot
  Parquet é somente-leitura.
- **Custo:** refresh de ~1,2 s a 5M, freshness datada explícita, e storage 2×.

# Consequências

**O HTAP do TheoDB é lakehouse-materializado assistido** — **não** é chamada única transparente
(restrição do pg_duckdb) nem in-memory auto-mantido (que é o modelo do
[AlloyDB](/technologies/alloydb.md)). É a aposta declarada, dita honestamente.

**Freshness é exposta**, não escondida: o operador decide quando fazer refresh. Um scheduler ou CDC
automático ficou como follow-up.

A superfície é **código próprio** (codegen mais catálogo) compondo sobre a peça adotada.[^adr0021]

# Ressalvas

Dados sintéticos. O ~31× é de `GROUP BY`; o full-scan do M61 dá ~9× — são queries diferentes. E a
freshness é manual. Os números são os medidos, sem claim de paridade com o AlloyDB.

[^adr0021]: ADR 0021 — Superfície HTAP unificada via codegen statement-level
