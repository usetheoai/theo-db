# ADR 0021 — Superfície HTAP unificada via codegen statement-level (M62)

**Status:** Accepted · **Date:** 2026-07-09 · **Milestone:** M62 · **Deciders:** CTO (paulohenriquevn)
**Relacionado:** ADR `0020-m61-embed-pgduckdb` (adoção pg_duckdb), ADR `0013` (KEEP columnar permissivo), ADR `0002` (measurement-first)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m62-htap-unified-surface-blueprint.md`
**Evidência:** `docs/benchmarks/m62-htap.{md,json}` + `docs/benchmarks/m62-raw/*.json`

## Contexto e problema

O M61 embarcou o pg_duckdb; o ganho analítico materializa sobre dados colunares (Parquet, ~9-31×), não sobre o
heap. O M62 constrói a **superfície HTAP unificada** — a mesma tabela transacional servindo OLAP. O blueprint
recomendou o padrão **lakehouse-materializado** (`theodb.htap_refresh` row→Parquet + `theodb.olap` roteia a query
para o snapshot colunar).

## O bloqueador medido (a incerteza Q2 do plano resolvida)

A implementação inicial fez `theodb.htap_refresh`/`theodb.olap` como funções plpgsql que chamavam `COPY TO parquet`
/`duckdb.query` INTERNAMENTE. **Medido:** o pg_duckdb **proíbe execução DuckDB dentro de funções** — `ERROR: DuckDB
execution is not supported inside functions`. Não há GUC para permitir (verificado: só `duckdb.max_memory`/
`workers`/`motherduck*`). No nível de **statement** (conexão) o COPY→Parquet e o read_parquet FUNCIONAM (M61 provou).

## Decisão

**Pivotar a superfície para CODEGEN statement-level** (não é workaround — é o design arquiteturalmente correto dada
a restrição do pg_duckdb):

- `theodb.htap_refresh_sql(rel) → text` e `theodb.olap_sql(rel) → text` **geram** o SQL (COPY / read_parquet via
  duckdb.query) que o **cliente executa no nível de conexão** (onde o pg_duckdb permite DuckDB).
- `theodb.htap_register(rel, path) → timestamptz` e `theodb.htap_freshness(rel) → interval` são **SQL puro**
  (catálogo `theodb._htap_snapshots`) — funcionam dentro de função.
- Nenhuma função chama `duckdb.query` internamente.

## Alternativas rejeitadas

1. **Funções que executam DuckDB internamente** — REJEITADA por medição (pg_duckdb proíbe; erro reprodutível).
2. **Background worker que roda o COPY** — over-engineering (YAGNI); o cliente já roda statements. Follow-up se um
   scheduler/CDC for necessário.
3. **MotherDuck** (columnstore nativo sincronizado, single-call transparente) — SaaS proprietário; barrado.
4. **pg_mooncake Iceberg-mirror** — depende do moonlink (BSL 1.1, barrado D1).
5. **Só documentar o padrão sem funções** — perde a rastreabilidade de freshness (o catálogo + `htap_freshness` é o
   valor own-code que compõe sobre o pg_duckdb).

## Evidência (medida — 3 eixos, 5M)

- **OLAP colunar ~31×** (`olap_sql`/read_parquet 15.9ms vs heap GROUP BY 492.9ms), checksum-matched.
- **Não-interferência:** OLTP p95 NÃO degrada sob OLAP concorrente (snapshot Parquet read-only).
- **Custo:** refresh ~1.2s @5M + freshness datada explícita + storage 2×.
- **16 pytest GREEN** (fluxo, freshness/staleness, race-aware, negativos tipados).

## Consequências

- **O HTAP do TheoDB é lakehouse-materializado assistido** — NÃO single-call transparente (pg_duckdb constraint),
  NÃO in-memory auto-mantido (AlloyDB). Aposta D2, declarada honestamente (Regra 5).
- **Freshness é EXPOSTA** (`htap_freshness`) — o operador decide quando refresh; um scheduler/CDC automático é
  follow-up (M-futuro), não escopo do M62.
- **A superfície é own-code SQL** (codegen + catálogo) compondo sobre a peça adotada (pg_duckdb) — Regra 9.
- **M64 (RAG-sobre-SQL unificado)** usa esta superfície + o vetor (M52/M63) na query unificada.

## Caveats

Dados sintéticos; ~31× é o group-by (full-scan sum do M61 = ~9×, queries diferentes); freshness manual (refresh
explícito). Os números são os medidos — sem claim de paridade com AlloyDB.
