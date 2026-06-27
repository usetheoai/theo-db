---
slug: pgvectorscale-streamingdiskann-integration
created_at: 2026-06-27
goal: "Enable o TheoDB a oferecer o índice ANN avançado StreamingDiskANN (pgvectorscale) na imagem oficial e medi-lo vs HNSW com o harness, measured by `CREATE EXTENSION vectorscale` + um índice `USING diskann` funcionando no container E um relatório docs/benchmarks/*-diskann-*.json com recall@10 medido em [0,1] e QPS > 0."
---

# Plan: pgvectorscale StreamingDiskANN — integração + medição (M2 DoD-2)

> **Version 1.0** — Slice DoD-2 do M2 (pilar killer): trazer o índice ANN avançado **StreamingDiskANN** (`pgvectorscale` 0.9.0) para a imagem oficial do TheoDB via build multi-stage (Rust/cargo-pgrx no builder; só artefatos no runtime), estender o harness (já shipado na v0.1.0) para medir o `diskann`, rodar o benchmark e registrar a **decisão de índice por evidência** (ADR 0002 — pgvectorscale vs fork vs ScaNN-AM). Sem fork (D3 upstream-first; commit pinado). NÃO é o M2 inteiro (DoD-3 embeddings é outra slice).

## Goal

> "Enable o TheoDB a oferecer o índice ANN avançado StreamingDiskANN (`pgvectorscale`) na imagem oficial e medi-lo vs HNSW com o harness, measured by `CREATE EXTENSION vectorscale` + um índice `USING diskann` funcionando no container E um relatório `docs/benchmarks/*-diskann-*.json` com recall@10 medido em [0,1] e QPS > 0."

## Context

ADR `docs/adr/0002-north-star-equal-or-superior-to-alloydb.md` (LOCKED) + `ROADMAP.md` M2: o harness de benchmark (DoD-1) shipou na v0.1.0 e destravou a **decisão de índice por evidência**. O próximo passo é o **índice ANN além do HNSW** (DoD-2): `pgvectorscale` StreamingDiskANN é o análogo OSS permissivo mais próximo do ScaNN (blueprint `alloydb-vector-ai-implementation` §T2). Este slice o integra e o mede — convertendo o `UNBENCHMARKED` de StreamingDiskANN em número, fechando a comparação HNSW vs DiskANN que sustenta a decisão de índice.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `Dockerfile` | 31 | `a10efad`/M0 (2026-06-26) | Imagem M0: postgres:17 + pgvector (digest/SHA pinados, HEALTHCHECK) | DoD-M0 intactos: `CREATE EXTENSION vector` + `<=>` + SMOKE PASSED; base por digest; pgvector por SHA |
| `Dockerfile.pgvectorscale` (NEW→merge) | 0 | — | spike multi-stage de viabilidade; será fundido no `Dockerfile` | — |
| `benchmarks/theodb_bench/__main__.py` | 86 | `651bf65` (2026-06-27) | CLI + `build_config` (só HNSW hoje) | `build_parser`/`build_config` continuam testáveis; harness index-agnóstico |
| `benchmarks/tests/test_integration.py` | 82 | `651bf65` | integração contra container | markers `integration`; DIP |
| `benchmarks/tests/test_harness.py` | 97 | `651bf65` | unit do harness (FakeVectorDB) | — |
| `smoke.sh` | 22 | M0 | smoke wire + vector | DoD-M0 SMOKE PASSED preservado |
| `docs/benchmarks/` | — | — | saída de evidência | reprodutível |
| `CHANGELOG.md` | — | `caa656f` | contrato público | Keep a Changelog |

Git sha base: `0297a97`. Container de validação: imagem `theo-db:scale-spike` (em build).

### Current callers / dependents

- **`build_config(args)`** em `__main__.py`: caller = `main()`; testes `test_cli_parses_args`. Mudança = adiciona spec `diskann` à lista `index_specs` (aditivo, não quebra HNSW).
- **`Dockerfile`**: consumido por CI/operador (build da imagem). Mudança = multi-stage (adiciona pgvectorscale); o estágio runtime preserva o build de pgvector do M0.
- External: a imagem `theo-db:dev` é o artefato distribuído — adiciona a extensão `vectorscale` (não remove nada).

### Domain glossary

- **StreamingDiskANN** — índice ANN baseado em grafo Vamana/DiskANN, disk-resident, do `pgvectorscale` (`USING diskann`).
- **SBQ (Statistical Binary Quantization)** — quantização escalar por z-score do pgvectorscale (comprime os vetores no índice).
- **cargo-pgrx** — framework Rust↔Postgres; compila a extensão `vectorscale` (`cargo pgrx install`).
- **`diskann.query_search_list_size`** — GUC de query do diskann (recall↔velocidade; default 100).
- **multi-stage build** — Dockerfile com builder (compila) + runtime (copia só artefatos), p/ não enviar a toolchain Rust na imagem.

### Architecture boundaries affected

Per `rules/architecture.md`: o índice é uma **extensão** (camada de infraestrutura do banco), não código do engine (D3 — sem fork do engine; extensões permitidas). O harness fala com o DB só via `db.py` (DIP, já estabelecido). A mudança no Dockerfile é de empacotamento (infra), não cruza fronteira de aplicação/domínio.

## Prior Art & Related Work

- **Internal blueprint:** `knowledge-base/discoveries/blueprints/alloydb-vector-ai-implementation-blueprint.md` §T2 (StreamingDiskANN + SBQ internals — Vamana, α-pruning, resort; tuning defaults), §Coverage Corner 2 (deps: pgrx =0.16.1, PG14-18). **(discover satisfeito — sem re-trabalho.)**
- **Internal blueprint:** `knowledge-base/discoveries/blueprints/vector-recall-benchmark-harness-blueprint.md` (medição recall@k/QPS — o harness mede qualquer índice via DDL).
- **Reference project:** `knowledge-base/references/pgvectorscale/DEVELOPMENT.md` (build chain: `cargo pgrx init --pgNN` → `cargo pgrx install --release`); `knowledge-base/references/pgvectorscale/README.md` (§StreamingDiskANN: `CREATE INDEX ... USING diskann (embedding vector_cosine_ops)`, tuning).
- **External:** pgvectorscale `github.com/timescale/pgvectorscale` (Apache-2.0... PostgreSQL License permissiva — D1-clean), pinned commit `57c88b7`.

## Dependencies

| Ecosystem | Package | Version | License | CVE | Rule-9 (reuso) |
|---|---|---|---|---|---|
| apt (build-only) | rustup/cargo + clang + postgresql-server-dev-17 | bookworm | permissivas (build-only, não vão no runtime) | n/a | toolchain padrão |
| cargo (build-only) | `cargo-pgrx` | `=0.16.1` | MIT/Apache | n/a | framework pgrx — não reinventar |
| pg ext (runtime) | `pgvectorscale` (vectorscale) | 0.9.0 (`57c88b7`) | PostgreSQL License | nenhuma conhecida | usar as-is (D3 upstream-first, sem fork) |

**Nota D1/D3:** pgvectorscale é PostgreSQL License (permissiva, D1-clean). Usado **as-is** (sem fork) → D3 honrado (upstream-first; commit pinado = base do CI-de-rebase). A toolchain Rust fica só no estágio builder (não no runtime distribuído).

## Objective

- [ ] `pgvectorscale` (vectorscale) compila no estágio builder e seus artefatos vão para o runtime (sem Rust no runtime)
- [ ] `CREATE EXTENSION vectorscale CASCADE` funciona no container; `CREATE INDEX ... USING diskann` constrói
- [ ] O harness mede o `diskann` (recall@k + latência/QPS) via uma index spec
- [ ] Relatório `docs/benchmarks/*-diskann-*.json` com recall@10 medido; comparação HNSW vs DiskANN
- [ ] Decisão de índice registrada por evidência (ADR/doc) honrando D3

## ADRs

### D1 — Build multi-stage (Rust no builder, só artefatos no runtime)

**Decision:** `Dockerfile` vira multi-stage: estágio `scale-builder` instala Rust + cargo-pgrx 0.16.1 e compila `vectorscale`; estágio `runtime` (base M0: postgres:17 + pgvector) copia `vectorscale*.so` + `.control` + `.sql`.
**Rationale:** não enviar a toolchain Rust (~1-2GB) na imagem distribuída (KISS no artefato; o M0 ADR D2 previa re-avaliar multi-stage). Build-only deps não tocam D1 (não vão no runtime).
**Alternatives considered:** single-stage com Rust (rejeitado — bloat de ~1-2GB na imagem distribuída); pacote .deb pré-built do pgvectorscale (rejeitado — não há .deb permissivo pinável ao nosso PG17 + perde controle do commit/CI-rebase).
**Consequences:** build mais longo (Rust compile); imagem runtime cresce só pelo `.so` do vectorscale.

### D2 — Usar pgvectorscale as-is (sem fork) — D3 honrado

**Decision:** integrar `pgvectorscale@57c88b7` as-is, pinado por commit. Nenhum patch/fork.
**Rationale:** PRD D3 = upstream-first; só forka com benchmark de gatilho. Não há gatilho — usamos como está. Commit pinado é a base do CI-de-rebase (`rules/parsimony-ladder.md` Regra 9 — não reinventar).
**Alternatives considered:** forkar para customizar (rejeitado — viola D3 sem evidência); StreamingDiskANN reimplementado (rejeitado — Regra 9).
**Consequences:** TheoDB segue o upstream; bumps de versão são rebases pinados.

### D3 — Decisão de índice é guiada pela evidência do harness, não pré-fixada

**Decision:** após medir HNSW vs DiskANN no harness, registrar a decisão (qual índice é o "ANN avançado" do M2) num doc/ADR com os números.
**Rationale:** ADR 0002 measurement-first — a escolha sai do benchmark, não da vontade.
**Alternatives considered:** assumir DiskANN superior sem medir (rejeitado — viola ADR 0002 / public-copy).
**Consequences:** o doc de decisão cita `docs/benchmarks/`.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Build Rust do pgvectorscale é lento (10-30min) e pesado em disco | Medium | multi-stage + cache de layers; spike valida viabilidade antes do plano fechar | dev |
| `cargo-pgrx` version mismatch com pgrx do crate quebra o build | High | pinar `cargo-pgrx 0.16.1` (= pgrx do Cargo.toml); spike confirma | dev |
| Imagem runtime cresce com o `.so` do vectorscale | Low | só o artefato é copiado (sem toolchain); medir tamanho final | dev |
| pgvectorscale parallel-build não-determinístico afeta recall medido | Low | já documentado no harness (recall ~estável ±variância); reportar como tal | dev |

## Unresolved Questions

- Q1 — `vector_cosine_ops` vs `vector_l2_ops` para o diskann no benchmark? → medir com a métrica que o harness usa (l2 por default; cosine como sweep). *Não bloqueia.*
- Q2 — caminhos exatos de instalação do `cargo pgrx install` no postgres:17-bookworm? → o spike confirma (pkglibdir `/usr/lib/postgresql/17/lib`, extdir `/usr/share/postgresql/17/extension`). *De-riscado pelo spike.*

## Dependency Graph

```
Phase 1 (Dockerfile multi-stage) ──▶ Phase 3 (medir + decisão) ──▶ Phase 4 (Integration Validation)
        │                                  ▲
        └──▶ Phase 2 (harness diskann spec)┘
```

---

## Phase 1: Dockerfile multi-stage com pgvectorscale

### T1.1 — Fundir o spike multi-stage no Dockerfile + validar extensão carrega

#### Objective
`Dockerfile` multi-stage: builder compila `vectorscale`; runtime (M0 + pgvector) copia os artefatos. `CREATE EXTENSION vectorscale CASCADE` funciona.

#### Why this step (action + reasoning)
1. **What:** formalizar `Dockerfile.pgvectorscale` (spike) no `Dockerfile`.
2. **Why now:** é a fundação — sem a extensão na imagem, não há índice nem medição. O spike (em build) de-risca empiricamente antes de commitar (ADR 0002 — evidência primeiro).

#### Evidence
`Dockerfile.pgvectorscale` (spike). Blueprint `alloydb-vector-ai-implementation` §Corner2 (pgrx =0.16.1, PG17). `references/pgvectorscale/DEVELOPMENT.md` (cargo pgrx install).

#### Files to edit
```
Dockerfile — vira multi-stage (builder pgvectorscale + runtime pgvector + COPY artefatos)
Dockerfile.pgvectorscale — removido após fundir (era spike)
```

#### Deep file dependency analysis
`Dockerfile` (M0, 31 linhas) builda postgres:17 + pgvector. Mudança: adiciona estágio builder + COPY do vectorscale. Downstream: CI/operador buildam a imagem; preserva os DoD do M0 (vector + smoke).

#### Deep Dives
- Builder: `FROM postgres:17-bookworm`; instala build-essential, postgresql-server-dev-17, clang, libssl-dev, git, curl, rustup; `cargo install cargo-pgrx --version 0.16.1`; clone `pgvectorscale@57c88b7`; `cargo pgrx init --pg17 $(which pg_config)`; `cargo pgrx install --release --features pg17`.
- Runtime: base M0 (digest pinado) + build pgvector (SHA pinado, como hoje) + `COPY --from=scale-builder` do `vectorscale*` (.so para pkglibdir, .control/.sql para extdir).
- **Invariante:** os 3 DoD do M0 continuam (vector + `<=>` + SMOKE PASSED); base por digest, pgvector por SHA.
- Edge/negative: se o COPY não achar `vectorscale*` → build falha alto (não silencioso).

#### Tasks
1. Reescrever `Dockerfile` como multi-stage (builder + runtime).
2. `docker build -t theo-db:dev .` exits 0.
3. Remover `Dockerfile.pgvectorscale`.

#### TDD
```
RED:     test_vectorscale_extension_loads() — integration: CREATE EXTENSION vectorscale CASCADE não levanta; falha antes do build da imagem nova
RED:     test_diskann_index_builds() — integration: CREATE INDEX ... USING diskann constrói
GREEN:   Dockerfile multi-stage + rebuild
REFACTOR: confirmar runtime sem toolchain Rust (which cargo → vazio)
VERIFY:  PGPORT=<p> python -m pytest benchmarks/tests/test_integration.py -k vectorscale -m integration -q
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] Pass: `docker build -t theo-db:dev .` exits 0
- [ ] Pass: container roda + `psql -c "CREATE EXTENSION vectorscale CASCADE; SELECT extversion FROM pg_extension WHERE extname='vectorscale';"` retorna a versão
- [ ] Pass: `docker run --rm theo-db:dev which cargo` retorna vazio (sem Rust no runtime)
- [ ] Pass: os 3 DoD do M0 seguem (`bash smoke.sh` → SMOKE PASSED)
- [ ] Pass: `wc -l Dockerfile` ≤ 500 linhas

#### DoD
- [ ] Pass: `psql -c "CREATE EXTENSION vectorscale CASCADE"` no container exits 0
- [ ] Pass: `bash smoke.sh` → SMOKE PASSED (M0 preservado)

---

## Phase 2: Harness mede o diskann

### T2.1 — Adicionar index spec `diskann` ao build_config + teste

#### Objective
`build_config` ganha uma spec `diskann` (`CREATE INDEX ... USING diskann`) com sweep em `diskann.query_search_list_size`, medível pelo harness.

#### Why this step (action + reasoning)
1. **What:** estender `build_config` com a spec diskann (aditivo).
2. **Why now:** o harness é index-agnóstico (recebe DDL); só falta a spec p/ medir o diskann e comparar com HNSW (a evidência da decisão).

#### Evidence
`benchmarks/theodb_bench/__main__.py` `build_config` (HNSW spec hoje). Blueprint StreamingDiskANN tuning (`query_search_list_size` default 100).

#### Files to edit
```
benchmarks/theodb_bench/__main__.py — build_config: adiciona spec diskann (flag --index)
benchmarks/tests/test_harness.py — unit: build_config inclui diskann quando solicitado
benchmarks/tests/test_integration.py — integration: harness mede diskann (recall ≥ 0.90)
```

#### Deep file dependency analysis
`build_config` retorna `index_specs`; adiciono diskann. `test_harness` (FakeVectorDB) valida a config; `test_integration` mede contra o container.

#### Deep Dives
- diskann spec: `name="diskann"`, `index_name="bench_diskann"`, `ddl="CREATE INDEX bench_diskann ON bench_vectors USING diskann (embedding vector_l2_ops)"`, sweep `[{"label":"sls=100","session":["SET enable_seqscan=off","SET diskann.query_search_list_size=100"]}]`.
- `--index {hnsw,diskann,both}` na CLI (default both).
- **Invariante:** HNSW spec intacta; diskann aditivo.
- Edge: se vectorscale não instalado → build do índice falha (IndexNotUsedError/erro tipado).

#### Pseudo-code / Signatures
```pseudocode
def build_config(args):
  specs = []
  if args.index in ("hnsw","both"): specs.append(hnsw_spec)
  if args.index in ("diskann","both"): specs.append(diskann_spec)
  return { ..., "index_specs": specs }
```

#### Tasks
1. Adicionar `--index` ao parser + diskann spec em build_config.
2. Unit test: `build_config(--index diskann)` tem só diskann; `both` tem 2.
3. Integration test: harness mede diskann.

#### TDD
```
RED:     test_build_config_diskann_only() — --index diskann → index_specs[0].name == "diskann"
RED:     test_build_config_both() — --index both → 2 specs (hnsw + diskann)
RED:     test_diskann_recall_high(db) — integration: harness mede diskann, recall@10 ≥ 0.90
GREEN:   estender build_config + parser
REFACTOR: extrair specs factory se necessário
VERIFY:  cd benchmarks && python -m pytest tests/test_harness.py -k diskann -q
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] Pass: `cd benchmarks && python -m pytest tests/test_harness.py -k diskann -q` exits 0
- [ ] Pass: `--index both` produz specs HNSW + diskann (`pytest -k both`)
- [ ] Pass: `cd benchmarks && python -m pytest tests/ -k hnsw -q` exits 0 (HNSW spec inalterada)
- [ ] Pass: `wc -l benchmarks/theodb_bench/__main__.py` ≤ 500

#### DoD
- [ ] Pass: `cd benchmarks && python -m pytest tests/test_integration.py -k diskann -m integration -q` exits 0

---

## Phase 3: Medir + registrar a decisão de índice

### T3.1 — Run real HNSW vs DiskANN + doc de decisão por evidência

#### Objective
Rodar o harness com `--index both` contra o container, emitir `docs/benchmarks/*-diskann-*.json`, e escrever o doc de decisão de índice citando os números (D3).

#### Why this step (action + reasoning)
1. **What:** medir os dois índices e registrar a decisão por evidência.
2. **Why now:** é o produto do M2 DoD-2 — a evidência que escolhe o índice (ADR 0002). Sem o número, é opinião.

#### Evidence
O harness + a imagem com vectorscale (T1/T2). ADR 0002 (decisão por evidência).

#### Files to edit
```
docs/benchmarks/{date}-pgvector-l2.json — atualizado com diskann (HNSW + DiskANN)
docs/decisions/m2-index-decision.md (NEW) — decisão por evidência citando os números
CHANGELOG.md — entrada
```

#### Deep file dependency analysis
`docs/decisions/m2-index-decision.md` NEW — consome o JSON. Cita HNSW vs DiskANN recall/QPS.

#### Deep Dives
- Run: `python -m theodb_bench --index both --seed 42 --n 5000 --dim 128 --k 10 --metric l2`.
- Doc: tabela HNSW vs DiskANN (recall@10, QPS, build, size) + recomendação honesta (qual adotar como "ANN avançado" do M2) + nota UNBENCHMARKED→medido + D3 (sem fork).
- **Runtime-metric proof:** o JSON com diskann recall∈[0,1], qps>0 é a prova.

#### Tasks
1. Rodar o benchmark `--index both`.
2. Escrever `docs/decisions/m2-index-decision.md` com os números.
3. CHANGELOG.

#### TDD
```
RED:     (validação) — o JSON não tem linha diskann; doc de decisão ausente
GREEN:   rodar o run real; escrever o doc
REFACTOR: None
VERIFY:  python3 -c "import json; d=json.load(open('docs/benchmarks/...json')); assert any(r['index']=='diskann' for r in d['results'])"
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] Pass: `python3 -c "import json,glob; d=json.load(open(sorted(glob.glob('docs/benchmarks/*-pgvector-*.json'))[-1])); assert any(r['index']=='diskann' and 0<=r['recall_at_k']<=1 and r['qps']>0 for r in d['results'])"` exits 0
- [ ] Pass: `test -f docs/decisions/m2-index-decision.md`
- [ ] Pass: o doc cita números de HNSW E DiskANN (não UNBENCHMARKED)

#### DoD
- [ ] Evidência diskann publicada; decisão registrada honrando D3

---

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | pgvectorscale na imagem (build multi-stage) | T1.1 | Dockerfile multi-stage |
| 2 | CREATE EXTENSION vectorscale + USING diskann funcionam | T1.1 | integration test |
| 3 | runtime sem toolchain Rust | T1.1 | `which cargo` vazio |
| 4 | harness mede diskann | T2.1 | diskann index spec + integration |
| 5 | HNSW spec preservada | T2.1 | testes HNSW verdes |
| 6 | evidência diskann publicada (recall/QPS) | T3.1 | run real + JSON |
| 7 | decisão de índice por evidência (D3) | T3.1 | doc de decisão |
| 8 | M0 DoD preservados | T1.1 | smoke PASSED |

**Coverage: 8/8 (100%)**

## Global Definition of Done

- [ ] Todas as fases completas
- [ ] Testes verdes — `cd benchmarks && python -m pytest` (unit) + `-m integration` (container)
- [ ] Zero lint — `ruff check benchmarks/`
- [ ] File-size ≤ 500 por arquivo
- [ ] CHANGELOG `[Unreleased]` atualizado
- [ ] **Runtime-metric proof** — JSON com diskann recall∈[0,1] + qps>0, gerado por run real contra o container
- [ ] M0 DoD preservados (smoke PASSED)
- [ ] Decisão de índice registrada (D3 honrado)

## Failure scenarios (when I/O external)

I/O externo: PostgreSQL via psycopg2 + o build Docker.

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `postgres` (vectorscale) | `CREATE EXTENSION vectorscale` falha (artefato ausente) | integration: assert extensão carrega | erro tipado; build da imagem falha alto se o COPY não achou o `.so` |
| `postgres` (planner) | diskann não usado (seqscan) | integration: `assert_index_used` | `IndexNotUsedError` (já no harness) |
| docker build | cargo-pgrx version mismatch | spike/build | build falha alto com erro do cargo (não silencioso) |

## Final Phase: Integration Validation (MANDATORY)

### T4.1 — Validação end-to-end

#### Concurrency tests

(none — single-threaded)

#### Execution
```
docker build -t theo-db:dev .                 # imagem com vectorscale
docker run -d -e POSTGRES_PASSWORD=postgres -p <p>:5432 --name theo-db-m2 theo-db:dev
bash smoke.sh                                  # M0 DoD preservado
psql -c "CREATE EXTENSION vectorscale CASCADE; CREATE INDEX t ON ... USING diskann ..."   # DoD-2
cd benchmarks && python -m pytest -q && python -m pytest -q -m integration
ruff check benchmarks/
python -m theodb_bench --index both ...        # evidência diskann
```

#### Acceptance Criteria
- [ ] Unit + integration verdes (`cd benchmarks && python -m pytest -q` e `python -m pytest -q -m integration`)
- [ ] `bash smoke.sh` → SMOKE PASSED (M0 preservado)
- [ ] `CREATE EXTENSION vectorscale` + `USING diskann` funcionam
- [ ] `which cargo` vazio no runtime
- [ ] Runtime-metric proof — JSON com diskann recall∈[0,1], qps>0
- [ ] Pass: `test -f docs/decisions/m2-index-decision.md && grep -q diskann docs/decisions/m2-index-decision.md`

### If Validation Fails
1. Separar falhas do slice vs pré-existentes.
2. Corrigir as do slice; re-rodar.
