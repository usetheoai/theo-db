# Implementation summary — M143 remoção total do pg_duckdb

**Slug:** `m143-pgduckdb-total-removal` · **Milestone:** M143 · **Date:** 2026-07-22 · **Branch:** develop

## Goal (atingido)

Remover o pg_duckdb inteiro, substituindo o lakehouse por own-code (DataFusion/Arrow), provado por round-trip
sem pg_duckdb + delta de tamanho na imagem shipada. **Medido:** imagem M143 default 724 MB com o lakehouse
own-code, `pg_extension` sem pg_duckdb, 118 MB de C++ removidos (`docs/benchmarks/m143-pgduckdb-removal.md`).

## Fases entregues (todas validadas no droplet)

| Fase | O que | Prova (droplet PG18.4) |
|---|---|---|
| 1 — read own-code | `theodb.read_parquet(path)`→SETOF jsonb (arrow-json) + `olap(path)` tipado; feature `spike-parquet`→permanente | olap `a\|2\|15`, read_parquet jsonb, fail-closed |
| 2 — write own-code | `theodb.write_parquet(rel, path)` (SPI→Arrow→`ArrowWriter`, atômico) | round-trip write→read→olap multi-tipo; fail-closed timestamp |
| 3 — reescrita M62 | `sql/85`: `htap_refresh`/`olap` diretos (colapsa o codegen); upgrade 1.5→1.6; drop guards | M62 own-code `a\|2\|15` SEM pg_duckdb (count 0) |
| 4 — drop total | delete Dockerfile.htap/m61-smoke/m142-validate/htap_guard_test; CI job removido; fold no default; ADR-0057/README/CHANGELOG | imagem shipada M143_REMOVAL_OK |

## Decisões-chave

- **D1 — `SETOF jsonb`** no leitor geral (via arrow-json) — cobre todos os tipos sem `SETOF record` dinâmico (evita re-work; grill R2).
- **D2 — codegen colapsa** — own-code roda dentro da função (a restrição do pg_duckdb que exigia o codegen some); `htap_refresh`/`olap` viram diretas.
- **D3 — uma imagem só** — feature parquet permanente; `theodb-htap` aposentada; +9 MB own-code vs 118 MB DuckDB.
- **D4 — tipos:** leitura ampla (jsonb); escrita v1 escalares + fail-closed (nested/timestamp/decimal na escrita = follow-on, measurement-first).

## Reuso (Regra 9 / parsimony rung 4)

DataFusion+Arrow já no binário (M98/M100); runtime tokio in-extension do `df_executor`; `parquet::arrow::ArrowWriter`
(feature); `arrow-json` (feature). Nenhuma dep nova de motor — só ligar features + escrever a superfície.

## Validação (imagem shipada M143, e2e-runner)

`scripts/m143-removal-validate.sh` → NO_PGDUCKDB + M62_OWNCODE + READ_MULTI + WRITE_FAILCLOSED + imagem 724 MB
→ **M143_REMOVAL_OK**. Round-trip own-code sem pg_duckdb; leitor jsonb multi-tipo; escrita fail-closed.
Evidência: `docs/benchmarks/m143-pgduckdb-removal.md` + `docs/benchmarks/parquet-reader-owncode-spike.md`.

## Lições

- `ctx.sql()` exige o feature `datafusion/sql`; usar a DataFrame API (o que o `df_executor` já faz) mantém o custo de tamanho honesto.
- As funções `#[pg_extern]` do theodb_rs vivem em `public`; a superfície `theodb.*` (sql/85) as chama qualificadas (`public.write_parquet`/`olap`).
- pg_duckdb COPY→parquet rejeita boolean/timestamp — irônico; o writer own-code (`ArrowWriter`) escreve esses tipos sem problema (bool/int/float/text).
- Mudar o Cargo.toml (feature) invalida o cache do docker build (recompile completo ~20min) — esperado.
