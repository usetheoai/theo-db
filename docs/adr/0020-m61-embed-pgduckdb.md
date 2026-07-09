# ADR 0020 — Embarcar pg_duckdb (columnar/HTAP) na distribuição TheoDB (M61)

**Status:** Accepted · **Date:** 2026-07-09 · **Milestone:** M61 · **Deciders:** CTO (paulohenriquevn)
**Relacionado:** ADR `0013-v1-legacy-columnar-bm25-scope` (decisão MANTER columnar permissivo), ADR `0006` (own-code — exceção Regra 9), ADR `0002` (measurement-first)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m61-columnar-htap-adoption-blueprint.md`
**Evidência:** `docs/benchmarks/m61-columnar-adoption.{md,json}`

## Contexto e problema

O M30/ADR-0013 decidiu MANTER o pilar columnar/HTAP via peça permissiva (Regra 9 — reescrever um motor colunar
vetorizado é PhD-level/anos), medindo ~14× no columnstore do `pg_mooncake`. Mas nunca **embarcou** a peça na imagem
PG17. O roadmap v3 (amplitude) abre a adoção como M61.

## Decisão

**Embarcar `pg_duckdb` (github.com/duckdb/pg_duckdb, MIT), NÃO `pg_mooncake`.** pg_duckdb é GA (v1.1.x), PG14–18
nativo (inclui PG17), e o pg_mooncake é na verdade uma **camada sobre** o pg_duckdb (`requires='pg_duckdb'`) com
default PG18 — a exata rota que travou o build PG17 no ADR-0013. Build via novo estágio multi-stage
`pgduckdb-builder` (`DUCKDB_BUILD=ReleaseStatic` → bundle DuckDB estático num só `.so`), COPY artifact-only,
`shared_preload_libraries='pg_duckdb'` (append idempotente ao `postgresql.conf.sample`), `CREATE EXTENSION` no init.

## Alternativas rejeitadas

1. **pg_mooncake** — camada sobre pg_duckdb, default PG18 (travou no ADR-0013); adotar a base (pg_duckdb) é mais direto.
2. **Reescrever columnar próprio** — Regra 9, PhD-level/anos (ADR-0013).
3. **Citus columnar / Hydra** — AGPLv3, barrados por D1 (confirmado nos LICENSE upstream).
4. **DuckDB dynamic-link** — exigiria `libduckdb.so` avulso + version-skew; static (`ReleaseStatic`) é um artefato só.

## Evidência (medida, honesta — Regra 5)

pg_duckdb embarcado com sucesso (smoke: 6 extensões coexistem, `duckdb.query`→42, `force_execution` sobre heap,
índice vetorial junto, `allow_community_extensions=off`). Benchmark de adoção (5M, ≥3 runs mean±std):

- **Sobre o HEAP row-store (`force_execution`): honest-negative** — DuckDB PERDE (0.63–0.89×). Ler dados row-format
  via DuckDB adiciona overhead; a vantagem vetorizada exige dados já colunares.
- **Sobre PARQUET colunar: DuckDB VENCE e escala — ~9× a 5M** (1.56× → 8.78×, checksum correto). É onde o ganho
  analítico materializa (na faixa do ~14× do mooncake, com o caveat de que os dados precisam estar em Parquet).

## Consequências

- **O valor entregue é analytics colunar sobre arquivos (Parquet/Iceberg/CSV) — uma capacidade data-lake/lakehouse
  (aposta D2, declarada), NÃO um acelerador transparente do heap Postgres.** Sem MotherDuck não há columnstore
  nativo persistente (medido).
- **Honestidade (Regra 9/5):** pg_duckdb é **exceção permissiva adotada**, não own-code; o número de vantagem é o
  medido sobre pg_duckdb/Parquet, não herdado do mooncake.
- **Peso da imagem:** +170 MB (bundle DuckDB estático) → 813 MB. Tiering (imagem `theodb-htap` separada) fica como
  follow-up (Unresolved) se o peso incomodar.
- **Runtime dep:** `libcurl4` (o pg_duckdb.so linka libcurl da httpfs) — adicionado ao runtime.
- **M62** (HTAP surface unificada) constrói sobre esta adoção — o caminho analítico é o Parquet/read_parquet + o
  `force_execution` para queries ad-hoc.

## Segurança

`duckdb.allow_community_extensions=off` (default, verificado) — nenhuma extensão DuckDB não-auditada carrega.
`/deps-audit` das transitivas no gate de adoção. `shared_preload_libraries` fail-closed (o smoke assere o load).
