---
slug: m61-columnar-htap-adoption
milestone_id: M61
created_at: 2026-07-08
goal: Embarcar pg_duckdb (MIT) na imagem PG17 do TheoDB com smoke + gate de licença/CVE + benchmark de adoção reproduzível.
---

# Plan: M61 — Embarcar o columnar/HTAP (pg_duckdb) na distribuição PG17

> **Version 1.0** — O M30/ADR-0013 decidiu MANTER o pilar columnar permissivo (medido ~9× @1M / ~14× @5M num `GROUP BY` sobre o substrato throwaway `mooncakelabs/pg_mooncake` PG18) mas **não o embarcou**. Este plano faz a adoção na imagem shipada (PG17): adiciona um estágio multi-stage `pgduckdb-builder` ao `Dockerfile` (git clone `pg_duckdb` v1.1.1 + submódulo + `DUCKDB_BUILD=ReleaseStatic make install`, C++/CMake/Ninja, **sem Rust/pgrx**), COPY artifact-only dos `pg_duckdb*` para o runtime, append idempotente de `shared_preload_libraries='pg_duckdb'`, `CREATE EXTENSION pg_duckdb` + smoke analítico verde em CI, gate de licença (D1 — MIT ✓) + `/deps-audit` das transitivas, e um benchmark de adoção reproduzível (columnstore/pg_duckdb `force_execution` vs row-store heap na MESMA box) → `docs/benchmarks/m61-columnar-adoption.{md,json}`. **Honestidade (Regra 5):** o ~14× do M30 é do substrato mooncake (`DuckDBScan` mirror), NÃO do heap-scan `force_execution` do pg_duckdb — o número da superfície pg_duckdb é `UNBENCHMARKED` até a Fase 3 medir; honest-negative (pg_duckdb não vencer o heap no nosso dataset) é resultado válido.

## Goal

> "Enable a distribuição TheoDB (imagem PG17) to servir analytics columnar-vetorizado via pg_duckdb embarcado so that `CREATE EXTENSION pg_duckdb` + uma query analítica planejam sob o executor DuckDB numa init limpa, measured by o smoke CI `test_pg_duckdb_analytic_query_plans_under_duckdb()` passando E o artefato `docs/benchmarks/m61-columnar-adoption.json` existir com ≥3 runs mean±std na mesma box."

**Métrica nomeada:** o smoke `test_pg_duckdb_analytic_query_plans_under_duckdb()` verde (oracle = plano DuckDB, não Seq Scan) + `docs/benchmarks/m61-columnar-adoption.json` presente.

## Context

M30/ADR-0013 (`docs/adr/0013-v1-legacy-columnar-bm25-scope.md:41`) decidiu **KEEP** o columnar permissivo com evidência de escala (`docs/benchmarks/m30-columnar-scale.md:11` — ~14× @5M) mas **não embarcou**: a imagem shipada é PG17 (`Dockerfile:8`, `postgres:17-bookworm`) com pgvector + pgvectorscale + theodb_rs; o M30 rodou sobre `mooncakelabs/pg_mooncake:latest` **PG18** (`docs/benchmarks/m30-columnar-scale.md:3`) — um throwaway. A nota de feasibility do ADR-0013 (`docs/adr/0013-v1-legacy-columnar-bm25-scope.md:81-84`) flagrou o build PG17 from-source do mooncake como o risco aberto (travou num pin rustc/MSRV).

O blueprint de discovery (`m61-columnar-htap-adoption-blueprint.md`, verdict SHIPPABLE_WITH_CAVEATS, 11 fontes web primárias) **decide embarcar `pg_duckdb`, NÃO `pg_mooncake`** — porque `pg_mooncake` **não é uma alternativa** a `pg_duckdb`, é uma **camada Rust/pgrx sobre** ele (`pg_mooncake.control` → `requires = 'pg_duckdb'` [F2d]). Adotar a base MIT-madura-GA-PG17-native (`pg_duckdb` v1.1.1, PG14-18 nativo [F1b,F1c]) entrega o win columnar-vetorizado medido com o menor build (C++/CMake, sem Rust) e zero bump de plataforma (Regra 9 + Regra 10). A camada mooncake (Iceberg/compressão/mirror sync) vira decisão **medida** em M62.

Esta milestone é o **gate de adoção**: build + smoke + licença/CVE + re-medição honesta na superfície pg_duckdb.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `Dockerfile` | 119 | (ver `git log -1 -- Dockerfile`) | Build multi-stage PG17 + pgvector + pgvectorscale + theodb_rs; runtime artifact-only ~445 MB (`Dockerfile:4`) | Runtime segue artifact-only (nenhum toolchain no runtime); pgvector+vectorscale+theodb_rs continuam carregando; `CREATE EXTENSION theodb CASCADE` do init (`Dockerfile:107-116`) intocado; base pinada por digest (`Dockerfile:8`) |
| `benchmarks/theodb_bench/columnar.py` | ~90 | (ver `git log -1`) | Helpers M6/M30 do columnstore mirror mooncake (`_AGG`, `seed_metrics`, `_wait_mirror_synced`, `create_columnstore_mirror`) | `seed_metrics` + `_AGG` reusados como-estão (Rule 9); a superfície mooncake (`ensure_mooncake_extension`, `create_columnstore_mirror`) NÃO é usada pelo caminho pg_duckdb — não removê-la (M62) |
| `benchmarks/theodb_bench/columnar.py` (mesmo arquivo, superfície nova) | — | — | (adiciona helper `force_execution` do pg_duckdb) | Não quebrar os callers M6/M30 existentes de `columnar.py` |
| `benchmarks/run_m61_columnar_adoption.py` (NEW) | 0 | — | (driver de adoção — espelha `benchmarks/run_m30_columnar_scale.py`) | — |
| `benchmarks/tests/test_columnar_pgduckdb.py` (NEW) | 0 | — | (smoke/gate CI do embed pg_duckdb) | — |
| `docs/benchmarks/m61-columnar-adoption.md` (NEW) | 0 | — | (artefato de adoção — gate DoD) | — |
| `docs/benchmarks/m61-columnar-adoption.json` (NEW) | 0 | — | (dados brutos ≥3 runs mean±std) | — |
| `docs/adr/0020-m61-embed-pgduckdb.md` (NEW) | 0 | — | (ADR: qual peça embarcar + como buildar — reabre a nota de adoção do ADR-0013; 0014-0019 ocupados, 0020 é o próximo livre) | — |
| `CHANGELOG.md` | (existe) | (ver `git log -1`) | Contrato público de mudanças (`CHANGELOG.md:13` `[Unreleased]`) | Formato Keep a Changelog; entrada em `[Unreleased] § Added` |

Todo path listado em qualquer `#### Files to edit` abaixo aparece nesta tabela.

### Current callers / dependents

- **Símbolo:** `seed_metrics(db, table, n)` + `_AGG` em `benchmarks/theodb_bench/columnar.py`
  - **Callers (produção/bench):** `benchmarks/run_m30_columnar_scale.py:28` (import `from theodb_bench.columnar import _AGG, _results_match, _wait_mirror_synced, seed_metrics`), `benchmarks/theodb_bench/columnar.py:44` (`run_columnar_vs_row`)
  - **Callers (tests):** verificar `benchmarks/tests/` — enumerar com `grep -rln 'seed_metrics\|_AGG' benchmarks/ --include='*.py'`
  - **Externo (API pública consumida por outros repos):** não — helper de benchmark interno
- **Símbolo:** estágios do `Dockerfile` (`scale-builder` `Dockerfile:11`, `theodb-rs-builder` `Dockerfile:32`, runtime `Dockerfile:51`)
  - **Callers (produção):** `docker build` (CI de imagem); o COPY artifact-only dos builders (`Dockerfile:72-73` vectorscale, `Dockerfile:78-79` theodb_rs) é o padrão a espelhar
  - **Externo:** a imagem é o produto shipado — mudança de `shared_preload_libraries` afeta boot de toda instância

Enumerar com `grep -rln 'seed_metrics' --include='*.py'`; citações resolvem.

### Domain glossary

- **pg_duckdb** — extensão oficial DuckDB-in-Postgres (MIT, GA v1.1.1, PG14-18). Vetorização columnar **on-demand por query** sobre o heap MVCC ao vivo via `SET duckdb.force_execution=true` (blueprint [F1b,F4b]).
- **`shared_preload_libraries`** — GUC do Postgres que carrega bibliotecas no startup do postmaster. pg_duckdb **exige** preload (hook do executor) — diferente de pgvector/vectorscale (LOAD lazy) (blueprint § Gotcha crítico).
- **`DUCKDB_BUILD=ReleaseStatic`** — modo de build do pg_duckdb que linka a engine DuckDB estaticamente no `.so` (um só artefato; evita `libduckdb.so` avulso + version-skew) (blueprint § Caminho de build).
- **columnstore mirror (mooncake)** — 2a cópia colunar Iceberg sincronizada por CDC/logical replication. É a superfície M30, **não** a superfície pg_duckdb — pg_duckdb lê o heap direto, sem 2a cópia (blueprint [F5a,F5b]).
- **artifact-only COPY** — padrão multi-stage: o builder compila; o runtime `COPY --from=builder` só os artefatos (`.so`+`.control`+`.sql`), sem toolchain (`Dockerfile:3,71-73`).

### Architecture boundaries affected

- **Build/infra (Dockerfile)** — adiciona um estágio builder + um COPY artifact-only no runtime. Cruza a fronteira "toolchain fica no builder, runtime só recebe artefato" (`rules/architecture.md § 1` composition root; `Dockerfile:3`). Direção: builder → runtime (uma via, artifact-only).
- **Config (postgresql.conf)** — `shared_preload_libraries` muda a config de boot. Fronteira de configuração do postmaster (não de código de domínio).
- **Benchmark (interface externa `benchmarks/`)** — driver de adoção fala com um container Postgres via DSN (I/O externo). Fronteira interface→infra; reusa `theodb_bench.db.VectorDB` (Rule 9).

## Prior Art & Related Work

- **Internal blueprints** — `Blueprint §"ADR (intra-blueprint) — Qual peça embarcar"` (decisão pg_duckdb vs pg_mooncake, tabela de alternativas), `Blueprint §"Caminho de build no Dockerfile"` (esboço do estágio + gotcha `shared_preload_libraries`), `Blueprint §"Coverage Corner 1 — Integration Tests"` (os 4 smokes: extension, analytic, correctness, fail-closed) — `m61-columnar-htap-adoption-blueprint.md`.
- **Internal ADRs** — ADR-0013 (`docs/adr/0013-v1-legacy-columnar-bm25-scope.md:81-84`, a nota de adoção gated que este plano executa), ADR-0002 (D2 — columnar é lakehouse/vetorizado, não in-memory AlloyDB).
- **Internal benchmarks** — `docs/benchmarks/m30-columnar-scale.md` (~14× @5M no substrato mooncake — a CAPACIDADE, não o número transferível), `benchmarks/run_m30_columnar_scale.py` (o driver a espelhar), `benchmarks/theodb_bench/columnar.py:13` (`_AGG` + `seed_metrics` reusados).
- **Reference projects** — `.claude/knowledge-base/references/pg_mooncake/.gitmodules` (submódulo `pg_duckdb`, prova que mooncake empilha sobre pg_duckdb), `.claude/knowledge-base/references/duckdb/` (LICENSE MIT).
- **External literature** — pg_duckdb README + `https://github.com/duckdb/pg_duckdb/blob/main/docs/compilation.md` (requisitos de build C++/CMake/Ninja, `DUCKDB_BUILD=ReleaseStatic`, `shared_preload_libraries` — arquivo do repo upstream pg_duckdb, NÃO path local do TheoDB), `https://github.com/duckdb/pg_duckdb/blob/main/.github/workflows/build_and_test.yaml` (matriz de CI PG14-18 a espelhar) — todos citados como fontes web primárias no blueprint (F1a-e, F3c).
- **Rules** — `rules/parsimony-ladder.md` (rung 2/4 — reusar `seed_metrics`/`VectorDB` antes de escrever; rung 1 — não empilhar mooncake), `rules/architecture.md § 1` (artifact-only, composition root), `rules/testing.md § 4.1` (negative case = fail-closed com mensagem tipada).

## Objective

- [ ] Estágio `pgduckdb-builder` adicionado ao `Dockerfile` (clone pg_duckdb v1.1.1 + submódulo + `ReleaseStatic make install`, sem Rust) e COPY artifact-only dos `pg_duckdb*` para o runtime — Fase 1
- [ ] `shared_preload_libraries='pg_duckdb'` append idempotente na config + `CREATE EXTENSION IF NOT EXISTS pg_duckdb` no init — Fase 1
- [ ] Smoke CI: extension carrega + query analítica planeja sob DuckDB + correctness cross-engine + fail-closed sem preload — Fase 2
- [ ] Gate de licença (D1 — MIT) documentado no ADR + `/deps-audit` das transitivas (DuckDB core MIT + libs C) verde — Fase 2
- [ ] Benchmark de adoção reproduzível (pg_duckdb `force_execution` vs heap, mesma box, ≥3 runs mean±std) → `docs/benchmarks/m61-columnar-adoption.{md,json}` — Fase 3
- [ ] Integration Validation: imagem builda, pgvector+vectorscale+theodb_rs+pg_duckdb coexistem, suíte existente não regride — Fase 4

## ADRs

### D1 — Embarcar `pg_duckdb` (não `pg_mooncake`)

- **Decisão:** Embarcar `pg_duckdb` v1.1.1 (MIT, PG17-native, GA) diretamente na imagem PG17.
- **Rationale:** `pg_mooncake` **é uma camada sobre** `pg_duckdb` (`requires = 'pg_duckdb'`, blueprint [F2d,F2e]), não uma alternativa. Adotar a base MIT-madura-GA-PG17-native entrega o win columnar-vetorizado medido (a query M30 é um `GROUP BY` que o executor DuckDB acelera) com o menor build (C++/CMake, sem Rust/pgrx/moonlink) e zero bump de plataforma. Honra Regra 9 (adotar a peça madura) + `rules/parsimony-ladder.md` rung 1 (não empilhar a camada mooncake sem necessidade medida).
- **Alternativas consideradas:**
  - **(B) `pg_mooncake`** — REJEITADA: menos maduro (último release GA v0.1.2 fev/2025, `main` v0.2.0 não-lançado, blueprint [F2b,F2c]); puxa pg_duckdb de qualquer jeito + Rust/pgrx/moonlink/fork-de-rust-postgres → build mais pesado; `default=["pg18"]` (PG17 é feature não-default); foi exatamente a rota que travou no build PG17 (ADR-0013:83). Reavaliar em M62 se compressão/Iceberg for requisito medido.
  - **(C) Bump PG17→PG18** para usar mooncake prebuilt — REJEITADA: muda o gate wire-compat de todo o produto (pgvector/pgvectorscale/theodb_rs recompilam contra PG18); risco desproporcional só para columnar; YAGNI.
  - **(D) Reescrever columnar próprio** — REJEITADA (já em ADR-0013 opção B): DuckDB é battle-tested (Regra 9), reescrita é PhD-level/anos.
- **Consequências:** Habilita analytics columnar permissivo on-prem sem bump de plataforma. Constrange: sem columnstore comprimido/Iceberg "mesma tabela" (isso é a camada mooncake, decisão M62); `.so` DuckDB grande (peso da imagem — ver Drawbacks).

### D2 — Build static-link (`DUCKDB_BUILD=ReleaseStatic`) vs dynamic

- **Decisão:** Build `DUCKDB_BUILD=ReleaseStatic make install` (engine DuckDB linkada estaticamente no `pg_duckdb.so`).
- **Rationale:** Static-link produz **um só artefato** (`pg_duckdb*`) para o COPY artifact-only — sem `libduckdb.so` avulso e sem risco de version-skew entre a lib DuckDB e a extensão (blueprint § Caminho de build). Espelha a garantia reprodutível do `scale-builder` (build contra o PG17 exato, `Dockerfile:6-7`).
- **Alternativas consideradas:**
  - **Dynamic link (default)** — REJEITADA: exige COPY do `libduckdb.so` separado + garantir que o loader o ache no runtime; adiciona um artefato + um ponto de version-skew sem benefício para o embed (imagem única, sem compartilhamento de lib DuckDB).
- **Consequências:** Habilita COPY de artefato único (mais simples, `rules/parsimony-ladder.md` rung 5). Constrange: `.so` maior (a engine vai dentro) — o custo de peso é medido no gate (Drawback R1).

### D3 — `shared_preload_libraries` via append idempotente ao `postgresql.conf.sample` vs `ALTER SYSTEM`

- **Decisão:** Append idempotente de `shared_preload_libraries='pg_duckdb'` ao `postgresql.conf.sample` da imagem, no build (grep-guarded para não duplicar).
- **Rationale:** pg_duckdb exige preload no startup do postmaster (hook do executor, blueprint § Gotcha crítico) — precisa valer já na **primeira init** de uma DB fresh, antes de qualquer conexão. `ALTER SYSTEM` roda numa sessão SQL (post-init) e só teria efeito após restart — tarde demais para o `CREATE EXTENSION` do init-script. O append no `.sample` (fonte do `postgresql.conf` gerado no `initdb`) garante o preload desde o boot. Append (não overwrite) preserva qualquer valor pré-existente; idempotência evita duplicar em rebuilds.
- **Alternativas consideradas:**
  - **`ALTER SYSTEM SET shared_preload_libraries='pg_duckdb'`** no init-script — REJEITADA: efeito só após restart; não vale na primeira init onde o `CREATE EXTENSION pg_duckdb` do init roda → `CREATE EXTENSION` falharia por preload ausente.
  - **Overwrite do `.sample`** — REJEITADA: se o TheoDB já setar `shared_preload_libraries` (checar no build), overwrite apaga o valor existente e quebra outra extensão.
- **Consequências:** Habilita `CREATE EXTENSION pg_duckdb` verde na primeira init. Constrange: um append errado quebra o boot → mitigado pelo smoke fail-closed (T2.3) + guard de idempotência.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| **Peso da imagem** — engine DuckDB é grande (~41 MB zip → ~150-200 MB `.so` descomprimido; imagem oficial pgduckdb ~224 MB comprimida). Runtime TheoDB salta de ~445 MB (`Dockerfile:4`) | Medium-High | Static-link (`ReleaseStatic`, D2) evita lib avulsa; **medir o delta real** de tamanho da imagem no gate (T4.1); decisão de tiering (imagem `theodb-htap` separada) = medição futura, não agora (Unresolved Q1) | paulohenriquevn |
| **Compat de build PG17** — a árvore DuckDB C++ é build pesado (cmake/ninja, minutos de CI); gotchas de compat possíveis | Medium (reduzido vs mooncake) | pg_duckdb declara PG17 nativo [F1b] + tem imagem GA `17-v1.1.1` [F1d] → risco muito menor que a rota mooncake-from-source que travou (ADR-0013:83); pin `PGDUCKDB_REF=v1.1.1` a tag GA; cache de layer do submódulo | paulohenriquevn |
| **Licença transitiva / community extensions** — DuckDB core MIT [F3c], mas community extensions (`allow_community_extensions`) são não-auditadas (vetor supply-chain) | Medium | Manter `duckdb.allow_community_extensions` **OFF por default** no embed; `/deps-audit` sobre pg_duckdb + libs C transitivas (libcurl/openssl/lz4) como gate (T2.4) | paulohenriquevn |
| **`shared_preload_libraries` obrigatório** — muda `postgresql.conf`; append errado quebra o boot | Medium | Append idempotente grep-guarded (D3) + smoke fail-closed no CI (T2.3) | paulohenriquevn |
| **Número do M30 não é transferível 1:1** — M30 mediu mooncake (`DuckDBScan` mirror); pg_duckdb é heap-scan `force_execution` (planos DuckDB distintos) | Low (honestidade) | Re-medir na superfície pg_duckdb (T3.1); marcar `UNBENCHMARKED` até lá; honest-negative é resultado válido (Regra 5) | paulohenriquevn |

## Unresolved Questions

- Q1 — **Tiering da imagem?** columnar no runtime default (+~150-200 MB) vs imagem `theodb-htap` opcional → decisão de **medição** (o gate T4.1 mede o delta), fora deste plano. Se o delta for inaceitável para o deploy padrão, abrir milestone de tiering.
- Q2 — **Static vs dynamic confirmado no peso final?** D2 escolhe static; o tamanho final do `.so` só é conhecido após o build (T4.1 mede).
- Q3 — **O `PGDUCKDB_REF=v1.1.1` builda limpo contra o PG17 exato da base pinada** (`Dockerfile:8` digest), ou há um pin de DuckDB/CMake a ajustar? Resolvido empiricamente na T1.1 (se o build falhar, o log dita o ajuste — Failure scenario 1).

## Dependency Graph

```
Fase 1 (Build) ──▶ Fase 2 (Smoke/Gate) ──▶ Fase 3 (Benchmark) ──▶ Fase 4 (Integration Validation)
   T1.1 estágio        T2.1 extension smoke      T3.1 driver+helper       T4.1 imagem+coexistência
   T1.2 COPY runtime    T2.2 analytic+correctness  T3.2 artefato .md/.json
   T1.3 preload+init    T2.3 fail-closed
                        T2.4 licença+deps-audit
```

- **Fase 1 é bloqueador sequencial** de tudo (sem a imagem buildada, nada carrega). T1.1→T1.2→T1.3 são sequenciais (o COPY depende do artefato do builder; o preload depende do COPY).
- **Fase 2** depende da imagem da Fase 1. T2.1→T2.2→T2.3 sequenciais (mesmo fixture de container); T2.4 (licença/deps-audit) pode rodar em paralelo a T2.1-2.3.
- **Fase 3** depende de Fase 2 verde (só benchmarka o que carrega).
- **Fase 4** depende de tudo (validação end-to-end).

---

## Phase 1: Build — estágio pgduckdb-builder + COPY artifact-only + preload

**Objective:** Buildar pg_duckdb v1.1.1 num estágio dedicado e embarcar seus artefatos + preload na imagem runtime PG17.

### T1.1 — Adicionar estágio `pgduckdb-builder` ao Dockerfile

#### Objective
Compilar pg_duckdb v1.1.1 (static-link) contra o PG17 exato da base pinada, num estágio multi-stage espelhando `scale-builder`.

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** adiciona `FROM ${BASE_IMAGE} AS pgduckdb-builder` que instala as build-deps C++ (`build-essential postgresql-server-dev-17 cmake ninja-build pkg-config git ca-certificates libc++-dev libc++abi-dev liblz4-dev libcurl4-openssl-dev libssl-dev`), clona pg_duckdb no tag `v1.1.1` + submódulo, e roda `DUCKDB_BUILD=ReleaseStatic make install -j"$(nproc)"`.
2. **Por que agora:** o COPY artifact-only (T1.2) e o preload (T1.3) dependem do artefato existir. O estágio dedicado (D1, D2) mantém o runtime artifact-only (`rules/architecture.md § 1`; `Dockerfile:3`) — o toolchain C++ pesado fica no builder, fora da imagem final. Espelha o padrão já validado de `scale-builder` (`Dockerfile:11`) e `theodb-rs-builder` (`Dockerfile:32`).

#### Evidence
`Dockerfile:11-28` (`scale-builder` — padrão de estágio builder com base pinada por digest e `git clone`+`checkout $REF`); `Dockerfile:6-8` (a base pinada compartilhada garante build contra o PG17 exato do runtime). Blueprint § "Caminho de build no Dockerfile" (lista exata de build-deps + `DUCKDB_BUILD=ReleaseStatic`, D2). pg_duckdb `https://github.com/duckdb/pg_duckdb/blob/main/docs/compilation.md` "Install Build Dependencies" (fonte web primária do repo upstream, NÃO path local).

#### Files to edit
```
Dockerfile — novo estágio `FROM ${BASE_IMAGE} AS pgduckdb-builder` (após o bloco theodb-rs-builder, ~linha 48), com ARG PGDUCKDB_REF=v1.1.1, apt build-deps, git clone+submodule, DUCKDB_BUILD=ReleaseStatic make install
```

#### Deep file dependency analysis
- **`Dockerfile` hoje:** dois estágios builder (`scale-builder`, `theodb-rs-builder`) que compilam extensões Rust e um runtime que COPY os artefatos (`Dockerfile:72-73,78-79`). Nenhum estágio C++.
- **Como muda:** adiciona um 3º estágio builder (C++, não Rust). Um `ARG PGDUCKDB_REF=v1.1.1` pinado. O `make install` grava `pg_duckdb*` em `/usr/lib/postgresql/17/lib/` e `/usr/share/postgresql/17/extension/` DENTRO do estágio builder (não no runtime ainda).
- **Downstream:** T1.2 (COPY --from=pgduckdb-builder) depende deste artefato.

#### Deep Dives
- **Invariante:** o build usa `${BASE_IMAGE}` (mesmo digest do runtime, `Dockerfile:8`) → o `.so` linka contra o PG17 exato shipado (reprodutível, sem moving target). Preservar esta garantia (célula "base pinada por digest" da Baseline).
- **Edge case:** submódulo — `git submodule update --init --recursive` é obrigatório (a árvore DuckDB é submódulo); sem ele o `make` falha. Static-link (`ReleaseStatic`) evita `libduckdb.so` avulso.
- **Edge case:** build falha por gotcha PG17/CMake → Failure scenario 1 (o log dita o pin a ajustar; honest-BLOCKED se irremediável).

#### Pseudo-code / Signatures
```dockerfile
# ---- Stage 1c: build pg_duckdb (C++/CMake/DuckDB — NO Rust) ----
FROM ${BASE_IMAGE} AS pgduckdb-builder
ARG PG_MAJOR=17
ARG PGDUCKDB_REF=v1.1.1
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential postgresql-server-dev-$PG_MAJOR cmake ninja-build pkg-config git ca-certificates \
      libc++-dev libc++abi-dev liblz4-dev libcurl4-openssl-dev libssl-dev && \
    rm -rf /var/lib/apt/lists/*
RUN git clone --branch $PGDUCKDB_REF https://github.com/duckdb/pg_duckdb /tmp/pg_duckdb && \
    cd /tmp/pg_duckdb && git submodule update --init --recursive
RUN cd /tmp/pg_duckdb && DUCKDB_BUILD=ReleaseStatic make install -j"$(nproc)"
```

#### Tasks
1. Localizar o ponto de inserção (após `theodb-rs-builder`, antes do runtime `FROM ${BASE_IMAGE}` em `Dockerfile:51`).
2. Adicionar o estágio `pgduckdb-builder` com `ARG PGDUCKDB_REF=v1.1.1`.
3. Instalar build-deps C++ (lista da Baseline/blueprint).
4. `git clone --branch v1.1.1` + `git submodule update --init --recursive`.
5. `DUCKDB_BUILD=ReleaseStatic make install -j"$(nproc)"`.

#### TDD
```
RED:     test_dockerfile_has_pgduckdb_builder_stage() — grep no Dockerfile por `AS pgduckdb-builder` + `DUCKDB_BUILD=ReleaseStatic` + `PGDUCKDB_REF` (falha antes da edição). Oracle: string presente.
RED:     test_pgduckdb_builder_produces_artifact() — docker build --target pgduckdb-builder + `docker run --rm <img> ls /usr/lib/postgresql/17/lib/pg_duckdb.so` retorna 0 (falha antes do build funcionar).
GREEN:   Implementar o estágio no Dockerfile até os dois testes passarem.
REFACTOR: Comentário explicando "C++/CMake, sem Rust — espelha scale-builder" (None extra esperado).
VERIFY:  docker build --target pgduckdb-builder -t theodb-pgduckdb-builder-test . && pytest benchmarks/tests/test_columnar_pgduckdb.py -k builder
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `docker build --target pgduckdb-builder -t theodb-pgduckdb-builder-test .` retorna exit 0
- [ ] `docker run --rm theodb-pgduckdb-builder-test test -f /usr/lib/postgresql/17/lib/pg_duckdb.so` retorna exit 0
- [ ] `sed -n '/AS pgduckdb-builder/,/^FROM /p' Dockerfile | grep -ciE 'rustup|cargo'` retorna `0` (nenhum toolchain Rust no bloco)
- [ ] Pass: size — `wc -l Dockerfile` ≤ 500
- [ ] Pass: lint — `hadolint Dockerfile` sem novos findings de nível `error` (se `hadolint` disponível; senão skip documentado)

#### DoD (Definition of Done)
- [ ] Tasks completas e validadas
- [ ] `test_dockerfile_has_pgduckdb_builder_stage` + `test_pgduckdb_builder_produces_artifact` verdes
- [ ] Build do estágio reprodutível (mesmo digest de base)
- [ ] CHANGELOG `[Unreleased] § Added` atualizado

---

### T1.2 — COPY artifact-only dos `pg_duckdb*` para o runtime

#### Objective
Copiar apenas os artefatos (`.so` + `.control` + `.sql`) do builder para o runtime, sem toolchain.

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** adiciona dois `COPY --from=pgduckdb-builder` no estágio runtime (o `.so` de `/usr/lib/postgresql/17/lib/pg_duckdb*` e os `.control`/`.sql` de `/usr/share/postgresql/17/extension/pg_duckdb*`), espelhando o COPY de pgvectorscale.
2. **Por que agora:** sem o COPY, o artefato buildado (T1.1) não chega ao runtime → `CREATE EXTENSION` (T1.3) não acha o `.so`. O padrão artifact-only mantém o runtime sem o toolchain C++ (invariante da Baseline; `Dockerfile:71-73`).

#### Evidence
`Dockerfile:71-73` (COPY artifact-only de pgvectorscale — `COPY --from=scale-builder /usr/lib/postgresql/$PG_MAJOR/lib/vectorscale* ...` + `.../extension/vectorscale*`); `Dockerfile:78-79` (mesmo padrão para theodb_rs). Blueprint § "Caminho de build" ("COPY --from=pgduckdb-builder os `pg_duckdb*` ... mesmo COPY artifact-only que pgvectorscale usa").

#### Files to edit
```
Dockerfile — dois COPY --from=pgduckdb-builder no runtime (após os COPY de theodb_rs, ~linha 79)
```

#### Deep file dependency analysis
- **`Dockerfile` runtime hoje:** COPY dos artefatos vectorscale (`:72-73`) e theodb_rs (`:78-79`).
- **Como muda:** adiciona 2 linhas COPY para `pg_duckdb*` (static-link → um `.so`, sem `libduckdb.so` separado — D2).
- **Downstream:** T1.3 (`CREATE EXTENSION`) depende do `.control`/`.sql` estarem no extension dir.

#### Deep Dives
- **Invariante:** runtime segue artifact-only (nenhum `build-essential`/cmake no runtime). Preservar (célula da Baseline).
- **Edge case (D2):** static-link garante que basta copiar `pg_duckdb*` — se por engano o build for dynamic, faltaria `libduckdb.so` e o `CREATE EXTENSION` falharia com "cannot open shared object" (coberto pela verificação de T2.1).

#### Pseudo-code / Signatures
```dockerfile
# pg_duckdb artifacts (M61) — static-linked .so + .control + .sql. Same artifact-only COPY as pgvectorscale
# (no C++ toolchain in runtime). DuckDB engine is statically linked (D2) → no separate libduckdb.so.
COPY --from=pgduckdb-builder /usr/lib/postgresql/$PG_MAJOR/lib/pg_duckdb* /usr/lib/postgresql/$PG_MAJOR/lib/
COPY --from=pgduckdb-builder /usr/share/postgresql/$PG_MAJOR/extension/pg_duckdb* /usr/share/postgresql/$PG_MAJOR/extension/
```

#### Tasks
1. Adicionar os 2 COPY após `Dockerfile:79` (bloco theodb_rs).
2. Comentar que é static-link (sem `libduckdb.so`).

#### TDD
```
RED:     test_runtime_has_pgduckdb_so() — build da imagem full + `docker run --rm <img> ls /usr/lib/postgresql/17/lib/pg_duckdb.so` → 0 (falha antes do COPY).
RED:     test_runtime_has_pgduckdb_control() — `ls /usr/share/postgresql/17/extension/pg_duckdb.control` → 0.
RED:     test_runtime_has_no_cpp_toolchain() — `docker run --rm <img> sh -c 'which cmake || echo absent'` → `absent` (artifact-only preservado).
GREEN:   Adicionar os 2 COPY até os testes passarem.
REFACTOR: None expected.
VERIFY:  docker build -t theodb-m61-test . && pytest benchmarks/tests/test_columnar_pgduckdb.py -k runtime
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `docker run --rm theodb-m61-test test -f /usr/lib/postgresql/17/lib/pg_duckdb.so` retorna exit 0
- [ ] `docker run --rm theodb-m61-test sh -c 'ls /usr/share/postgresql/17/extension/pg_duckdb.control /usr/share/postgresql/17/extension/pg_duckdb--*.sql'` retorna exit 0
- [ ] `docker run --rm theodb-m61-test sh -c 'which cmake || echo absent'` imprime `absent` (runtime artifact-only, sem toolchain C++)
- [ ] Pass: size — `wc -l Dockerfile` ≤ 500

#### DoD (Definition of Done)
- [ ] Tasks completas
- [ ] Os 3 testes de runtime verdes
- [ ] CHANGELOG atualizado

---

### T1.3 — Append idempotente de `shared_preload_libraries` + `CREATE EXTENSION` no init

#### Objective
Garantir preload de pg_duckdb no boot e criar a extensão na init de uma DB fresh.

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** append idempotente (grep-guarded) de `shared_preload_libraries='pg_duckdb'` ao `postgresql.conf.sample` da imagem no build, e adiciona `CREATE EXTENSION IF NOT EXISTS pg_duckdb;` ao init-script (junto ao `00-create-theodb.sql`, `Dockerfile:107`).
2. **Por que agora:** pg_duckdb exige preload (hook do executor — glossário) que precisa valer na PRIMEIRA init, antes do `CREATE EXTENSION`. `ALTER SYSTEM` só valeria pós-restart (D3, rejeitado). Sem preload, o `CREATE EXTENSION` falha (Failure scenario 2 / fail-closed de T2.3).

#### Evidence
`Dockerfile:107-116` (o init-script `00-create-theodb.sql` com `CREATE EXTENSION IF NOT EXISTS theodb CASCADE`); blueprint § "Gotcha crítico (build): pg_duckdb precisa de `shared_preload_libraries='pg_duckdb'` ... Se o TheoDB já setar `shared_preload_libraries` (checar), append, não overwrite" (D3).

#### Files to edit
```
Dockerfile — RUN grep-guarded que faz append de `shared_preload_libraries='pg_duckdb'` ao postgresql.conf.sample; adicionar CREATE EXTENSION pg_duckdb ao heredoc do init (~linha 107) OU um novo init-script 01-create-pgduckdb.sql
```

#### Deep file dependency analysis
- **`Dockerfile` hoje:** o init cria `theodb` + `theodb_rs` via heredoc (`:107-116`); nenhum `shared_preload_libraries` setado (verificar com grep no build — se já houver, append).
- **Como muda:** adiciona um RUN que faz append idempotente ao `.sample` (fonte do `postgresql.conf` gerado no `initdb`), e estende o init com `CREATE EXTENSION IF NOT EXISTS pg_duckdb;`.
- **Downstream:** T2.1 (extension smoke) depende do preload valer no boot.

#### Deep Dives
- **Invariante:** o init `CREATE EXTENSION theodb CASCADE` (`:107-116`) fica intocado; pg_duckdb é adicionado, não substitui.
- **Algoritmo (idempotência):** `grep -q "shared_preload_libraries.*pg_duckdb" $CONF || echo "shared_preload_libraries='pg_duckdb'" >> $CONF` — só faz append se ainda não presente (rebuild-safe). Se já houver um `shared_preload_libraries` com outro valor, o append correto seria mesclar (`'existing,pg_duckdb'`); como o TheoDB hoje **não** seta nenhum (verificar no build), o append simples basta — mas o guard deve detectar e falhar-alto se encontrar um valor conflitante inesperado (fail-fast, `rules/error-handling.md`).
- **Edge case:** append duplicado em rebuild → guard idempotente previne.
- **Edge case:** `shared_preload_libraries` pré-existente com outro valor → o RUN detecta e faz merge OU falha com mensagem clara (não overwrite silencioso — D3).

#### Pseudo-code / Signatures
```dockerfile
RUN set -eux; \
    CONF="/usr/share/postgresql/$PG_MAJOR/postgresql.conf.sample"; \
    if grep -qE "^\s*shared_preload_libraries" "$CONF"; then \
      grep -q "pg_duckdb" "$CONF" || sed -i "s/^\(\s*shared_preload_libraries\s*=\s*'\)/\1pg_duckdb,/" "$CONF"; \
    else \
      echo "shared_preload_libraries = 'pg_duckdb'" >> "$CONF"; \
    fi; \
    grep -q "pg_duckdb" "$CONF"   # assert append succeeded (fail-fast if not)
# init: append to the existing heredoc OR add a new init script
```

#### Tasks
1. Adicionar o RUN grep-guarded de append ao `.sample`.
2. Estender o init (`00-create-theodb.sql` heredoc ou novo `01-create-pgduckdb.sql`) com `CREATE EXTENSION IF NOT EXISTS pg_duckdb;`.
3. Assert final (`grep -q pg_duckdb "$CONF"`) para fail-fast se o append não pegou.

#### TDD
```
RED:     test_conf_sample_has_pgduckdb_preload() — `docker run --rm <img> grep pg_duckdb /usr/share/postgresql/17/postgresql.conf.sample` → 0 (falha antes do append).
RED:     test_append_is_idempotent() — rodar o snippet de append 2x sobre um .sample fixture; assert que `pg_duckdb` aparece exatamente 1x (não duplica).
RED:     test_init_creates_pgduckdb() — subir o container (fresh init) e `SELECT extname FROM pg_extension WHERE extname='pg_duckdb'` retorna 1 linha (falha antes do init-script).
GREEN:   Implementar o append + init até os testes passarem.
REFACTOR: Extrair o append para comentário explicativo (D3). None extra.
VERIFY:  docker build -t theodb-m61-test . && pytest benchmarks/tests/test_columnar_pgduckdb.py -k "preload or init"
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `docker run --rm theodb-m61-test grep -q pg_duckdb /usr/share/postgresql/17/postgresql.conf.sample` retorna exit 0
- [ ] `test_append_is_idempotent` verde: aplicar o snippet 2× sobre um `.sample` fixture → `grep -c pg_duckdb` == 1 (não duplica)
- [ ] Fresh init: `docker exec <c> psql -U postgres -tAc "SELECT count(*) FROM pg_extension WHERE extname='pg_duckdb'"` retorna `1`
- [ ] `test_preexisting_preload_not_overwritten` verde: com um `.sample` que já tem `shared_preload_libraries='foo'`, o resultado contém AMBOS `foo` e `pg_duckdb` (merge, não overwrite)

#### DoD (Definition of Done)
- [ ] Tasks completas
- [ ] 3 testes verdes
- [ ] CHANGELOG atualizado

---

## Phase 2: Smoke/gate — extension + analytic + fail-closed + licença/CVE

**Objective:** Provar em CI que o embed carrega, planeja sob DuckDB, é correto cross-engine, falha-fechado sem preload, e passa o gate de licença/CVE.

### T2.1 — Smoke: `CREATE EXTENSION pg_duckdb` numa fresh DB

#### Objective
Provar que o `.so` linka contra o PG17 exato da imagem (extensão carrega).

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** teste de integração que sobe o container M61, conecta, e assere que `CREATE EXTENSION IF NOT EXISTS pg_duckdb` (com preload já setado) retorna sem erro e `pg_extension` lista `pg_duckdb`.
2. **Por que agora:** é o smoke mais barato que prova o embed funcional end-to-end (mesma garantia que o multi-stage de scale-builder dá, `Dockerfile:7`). Bloqueia as fases seguintes se o `.so` não linkar.

#### Evidence
Blueprint § "Coverage Corner 1 — Integration Tests" item 1 ("Extension smoke: `CREATE EXTENSION pg_duckdb;` retorna sem erro numa fresh DB init"); `benchmarks/theodb_bench/db.py` (`VectorDB` — o helper de conexão a reusar, Rule 9); `benchmarks/run_m30_columnar_scale.py:29` (import `from theodb_bench.db import VectorDB`).

#### Files to edit
```
benchmarks/tests/test_columnar_pgduckdb.py (NEW) — fixture que sobe o container M61 + test_create_extension_pgduckdb_succeeds()
```

#### Deep file dependency analysis
- **Arquivo novo:** `test_columnar_pgduckdb.py` reusa `VectorDB` (conexão) e o padrão de fixture de container dos testes existentes em `benchmarks/tests/`.
- **Downstream:** T2.2 e T2.3 compartilham o mesmo fixture de container.

#### Deep Dives
- **Invariante:** o teste roda contra a imagem M61 buildada (não o substrato mooncake) — é a superfície pg_duckdb.
- **Edge case:** container não sobe (preload quebra boot) → o fixture falha alto com o log do postmaster (fail-fast).

#### Pseudo-code / Signatures
```python
def test_create_extension_pgduckdb_succeeds(pgduckdb_container):
    db = VectorDB(dsn(pgduckdb_container.port)).connect()
    db._cursor().execute("CREATE EXTENSION IF NOT EXISTS pg_duckdb")
    rows, _ = db.timed_query("SELECT extname FROM pg_extension WHERE extname='pg_duckdb'")
    assert rows == [("pg_duckdb",)]
```

#### Tasks
1. Criar o fixture `pgduckdb_container` (sobe a imagem M61, espera healthcheck).
2. Escrever `test_create_extension_pgduckdb_succeeds`.

#### TDD
```
RED:     test_create_extension_pgduckdb_succeeds() — assere `pg_extension` lista pg_duckdb (falha se o embed não carregar).
GREEN:   O embed da Fase 1 faz passar; o teste é o oracle.
REFACTOR: Extrair `dsn()` helper se não existir. None extra.
VERIFY:  pytest benchmarks/tests/test_columnar_pgduckdb.py -k create_extension
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `docker exec <c> psql -U postgres -c "CREATE EXTENSION IF NOT EXISTS pg_duckdb"` retorna exit 0 (sem `ERROR:`)
- [ ] `docker exec <c> psql -U postgres -tAc "SELECT extname FROM pg_extension WHERE extname='pg_duckdb'"` retorna exatamente `pg_duckdb` (1 linha)
- [ ] `docker exec <c> psql -U postgres -tAc "SELECT * FROM duckdb.query('SELECT 1 AS x')"` retorna `1` (o executor DuckDB responde, não só a extensão registra)
- [ ] `test_create_extension_pgduckdb_succeeds` verde em `pytest benchmarks/tests/test_columnar_pgduckdb.py -k create_extension`

#### DoD (Definition of Done)
- [ ] Teste verde
- [ ] CHANGELOG atualizado

---

### T2.2 — Smoke analítico (plano DuckDB) + correctness cross-engine

#### Objective
Provar que uma query analítica planeja sob o executor DuckDB (não Seq Scan) e é correta vs o row engine.

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** popula uma tabela heap (`seed_metrics`, Rule 9), roda `SET duckdb.force_execution=true; EXPLAIN (ANALYZE) SELECT category, count(*), avg(amount) FROM t GROUP BY category` e assere que o plano é DuckDB-executed; depois compara `count` exato + `avg` dentro de `1e-3` vs a mesma query com `force_execution=false`.
2. **Por que agora:** é o win do M30 re-medido na superfície pg_duckdb — o oracle é o **plano** (DuckDB), não só o resultado. Prova que o embed entrega vetorização columnar, não só carrega. A tolerância `1e-3` (não byte-idêntico) é honesta: PG vs DuckDB somam no último decimal diferente (`docs/benchmarks/m30-columnar-scale.md:15`).

#### Evidence
Blueprint § "Coverage Corner 1" itens 2-3 (analytic smoke: oracle é o plano DuckDB; correctness cross-engine dentro de 1e-3, NÃO byte-idêntico); `benchmarks/theodb_bench/columnar.py:13` (`_AGG` reusado); `docs/benchmarks/m30-columnar-scale.md:15` (avg cross-engine dentro de 1e-3).

#### Files to edit
```
benchmarks/theodb_bench/columnar.py — helper novo `run_pgduckdb_force_execution(db, table, n)` (seed heap + force_execution + plano + correctness), sem tocar o caminho mooncake
benchmarks/tests/test_columnar_pgduckdb.py — test_analytic_query_plans_under_duckdb() + test_cross_engine_correctness_within_epsilon()
```

#### Deep file dependency analysis
- **`columnar.py` hoje:** helpers mooncake (`create_columnstore_mirror`, `_wait_mirror_synced`) + `seed_metrics`/`_AGG` genéricos.
- **Como muda:** adiciona `run_pgduckdb_force_execution` que reusa `seed_metrics`/`_AGG` mas NÃO cria mirror (heap-scan direto via `force_execution`). Não remove nada mooncake (M62 usa).
- **Downstream:** T3.1 (driver de benchmark) reusa este helper.

#### Deep Dives
- **Invariante:** o plano da query com `force_execution=true` contém "DuckDB" (oracle); sem ele, seria `Seq Scan` (falha o smoke — prova que a vetorização de fato dispara).
- **Edge case (negative/correctness):** `avg` cross-engine difere no último decimal → tolerância `1e-3`, NÃO `assert ==` (evita flaky honesto).
- **Edge case:** heap vazio (n=0) → `count=0`, `avg=NULL` em ambos engines (correctness trivialmente igual).

#### Pseudo-code / Signatures
```python
def run_pgduckdb_force_execution(db, table="metrics", n=1_000_000) -> dict:
    seed_metrics(db, table, n)                       # reuse — Rule 9
    with db._cursor() as cur: cur.execute("SET duckdb.force_execution=true")
    col_plan = db.explain_plan(_AGG.format(t=table)) # oracle: "DuckDB" in plan
    col_rows, col_ms = db.timed_query(_AGG.format(t=table))
    with db._cursor() as cur: cur.execute("SET duckdb.force_execution=false")
    row_rows, row_ms = db.timed_query(_AGG.format(t=table))
    return {"plan_duckdb": "DuckDB" in col_plan, "match": _results_match(row_rows, col_rows),
            "col_ms": col_ms, "row_ms": row_ms}
# Example: n=1_000_000 → {"plan_duckdb": True, "match": True, ...}
```

#### Tasks
1. Adicionar `run_pgduckdb_force_execution` a `columnar.py` (reusa `seed_metrics`/`_AGG`/`_results_match`).
2. `test_analytic_query_plans_under_duckdb` (oracle = plano DuckDB).
3. `test_cross_engine_correctness_within_epsilon` (count exato + avg 1e-3).

#### TDD
```
RED:     test_analytic_query_plans_under_duckdb() — assere "DuckDB" no plano com force_execution=true (falha se planeja Seq Scan).
RED:     test_cross_engine_correctness_within_epsilon() — count exato + avg dentro de 1e-3 vs row engine.
GREEN:   Implementar `run_pgduckdb_force_execution` até passar.
REFACTOR: Reusar `_results_match` existente (não reimplementar tolerância — Rule 9).
VERIFY:  pytest benchmarks/tests/test_columnar_pgduckdb.py -k "duckdb or correctness"
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `test_analytic_query_plans_under_duckdb` verde: `"DuckDB" in db.explain_plan(_AGG)` com `force_execution=true` é True (oracle = plano, não Seq Scan)
- [ ] `test_cross_engine_correctness_within_epsilon` verde: `count` idêntico entre engines E `abs(avg_col - avg_row) < 1e-3` por grupo (não `assert ==`)
- [ ] `pytest benchmarks/tests/ -k "m30 or columnar" ` verde (callers M6/M30 de `columnar.py` não regridem após o helper novo)

#### DoD (Definition of Done)
- [ ] 2 testes verdes
- [ ] Callers existentes de `columnar.py` continuam verdes
- [ ] CHANGELOG atualizado

---

### T2.3 — Fail-closed: sem `shared_preload_libraries`, `CREATE EXTENSION` falha com erro claro

#### Objective
Provar o comportamento fail-closed (negative case) quando o preload está ausente.

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** sobe um Postgres SEM `shared_preload_libraries='pg_duckdb'` e assere que `CREATE EXTENSION pg_duckdb` falha com uma mensagem **tipada/clara** (não um crash genérico), assertando a substring da mensagem.
2. **Por que agora:** é o negative case que prova error-handling (`rules/testing.md § 4.1`, `rules/error-handling.md`): um append errado de config (Failure scenario 3) deve falhar-alto e claro, não silenciosamente. Fecha o risco operacional do `shared_preload_libraries` (Drawback).

#### Evidence
Blueprint § "Coverage Corner 1" item 4 ("Fail-closed: sem `shared_preload_libraries`, `CREATE EXTENSION pg_duckdb` deve falhar com erro claro (typed) — assertar a mensagem, não só 'throws'"); `rules/testing.md § 4.1` (negative case assere a mensagem específica).

#### Files to edit
```
benchmarks/tests/test_columnar_pgduckdb.py — test_create_extension_fails_closed_without_preload()
```

#### Deep file dependency analysis
- **Arquivo:** o mesmo test file; adiciona um fixture que sobe um Postgres base (ou a imagem M61 com preload removido via `-c shared_preload_libraries=''`).
- **Downstream:** nenhum — é um gate terminal.

#### Deep Dives
- **Invariante:** sem preload, pg_duckdb NÃO carrega o hook do executor → `CREATE EXTENSION` deve erro-tipar (não segfault).
- **Edge case:** a mensagem exata pode variar por versão do pg_duckdb — assertar uma substring estável (ex.: "shared_preload_libraries" ou "pg_duckdb"), não a string inteira (evita flaky).

#### Pseudo-code / Signatures
```python
def test_create_extension_fails_closed_without_preload(pg_no_preload):
    db = VectorDB(dsn(pg_no_preload.port)).connect()
    with pytest.raises(Exception) as exc:
        db._cursor().execute("CREATE EXTENSION pg_duckdb")
    assert "shared_preload_libraries" in str(exc.value).lower() or "pg_duckdb" in str(exc.value).lower()
```

#### Tasks
1. Fixture `pg_no_preload` (Postgres M61 com `shared_preload_libraries` vazio, override no comando).
2. `test_create_extension_fails_closed_without_preload` assertando a substring da mensagem.

#### TDD
```
RED:     test_create_extension_fails_closed_without_preload() — assere que sem preload o CREATE EXTENSION lança com mensagem contendo "shared_preload_libraries"/"pg_duckdb" (falha se o embed carregasse sem preload OU se o erro fosse genérico).
GREEN:   O comportamento nativo do pg_duckdb faz passar; o teste é o oracle do fail-closed.
REFACTOR: None expected.
VERIFY:  pytest benchmarks/tests/test_columnar_pgduckdb.py -k fails_closed
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `test_create_extension_fails_closed_without_preload` verde: com `-c shared_preload_libraries=''`, `CREATE EXTENSION pg_duckdb` lança (`pytest.raises`)
- [ ] `str(exc.value).lower()` contém `"shared_preload_libraries"` OU `"pg_duckdb"` (substring estável assertada, não crash genérico/segfault)
- [ ] O teste assere a mensagem via `assert <substring> in str(exc.value)`, não apenas `pytest.raises` vazio (`rules/testing.md § 4.1` negative case)

#### DoD (Definition of Done)
- [ ] Teste verde
- [ ] CHANGELOG atualizado

---

### T2.4 — Gate de licença (D1 — MIT) + `/deps-audit` das transitivas

#### Objective
Provar que pg_duckdb + a árvore DuckDB são permissivos (D1) e sem CVE crítico/alto.

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** documenta no ADR-0020 as licenças (pg_duckdb MIT [F1a], DuckDB core MIT [F3c]), confirma que community extensions ficam OFF por default, e roda `/deps-audit` sobre a árvore (pg_duckdb + libs C transitivas libcurl/openssl/lz4).
2. **Por que agora:** licença é gate de release (D1, PRD §11) e o DoD do M61 exige o gate de licença + CVE ANTES de qualquer claim de adoção. As rotas AGPL (Citus/Hydra) já estão barradas [F3a,F3b]; a rota DuckDB é a única permissiva.

#### Evidence
Blueprint § "Coverage Corner 2 — Dependencies" (tabela de licenças: pg_duckdb MIT, DuckDB MIT, Citus/Hydra AGPL barrados; "manter community extensions OFF por default"); ROADMAP `M61 DoD` (`ROADMAP.md:994` — "Gate de licença (D1 — MIT ✓) + `/deps-audit` (CVE)"); `docs/adr/0013-v1-legacy-columnar-bm25-scope.md:29-32` (D1 barra AGPL).

#### Files to edit
```
docs/adr/0020-m61-embed-pgduckdb.md (NEW) — ADR com a tabela de licenças + decisão community-extensions OFF
CHANGELOG.md — entrada [Unreleased] § Added do embed pg_duckdb
```

#### Deep file dependency analysis
- **ADR-0020 (novo):** registra D1/D2/D3 deste plano + a tabela de licenças do blueprint; reabre a nota de adoção gated do ADR-0013.
- **Downstream:** o gate de release (`cycle-release`) lê o ADR + o resultado do deps-audit.

#### Deep Dives
- **Invariante:** só Apache/MIT/BSD/PostgreSQL entram (D1). pg_duckdb=MIT, DuckDB=MIT → passa. Community extensions (não-auditadas) ficam OFF (`duckdb.allow_community_extensions` default false).
- **Edge case:** uma lib C transitiva (libcurl/openssl/lz4) com CVE alto → é CVE da imagem base, não do extension per se, mas entra no scan; se crítico, gate bloqueia (deps-audit golden rule).

#### Pseudo-code / Signatures
```
# ADR-0020 tabela (do blueprint Corner 2):
# pg_duckdb v1.1.1 → MIT [F1a] → D1-clean
# DuckDB engine    → MIT [F3c] → D1-clean
# community exts   → OFF por default (allow_community_extensions=false)
# Citus/Hydra columnar → AGPL [F3a,F3b] → BARRADOS (não embarcados)
```

#### Tasks
1. Escrever `docs/adr/0020-m61-embed-pgduckdb.md` (D1/D2/D3 + tabela de licenças).
2. Rodar `/deps-audit` sobre pg_duckdb + transitivas; registrar o resultado.
3. Confirmar `duckdb.allow_community_extensions=false` no embed.

#### TDD
```
RED:     test_community_extensions_off_by_default() — `SHOW duckdb.allow_community_extensions` → 'off'/'false' na imagem M61 (falha se vier ligado).
GREEN:   Garantir o default OFF (config/GUC) até passar.
REFACTOR: None expected.
VERIFY:  pytest benchmarks/tests/test_columnar_pgduckdb.py -k community_extensions && /deps-audit m61-columnar-htap-adoption
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `grep -c "MIT" docs/adr/0020-m61-embed-pgduckdb.md` ≥ 2 (linhas de pg_duckdb e DuckDB) E o arquivo contém as strings `[F1a]` e `[F3c]` (`grep -q '\[F1a\]' && grep -q '\[F3c\]'`)
- [ ] `docker exec <c> psql -U postgres -tAc "SHOW duckdb.allow_community_extensions"` retorna `off` (não `on`)
- [ ] `/deps-audit m61-columnar-htap-adoption` emite verdict ∈ {PASS, PASS_WITH_CAVEATS} (nenhum `cve_critical_*`/`cve_high_*` não-allowlisted)
- [ ] `grep -riE "AGPL" docs/adr/0020-m61-embed-pgduckdb.md` só aparece em linhas de dependências REJEITADAS (Citus/Hydra "BARRADO"), nunca embarcadas

#### DoD (Definition of Done)
- [ ] ADR-0020 escrito com alternativas
- [ ] deps-audit verde (ou allowlist + ADR para achados residuais)
- [ ] CHANGELOG atualizado

---

## Phase 3: Benchmark de adoção — pg_duckdb `force_execution` vs heap (mesma box)

**Objective:** Medir honestamente a vantagem analítica na superfície pg_duckdb embarcada e emitir o artefato reproduzível.

### T3.1 — Driver `run_m61_columnar_adoption.py` (espelha o M30, superfície pg_duckdb)

#### Objective
Medir columnstore (pg_duckdb `force_execution`) vs row-store (heap) no mesmo dataset/box, ≥3 runs mean±std.

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** cria `benchmarks/run_m61_columnar_adoption.py` espelhando `run_m30_columnar_scale.py`, mas usando `run_pgduckdb_force_execution` (T2.2) em vez do mirror mooncake — mede a query `_AGG` com `force_execution=true` vs `false`, mean±std sobre ≥3 runs, na imagem M61 embarcada (não o substrato mooncake).
2. **Por que agora:** o DoD M61 exige o benchmark de adoção reproduzível na MESMA box (`ROADMAP.md:995`). Reusa o padrão M30 (Rule 9 — mesmo `VectorDB`, `seed_metrics`, `statistics.mean/pstdev`, flag `--write-doc`). Re-mede honestamente a superfície pg_duckdb (o ~14× do M30 é do mooncake — Regra 5).

#### Evidence
`benchmarks/run_m30_columnar_scale.py:50-98` (o driver a espelhar: `_measure_scale` mean±std, `effect_gt_variance`, `crossover_n`, `--write-doc`); blueprint § "Coverage Corner 3 — Tools" ("reusar o harness do M30 ... adaptar para a superfície pg_duckdb `force_execution` em vez do mooncake `create_table`"); `ROADMAP.md:995` (artefato `m61-columnar-adoption.{md,json}`).

#### Files to edit
```
benchmarks/run_m61_columnar_adoption.py (NEW) — driver espelhando run_m30_columnar_scale.py, superfície pg_duckdb force_execution
```

#### Deep file dependency analysis
- **`run_m30_columnar_scale.py`:** driver mooncake (mirror). Reusa `theodb_bench.columnar` + `VectorDB`.
- **Como muda (arquivo novo):** `run_m61_columnar_adoption.py` reusa `run_pgduckdb_force_execution` (T2.2) + `statistics` + `--write-doc`; roda contra a imagem M61 (`_IMAGE`/porta), não mooncake.
- **Downstream:** T3.2 (artefato .md/.json) é a saída do `--write-doc`.

#### Deep Dives
- **Invariante:** ≥3 runs, mean±std, warm cache, MESMA box, controle row-store (`force_execution=false`) — mesma disciplina estatística do M30 (`run_m30_columnar_scale.py:66-77` `effect_gt_variance`).
- **Edge case (honest-negative):** se pg_duckdb NÃO vencer o heap no nosso dataset (`speedup ≤ 1` ou `effect ≤ variance`), o driver reporta o negativo honesto (`crossover_n = None`), exatamente como o M30 já faz (`run_m30_columnar_scale.py:121-122`). Resultado válido (Regra 5).
- **Edge case:** dataset pequeno onde o overhead DuckDB domina → speedup < 1 (reportado, não escondido).

#### Pseudo-code / Signatures
```python
def run(port, scales=(100_000, 1_000_000, 5_000_000), runs=3):
    db = VectorDB(dsn(port)).connect()
    points = [_measure_scale_pgduckdb(db, n, runs) for n in scales]  # reuse mean±std pattern from M30
    crossover = next((p["n"] for p in points if p["speedup"] and p["speedup"] > 1.0 and p["effect_gt_variance"]), None)
    return {"surface": "pg_duckdb force_execution (heap-scan, M61 embed)", "points": points, "crossover_n": crossover, ...}
# honest-negative: crossover_n=None is a VALID result (Regra 5)
```

#### Tasks
1. Criar `run_m61_columnar_adoption.py` espelhando o M30 (mean±std, `effect_gt_variance`, `--write-doc`).
2. Usar `run_pgduckdb_force_execution` (T2.2) para o par columnstore/row.
3. Rodar contra a imagem M61 (não mooncake).

#### TDD
```
RED:     test_m61_driver_reports_mean_std_and_effect() — chamar `run()` contra um container M61 com n pequeno; assere que cada ponto tem `row_ms_mean`,`row_ms_std`,`columnar_ms_mean`,`speedup`,`effect_gt_variance` (falha antes do driver existir).
RED:     test_m61_driver_honest_negative() — com um dataset onde columnar não vence, `crossover_n` é None (não fabrica win).
GREEN:   Implementar o driver até passar.
REFACTOR: Reusar `_measure_scale` shape do M30 sem copiar-colar a lógica estatística (extrair se compartilhável).
VERIFY:  python3 benchmarks/run_m61_columnar_adoption.py --port <p> --scales 100000 --runs 3 && pytest benchmarks/tests/test_columnar_pgduckdb.py -k m61_driver
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `test_m61_driver_reports_mean_std_and_effect` verde: cada `point` do `run()` tem as chaves `row_ms_mean`,`row_ms_std`,`columnar_ms_mean`,`speedup`,`effect_gt_variance` (assert por chave); `runs >= 3`
- [ ] `effect_gt_variance == abs(row_mean - col_mean) > (row_std + col_std)` computado por ponto (mesma fórmula de `run_m30_columnar_scale.py:74`)
- [ ] `test_m61_driver_honest_negative` verde: dataset sem win → `res["crossover_n"] is None` (não lança, não fabrica)
- [ ] `grep -c "import.*theodb_bench" benchmarks/run_m61_columnar_adoption.py` ≥ 1 (reusa `VectorDB`/`seed_metrics`/`_AGG`, Rule 9 — não reimplementa)

#### DoD (Definition of Done)
- [ ] 2 testes verdes
- [ ] Driver roda contra a imagem M61
- [ ] CHANGELOG atualizado

---

### T3.2 — Emitir `docs/benchmarks/m61-columnar-adoption.{md,json}`

#### Objective
Persistir o artefato de adoção (dados brutos + relatório honesto) como gate do DoD.

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** o `--write-doc` do driver (T3.1) grava os artefatos NEW `docs/benchmarks/m61-columnar-adoption.json` (dados brutos ≥3 runs) + `docs/benchmarks/m61-columnar-adoption.md` (NEW — relatório com a superfície pg_duckdb explícita, caveat de que o número do M30 é do mooncake, e honest-negative se aplicável). Ambos criados por esta task.
2. **Por que agora:** performance é claim, não opinião (Regra 5) — nenhuma afirmação de vantagem analítica sem este artefato (`rules/public-copy.md § 4`). É o segundo termo da métrica do Goal.

#### Evidence
`benchmarks/run_m30_columnar_scale.py:101-156` (`_render_md` + `--write-doc` que grava `.md`+`.json`); `docs/benchmarks/m30-columnar-scale.md` (o formato/tom honesto a espelhar); `ROADMAP.md:995` (`docs/benchmarks/m61-columnar-adoption.{md,json}`); `rules/public-copy.md § 4`.

#### Files to edit
```
benchmarks/run_m61_columnar_adoption.py — função `_render_md` (superfície pg_duckdb + caveats honestos)
docs/benchmarks/m61-columnar-adoption.md (NEW) — saída do --write-doc
docs/benchmarks/m61-columnar-adoption.json (NEW) — dados brutos
```

#### Deep file dependency analysis
- **`_render_md`:** espelha o M30 mas declara a superfície pg_duckdb `force_execution` (heap-scan), o caveat de que o ~14× do M30 é do substrato mooncake (não transferível 1:1), e o honest-negative se `crossover_n=None`.
- **Downstream:** o gate de release + a métrica do Goal leem o `.json`.

#### Deep Dives
- **Invariante:** o `.md` diz explicitamente "superfície pg_duckdb `force_execution`", NÃO reivindica o número mooncake como se fosse pg_duckdb (Regra 5 / honestidade).
- **Edge case (honest-negative):** se columnar não vencer, o verdict do `.md` é "No effect-exceeds-variance win (honest negative)" — mesmo padrão do M30 (`run_m30_columnar_scale.py:121-122`).

#### Pseudo-code / Signatures
```
# m61-columnar-adoption.md header:
# **Surface:** pg_duckdb force_execution (heap-scan) on the EMBEDDED PG17 TheoDB image (M61), NOT the mooncake
# mirror substrate. The ~14× @5M of M30 is the mooncake DuckDBScan surface — re-measured here; UNBENCHMARKED
# until this table. Runs: ≥3 mean±std, same box, control = force_execution=false.
```

#### Tasks
1. Implementar `_render_md` (superfície explícita + caveats honestos + honest-negative).
2. Rodar o driver com `--write-doc` → gerar os 2 artefatos.
3. Revisar o `.md` para não afirmar o número mooncake como pg_duckdb.

#### TDD
```
RED:     test_render_md_declares_pgduckdb_surface() — o markdown gerado contém "pg_duckdb force_execution" + o caveat do M30/mooncake (falha se o texto reivindicar o número mooncake).
RED:     test_write_doc_emits_both_artifacts() — após `--write-doc`, ambos `m61-columnar-adoption.md` e `.json` existem e o .json tem `points` com ≥1 entrada.
GREEN:   Implementar `_render_md` + `--write-doc` até passar.
REFACTOR: Reusar o esqueleto do M30 `_render_md` (não duplicar o loop de tabela).
VERIFY:  python3 benchmarks/run_m61_columnar_adoption.py --port <p> --write-doc && test -f docs/benchmarks/m61-columnar-adoption.json && pytest -k "render_md or write_doc"
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `test -f docs/benchmarks/m61-columnar-adoption.json` exit 0 E `python3 -c "import json;assert len(json.load(open('docs/benchmarks/m61-columnar-adoption.json'))['points'])>=1"` passa (métrica do Goal)
- [ ] `grep -q "pg_duckdb force_execution" docs/benchmarks/m61-columnar-adoption.md` exit 0 (superfície declarada explicitamente)
- [ ] `test_render_md_declares_pgduckdb_surface` verde: o `.md` contém o caveat "M30 ... mooncake" E NÃO reivindica o ~14× como número pg_duckdb (Regra 5)
- [ ] honest-negative: quando `crossover_n is None`, `grep -q "honest negative" docs/benchmarks/m61-columnar-adoption.md` exit 0

#### DoD (Definition of Done)
- [ ] Ambos artefatos gerados
- [ ] 2 testes verdes
- [ ] CHANGELOG atualizado

---

## Phase 4: Integration Validation — coexistência das extensões + não-regressão

**Objective:** Provar que a imagem builda e pgvector + vectorscale + theodb_rs + pg_duckdb coexistem numa init, sem regredir a suíte existente.

### T4.1 — Coexistência das 4 extensões + delta de peso da imagem

#### Objective
Validar que numa fresh init as 4 extensões carregam e medir o delta de tamanho da imagem.

#### Why this step (action + reasoning — ReAct discipline)

1. **O que faz:** sobe a imagem M61 fresh e assere que `pg_extension` lista `vector`, `vectorscale`, `theodb`/`theodb_rs` E `pg_duckdb` (coexistência); mede `docker image inspect` size vs a imagem pré-M61 (delta de peso — Drawback R1).
2. **Por que agora:** o DoD M61 exige que todas as extensões coexistam (`ROADMAP.md` M61 escopo Fase 4). O preload de pg_duckdb (T1.3) não pode quebrar o `CREATE EXTENSION theodb CASCADE` do init (`Dockerfile:107-116`). O delta de peso alimenta a decisão de tiering (Unresolved Q1).

#### Evidence
`Dockerfile:107-116` (init cria theodb+theodb_rs via CASCADE — não pode regredir); `Dockerfile:4` (runtime ~445 MB baseline); blueprint § "Riscos honestos" R1 (peso da imagem — medir delta real no gate).

#### Files to edit
```
benchmarks/tests/test_columnar_pgduckdb.py — test_all_four_extensions_coexist() + test_image_size_delta_recorded()
```

#### Deep file dependency analysis
- **Teste:** reusa o fixture `pgduckdb_container` (T2.1); adiciona a asserção de coexistência + a medição de size.
- **Downstream:** o gate de release lê o delta de peso.

#### Deep Dives
- **Invariante:** as 4 extensões carregam na MESMA DB fresh; o preload de pg_duckdb não quebra o boot nem o CASCADE do theodb.
- **Edge case:** se o preload de pg_duckdb conflitar com o boot → o container não sobe (fixture falha alto — Failure scenario 3).

#### Pseudo-code / Signatures
```python
def test_all_four_extensions_coexist(pgduckdb_container):
    db = VectorDB(dsn(pgduckdb_container.port)).connect()
    rows, _ = db.timed_query("SELECT extname FROM pg_extension ORDER BY extname")
    names = {r[0] for r in rows}
    assert {"vector", "vectorscale", "theodb", "pg_duckdb"} <= names
```

#### Tasks
1. `test_all_four_extensions_coexist` (as 4 na mesma init).
2. `test_image_size_delta_recorded` (mede + registra o delta vs baseline ~445 MB).

#### TDD
```
RED:     test_all_four_extensions_coexist() — assere vector+vectorscale+theodb+pg_duckdb no pg_extension (falha se o preload quebrar o CASCADE do theodb).
RED:     test_image_size_delta_recorded() — o size da imagem M61 é medido e > baseline (registra o delta para Q1).
GREEN:   A imagem da Fase 1 faz coexistir; os testes são o oracle.
REFACTOR: None expected.
VERIFY:  docker build -t theodb-m61-test . && pytest benchmarks/tests/test_columnar_pgduckdb.py -k "coexist or image_size"
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `docker exec <c> psql -U postgres -tAc "SELECT extname FROM pg_extension WHERE extname IN ('vector','vectorscale','theodb','pg_duckdb')" | wc -l` retorna `4` (as 4 coexistem numa fresh init)
- [ ] `docker exec <c> psql -U postgres -tAc "SELECT 1 FROM pg_extension WHERE extname='theodb'"` retorna `1` (o `CREATE EXTENSION theodb CASCADE` do init `Dockerfile:107-116` não regride)
- [ ] `test_image_size_delta_recorded` verde: `docker image inspect -f '{{.Size}}' theodb-m61` é medido e o delta vs baseline ~445 MB é escrito no log do teste (alimenta Q1)

#### DoD (Definition of Done)
- [ ] 2 testes verdes
- [ ] Suíte de benchmark existente (`benchmarks/tests/`) não regride
- [ ] CHANGELOG atualizado

---

## Coverage Matrix

| # | Gap / Requirement (do DoD M61 / ROADMAP.md:993-996) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Fase 1 — estágio `pgduckdb-builder` no Dockerfile (clone+submódulo+`ReleaseStatic make install`, sem Rust) | T1.1 | Estágio C++/CMake espelhando scale-builder; pin `PGDUCKDB_REF=v1.1.1` |
| 2 | Fase 1 — COPY artifact-only dos `pg_duckdb*` para o runtime | T1.2 | 2 COPY --from=pgduckdb-builder (static-link → um `.so`) |
| 3 | Fase 1 — append `shared_preload_libraries='pg_duckdb'` na config | T1.3 | Append idempotente grep-guarded ao `.sample` (D3) + CREATE EXTENSION no init |
| 4 | Fase 2 — `CREATE EXTENSION pg_duckdb` + smoke end-to-end verde | T2.1, T2.2 | Extension smoke + analytic (oracle=plano DuckDB) + correctness 1e-3 |
| 5 | Fase 2 — gate de licença (D1 — MIT ✓, DuckDB core MIT) | T2.4 | ADR-0020 tabela de licenças [F1a,F3c]; AGPL barrado |
| 6 | Fase 2 — `/deps-audit` (CVE das transitivas) | T2.4 | deps-audit sobre pg_duckdb + libs C; community extensions OFF |
| 7 | Fase 2 — fail-closed sem preload (implícito no smoke/gate) | T2.3 | Negative case: erro tipado com substring assertada |
| 8 | Fase 3 — benchmark columnstore vs row-store mesma box/dataset | T3.1 | Driver `run_m61_columnar_adoption.py` mean±std ≥3 runs |
| 9 | Fase 3 — `docs/benchmarks/m61-columnar-adoption.{md,json}` | T3.2 | `--write-doc` emite ambos; superfície pg_duckdb explícita |
| 10 | Fase 3 — honestidade (número M30 é mooncake, não pg_duckdb; honest-negative válido) | T3.1, T3.2 | Caveat no `.md`; `crossover_n=None` suportado |
| 11 | Fase 4 — imagem builda; pgvector+vectorscale+theodb+pg_duckdb coexistem | T4.1 | test_all_four_extensions_coexist |
| 12 | Fase 4 — suíte existente não regride | T4.1 + Integration Validation | Suíte `benchmarks/tests/` verde |
| 13 | Honestidade (Regra 9): columnar é exceção permissiva adotada, não own-code | T2.4 (ADR-0020) | ADR declara adoção permissiva explícita |

**Coverage: 13/13 gaps covered (100%)**

## Global Definition of Done

- [ ] Todas as fases completas
- [ ] Todos os testes verdes — `pytest benchmarks/tests/test_columnar_pgduckdb.py` + `docker build .`
- [ ] Zero erros de build — `docker build -t theodb-m61 .` conclui
- [ ] Zero lint — `ruff check benchmarks/` + `hadolint Dockerfile` sem novos erros
- [ ] File-size budget respeitado (`Dockerfile` ≤ 500 linhas; novos `.py` ≤ 500)
- [ ] CHANGELOG.md atualizado sob `[Unreleased] § Added` (Regra 6)
- [ ] Backward compat: pgvector+vectorscale+theodb_rs continuam carregando; `CREATE EXTENSION theodb CASCADE` do init intocado
- [ ] Plan-specific: `docs/benchmarks/m61-columnar-adoption.json` existe com ≥3 runs; ADR-0020 escrito; `/deps-audit` verde; smoke `test_analytic_query_plans_under_duckdb` verde (métrica do Goal)
- [ ] **Runtime-metric proof** — não há counter de runtime neste plano (é build/adoção); o oracle observável é o smoke CI + os artefatos de benchmark, verificados em workload de integração (Fase 4), não só "compila"
- [ ] **Plan archived** — após `/review READY_TO_MERGE` + PR merged, mover para `knowledge-base/plans/completed/m61-columnar-htap-adoption-plan.md`

## Failure scenarios (when I/O external)

O plano toca I/O externo: `git clone` (build), o container Postgres (benchmark/smoke via DSN), e o `make install` (build C++). Cenários:

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `git clone pg_duckdb v1.1.1` + submódulo (build) | tag inexistente / submódulo falha / network | `docker build --target pgduckdb-builder` com um `PGDUCKDB_REF` inválido | build FALHA-ALTO com o erro do git (não produz imagem parcial); pin corrigido OU honest-BLOCKED se irremediável (Q3) |
| `make install` (build C++ DuckDB, PG17) | gotcha de compat CMake/PG17 (o risco do ADR-0013:83) | build do estágio falha no compile | log do compile dita o ajuste (pin DuckDB/CMake); se irremediável sem scope-creep → BLOCKED para `cycle-plan`, não fingir PASS (Regra 3) |
| container Postgres M61 (smoke/benchmark, DSN) | boot falha porque `shared_preload_libraries` quebrou | fixture sobe o container e o healthcheck (`Dockerfile:118`) nunca fica verde | fixture FALHA-ALTO com o log do postmaster; T2.3 já cobre o fail-closed do preload ausente com mensagem tipada |
| `CREATE EXTENSION pg_duckdb` (SQL, init) | extensão não linka contra o PG17 exato (`.so` incompatível) | T2.1 roda `CREATE EXTENSION` na imagem | erro "cannot open shared object" claro; indica COPY/static-link errado (T1.2/D2) — não crash silencioso |

## Final Phase: Integration Validation (MANDATORY)

> Roda APÓS todas as fases. O plano NÃO está pronto até esta cadeia passar.

**Objective:** Validar que o embed funciona num workload real — a imagem builda, as 4 extensões coexistem, o benchmark roda e a suíte não regride.

### Execution

```
docker build -t theodb-m61 .                                  # imagem builda (Fase 1)
pytest benchmarks/tests/test_columnar_pgduckdb.py -v          # smokes + coexistência (Fases 2,4)
python3 benchmarks/run_m61_columnar_adoption.py --port <p> --runs 3 --write-doc   # benchmark (Fase 3)
ruff check benchmarks/                                        # lint
hadolint Dockerfile                                           # lint Dockerfile (se disponível)
/deps-audit m61-columnar-htap-adoption                        # CVE gate (Fase 2)
pytest benchmarks/tests/                                      # suíte existente não regride
```

### Acceptance Criteria

- [ ] Imagem builda; as 4 extensões (vector+vectorscale+theodb+pg_duckdb) coexistem numa fresh init
- [ ] Smoke analítico verde (plano DuckDB — métrica do Goal) + correctness 1e-3 + fail-closed
- [ ] `docs/benchmarks/m61-columnar-adoption.{md,json}` gerados (≥3 runs mean±std)
- [ ] `/deps-audit` sem CVE crítico/alto não-allowlisted; nenhuma dep AGPL embarcada
- [ ] Zero lint; `Dockerfile` ≤ 500 linhas; novos `.py` ≤ 500
- [ ] Failure scenarios exercitados (build inválido falha-alto; preload ausente → fail-closed tipado)
- [ ] Suíte `benchmarks/tests/` existente não regride

### If Validation Fails

1. Identificar falhas causadas por este plano vs pré-existentes
2. Corrigir todas as causadas pelo plano antes de declarar completo
3. Re-rodar a cadeia
4. Se o build C++ PG17 for irremediável sem scope-creep → BLOCKED para `cycle-plan` (não fingir PASS — Regra 3); reconsiderar a rota (Q3 / Failure scenario 2)
