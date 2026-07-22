---
slug: m143-pgduckdb-total-removal
milestone_id: M143
created_at: 2026-07-22
goal: Remove pg_duckdb entirely by replacing the M62 lakehouse surface with own-code Parquet read+write (DataFusion), proven by a no-pg_duckdb round-trip + image-size delta on the droplet.
---

# Plan: M143 — Remoção total do pg_duckdb (lakehouse Parquet own-code)

> **Version 1.0** — Substitui a superfície M62 (write via `COPY…parquet`, read+aggregate via `duckdb.query`) por
> own-code Rust (DataFusion + Arrow, já no binário; Apache-2.0), remove o `pg_duckdb` inteiro, dobra a capacidade
> lakehouse no build default e aposenta a imagem `theodb-htap`. O spike (Fase 4, GO) provou a viabilidade do read;
> este milestone entrega write + read geral + reescrita do `sql/85` + drop do pg_duckdb, medido no droplet.

## Goal

> "Enable o TheoDB to ler e escrever Parquet externo own-code (sem pg_duckdb/DuckDB) so that o último componente C++/httpfs sai do projeto e o lakehouse roda no build default, measured by o round-trip own-code (write→read→aggregate) passar SEM pg_duckdb no droplet E `pg_extension` não conter `pg_duckdb` na imagem default, registrado em `docs/benchmarks/m143-pgduckdb-removal.md`."

## Context

O M142 (ADR-0056) tierou o `pg_duckdb` para uma imagem opcional `theodb-htap`. O spike da Fase 4
(`docs/benchmarks/parquet-reader-owncode-spike.md`) mediu que ler Parquet own-code via DataFusion é **VIÁVEL** —
paridade byte-a-byte vs `pg_duckdb.read_parquet` a **+9 MB** no `.so` vs **118 MB** do bundle DuckDB. Com a
viabilidade provada, este milestone conclui a jornada: substitui a superfície M62 por own-code, remove o
`pg_duckdb`, e dobra o lakehouse no default (a imagem `theodb-htap` deixa de existir). Toda a dependência de
pg_duckdb no repo está em `sql/85-theodb-htap.sql` (2 funções codegen) — não há outros consumidores.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/parquet_spike.rs` | 93 | `cc2ea33` (2026-07-22) | Spike do read+aggregate own-code (feature `spike-parquet`) | Promover para produção; a lógica de leitura/agregação validada não regride |
| `theodb_rs/src/am/df_executor.rs` | 760 | `c6025d3` (2026-07-21) | Executor DataFusion + bridge Arrow↔PG (`arrow_value_to_datum`, `build_arrow`) + runtime tokio in-extension | Reusar o bridge + o padrão de runtime; não alterar o caminho columnar (M100) |
| `sql/85-theodb-htap.sql` | 216 | `f3cbee2` (2026-07-22) | Superfície M62 codegen (`htap_refresh_sql`/`olap_sql` retornam texto pg_duckdb) | `htap_register`/`_htap_path`/`htap_freshness` (SQL puro) preservados; superfície funciona SEM pg_duckdb |
| `theodb_rs/Cargo.toml` | 94 | `25c2367` (2026-07-22) | Deps + features (`spike-parquet`) | Promover `datafusion/parquet` (+`arrow-json`) para permanente; sem AGPL (D1) |
| `theodb_rs/src/lib.rs` | (existe) | — | Wiring de módulos | Módulo parquet own-code ligado por default (não atrás de feature) |
| `Dockerfile` | 87 | `52b5977` (2026-07-22) | Build da imagem default (sem pg_duckdb desde M142) | Continua sem pg_duckdb; o lakehouse own-code já vem do theodb_rs |
| `packaging/Dockerfile.htap` (DELETE) | 67 | `f3cbee2` (2026-07-22) | Imagem opcional com pg_duckdb (M142) | **Removida** — o lakehouse dobra no default |
| `theodb_rs/src/parquet.rs` (NEW) | 0 | — | (a criar) superfície própria: `read_parquet`(SETOF jsonb) + `write_parquet` + `olap` | — |
| `theodb_rs/src/parquet_spike.rs` (DELETE após promoção) | — | — | o spike vira `parquet.rs` de produção | — |
| `.github/workflows/ci.yml` | (existe) | — | CI (job `htap-image` do M142) | Remover o job htap-image; smoke default valida o lakehouse own-code |
| `docs/adr/0057-*.md` (NEW), `README.md`, `CHANGELOG.md`, `theodb.control` | — | — | ADR emenda + docs + bump 1.5→1.6 | — |

### Current callers / dependents

- **Símbolo:** `theodb.olap_sql`, `theodb.htap_refresh_sql` (`sql/85`). Callers: nenhum interno; superfície pública consumida por `scripts/m61-pgduckdb-smoke.sh`, `benchmarks/run_m62_htap.py`, `benchmarks/tests/test_htap.py` (testes, NÃO no CI atual). External: sem dogfood em produção.
- **Símbolo:** `arrow_value_to_datum`, `build_arrow` (`df_executor.rs`). Callers: o caminho columnar M100 (`am/`). Preservar.
- **Símbolo:** `read_parquet_agg_spike` (`parquet_spike.rs`): só testes/spike. Vira produção.

### Domain glossary

- **lakehouse** — ler/agregar arquivos colunares externos (Parquet) sem carregá-los como tabelas PG.
- **codegen surface (M62)** — funções que RETORNAM texto SQL que o cliente executa (necessário só porque o pg_duckdb proíbe DuckDB dentro de função — restrição que some com own-code).
- **bridge Arrow↔PG** — `arrow_value_to_datum` (Arrow→datum PG, leitura) e `build_arrow` (PG→Arrow, escrita) em `df_executor.rs`.
- **SETOF jsonb** — o shape do leitor geral: cada linha Parquet → um `jsonb` (cobre todos os tipos, incl. nested, via `arrow-json`).

### Architecture boundaries affected

- **Extension surface** (`theodb_rs` schema): novas funções `#[pg_extern]` (`read_parquet`, `write_parquet`, `olap`). Idênticas em qualquer imagem (uma imagem só agora).
- **Packaging** (Dockerfile): a distinção default/htap **colapsa** numa imagem só — o lakehouse vem do theodb_rs.
- **Extension SQL** (`sql/85`): reescrita para chamar own-code; cadeia de upgrade M137 (novo `theodb--1.5--1.6.sql`).

## Prior Art & Related Work

- **Internal blueprint:** `knowledge-base/discoveries/blueprints/m143-pgduckdb-total-removal-blueprint.md`.
- **Spike medido (GO):** `docs/benchmarks/parquet-reader-owncode-spike.md` + `theodb_rs/src/parquet_spike.rs`.
- **Reuso own-code:** `theodb_rs/src/am/df_executor.rs` (bridge + runtime), ADR-0042/M100 (DataFusion executor).
- **pg_duckdb / M62:** ADR-0020/0021/0023/0056, `sql/85-theodb-htap.sql`.
- **Rules:** `parsimony-ladder.md` (rung 4), `error-handling.md` (§2 fail-closed typed), `public-copy.md`.

## Objective

- [ ] `theodb.read_parquet(path) → SETOF jsonb` own-code (broad — todos os tipos via arrow-json), fail-closed em erro.
- [ ] `theodb.write_parquet(rel, path)` own-code (tabela → parquet via SPI+Arrow+`write_parquet`).
- [ ] `theodb.olap(rel)` own-code (parquet snapshot → agregado M62 tipado) — paridade byte-a-byte vs pg_duckdb.
- [ ] `sql/85` reescrito para o caminho own-code; guard M142 removido; `pg_duckdb` não referenciado.
- [ ] `pg_duckdb` removido: `Dockerfile.htap` deletado, feature `spike-parquet`→permanente, CI job htap removido.
- [ ] Round-trip own-code + `pg_extension` sem pg_duckdb + delta de tamanho medidos no droplet.
- [ ] ADR-0057 (emenda 0056) + README + CHANGELOG + bump extensão 1.6.

## ADRs

### D1 — Leitor geral retorna `SETOF jsonb` (não composite record dinâmico)

- **Decision:** `theodb.read_parquet(path)` retorna `SETOF jsonb` — cada linha Parquet vira um `jsonb` via o writer `arrow-json` (já puxado pelo feature parquet). O agregado M62 (`theodb.olap`) mantém shape **tipado** (`TableIterator`, como o spike).
- **Rationale:** cobre **todos os tipos** (escalares→json, nested/list/struct→objeto/array) sem a complexidade/re-work de `SETOF record` dinâmico no pgrx (grill R2). Reusa `arrow-json` (Regra 9). O cliente extrai tipado via `(r->>'col')::type`.
- **Alternatives considered:** (a) **SETOF record dinâmico** (column-def-list) — REJEITADO: complexo no pgrx, e nested/struct não mapeiam para colunas escalares → re-work. (b) **Shape fixo por chamada** — REJEITADO: não é "amplo".
- **Consequences:** habilita leitor broad simples; constringe: quem quer colunas tipadas faz o cast do jsonb (documentado).

### D2 — O codegen do M62 colapsa em funções diretas (own-code roda in-function)

- **Decision:** `htap_refresh`/`olap` viram funções que **executam** (escrevem/leem+agregam) dentro do corpo, não retornam texto para o cliente rodar. O `sql/85` chama as funções own-code (`theodb_rs`).
- **Rationale:** o design codegen existia SÓ porque "pg_duckdb proíbe DuckDB dentro de função" (`sql/85:6-16`) — restrição que **desaparece** com own-code (o spike é um `#[pg_extern]` que lê+agrega in-function). Simplificação, não workaround.
- **Alternatives considered:** manter o codegen chamando own-code — REJEITADO: mantém a dança cliente-executa sem a restrição que a justificava (complexidade acidental).
- **Consequences:** superfície mais simples; `htap_register`/`_htap_path`/`htap_freshness` (catálogo SQL) permanecem.

### D3 — Uma imagem só (lakehouse own-code no default; `theodb-htap` aposentada)

- **Decision:** promover o feature `spike-parquet` a permanente (default-on) → o lakehouse vem no `theodb_rs` do build default. Deletar `packaging/Dockerfile.htap` e o job CI `htap-image`.
- **Rationale:** own-code custa +9 MB (medido) vs 118 MB do DuckDB — o motivo do tier-out (M142) some. Decisão do owner.
- **Alternatives considered:** manter imagem htap own-code — REJEITADO pelo owner (custo baixo não justifica 2 imagens).
- **Consequences:** uma imagem; o ADR-0056 (tier-out) é emendado (o tier-out foi um passo intermediário; agora removido).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Suporte "amplo" a tipos (nested/list/struct) pode estourar um milestone | Medium | `SETOF jsonb` cobre nested naturalmente (JSON); tipos de arquivo raros → erro tipado fail-closed; dividir v1/v2 se estourar (grill R1) | Eng |
| Feature parquet permanente incha o binário default (+9 MB + arrow-json) | Low | Medido (+9 MB vs 118 MB DuckDB — ganho líquido enorme); registrado em docs/benchmarks | Eng |
| Escrever tabela PG grande → Arrow → parquet estoura memória | Medium | `GreedyMemoryPool(work_mem)` (padrão df_executor); streaming por batches; erro tipado se exceder | Eng |
| Consumidores externos (benchmarks/testes) que usavam duckdb.query quebram | Low | Não estão no CI; atualizar `run_m62_htap.py`/`test_htap.py` para a superfície own-code; blast radius baixo (pré-1.0) | Eng |

## Unresolved Questions

- Q1 — `write_parquet` de tabela arbitrária: ler via `SPI SELECT * FROM rel` e mapear cada tipo PG→Arrow — reusar `build_arrow` (que hoje serve o columnar) ou um caminho SPI novo? (Resolução no implement: começar com os tipos escalares do bridge; fail-closed no resto.)
- Q2 — paridade de `round(avg,4)`: own-code arredonda em Rust (o spike já provou paridade) — manter.

## Dependency Graph

```
Phase 1 (read_parquet SETOF jsonb + olap tipado — promove o spike)
   │
   ▼
Phase 2 (write_parquet own-code: tabela→parquet)
   │
   ▼
Phase 3 (reescreve sql/85 → own-code, remove guard)
   │
   ▼
Phase 4 (drop pg_duckdb: Dockerfile.htap DELETE + feature permanente + CI + docs/ADR)
   │
   ▼
Final Phase: Integration Validation (droplet: round-trip own-code SEM pg_duckdb + delta de tamanho)
```

Sequencial (cada fase depende da anterior). A Final Phase prova o todo.

---

## Phase 1: Superfície de leitura own-code (promove o spike)

**Objective:** `theodb.read_parquet(path) → SETOF jsonb` (broad) + `theodb.olap(path) → (category,c,a)` tipado, own-code, ligados por default (não atrás de feature).

### T1.1 — `parquet.rs` de produção: read_parquet (jsonb) + olap (tipado)

#### Objective
Promover `parquet_spike.rs` para `theodb_rs/src/parquet.rs` (produção, default-on): `read_parquet(path)→SETOF jsonb` via arrow-json + `olap(path)→TableIterator(category,c,a)` (o agregado do spike). Feature `spike-parquet` vira `datafusion/parquet`+`arrow-json` permanentes.

#### Why this step (action + reasoning)
1. **What this step does** — cria `src/parquet.rs` com as 2 funções; liga o feature parquet por default no Cargo.toml; wire no lib.rs sem `#[cfg]`.
2. **Why it is necessary now** — é a base own-code que o `sql/85` (Phase 3) vai chamar; o read já é GO (spike), então promover primeiro é o menor risco.

#### Evidence
`theodb_rs/src/parquet_spike.rs` (o read+agg validado, paridade byte-a-byte), `docs/benchmarks/parquet-reader-owncode-spike.md`. `arrow-json` já no tree (compilou no build do spike).

#### Files to edit
```
theodb_rs/src/parquet.rs (NEW) — read_parquet(path)→SETOF jsonb + olap(path)→TableIterator(category,c,a)
theodb_rs/src/parquet_spike.rs (DELETE) — promovido
theodb_rs/Cargo.toml — datafusion/parquet + arrow-json permanentes (remover a feature spike-parquet)
theodb_rs/src/lib.rs — `mod parquet;` (sem cfg)
theodb_rs/src/tests/... — teste do read (jsonb) + olap (parity)
```

#### Deep file dependency analysis
- `parquet.rs` (NEW): reusa o padrão tokio+block_on do `df_executor` e o `read_parquet(...).aggregate(...)` do spike; adiciona o caminho jsonb (RecordBatch → arrow-json writer → serde_json::Value → pgrx JsonB).
- `Cargo.toml`: `datafusion = { ..., features = ["parquet"] }` + `arrow-json` (se necessário explícito). Remove `spike-parquet`.

#### Deep Dives
- **read_parquet jsonb:** `arrow_json::writer::LineDelimitedWriter` (ou `ArrayWriter`) serializa cada RecordBatch em JSON; parse por linha → `pgrx::JsonB`. Cobre nested/list/struct nativamente.
- **olap tipado:** idêntico ao spike (category text, count i64, avg f64, round em Rust).
- Invariante: as funções `CREATE`-áveis sempre (o feature parquet é permanente agora); erro do DataFusion → `err_input` tipado (fail-closed).
- Edge: arquivo inexistente / corrompido → erro tipado (não panic atravessando C).

#### Pseudo-code / Signatures
```rust
#[pg_extern] fn read_parquet(path: String) -> SetOfIterator<'static, pgrx::JsonB> { … arrow-json … }
#[pg_extern] fn olap(path: String) -> TableIterator<'static,(name!(category,String),name!(c,i64),name!(a,f64))> { … }
```

#### Tasks
1. Criar `src/parquet.rs` com `olap` (do spike) + `read_parquet` (jsonb via arrow-json).
2. Cargo.toml: promover `datafusion/parquet` + `arrow-json`; remover `spike-parquet`.
3. lib.rs: `mod parquet;` (sem cfg). Deletar `parquet_spike.rs` + a linha cfg.

#### TDD
```
RED:  read_parquet_jsonb — ler um parquet multi-tipo (int/float/text/bool/ts) retorna N jsonb com os valores certos
RED:  olap_parity — olap(parquet) retorna a|2|15, b|1|5 (paridade com o spike/pg_duckdb)
GREEN: parquet.rs
REFACTOR: extrair o setup do SessionContext/runtime num helper se duplicar com df_executor
VERIFY: cargo pgrx install --features pg18 + spike-parquet-validate estendido (droplet)
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `SELECT theodb.olap('/path.parquet')` retorna `a|2|15`, `b|1|5` (paridade).
- [ ] `SELECT theodb.read_parquet('/path.parquet')` retorna 1 jsonb por linha, com todos os tipos escalares corretos.
- [ ] `read_parquet` de um arquivo inexistente → erro tipado (não panic).
- [ ] Build default (sem `--features spike-parquet`) compila com as funções (feature permanente).
- [ ] `parquet.rs` ≤ 500 linhas.

#### DoD
- [ ] `cargo pgrx install --features pg18` instala as funções; smoke read+olap verde no droplet.

---

## Phase 2: Escrita own-code (tabela → Parquet)

**Objective:** `theodb.write_parquet(rel, path)` materializa uma tabela PG em Parquet own-code (DataFusion `write_parquet`), dentro da função.

### T2.1 — write_parquet own-code

#### Objective
Ler as linhas de `rel` (via `Spi::connect`+`select`), construir RecordBatch (bridge PG→Arrow), escrever Parquet via `DataFrame::write_parquet`.

#### Why this step (action + reasoning)
1. **What this step does** — `theodb.write_parquet(rel regclass, path text)` lê a tabela e escreve o snapshot Parquet own-code, substituindo o `COPY…parquet` do pg_duckdb.
2. **Why it is necessary now** — o `htap_refresh` (Phase 3) precisa do write own-code para não depender do writer do DuckDB.

#### Evidence
`df_executor.rs:build_arrow` (PG→Arrow existe p/ o columnar), `Spi::connect` (ann_query.rs:80). `DataFrame::write_parquet` (DataFusion, feature parquet).

#### Files to edit
```
theodb_rs/src/parquet.rs — write_parquet(rel regclass, path text)
theodb_rs/src/tests/... — round-trip: write_parquet(t) → read_parquet(path) = linhas de t
```

#### Deep file dependency analysis
- Lê `SELECT * FROM rel` via SPI → mapeia cada coluna PG→Arrow (reusa a lógica de `build_arrow`/o bridge; tipos escalares) → `ctx.read_batch(batch).write_parquet(path, opts)`. Tipo não-suportado → erro tipado.

#### Deep Dives
- Memória: `GreedyMemoryPool(work_mem)`; para tabelas grandes, escrever por batches (streaming). v1: batch único com guarda de work_mem + erro tipado se exceder (fail-closed, não OOM).
- Invariante: escreve atômico (arquivo temp + rename) para não deixar parquet meio-escrito num crash.

#### Tasks
1. `write_parquet(rel, path)`: SPI select → colunas → Arrow RecordBatch → `write_parquet`.
2. Mapear os tipos escalares do bridge; fail-closed no resto.

#### TDD
```
RED:  write_read_roundtrip — write_parquet('t') depois read_parquet(path) devolve as linhas de t (escalares)
RED:  write_unsupported_type — coluna de tipo não-suportado → erro tipado (não panic)
GREEN: write_parquet
REFACTOR: compartilhar o PG→Arrow com build_arrow se aplicável
VERIFY: droplet round-trip
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `theodb.write_parquet('t', '/tmp/t.parquet')` cria o arquivo; `theodb.read_parquet('/tmp/t.parquet')` devolve as linhas.
- [ ] `write_parquet` de tipo não-suportado retorna SQLSTATE tipado (ex. `0A000`) E `SELECT 1` após = 1 (backend vivo, nunca panic).

#### DoD
- [ ] Round-trip write→read verde no droplet.

---

## Phase 3: Reescrever `sql/85` para own-code

**Objective:** `olap_sql`/`htap_refresh_sql` deixam de gerar `duckdb.query`/`COPY parquet`; a superfície M62 funciona SEM pg_duckdb.

### T3.1 — sql/85 → own-code + upgrade 1.5→1.6

#### Objective
Substituir o corpo de `htap_refresh`/`olap` para chamar `theodb.write_parquet`/`theodb.olap`; remover o guard M142; `theodb--1.5--1.6.sql` + bump control 1.6.

#### Why this step (action + reasoning)
1. **What this step does** — reescreve `sql/85`: `htap_refresh(rel)` chama `theodb.write_parquet` + registra o snapshot; `olap(rel)` resolve o snapshot + chama `theodb.olap`. Remove o guard `pg_extension pg_duckdb`.
2. **Why it is necessary now** — é o que efetivamente **desliga** o pg_duckdb da superfície; sem isso o drop (Phase 4) quebraria o M62.

#### Evidence
`sql/85` (as 2 funções codegen + o catálogo `_htap_snapshots`), a disciplina de upgrade M137 (`theodb--1.4--1.5.sql`).

#### Files to edit
```
sql/85-theodb-htap.sql — htap_refresh/olap own-code; remove guard; _htap_snapshots/_htap_path/freshness mantidos
sql/theodb--1.5--1.6.sql (NEW) — upgrade in-place das funções reescritas
theodb.control — default_version 1.5→1.6
Dockerfile — install inclui theodb--1.5--1.6.sql
```

#### Deep file dependency analysis
- `htap_refresh(rel)` (era `_sql`): `SELECT theodb.write_parquet(rel, theodb._htap_path(rel)); INSERT/UPDATE _htap_snapshots; RETURN now()`. `olap(rel)`: resolve o path do snapshot + `RETURN QUERY SELECT * FROM theodb.olap(path)`. Downstream: os testes/bench.

#### Deep Dives
- Invariante: extensão idêntica em qualquer imagem (uma só agora); cadeia de upgrade intacta (o 1.5→1.6 re-aplica as funções, byte-idêntico em intenção ao sql/85).
- Edge: chamada sem snapshot → erro tipado `no_data_found` (preservado).

#### Tasks
1. Reescrever `htap_refresh`/`olap` em `sql/85` (chamam own-code; sem duckdb.query/COPY; sem guard).
2. Criar `theodb--1.5--1.6.sql` re-aplicando; bump control 1.6; add ao install do Dockerfile.

#### TDD
```
RED:  m62_owncode — htap_refresh('t') + olap('t') retorna o agregado correto SEM pg_duckdb instalado
GREEN: reescrita
REFACTOR: None
VERIFY: droplet (imagem sem pg_duckdb) — a superfície M62 funciona
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `theodb.htap_refresh('t')` + `theodb.olap('t')` retornam o agregado SEM pg_duckdb em `pg_extension`.
- [ ] `sql/85` não contém `duckdb.query`/`FORMAT parquet`/guard pg_duckdb.
- [ ] `ALTER EXTENSION theodb UPDATE TO '1.6'` idempotente.

#### DoD
- [ ] M62 own-code verde no droplet; cadeia de upgrade OK.

---

## Phase 4: Drop do pg_duckdb + uma imagem só + docs

**Objective:** `pg_duckdb` some do projeto; o lakehouse own-code vem no default; `theodb-htap` aposentada.

### T4.1 — Deletar pg_duckdb + feature permanente + CI + docs

#### Objective
Deletar `packaging/Dockerfile.htap`; feature `spike-parquet`→permanente (já em T1.1); remover o job CI `htap-image` + as asserções M142 de pg_duckdb; ADR-0057 (emenda 0056) + README + CHANGELOG.

#### Why this step (action + reasoning)
1. **What this step does** — remove o Dockerfile.htap, o job CI htap, as menções pg_duckdb; documenta a remoção total.
2. **Why it is necessary now** — fecha o milestone: pg_duckdb 0 referências; uma imagem só.

#### Evidence
`packaging/Dockerfile.htap`, `.github/workflows/ci.yml:482` (job htap-image + asserções M142), `README.md`, ADR-0056.

#### Files to edit
```
packaging/Dockerfile.htap (DELETE)
.github/workflows/ci.yml — remove job htap-image; remove as asserções "pg_duckdb ABSENT" (agora trivial — nunca existe)
README.md — lakehouse own-code no default (sem menção a theodb-htap/pg_duckdb)
docs/adr/0057-m143-pgduckdb-total-removal.md (NEW) — emenda 0056/0020
CHANGELOG.md — Removed: pg_duckdb; Changed: lakehouse own-code no default
scripts/m61-pgduckdb-smoke.sh (DELETE — órfão, referencia vectorscale/pg_duckdb)
scripts/m143-removal-validate.sh (NEW) — round-trip own-code + read multi-tipo + paridade + delta de tamanho (a suíte da Final Phase)
```

#### Deep file dependency analysis
- Remove todo traço de pg_duckdb. `Dockerfile` (default) já não tinha pg_duckdb (M142) — só confirma que o lakehouse own-code vem do theodb_rs.

#### Tasks
1. `git rm packaging/Dockerfile.htap scripts/m61-pgduckdb-smoke.sh`.
2. ci.yml: remover job htap-image + asserções M142.
3. ADR-0057 + README + CHANGELOG (Removed pg_duckdb).

#### TDD
```
RED:  (validação de imagem — Final Phase) no_pgduckdb — grep pg_duckdb no repo (fora de docs/CHANGELOG histórico) = 0
GREEN: as remoções
REFACTOR: None
VERIFY: grep + build default + smoke (Final Phase)
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `test -f packaging/Dockerfile.htap` retorna 1 (não existe); `grep -rl pg_duckdb Dockerfile theodb_rs/src sql .github` = 0 ocorrências.
- [ ] `grep -c 'htap-image' .github/workflows/ci.yml` = 0.
- [ ] `test -f docs/adr/0057-m143-pgduckdb-total-removal.md` = 0 (existe) E `grep -c '0056' docs/adr/0057-*.md` ≥ 1 E `grep -c 'Removed' CHANGELOG.md` (seção Unreleased) ≥ 1.

#### DoD
- [ ] pg_duckdb 0 referências ativas; uma imagem só.

---

## Coverage Matrix

| # | Gap / Requirement (DoD M143) | Task(s) | Resolution |
|---|---|---|---|
| 1 | read_parquet own-code (broad, jsonb) | T1.1 | SETOF jsonb via arrow-json |
| 2 | write_parquet own-code | T2.1 | SPI+Arrow+write_parquet |
| 3 | olap own-code (paridade M62) | T1.1 | TableIterator tipado (spike promovido) |
| 4 | sql/85 sem duckdb.query; M62 sem pg_duckdb | T3.1 | reescrita + upgrade 1.6 |
| 5 | pg_duckdb removido; uma imagem só | T4.1 | Dockerfile.htap DELETE + feature permanente + CI |
| 6 | tipos amplos + fail-closed | T1.1, T2.1 | jsonb cobre todos; erro tipado no resto |
| 7 | ADR + README + CHANGELOG + bump 1.6 | T3.1, T4.1 | docs |
| 8 | round-trip + no-pg_duckdb + delta medidos | T4.1 | Script `m143-removal-validate.sh` mede no droplet |

**Coverage: 8/8 (100%)**

## Global Definition of Done

- [ ] Todas as fases completas.
- [ ] Round-trip own-code (write→read→olap) verde SEM pg_duckdb no droplet.
- [ ] `pg_extension` sem pg_duckdb na imagem default; `Dockerfile.htap` deletado.
- [ ] `read_parquet` (jsonb, multi-tipo) + `olap` (paridade) + `write_parquet` (round-trip) verdes.
- [ ] Delta de tamanho medido (pg_duckdb 118 MB fora, ~9 MB Rust dentro) em `docs/benchmarks/m143-pgduckdb-removal.md`.
- [ ] Cadeia de upgrade `theodb` 1.5→1.6 idempotente.
- [ ] ADR-0057 + README + CHANGELOG (Removed pg_duckdb).
- [ ] File-size budget (parquet.rs ≤ 500).
- [ ] **Plan archived** após review READY_TO_MERGE + merge.

## Failure scenarios

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| Parquet file (I/O) | arquivo inexistente/corrompido | `read_parquet('/nao/existe')` | erro tipado (SQLSTATE), nunca panic atravessando C |
| Parquet type | tipo Arrow não mapeável a PG (read tipado) ou PG→Arrow (write) | coluna de tipo exótico | erro tipado fail-closed (o jsonb cobre a leitura; o write declina) |
| Memória (write) | tabela maior que work_mem | tabela grande | erro tipado (GreedyMemoryPool), nunca OOM do backend |

## Final Phase: Integration Validation (MANDATORY)

> Droplet e2e-runner (PG18.4, Docker). Prova o todo SEM pg_duckdb.

### Execution
```bash
cd theodb_rs && cargo pgrx install --release --features pg18 --pg-config ~/.pgrx/18.4/pgrx-install/bin/pg_config
bash scripts/m143-removal-validate.sh   # (NEW, estende spike-parquet-validate)
#  → round-trip own-code (write_parquet→read_parquet→olap) SEM pg_duckdb
#  → read_parquet multi-tipo (int/float/text/bool/ts) correto + tipo exótico = erro tipado
#  → paridade olap vs baseline pg_duckdb (gerado 1× pela htap antiga)
#  → build default: pg_extension sem pg_duckdb; delta de tamanho vs a imagem htap (M142)
#  → M143_REMOVAL_OK
```

### Acceptance Criteria
- [ ] `bash scripts/m143-removal-validate.sh` imprime `M143_REMOVAL_OK` e sai 0 (round-trip write→read→olap sem pg_duckdb).
- [ ] `read_parquet` de um parquet com int/float/text/bool/timestamp retorna os valores certos; tipo não-suportado → SQLSTATE tipado.
- [ ] `theodb.olap` == baseline pg_duckdb byte-a-byte (`a\|2\|15`, `b\|1\|5`) no validate.
- [ ] Imagem default: `SELECT count(*) FROM pg_extension WHERE extname='pg_duckdb'` = 0; delta de tamanho registrado em `docs/benchmarks/m143-pgduckdb-removal.md`.

### If Validation Fails
1. Identificar se é do M143 ou pré-existente. 2. Corrigir tudo do M143. 3. Re-rodar. 4. Pré-existentes documentados no PR.
