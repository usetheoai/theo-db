# ADR 0056 — Tier-out do pg_duckdb: imagem default enxuta + imagem opcional theodb-htap (M142)

- **Status:** Accepted · **Date:** 2026-07-22 · **Milestone:** M142 · **Deciders:** CTO (paulohenriquevn)
- **Emenda:** ADR `0020-m61-embed-pgduckdb` (embarcar pg_duckdb no default — esta ADR resolve o follow-up
  "Unresolved" que aquela deixou: "Tiering (imagem `theodb-htap` separada) fica como follow-up se o peso incomodar").
- **Relacionado:** ADR `0021`/`0023` (superfície M62/M64 — pg_duckdb fora do hot path AI-native), ADR `0042`
  (own-code columnar TableAM M99), `docs/benchmarks/m97-htap-viability.md` (DEFER pilar columnar novo),
  `.claude/rules/parsimony-ladder.md` (anti-sunk-cost), `.claude/rules/public-copy.md` (honestidade).

## Contexto e problema

O M61 (ADR-0020) embarcou o `pg_duckdb` na imagem **default** como adoção permissiva (Regra 9) do pilar
columnar/HTAP. Três fatos mudaram a justificativa desde então:

1. **O own-code columnar madurou.** O `theodb_columnar` TableAM (M99–M115, ADR-0042) entrega o colunar
   transparente **in-database** sobre tabelas PG vivas (MVCC, pushdown de agregação/GROUP-BY/zone-map,
   7–13× byte-idêntico, ~1300× em min/max) — exatamente o terreno onde o pg_duckdb mediu **honest-negative**
   (0,63–0,89× sobre o heap, ADR-0020 § Evidência).
2. **pg_duckdb está fora do hot path AI-native.** O M64 (ADR-0023) provou que não há plano único PG+DuckDB
   (duas engines, dois planners) — o RAG/retrieval é 100% PostgreSQL.
3. **O espaço columnar permissivo está esgotado.** O M97 (`docs/benchmarks/m97-htap-viability.md`) recomendou
   **DEFER** um pilar columnar novo.

O valor **único** restante do pg_duckdb é lakehouse de **arquivos externos** (Parquet/Iceberg/CSV, aposta D2).
E ele é o **único componente C++** de uma stack Rust+PG: +170 MB (imagem ~813 MB), `shared_preload_libraries`
(load no boot), `libcurl4`/httpfs (superfície SSRF). Manter isso no default que a maioria puxa é custo sem
retorno para quem não usa lakehouse (anti-sunk-cost: o esforço do M61 não justifica manter no default).

## Decisões

**D1 — Guard fail-closed em runtime, NÃO concat condicional da extensão.** `sql/85-theodb-htap.sql` é plpgsql
puro de codegen (as funções **constroem** statements DuckDB, nunca chamam `duckdb.query` internamente — L38 do
arquivo) → elas `CREATE` sem pg_duckdb. Mantemos `sql/85` no concat da extensão `theodb` (idêntica nas duas
imagens — **sem version skew**, cadeia de upgrade M137 intacta) e adicionamos às funções que produzem statements
pg_duckdb (`olap_sql`, `htap_refresh_sql`) um guard `IF to_regproc('duckdb.query') IS NULL THEN RAISE
EXCEPTION ... ERRCODE '0A000' (feature_not_supported) USING HINT = 'pull the theodb-htap image'`. É fail-fast/
typed (`error-handling.md` §2 — o próprio arquivo já usa o padrão para no-snapshot). No default o cliente recebe
um erro claro com o próximo passo, nunca um statement quebrado. `htap_refresh_sql` foi convertida de `LANGUAGE
sql` para `plpgsql` para hospedar o guard. A mudança viaja pela cadeia de upgrade via `sql/theodb--1.4--1.5.sql`
(`ALTER EXTENSION theodb UPDATE TO '1.5'`) + bump de `theodb.control` para `1.5`.

**D2 — Imagem htap = `FROM <default>` + camada pg_duckdb (camada, não fork).** `packaging/Dockerfile.htap` recebe
`ARG THEODB_BASE` e faz `FROM ${THEODB_BASE}`, re-adicionando um estágio builder do pg_duckdb (idêntico ao que
saiu do default), o COPY dos artefatos, o `shared_preload_libraries`, o `libcurl4` e um initdb `01-create-
pgduckdb.sql`. A htap fica **em sync por construção** com a default (não duplica o build do theodb_rs). Precedente:
`packaging/Dockerfile.regress` (secundário já no CI).

**D3 — Compat sinalizada como Changed + BREAKING (pré-1.0, sem forçar semver-major).** CHANGELOG sob `### Changed`
com marca `BREAKING:` (a imagem default perde pg_duckdb; use `theodb-htap`), não sob `### Removed`. Pré-1.0 (0.x);
a capacidade **não** é removida (continua opt-in via htap) — é a *superfície default* que muda.

## Alternativas rejeitadas

1. **Concat condicional** (dois `theodb--1.0.sql`, um por imagem) — REJEITADO: cria duas variantes da extensão
   `theodb` → version skew + complexidade de upgrade (anti-KISS, quebra a premissa da cadeia M137).
2. **Deixar as funções sem guard** — REJEITADO: no default o cliente receberia um statement que falha com erro
   obscuro (`duckdb.query does not exist`), violando honestidade/UX (`error-handling.md`).
3. **`Dockerfile.htap` self-contained** (repetir todo o build) — REJEITADO: duplica o build do theodb_rs,
   propenso a drift.
4. **Remover a capacidade lakehouse de vez** (CHANGELOG `Removed`) — REJEITADO pelo owner: a capacidade D2 não
   some, só vira opt-in.

## Consequências

- **Habilita:** imagem default menor (delta ≥ 150 MB medido — `docs/benchmarks/m142-pgduckdb-tiering.md`), sem o
  único componente C++/httpfs no caminho default; a capacidade lakehouse continua via `theodb-htap`.
- **Restringe:** quem puxa o default e chama a superfície M62 recebe o guard fail-closed (0A000) — precisa da
  imagem htap. Documentado como mudança de compat (CHANGELOG Changed + BREAKING).
- **Rastreia:** o CI passa a buildar as duas imagens (o smoke default assere pg_duckdb ausente; o job htap assere
  presente + M62 e2e) para evitar drift.
- **Backward compat:** a superfície `theodb` (funções, assinaturas) é **idêntica** nas duas imagens; a única
  diferença observável é a presença do pg_duckdb (e, sem ele, o guard). Cadeia de upgrade M137 preservada
  (`theodb--1.4--1.5.sql`).

## Validação (medida — Regra 5)

Provado no droplet e2e-runner (Docker 29.4.1, PG18) via `scripts/m142-tiering-validate.sh`:
`docker images` das duas tags (delta ≥ 150 MB), smoke default (pg_duckdb ausente + guard RAISE 0A000 +
theodb_rs/`vector`/theodb_columnar verdes), smoke htap (pg_duckdb presente + M62 e2e), e
`sql/tests/htap_guard_test.sql` verde nas duas imagens. Evidência: `docs/benchmarks/m142-pgduckdb-tiering.md`.

## Referências

- ADR-0020 (a decisão emendada), ADR-0021/0023/0042, `docs/benchmarks/m97-htap-viability.md`.
- `Dockerfile` (default tierado), `packaging/Dockerfile.htap` (a camada), `sql/85-theodb-htap.sql` (o guard),
  `sql/theodb--1.4--1.5.sql` (o upgrade), `theodb.control` (bump 1.5), `sql/tests/htap_guard_test.sql`,
  `scripts/m142-tiering-validate.sh`, `docs/benchmarks/m142-pgduckdb-tiering.md`.
