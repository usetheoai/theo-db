# Blueprint: M61 — Embarcar o columnar/HTAP na distribuição (gate de adoção do M30)

> **Discovery verdict (self-assessed):** SHIPPABLE_WITH_CAVEATS. Synthesized 2026-07-08 from web primary
> sources (R0 — WebFetch via `curl`, 11 fetches) + the two local references (`pg_mooncake`, `duckdb`).
> **Recommendation: adopt `pg_duckdb` (NOT `pg_mooncake`) for the M61 embed** — reasoned via the ADR below
> (alternatives + rejection). Columnar is an **adopted permissive exception** (Regra 9), not own-code (ADR 0013).
> **Measurement-first:** this blueprint is the design; the M61 adoption benchmark (`docs/benchmarks/m61-columnar-adoption.*`)
> is the gate, NOT this discover (Regra 5).

**Slug:** `m61-columnar-htap-adoption`
**Owner:** paulohenriquevn
**Created:** 2026-07-08
**Milestone:** M61 (`ROADMAP.md:987`) · **Depends:** M30 (`docs/adr/0013-v1-legacy-columnar-bm25-scope.md`)

## Context

M30/ADR-0013 decided to **KEEP** the permissive columnar pillar (measured ~9x @1M, ~14x @5M on a `GROUP BY`
rollup — `docs/benchmarks/m30-columnar-scale.md`) but **did not embed it**. The shipped image is **PG17**
(`Dockerfile:8`, `postgres:17-bookworm`) with pgvector + pgvectorscale + theodb_rs; the M30 measurement ran on
the `mooncakelabs/pg_mooncake:latest` **PG18** substrate — a throwaway. M61 is the adoption: build the
permissive columnar piece into the PG17 image, `CREATE EXTENSION` + analytic smoke green in CI, license/CVE gate,
and a same-box reproducible adoption benchmark. ADR-0013's own feasibility note flagged the PG17 build as the
open risk ("travou num pin rustc/MSRV" for the mooncake-from-source route — `docs/adr/0013:83`).

## Objective

Decide **which permissive columnar piece to embed** (`pg_mooncake` vs `pg_duckdb`), and **how to build it into
the PG17 Dockerfile**. **Decision reached: embed `pg_duckdb` directly** — it is the mature, PG17-native,
MIT-licensed DuckDB-in-Postgres engine that `pg_mooncake` itself sits on top of; adopting the base removes the
less-mature `pg_mooncake` layer + its Rust/pgrx/moonlink build burden while still delivering the measured
columnar/vectorized-analytics win. Keep "revisit `pg_mooncake` for the columnstore-mirror sync model" a valid
future outcome (M62 depends on the row-to-column sync surface).

---

## Evidência web (R0) — >=2 fontes primárias por claim

Cada claim abaixo foi extraído por WebFetch (`curl`) de fonte primária (repo/README/LICENSE/release API/doc
oficial) em 2026-07-08. Fontes locais complementam (não substituem) a varredura web.

### Claim 1 — `pg_duckdb` é MIT, GA (v1.x), mantido ativamente, e suporta PG17 nativamente

- **[F1a]** `https://api.github.com/repos/duckdb/pg_duckdb` -> `spdx_id: "MIT"`, `stargazers_count: 3146`,
  `pushed_at: 2026-06-26T16:27:57Z`, `open_issues_count: 109`, `archived: false`,
  `description: "DuckDB-powered Postgres for high performance apps & analytics."`
- **[F1b]** `https://raw.githubusercontent.com/duckdb/pg_duckdb/main/README.md` -> **"Requirements — PostgreSQL:
  14, 15, 16, 17, 18"**. Título: *"pg_duckdb: Official PostgreSQL Extension for DuckDB… Built in collaboration
  with Hydra and MotherDuck."*
- **[F1c]** `https://api.github.com/repos/duckdb/pg_duckdb/releases?per_page=5` -> `v1.1.1` (2025-12-18),
  `v1.1.0` (2025-12-11), **`v1.0.0` (2025-09-04)** — GA há ~10 meses, released cadence recente.
- **[F1d]** `https://hub.docker.com/v2/repositories/pgduckdb/pgduckdb/tags?name=17` -> tags oficiais
  **`17-v1.1.1`, `17-v1.1.0`, `17-v1.0.0`, `17-main`** (imagem PG17 GA existe).
- **[F1e]** `https://raw.githubusercontent.com/duckdb/pg_duckdb/main/LICENSE` -> *"Copyright 2024-2025 Stichting
  DuckDB Foundation … granted, free of charge … without restriction"* (MIT texto).

### Claim 2 — `pg_mooncake` é MIT mas MENOS maduro: sem release tagueado desde v0.1.2 (fev/2025); `main` é v0.2.0 não-lançado; e **depende de `pg_duckdb`**

- **[F2a]** `https://api.github.com/repos/Mooncake-Labs/pg_mooncake` -> `spdx_id: "MIT"`,
  `stargazers_count: 1983`, `pushed_at: 2026-03-31T05:53:29Z`, `open_issues_count: 14`.
- **[F2b]** `https://api.github.com/repos/Mooncake-Labs/pg_mooncake/releases` -> **último release `v0.1.2`
  (2025-02-12)**; tags: `v0.1.0..v0.1.3` — **nenhum v0.2 tagueado** (>16 meses sem release GA).
- **[F2c]** `https://raw.githubusercontent.com/Mooncake-Labs/pg_mooncake/main/Cargo.toml` -> `version = "0.2.0"`,
  features `pg14..pg18` (`default = ["bgworker","pg18"]`), deps: `pgrx = "0.16.1"`, `moonlink_service`,
  e um **fork de rust-postgres** (`postgres.git = "https://github.com/Mooncake-Labs/rust-postgres.git"`).
- **[F2d]** `https://raw.githubusercontent.com/Mooncake-Labs/pg_mooncake/main/pg_mooncake.control` ->
  **`requires = 'pg_duckdb'`** — `pg_mooncake` NÃO substitui, ele **empacota** `pg_duckdb` por baixo.
- **[F2e]** Ref local `.claude/knowledge-base/references/pg_mooncake/.gitmodules` -> submódulos
  `duckdb_mooncake`, `moonlink`, **`pg_duckdb` (url = github.com/duckdb/pg_duckdb)** + `Dockerfile` local:
  `FROM pgduckdb/pgduckdb:18-main` (mooncake é uma camada sobre a imagem pg_duckdb).

### Claim 3 — as rotas columnar comprimidas de terceiros (Citus, Hydra) são **AGPL -> barradas por D1**; a rota DuckDB é a única permissiva

- **[F3a]** `https://raw.githubusercontent.com/citusdata/citus/main/LICENSE` -> **"GNU AFFERO GENERAL PUBLIC
  LICENSE Version 3"** -> barrado (D1).
- **[F3b]** `https://raw.githubusercontent.com/hydradatabase/hydra/main/columnar/LICENSE` -> **"GNU AFFERO
  GENERAL PUBLIC LICENSE Version 3"** (o repo-raiz Hydra é Apache, mas o **diretório columnar** é AGPL) ->
  barrado (D1). Confirma ADR-0013 driver 3.
- **[F3c]** `https://api.github.com/repos/duckdb/duckdb` -> `spdx_id: "MIT"`; latest `v1.5.4`. A engine DuckDB
  transitiva é MIT (Stichting DuckDB Foundation) — permissiva.

### Claim 4 — AlloyDB HTAP SOTA = **in-memory columnar engine, auto row<->column, planner-chosen, até 100x vs PG padrão**; a rota DuckDB é **lakehouse/vetorizado on-disk** (aposta diferente — D2, honesto)

- **[F4a]** `https://cloud.google.com/blog/products/databases/alloydb-for-postgresql-columnar-engine` (Google
  Cloud Blog, 2022-05-26, autores Sheshadri Ranganath & Ravi Murthy, Eng. Directors AlloyDB) ->
  *"vectorized columnar execution engine… keeps frequently queried data in an **in-memory, columnar format**…
  AlloyDB **automatically organizes your data between row-based and columnar formats**, choosing the right
  columns and tables **based on learning your workload**… the **query planner smartly chooses** between columnar
  and row-based… **up to 100x faster** than standard PostgreSQL for analytical queries, with **no schema
  changes, application changes, or ETL required**."*
- **[F4b]** `https://raw.githubusercontent.com/duckdb/pg_duckdb/main/README.md` -> contraste honesto: pg_duckdb
  *"integrates DuckDB's **columnar-vectorized analytics engine**… **No data export required** — works directly
  with your existing PostgreSQL tables… set `duckdb.force_execution=true`"* — vetorização columnar **on-demand
  por query**, não uma cópia in-memory auto-mantida. Confirma a natureza D2 (ADR 0002 / `docs/benchmarks/m30-columnar-scale.md:24`:
  "DuckDB+Iceberg lakehouse on disk — NOT AlloyDB in-memory").

### Claim 5 — modelo de sincronização row<->column (prepara M62): pg_duckdb = **scan direto do heap** (sem CDC); pg_mooncake = **columnstore-mirror + logical replication**

- **[F5a]** pg_duckdb README (`.../main/README.md`) -> *"**No data export required.** You do not need to export your
  data to Parquet… works **directly with your existing PostgreSQL tables**"* + `SET duckdb.force_execution=true`.
  Sync = **zero** (lê o heap MVCC ao vivo via DuckDB executor). **Sem 2a cópia, sem lag, sem `wal_level=logical`.**
- **[F5b]** pg_mooncake README (`.../main/README.md` + ref local) -> `CALL mooncake.create_table('t_iceberg','t')`
  cria um **columnstore mirror em Iceberg** que "stays in sync" via **moonlink streaming + `wal_level = logical`**
  (`shared_preload_libraries='pg_duckdb,pg_mooncake'`). Sync = **CDC/logical-replication para uma 2a cópia
  colunar** (freshness sub-segundo, mas materializada).
- **Implicação M62:** a superfície HTAP "mesma tabela" (M62) é **mais direta com pg_duckdb** (uma tabela, scan
  vetorizado on-demand) do que com a mirror-table do mooncake (duas relações a manter em sync). pg_mooncake
  entrega **compressão/Iceberg-native** que pg_duckdb-sobre-heap não dá — é o trade-off a medir em M62.

**Fontes ABERTAS e citadas por WebFetch: 11** (F1a-e, F2a-d, F3a-c, F4a, F5a — múltiplas extrações do mesmo
README contam como 1 fetch). **BLOCKED (honesto):** `https://cloud.google.com/alloydb/docs/columnar-engine/about`
retornou só nav-chrome JS-rendered (sem corpo estático) — **contornado** com o blog oficial equivalente [F4a]
(mesma autoria/engenharia AlloyDB), então o claim SOTA está coberto por fonte primária; não inventei o conteúdo
da página que não abriu.

---

## ADR (intra-blueprint) — Qual peça embarcar

**Decisão:** embarcar **`pg_duckdb`** (MIT, PG17-native, GA v1.1.1) diretamente na imagem PG17 do TheoDB.

**Alternativas consideradas + razão de rejeição:**

| Opção | O que é | Prós | Contras | Veredito |
|---|---|---|---|---|
| **(A) `pg_duckdb`** direto | DuckDB-in-PG oficial, MIT, GA, PG14-18 [F1b,F1c] | PG17-native (sem bump); GA/mantido (v1.0 set/2025, v1.1.1 dez/2025) [F1c]; build C++/CMake sem Rust/pgrx; static-link opcional; imagem oficial `17-v1.1.1` [F1d]; sync = scan direto do heap (zero 2a cópia) [F5a] | Sem columnstore comprimido/Iceberg-native "mesma tabela" (isso é a camada mooncake); DuckDB .so grande (~peso) | **ESCOLHIDA** |
| **(B) `pg_mooncake`** | Camada Rust sobre pg_duckdb: columnstore-mirror em Iceberg [F2d,F5b] | Columnstore comprimido + Iceberg-native; sync sub-segundo; foi o substrato do M30 | **Menos maduro**: último release GA v0.1.2 (fev/2025), `main` v0.2.0 não-lançado [F2b,F2c]; **puxa pg_duckdb de qualquer jeito** (`requires` [F2d]) + Rust/pgrx/moonlink/fork-de-rust-postgres [F2c] -> build MAIS pesado; `default=["pg18"]` (PG17 é feature não-default) [F2c]; foi exatamente a rota que travou no build PG17 (ADR-0013:83) | **REJEITADA (agora)**: adota-se a base madura primeiro; reavaliar a camada mirror em M62 se a compressão/Iceberg for requisito medido |
| **(C) Bump PG17->PG18** p/ usar mooncake prebuilt | Trocar a base da imagem | mooncake tem prebuilt PG18 | Muda o gate wire-compat de todo o produto (pgvector/pgvectorscale/theodb_rs recompilam contra PG18); risco desproporcional só p/ columnar; YAGNI | **REJEITADA**: mudança de plataforma grande demais para o objetivo |
| **(D) Reescrever columnar próprio** | Columnar engine em Rust | Controle total | PhD-level/anos; DuckDB é battle-tested (Regra 9); explicitamente fora de escopo (ADR-0013 opção B rejeitada) | **REJEITADA** (ADR-0013) |

**Razão-chave da escolha (A):** `pg_mooncake` **não é uma alternativa a** `pg_duckdb` — ele **é uma camada sobre**
`pg_duckdb` [F2d,F2e]. Embarcar a base MIT-madura-GA-PG17-native entrega o win columnar medido (a query do M30 é um
`GROUP BY` vetorizado que o executor DuckDB acelera) com o **menor build** e **zero bump de plataforma**, honrando
Regra 9 (adotar a peça madura) + Regra 10 (KISS — não empilhar a camada mooncake ainda). A camada mooncake
(compressão/Iceberg-native/mirror sync) vira uma decisão **medida** em M62, não um custo assumido cego agora.

**Caveat de honestidade (Regra 3):** o benchmark do M30 rodou sobre `pg_mooncake` (columnstore-mirror
`DuckDBScan`), NÃO sobre `pg_duckdb` puro (heap-scan `duckdb.force_execution`). São planos DuckDB distintos.
Portanto o **M61 adoption benchmark DEVE re-medir com pg_duckdb** na mesma box — o ~9x@1M/~14x@5M do M30 é
evidência da CAPACIDADE columnar-vetorizada, **não** uma promessa transferível 1:1 ao heap-scan do pg_duckdb até
re-medido. Sem re-medição, o número fica `UNBENCHMARKED` para a superfície pg_duckdb (Regra 5).

---

## Caminho de build no Dockerfile (PG17, artifact-only, espelha o padrão pgvectorscale)

pg_duckdb builda via **git clone + submodule + `make install`** (C++/CMake/DuckDB deps — `cmake ninja-build
libc++-dev libcurl4-openssl-dev liblz4-dev`), **sem Rust/pgrx** [fonte:
`https://raw.githubusercontent.com/duckdb/pg_duckdb/main/docs/compilation.md`]. Suporta **static link**
(`DUCKDB_BUILD=ReleaseStatic make install`) — recomendado para o embed (evita conflito de versão DuckDB e
simplifica o COPY de artefatos). Esboço do estágio (mesmo padrão de `scale-builder`/`theodb-rs-builder`,
`Dockerfile:11,32`; cleanup no estilo do próprio repo, `Dockerfile:65` usa `rm -r /tmp/pgvector`):

- `FROM ${BASE_IMAGE} AS pgduckdb-builder` (PG17 pinado por digest, igual ao scale-builder).
- apt install: `build-essential postgresql-server-dev-17 cmake ninja-build pkg-config git ca-certificates libc++-dev libc++abi-dev liblz4-dev libcurl4-openssl-dev libssl-dev`.
- `git clone --branch v1.1.1 https://github.com/duckdb/pg_duckdb /tmp/pg_duckdb && cd /tmp/pg_duckdb && git submodule update --init --recursive`.
- `DUCKDB_BUILD=ReleaseStatic make install -j"$(nproc)"` (static link -> um só artefato, sem libduckdb.so avulso).
- **Runtime stage:** `COPY --from=pgduckdb-builder` os `pg_duckdb*` de `/usr/lib/postgresql/17/lib/` e `/usr/share/postgresql/17/extension/` (mesmo COPY artifact-only que pgvectorscale usa, `Dockerfile:72-73`).
- **Append (não overwrite)** `shared_preload_libraries = 'pg_duckdb'` no `postgresql.conf.sample`.
- **Init-script** (junto ao `00-create-theodb.sql`, `Dockerfile:107`): `CREATE EXTENSION IF NOT EXISTS pg_duckdb;`.

**Gotcha crítico (build):** pg_duckdb **precisa de `shared_preload_libraries='pg_duckdb'`** (hook do executor)
[README+compilation] — diferente de pgvector/pgvectorscale (LOAD lazy). Isso muda `postgresql.conf`, não só o
extension dir. Se o TheoDB já setar `shared_preload_libraries` (checar), **append**, não overwrite. Static-link
(`ReleaseStatic`) evita ter que COPY o `libduckdb.so` separado e o risk de version-skew.

---

## Coverage Corner 1 — Integration Tests

Como validar o embed em CI (smoke end-to-end), reusando o padrão de smoke do TheoDB.

1. **Extension smoke:** `CREATE EXTENSION pg_duckdb;` retorna sem erro numa fresh DB init (com
   `shared_preload_libraries` setado) -> prova que o `.so` linka contra o PG17 exato da imagem (mesma garantia que
   o multi-stage de `scale-builder` dá, `Dockerfile:7`).
2. **Analytic smoke (o win do M30, re-medido na superfície pg_duckdb):** popular uma tabela heap, rodar
   `SET duckdb.force_execution=true; EXPLAIN (ANALYZE) SELECT category, count(*), avg(amount) FROM t GROUP BY
   category;` e **assertar que o plano é DuckDB-executed** (não Seq Scan) — o oráculo é o plano, não só o
   resultado. Espelha a query canônica do M30 (`docs/benchmarks/m30-columnar-scale.md:4`).
3. **Correctness cross-engine:** `count` exato + `avg` dentro de tolerância `1e-3` vs a mesma query com
   `duckdb.force_execution=false` (row engine) — o M30 já documentou que a soma PG vs DuckDB difere no último
   decimal (`docs/benchmarks/m30-columnar-scale.md:15`); **não** exigir byte-idêntico (negative-case honesto).
4. **Fail-closed:** sem `shared_preload_libraries`, `CREATE EXTENSION pg_duckdb` deve **falhar com erro claro**
   (typed) — assertar a mensagem, não só "throws" (`.claude/rules/testing.md` §4.1 negative case).

Referência de teste real do upstream: `https://github.com/duckdb/pg_duckdb/blob/main/.github/workflows/build_and_test.yaml`
(a matriz de build+test PG14-18 do próprio pg_duckdb — reusar como espelho do gate de CI do TheoDB).

## Coverage Corner 2 — Dependencies

O que o embed puxa + licenças (D1: só Apache/MIT/BSD/PostgreSQL; AGPL barrado).

| Dependência | Versão (ref) | Licença | Fonte primária | D1-clean? |
|---|---|---|---|---|
| **pg_duckdb** | v1.1.1 (2025-12-18) | **MIT** | [F1a] `api.github.com/repos/duckdb/pg_duckdb` `spdx_id:MIT` | sim |
| **DuckDB** (engine transitiva) | v1.5.x (pin do pg_duckdb submodule) | **MIT** | [F3c] `api.github.com/repos/duckdb/duckdb` `spdx_id:MIT` | sim |
| DuckDB extensions (iceberg/delta/httpfs) | on-demand | Majoritariamente MIT | verificar por extensão no `/deps-audit` (algumas são community) | AUDITAR |
| ~~Citus columnar~~ | — | **AGPLv3** | [F3a] `citus/LICENSE` | NÃO — **barrado (D1)** |
| ~~Hydra columnar~~ | — | **AGPLv3** (dir columnar) | [F3b] `hydra/columnar/LICENSE` | NÃO — **barrado (D1)** |
| pg_mooncake (rejeitado agora) | v0.2.0 (unreleased) | MIT | [F2a] | licença OK, rejeitado por maturidade |

**Gate `/deps-audit` (DoD M61):** rodar CVE scan sobre pg_duckdb + a árvore DuckDB. DuckDB puxa
libcurl/libssl/lz4 (transitivas C++) — CVEs dessas libs C entram no scan da imagem base, não do extension per se;
mas as **DuckDB community extensions** (`duckdb.allow_community_extensions`) são um vetor a **manter DESLIGADO por
default** no embed (superfície de supply-chain não-auditada).

## Coverage Corner 3 — Tools

Ferramentas do build + do gate.

- **Build:** `cmake`, `ninja-build`, `make`, `git submodule`, `postgresql-server-dev-17`, `libc++-dev`,
  `liblz4-dev`, `libcurl4-openssl-dev` (lista exata: pg_duckdb `docs/compilation.md` "Install Build Dependencies").
  **Sem Rust/pgrx** (contraste: mooncake precisaria de cargo-pgrx 0.16.1 + toolchain, `Dockerfile:41` do
  scale-builder — a mesma cadeia que travou no build PG17 do mooncake, ADR-0013:83).
- **Imagem oficial de referência (comparar peso):** `pgduckdb/pgduckdb:17-v1.1.1`,
  `full_size ~= 224 MB comprimido` [fonte: `hub.docker.com/v2/repositories/pgduckdb/pgduckdb/tags`]. O runtime
  TheoDB hoje é ~445 MB (`Dockerfile:4`); somar a superfície DuckDB (o `libduckdb-linux-amd64.zip` do release é
  ~41 MB comprimido -> ~150-200 MB descomprimido em .so [fonte: `api.github.com/repos/duckdb/duckdb/releases/latest`])
  é o **maior custo de peso** — medir o delta real da imagem no gate.
- **Benchmark de adoção (DoD):** reusar o harness do M30 (`benchmarks/run_m30_columnar_scale.py`, adaptar para
  a superfície pg_duckdb `force_execution` em vez do mooncake `create_table`) -> `docs/benchmarks/m61-columnar-adoption.{md,json}`,
  mean+-std sobre >=3 runs, MESMA box, controle row-store (`force_execution=false`).

## Coverage Corner 4 — Techniques

O SOTA e o posicionamento (R1 — ancorar no AlloyDB; R3 — perf é claim benchmarkado).

- **SOTA (AlloyDB) [F4a]:** columnar engine **in-memory**, **auto** row<->column por aprendizado de workload,
  planner escolhe, até **100x vs PG padrão**, zero schema/app/ETL change. É a barra.
- **Gap honesto (D2) [F4b, F5a]:** pg_duckdb NÃO é in-memory-auto-mantido — é **vetorização columnar on-demand
  por query** sobre o heap ao vivo (`duckdb.force_execution=true`). Vantagens vs AlloyDB: **permissivo (MIT),
  on-prem, model-agnostic, zero 2a cópia**; desvantagens: **sem auto-columnarização aprendida**, sem cache
  columnar in-memory persistente. É a **aposta lakehouse/vetorizada diferente** que o ADR 0002 D2 já declara —
  NÃO cópia do AlloyDB. **Nenhum claim "igual/superior ao AlloyDB em analytics" sem o benchmark M61** (Regra 5).
- **Técnica de aceleração:** DuckDB é um executor vetorizado (morsel-driven, colunar em memória por query),
  top-10 ClickBench (mooncake README cita ClickBench — datapoint, não benchmark próprio do TheoDB -> `UNBENCHMARKED`
  até re-medido na nossa box).
- **HTAP em PG — o campo (>=2 fontes):** (i) AlloyDB in-memory columnar [F4a]; (ii) a rota DuckDB-in-PG
  (pg_duckdb) [F1b/F4b] — as duas famílias de HTAP-em-Postgres permissivas/comerciais; as rotas columnar-comprimido
  clássicas (Citus/Hydra) estão **barradas por licença** [F3a,F3b], não por mérito técnico.

---

## Riscos honestos

1. **Peso da imagem (MÉDIO-ALTO).** A engine DuckDB é grande (~41 MB zip -> ~150-200 MB .so descomprimido; imagem
   oficial pgduckdb ~224 MB comprimida). O runtime TheoDB salta de ~445 MB. **Mitigação:** static-link
   (`ReleaseStatic`) + medir o delta real no gate; considerar se o columnar é um tier opcional da imagem (imagem
   `theodb-htap` separada) se o peso for inaceitável para o deploy padrão. **Decisão de tiering = medição, não
   agora.**
2. **Compat de build PG17 (MÉDIO — reduzido vs mooncake).** pg_duckdb declara PG17 nativo [F1b] e tem imagem GA
   `17-v1.1.1` [F1d] -> risco **muito menor** que a rota mooncake-from-source-PG17 que travou (ADR-0013:83).
   Resíduo: a árvore DuckDB C++ é build pesado (cmake/ninja, minutos de CI). **Mitigação:** pin `PGDUCKDB_REF` a
   um tag GA; cache de layer do submodule.
3. **Licença transitiva / supply-chain da DuckDB (MÉDIO).** DuckDB core = MIT [F3c], mas **community extensions**
   (`allow_community_extensions`) são não-auditadas. **Mitigação:** manter community extensions **OFF por default**;
   `/deps-audit` sobre pg_duckdb + libs C transitivas (libcurl/openssl/lz4) como gate de release (D1/PRD §11).
4. **`shared_preload_libraries` obrigatório (MÉDIO — operacional).** pg_duckdb exige preload (hook do executor),
   diferente de pgvector/vectorscale. Muda `postgresql.conf`; um append errado quebra o boot. **Mitigação:** smoke
   fail-closed no CI (Corner 1 item 4) + append idempotente no `.sample`.
5. **Número do M30 não é transferível 1:1 (BAIXO — honestidade).** M30 mediu mooncake (`DuckDBScan` mirror);
   pg_duckdb é heap-scan `force_execution`. **Mitigação:** re-medir na superfície pg_duckdb no benchmark M61;
   marcar `UNBENCHMARKED` até lá (Regra 5).

## Posicionamento honesto vs AlloyDB (Regra 5)

TheoDB M61 entrega **analytics columnar-vetorizado permissivo, on-prem, model-agnostic** via pg_duckdb (MIT) —
**vence já hoje** em abertura/custo/portabilidade. NÃO igualamos o **in-memory auto-columnar-aprendido** do AlloyDB
[F4a]; a nossa é a aposta **lakehouse/vetorizada on-demand** (D2), declaradamente diferente. **Zero claim de
paridade/superioridade de performance analítica sem o benchmark reproduzível `docs/benchmarks/m61-columnar-adoption.*`
na mesma box** — o M30 prova a capacidade columnar (~9x@1M no substrato mooncake), NÃO um número transferível ao
embed pg_duckdb até re-medido. Design goals são metas, não fatos (`.claude/rules/public-copy.md` §4).

## Prior Art

- **Interno:** ADR-0013 (KEEP columnar permissivo + evidência M30), `docs/benchmarks/m30-columnar-scale.md`
  (~9x@1M/~14x@5M), `Dockerfile` (padrão multi-stage artifact-only de pgvectorscale/theodb_rs a espelhar).
- **Externo (web, R0):** pg_duckdb (MIT, GA, PG17) [F1a-e], pg_mooncake (MIT, v0.1.2, sobre pg_duckdb) [F2a-e],
  DuckDB (MIT) [F3c], AlloyDB columnar blog [F4a], Citus/Hydra AGPL [F3a,F3b].
- **Refs locais:** `.claude/knowledge-base/references/pg_mooncake/` (snapshot com submódulo pg_duckdb),
  `.claude/knowledge-base/references/duckdb/` (LICENSE MIT).

## Unresolved Questions

1. **Tiering da imagem?** columnar no runtime default (peso +~150-200 MB) vs imagem `theodb-htap` opcional ->
   **decisão de medição** (risco 1), fora deste discover.
2. **Static vs dynamic link do DuckDB** — `ReleaseStatic` recomendado; confirmar tamanho/peso final no gate.
3. **M62 (superfície HTAP unificada):** heap-scan pg_duckdb (uma tabela) vs mirror mooncake (Iceberg comprimido).
   O trade-off compressão/Iceberg-native vs simplicidade é **medição de M62**, não M61.

## Drawbacks & Risks (resumo)

Ver § Riscos honestos (5 riscos, com severidade + mitigação). Os dois load-bearing: **peso da imagem** (MÉDIO-ALTO)
e **licença transitiva/community-extensions** (MÉDIO, mitigado por manter OFF + deps-audit).
