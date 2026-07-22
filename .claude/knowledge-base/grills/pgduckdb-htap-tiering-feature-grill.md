---
slug: pgduckdb-htap-tiering
generated_by: roadmap-feature
milestone_id: M142
date: 2026-07-22
status: completed
---

# Grill — M142 pg_duckdb HTAP tiering (tier-out do default)

## Q1 — O que é e por que agora?

Tirar o `pg_duckdb` da imagem **default** (tier-out) e mantê-lo numa imagem opcional
`theodb-htap`. Por quê agora (o que mudou):

- O `theodb_columnar` **own-code** (M99–M115), entregue **depois** do M61, já cobre o
  colunar transparente **in-database** sobre tabelas PG vivas (MVCC, pushdown de
  agregação/GROUP-BY/zone-map) — exatamente o terreno onde o pg_duckdb mediu
  **honest-negative** (0,63–0,89× sobre o heap, ADR-0020).
- O valor **único** restante do pg_duckdb é lakehouse de **arquivos externos**
  (Parquet/Iceberg/CSV, aposta D2) — medido ~9–23× **só** sobre Parquet, **fora** do
  hot path AI-native (M64 provou: não há plano único PG+DuckDB) e **não dogfoodado**.
- pg_duckdb é o **único componente C++** de uma stack Rust+PG: +170 MB (imagem ~813 MB),
  `shared_preload_libraries='pg_duckdb'` (load no boot), `libcurl4`/httpfs (superfície SSRF).
- O tier-out **já era o follow-up "Unresolved"** do próprio ADR-0020 ("Tiering (imagem
  `theodb-htap` separada) fica como follow-up se o peso incomodar").

Gatilho: limpeza estratégica da superfície default depois que o own-code columnar amadureceu
(anti-sunk-cost — esforço no M61 não justifica manter no default).

## Q2 — Dependências (devem estar `[x]`)

**M61** (pg_duckdb embarcado — a peça sendo tierada) + **M99** (own-code columnar TableAM
que torna o pg_duckdb secundário no default). Ambas `[x]`. Sem o M99, tirar o pg_duckdb
deixaria um buraco no colunar in-DB; com ele, o tier-out não remove capacidade in-DB.

## Q3 — Definition of done

1. Imagem **default** builda **sem** pg_duckdb: sem estágio `pgduckdb-builder`, sem COPY do
   `.so`, sem `shared_preload_libraries='pg_duckdb'`, sem `libcurl4`, sem
   `CREATE EXTENSION pg_duckdb`; queda de tamanho **≥ 150 MB** medida em `docs/benchmarks/`.
2. Smoke default: `pg_extension` sem `pg_duckdb`; `shared_preload_libraries` sem ele;
   `theodb_rs` + `theodb_columnar` own-code intactos (vetor/AM/columnar verdes).
3. `packaging/Dockerfile.htap` = base default + camada pg_duckdb; smoke htap: pg_duckdb
   presente + superfície M62 (`theodb.htap_refresh_sql`/`olap_sql`) funciona e2e.
4. ADR emendando o **0020** (decisão de tier-out) + `README` move pg_duckdb/HTAP de "default"
   para "opcional (imagem theodb-htap)" + `CHANGELOG` sob **Changed** com marca `BREAKING:`.
5. `sql/85-theodb-htap.sql` + `CREATE EXTENSION pg_duckdb` carregam **condicionalmente**
   (só na imagem htap), provado pelos dois smokes.

## Q4 — Top 2 riscos NOVOS

- **R1 (compat):** quem puxa a imagem default e chama `duckdb.query`/a superfície M62 quebra.
  Mitigação: a imagem htap mantém opt-in; nota/erro tipado claro ("pg_duckdb requer a imagem
  theodb-htap"); pré-1.0 e nenhum dogfood depende → blast radius baixo, mas documentado como
  mudança de compat (CHANGELOG Changed + BREAKING).
- **R2 (drift de 2 imagens):** a htap pode apodrecer / não buildar no CI. Mitigação: CI builda
  **as duas** + roda o smoke htap no mesmo job; a htap é *camada sobre* a default (não fork) →
  fica em sync por construção.

## Out-of-scope cross-check

Seção `### Explicitly out of scope` do ROADMAP está vazia → nenhum overlap a resolver.

## Decisões confirmadas pelo owner (2026-07-22)

- Direção: **tier out do default** (não remover de vez, não manter como está).
- Dependências: **M61 + M99**.
- Compat: **CHANGELOG Changed + nota `BREAKING:`** (pré-1.0, sem forçar semver-major via Removed).
