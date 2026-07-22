# Discovery blueprint — M142 pg_duckdb HTAP tier-out

> Slug: `m142-pgduckdb-htap-tiering` · Date: 2026-07-22 · Milestone: M142
> Tipo: packaging/operational (a barra SOTA da `/theodb-evolution` relaxa para o contrato baseline —
> não há questão de algoritmo; a "pesquisa" é o mapeamento cirúrgico do build atual + a escolha de
> mecanismo correto). Prior art primária: o **próprio repo** (Dockerfile, sql/85, ci.yml) + ADR-0020.

## Problema (Phase 1)

Tirar o `pg_duckdb` da imagem **default** e mantê-lo numa imagem opcional `theodb-htap`, sem quebrar
o init do banco, sem version-skew da extensão `theodb`, e provado por build+smoke das duas imagens.
Baseline: imagem default hoje ~813 MB (ADR-0020: +170 MB do bundle DuckDB estático + `libcurl4`).

## Mapa do build atual (evidência — `Dockerfile`)

| Linha | O que é | Ação no tier-out |
|---|---|---|
| 34–48 | Stage `pgduckdb-builder` (C++/cmake/ninja, `DUCKDB_BUILD=ReleaseStatic`) | **remover do default**; mover para `Dockerfile.htap` |
| 60–63 | `COPY` do `pg_duckdb.so` + `.control`/`.sql` da extensão | remover do default; re-adicionar na htap |
| 64–66 | append `shared_preload_libraries='pg_duckdb'` no `postgresql.conf.sample` | remover do default; re-adicionar na htap |
| 71 | `libcurl4` (dep do pg_duckdb httpfs) no apt do runtime | **remover do default** (manter só `ca-certificates`, que é do theodb_rs); re-adicionar na htap |
| 82–84 | concat de `sql/85-theodb-htap.sql` em `theodb--1.0.sql` | **manter** (ver decisão-chave abaixo) |
| 100 | `CREATE EXTENSION pg_duckdb` no initdb.d | **remover do default** (crítico: sem o `.so`+preload, isso QUEBRA o init); re-adicionar na htap |
| 103–105 | `mkdir /var/lib/postgresql/htap` (destino dos snapshots Parquet) | manter (inócuo; só usado quando htap) ou mover p/ htap |

## Decisão-chave (Phase 2/3) — guard em runtime, NÃO concat condicional

`sql/85-theodb-htap.sql` (193 linhas) é **plpgsql puro de codegen**: as funções
(`htap_refresh_sql`, `olap_sql`, `htap_register`, `htap_freshness`, `_htap_path`) **constroem** strings
SQL e **não chamam `duckdb.query` internamente** (L38 do arquivo — "NO function calls duckdb.query
internally"). Logo elas **`CREATE` sem pg_duckdb** — o extension install não falha no default.

O problema é só de UX/honestidade: no default (sem pg_duckdb) um usuário que chama `theodb.olap_sql(t)`
recebe um `SELECT * FROM duckdb.query(...)` que falha ao rodar.

**Mecanismos considerados:**

1. **Concat condicional** (dois `theodb--1.0.sql`, um por imagem) — **REJEITADO**: cria duas variantes
   da extensão `theodb` → version skew + complexidade de upgrade (anti-KISS, e a cadeia de upgrade M137
   assume um único conteúdo por versão).
2. **Guard fail-closed em runtime** (ESCOLHIDO) — as funções que produzem statements pg_duckdb
   (`htap_refresh_sql`, `olap_sql`) checam a presença de pg_duckdb
   (`to_regproc('duckdb.query') IS NOT NULL`) e `RAISE EXCEPTION` com mensagem clara + próximo passo
   ("pg_duckdb ausente — use a imagem theodb-htap") quando ausente. É o **padrão que o próprio arquivo
   já usa** (RAISE para no-snapshot, `error-handling.md` §2 — fail-fast, typed, next-step). Extensão
   **idêntica** nas duas imagens; default falha-claro, htap funciona.

Isto refina o **mecanismo** do DoD #5 (o critério de aceite — "default não expõe HTAP quebrado; htap
funciona e2e" — é integralmente atendido) e vira decisão de ADR (emenda ao 0020).

## Invariantes a preservar (Phase 2)

- **Init não quebra:** `CREATE EXTENSION IF NOT EXISTS theodb CASCADE` + `theodb_rs` continuam funcionando
  no default sem pg_duckdb (o `theodb.control` NÃO depende de pg_duckdb — L98 confirma "adjunto analítico").
- **Extensão `theodb` idêntica** nas duas imagens (sem version skew — a cadeia de upgrade M137 intacta).
- **Fail-closed honesto:** a superfície HTAP no default RAISE typed error, nunca produz silenciosamente
  um statement quebrado.
- **htap = camada sobre default (não fork):** `Dockerfile.htap` = `FROM <default>` + re-adiciona pg_duckdb
  → fica em sync por construção (mitiga o risco de drift R2 do grill).

## Estratégia de validação (Phase 6) — droplet e2e-runner (Docker 29.4.1, 110 GB livres)

1. `docker build -t theodb:m142-default .` → `docker images` (tamanho default pós-tier-out).
2. Smoke default: sobe container, `psql`: (a) `pg_extension` **sem** `pg_duckdb`; (b) `shared_preload_libraries`
   **sem** ele; (c) `theodb_rs`+`theodb_columnar` verdes (vetor/AM/columnar); (d) `theodb.olap_sql(t)` **RAISE**
   o erro fail-closed.
3. `docker build -f packaging/Dockerfile.htap -t theodb:m142-htap --build-arg THEODB_BASE=theodb:m142-default .`
   → `docker images` (tamanho htap) → **delta ≥ 150 MB** medido e escrito em `docs/benchmarks/`.
4. Smoke htap: pg_duckdb presente + `theodb.htap_refresh_sql`/`olap_sql` produzem e o cliente executa e2e.

## CI (Phase 6)

`.github/workflows/ci.yml` já tem múltiplos jobs buildando a imagem default via `docker/build-push-action@v6`
e um `packaging/Dockerfile.regress` (precedente de Dockerfile secundário no `pg-regression`). Wire: um job
que builda `Dockerfile.htap` FROM a default + roda o smoke htap; e uma asserção no smoke default de que
pg_duckdb está ausente.

## Referências

- ADR-0020 (embarcar pg_duckdb — a decisão emendada; tier-out já era seu follow-up Unresolved).
- ADR-0021/0023 (superfície M62/M64 — codegen statement-level; pg_duckdb fora do hot path AI-native).
- ADR-0042 (own-code columnar TableAM M99 — cobre o colunar in-DB, o que justifica o tier-out).
- `docs/benchmarks/m97-htap-viability.md` (DEFER a new columnar pillar — espaço permissivo esgotado).
- `sql/85-theodb-htap.sql` (o codegen a guardar), `Dockerfile` (o build a tierar), `.github/workflows/ci.yml`.
- `.claude/rules/parsimony-ladder.md` (anti-sunk-cost), `error-handling.md` (fail-fast typed), `public-copy.md`.
