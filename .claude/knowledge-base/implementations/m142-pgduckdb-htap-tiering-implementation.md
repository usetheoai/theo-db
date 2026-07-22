# Implementation summary — M142 pg_duckdb HTAP tier-out

**Slug:** `m142-pgduckdb-htap-tiering` · **Milestone:** M142 · **Date:** 2026-07-22 · **Branch:** develop

## Goal (atingido)

Tirar o `pg_duckdb` da imagem default do TheoDB para uma imagem opcional `theodb-htap`, provado por smokes das
duas imagens + delta de tamanho ≥ 150 MB. **Medido:** delta **175 MB** (default 712 → htap 887), smokes verdes
(`docs/benchmarks/m142-pgduckdb-tiering.md`).

## Tasks entregues (wiring triad por task)

| Task | O que | Caller / integração | Prova de runtime |
|---|---|---|---|
| T0.1 | Guard fail-closed em `olap_sql`/`htap_refresh_sql` (RAISE `0A000` sem pg_duckdb) + upgrade `theodb--1.4--1.5.sql` + bump control 1.5 | `sql/tests/htap_guard_test.sql` (self-adapting) + smoke default (guard RAISE) + smoke htap (positivo) | DEFAULT_OK: `theodb.olap_sql` RAISE 0A000; HTAP_OK: retorna text |
| T1.1 | Tier-out do `Dockerfile` default (remove estágio pgduckdb, COPY, shared_preload, libcurl4, CREATE EXTENSION, mkdir htap) | build default + smoke (pg_duckdb ausente) | `docker images` 712 MB; `pg_extension` count(pg_duckdb)=0 |
| T2.1 | `packaging/Dockerfile.htap` = FROM default + camada pg_duckdb | build htap + smoke (pg_duckdb presente + M62 e2e) | `docker images` 887 MB; olap e2e `a\|2\|15` |
| T3.1 | ADR-0056 (emenda 0020) + README opt-in + CHANGELOG Changed/BREAKING | — (docs) | xrefs + public-copy |
| T4.1 | CI: asserção pg_duckdb ausente no smoke default + job `htap-image` | `.github/workflows/ci.yml` | job htap-image builda + smoke |
| T4.2 | `scripts/m142-tiering-validate.sh` + `docs/benchmarks/m142-pgduckdb-tiering.md` | rodado no droplet | `M142_TIERING_OK` + delta 175 MB |

## Decisão-chave (D1): guard em runtime, não concat condicional

A extensão `theodb` é **idêntica** nas duas imagens (sem version skew — cadeia de upgrade M137 intacta). As
funções de codegen HTAP (`olap_sql`, `htap_refresh_sql`) checam `to_regproc('duckdb.query')` e RAISE
`feature_not_supported` (0A000) com dica ("use theodb-htap") quando pg_duckdb ausente. `htap_refresh_sql`
convertida sql→plpgsql para hospedar o guard.

## Fixes colaterais (a imagem default não buildava desde o M98)

A validação real revelou que o `Dockerfile` nunca acompanhou o upgrade pgrx do M98:
- `PGRX_VERSION` `0.16.1`→`0.19.0` (o crate usa pgrx `=0.19.0`; cargo-pgrx e pgrx são lockstep).
- `RUST_VERSION` `1.91.0`→`1.97.1` (cargo-pgrx 0.19 exige rustc ≥ 1.96).

Sem esses dois fixes nenhuma imagem TheoDB buildava. Registrados como `Fixed` no CHANGELOG.

## Validação (droplet e2e-runner, PG18.4, Docker 29.4.1)

`scripts/m142-tiering-validate.sh` → **DEFAULT_OK** + **HTAP_OK** + **delta 175 MB** + **M142_TIERING_OK**.
Evidência: `docs/benchmarks/m142-pgduckdb-tiering.md`. As duas imagens buildadas do zero.

## Lições

- `docker image inspect --format {{.Size}}` reporta valor divergente neste Docker; usar `docker images` (ground truth).
- Gate de smoke SQL: gatear no **exit code** do `psql -v ON_ERROR_STOP=1` (robusto), não grep de NOTICE (stderr, suprimível).
- pg_duckdb COPY→Parquet rejeita `NUMERIC` sem precisão (limitação M62 pré-existente) — usar `double precision`.
- O Dockerfile do produto tinha toolchain stale desde o M98 — a validação de imagem no CI (o job que faltava) teria pego.
