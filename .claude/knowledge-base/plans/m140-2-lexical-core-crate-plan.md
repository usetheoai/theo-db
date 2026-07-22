---
slug: m140-2-lexical-core-crate
milestone_id: M140.2
created_at: 2026-07-22
goal: Extrair o núcleo lexical pgrx-free para um crate próprio testável com cargo test stock, sem pgrx no Cargo.toml.
---

# Plan: M140.2 — Crate núcleo lexical sem pgrx

> **Version 1.0** — Extrair a superfície pgrx-free do motor lexical (o `Directory`/`SegmentStore`/`MemStore` do
> spike M139, hoje em `theodb_rs/src/lexical/pg_directory.rs`) para um crate próprio `theodb_lexical`, testável
> com `cargo test` stock — o M139 já descobriu que o núcleo é pgrx-free (o teste in-crate falha no link de
> símbolos PG; o standalone passa). Destrava a classe de testes hoje presos ao link pgrx e é a fundação
> testável das fatias M140.3/M140.4.

## Goal

> Enable os testes do núcleo lexical a rodarem com `cargo test` stock (sem link pgrx), extraindo o código
> pgrx-free para o crate `theodb_lexical` cujo `Cargo.toml` **não** depende de pgrx, measured by
> `cargo test -p theodb_lexical` passar verde E `cargo tree -p theodb_lexical | grep -c pgrx` retornar `0`.

## Context

O spike M139 (ADR 0051) provou que o núcleo do motor lexical — o trait `Directory` do Tantivy sobre `MemStore`
(buffer-then-flush) — é **pgrx-free por design**: `pg_directory.rs` importa só `std` + `tantivy`, zero
`pgrx`/`pg_sys`/`Spi`/`#[pg_extern]`. Mas hoje ele vive DENTRO do crate `theodb_rs` (cdylib pgrx), então
`cargo test` tenta linkar os símbolos do Postgres e **falha** (o M139 documentou: o teste in-crate não linka; um
crate standalone passa). Os 6 testes de `pg_directory.rs` (`test_pg_directory_indexes_and_searches`, etc.) só
rodam hoje via `cargo pgrx test` (que não linka na droplet). Extrair o núcleo para um crate próprio sem pgrx é o
que torna esses testes (e os das fatias seguintes) executáveis com `cargo test` stock.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/lexical/pg_directory.rs` | 298 | `becc5e6`~ (M139) | O núcleo pgrx-free: `MemStore`/`PgDirectory`/`SegmentStore` + 6 testes | Comportamento byte-idêntico; os 6 testes continuam verdes |
| `theodb_rs/src/lexical/pg_backing.rs` | 211 | M139 | A camada pgrx (SPI/`#[pg_extern]`) que consome o núcleo | Continua compilando; importa do crate núcleo em vez de `crate::lexical::pg_directory` |
| `theodb_rs/src/lexical/mod.rs` | 12 | M139 | Re-exporta o núcleo | Re-exporta do crate núcleo (compat de caminho) |
| `theodb_rs/Cargo.toml` | ~40 | M135 | Manifesto do cdylib pgrx | Vira workspace root; adiciona dep `theodb_lexical` sob a feature `spike-lexical` |
| `theodb_rs/lexical_core/Cargo.toml` (NEW) | 0 | — | (novo) manifesto do crate núcleo — **sem pgrx** | — |
| `theodb_rs/lexical_core/src/lib.rs` (NEW) | 0 | — | (novo) = conteúdo de `pg_directory.rs` movido | — |

### Current callers / dependents

- **Symbol:** `MemStore` / `PgDirectory` / `SegmentStore` (hoje em `theodb_rs/src/lexical/pg_directory.rs`)
  - **Callers (produção):** `theodb_rs/src/lexical/pg_backing.rs:13` (`use crate::lexical::pg_directory::{MemStore, SegmentStore}`), `pg_backing.rs:75` (`use crate::lexical::pg_directory::PgDirectory`)
  - **Callers (tests):** os 6 testes internos de `pg_directory.rs:201-296`
  - **External (outro repo):** não — é código atrás da feature `spike-lexical`, não shipado no default.
- **Nota:** o módulo inteiro está atrás de `--features spike-lexical`; o build default (shipado) não o compila.

### Domain glossary

- **pgrx-free** — código que não importa `pgrx`/`pg_sys` nem usa `#[pg_extern]`/`Spi`; compila e testa sem o link do Postgres.
- **SegmentStore** — o trait-seam que separa o CONTRATO do `Directory` (pgrx-free) da FONTE dos bytes (`MemStore` em memória, ou o futuro page-store pgrx).
- **cdylib** — crate-type do `theodb_rs` (biblioteca dinâmica C que o Postgres carrega); não é testável com `cargo test` stock por causa do link de símbolos PG.
- **workspace (cargo)** — agrupamento de crates com um `Cargo.lock` compartilhado; permite um membro pgrx-free (`theodb_lexical`) e um membro cdylib (`theodb_rs`) lado a lado.

### Architecture boundaries affected

Cruza uma fronteira de **camada** (`rules/architecture.md §1`): separa a **lógica pura** (o núcleo lexical, zero
I/O de PG) da **infra pgrx** (o page-backing SPI). Isso é DIP (`§2`): o núcleo define o trait `SegmentStore`; a
camada pgrx o implementa. Não cruza o boundary do ADR-0009 (que é sobre a superfície SQL `#[pg_extern]` — o núcleo
não tem nenhum extern). Ver ADR D1.

## Prior Art & Related Work

- **Internal ADR:** `docs/adr/0051-m139-tantivy-pg-page-directory-design.md` — o spike que provou o núcleo pgrx-free (o boundary que este milestone materializa em crate).
- **Internal ADR (a reconciliar):** `docs/adr/0009-theodb-rs-api-surface-single-module.md` — a superfície SQL é um `api.rs` único por restrição de schema pgrx. O núcleo lexical NÃO é superfície SQL (zero externs) → não conflita; a reconciliação é o ADR D1 deste plano.
- **Reference project:** ParadeDB — o único crate deles sem pgrx é `tokenizers`; extrair "a engine inteira" seria copiar forma que eles não têm (`CLAUDE.md` — Esforço ≠ Complexidade). Extraímos só o que É pgrx-free (o Directory core), não mais.
- **Skill de patterns:** nenhuma `skills/*-patterns/` casa (extração de crate Rust) — verificado; nada a citar/sobrepor.
- **External:** [The Cargo Book — Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html) — o modelo de workspace com membros heterogêneos (lib pgrx-free + cdylib pgrx). pgrx [suporta workspaces](https://github.com/pgcentralfoundation/pgrx) (o cdylib pode ser membro).

## Objective

- [ ] Sub-goal 1 — crate `theodb_lexical` criado (lib, dep só `tantivy`), com o conteúdo pgrx-free movido verbatim.
- [ ] Sub-goal 2 — os 6 testes do núcleo rodam via `cargo test -p theodb_lexical` (stock, sem pgrx).
- [ ] Sub-goal 3 — `theodb_rs` vira workspace root e consome `theodb_lexical` atrás da feature `spike-lexical`.
- [ ] Sub-goal 4 — `pg_backing.rs`/`mod.rs` importam do crate núcleo; o build `--features spike-lexical` compila.
- [ ] Sub-goal 5 — ADR-1 reconcilia com o ADR-0009 (por que o núcleo merece crate próprio sem reverter o módulo único da superfície SQL).
- [ ] Sub-goal 6 — build shipado (default, sem spike-lexical) e gates de CI (M136) seguem verdes.

## ADRs

### D1 — Crate núcleo pgrx-free separado NÃO contradiz o ADR-0009 (superfície SQL única)

- **Decision:** o núcleo lexical (`Directory`/`SegmentStore`/`MemStore`) vive num crate `theodb_lexical` sem
  pgrx; o `theodb_rs` (cdylib pgrx) o consome. O ADR-0009 (superfície SQL = `api.rs` único) permanece intacto.
- **Rationale:** o ADR-0009 restringe a camada de **`#[pg_extern]`** (por causa do `#[pg_schema] mod theodb_rs`
  compartilhado — o schema vem do ident do módulo). O núcleo lexical tem **zero externs** — é lógica pura, outra
  camada (`architecture.md §1`). Separá-lo por **testabilidade** (o problema de link pgrx do M139) é ortogonal à
  decisão de superfície SQL. Não há reversão silenciosa: são camadas diferentes.
- **Alternatives considered:** (a) manter o núcleo dentro de `theodb_rs` e testar só via `cargo pgrx test` —
  rejeitado: `cargo pgrx test` não linka na droplet (M139), então os testes do núcleo ficariam presos; (b) um
  workspace separado fora de `theodb_rs` — rejeitado: mais indireção; o membro cargo dentro de `theodb_rs/` é o
  mais simples (KISS).
- **Consequences:** habilita `cargo test` stock no núcleo (M140.3/M140.4 testam a query/scoring puras sem pgrx);
  restringe o núcleo a ser genuinamente pgrx-free (o `Cargo.toml` sem pgrx é o gate objetivo — se vazar um tipo
  pgrx, não compila).

### D2 — `theodb_rs` vira workspace root (membro `.` + `lexical_core`), não um workspace externo

- **Decision:** adicionar `[workspace] members = [".", "lexical_core"]` ao `theodb_rs/Cargo.toml`; o crate núcleo
  em `theodb_rs/lexical_core/`.
- **Rationale:** KISS — um único `Cargo.lock`, o cdylib pgrx e o lib pgrx-free lado a lado; pgrx suporta o cdylib
  como membro de workspace. Menos indireção que um workspace no repo root (que arrastaria outros diretórios).
- **Alternatives considered:** workspace no repo root — rejeitado (arrasta `benchmarks/`, docs, etc.); crate
  publicado — YAGNI (uso interno).
- **Consequences:** `cargo pgrx` opera sobre o membro `theodb_rs` normalmente; `cargo test -p theodb_lexical`
  testa o núcleo isolado.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| O boundary núcleo↔pgrx pode vazar (um tipo pgrx no núcleo) | Medium | O `Cargo.toml` sem pgrx é o gate objetivo — vazamento → não compila; `cargo tree` prova zero pgrx | dev |
| Tornar `theodb_rs` workspace pode quebrar `cargo pgrx run/test/package` | Medium | Validar `cargo pgrx` localmente (toolchain 0.19+PG18) antes do release; se quebrar, reverter a estrutura | dev |
| Divergência do ADR-0009 lida como reversão silenciosa | Low | ADR D1 explícito nomeia o ADR-0009 e explica por que é outra camada | dev |
| Comportamento do núcleo muda no move (regressão) | Medium | Mover **verbatim** (git mv + ajuste de imports só); os 6 testes provam byte-identidade | dev |

## Unresolved Questions

- Q1 — `cargo pgrx package`/`run` funcionam com `theodb_rs` como workspace root? → resolver no T3 validando localmente (toolchain 0.19+PG18 provisionado); se não, o D2 cai para "workspace no repo root" via ADR-amenda.
- Q2 — a feature `spike-lexical` deve virar `dep:theodb_lexical` (path dep opcional)? → sim (T3): a dep do núcleo entra sob a feature, não no default, para o build shipado não puxar tantivy.
- Q3 — os 6 testes movem verbatim ou precisam de ajuste de `use`? → movem; só o `use super::*`/paths internos podem precisar de ajuste (T1).

## Dependency Graph

```
Phase 1 (cria crate núcleo + move código + cargo test local) ──▶ Phase 2 (theodb_rs consome: workspace + imports)
                                                                          │
                                                                          ▼
                                                                  Phase 3 (ADR + validação build pgrx + CI)
```

---

## Phase 1: Crate núcleo pgrx-free

**Objective:** o crate `theodb_lexical` com o núcleo movido verbatim, testável com `cargo test` stock.

### T1.1 — Criar `theodb_lexical` e mover o núcleo pgrx-free

#### Objective
Criar `theodb_rs/lexical_core/{Cargo.toml,src/lib.rs}` com o conteúdo de `pg_directory.rs` (incl. os 6 testes),
dep só `tantivy`, sem pgrx.

#### Why this step (action + reasoning)
1. **What this step does** — cria o crate lib `theodb_lexical` e move `pg_directory.rs` → `lexical_core/src/lib.rs` verbatim.
2. **Why it is necessary now** — é a fundação testável (D1); sem o crate, os 6 testes do núcleo continuam presos ao link pgrx (M139).

#### Evidence
`theodb_rs/src/lexical/pg_directory.rs:12-23` (imports só std+tantivy — pgrx-free provado); `:201-296` (os 6 testes a preservar).

#### Files to edit
```
theodb_rs/lexical_core/Cargo.toml — (NEW) package theodb_lexical, lib, dep tantivy = "0.26"
theodb_rs/lexical_core/src/lib.rs — (NEW) = conteúdo de pg_directory.rs (move verbatim)
```

#### Deep file dependency analysis
- `lexical_core/src/lib.rs` (NEW) = `pg_directory.rs` movido; sem mudança de lógica. Os `use tantivy::...` continuam; nenhum `use crate::...` a ajustar (o núcleo não importa nada de theodb_rs).
- Downstream: `pg_backing.rs`/`mod.rs` (Phase 2) passam a importar deste crate.

#### Deep Dives
- `Cargo.toml` do núcleo: `[package] name="theodb_lexical"`, `edition="2024"`; `[dependencies] tantivy="0.26"`. **Sem** `pgrx`. Esse é o gate objetivo (D1).
- Invariante: os 6 testes (`test_pg_directory_indexes_and_searches` etc.) passam byte-idênticos.
- Edge case: se algum teste usava um helper de `theodb_rs`, ajustar; a inspeção mostra que não (o núcleo é auto-contido).

#### Tasks
1. Criar `lexical_core/Cargo.toml` (sem pgrx).
2. `git mv` (ou copiar) `pg_directory.rs` → `lexical_core/src/lib.rs`, ajustar `mod tests`/paths se necessário.
3. `cargo test -p theodb_lexical` → 6 verdes.

#### TDD
```
RED:  os 6 testes de pg_directory.rs, agora em lexical_core, FALHAM se o move quebrar imports
GREEN: mover verbatim; ajustar só paths internos
REFACTOR: None expected (move verbatim)
VERIFY: cd theodb_rs && cargo test -p theodb_lexical
```

#### Concurrency tests

O núcleo tem `Arc<RwLock<...>>` no `MemStore`, mas este milestone é **move verbatim** — nenhum código de
concorrência novo é escrito (a segurança multi-thread é herdada do `RwLock` da std, inalterada). O teste de
regressão de threads (`#153`) é escopo do M140.4, não deste move.

(none — single-threaded)

#### Acceptance Criteria
- [ ] `cargo test -p theodb_lexical` exit code 0 com os 6 testes do núcleo.
- [ ] `grep -c pgrx theodb_rs/lexical_core/Cargo.toml` retorna 0.
- [ ] `cargo tree -p theodb_lexical` não lista `pgrx`.
- [ ] `wc -l theodb_rs/lexical_core/src/lib.rs` ≤ 320 (o núcleo cabe).

#### DoD
- [ ] `cd theodb_rs && cargo test -p theodb_lexical` exit code 0.
- [ ] `cargo tree -p theodb_lexical | grep -c pgrx` retorna 0.
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Phase 2: `theodb_rs` consome o crate núcleo

**Objective:** o workspace + os imports atualizados; a camada pgrx passa a depender do crate núcleo.

### T2.1 — Workspace root + dep + imports

#### Objective
Adicionar `[workspace]` ao `theodb_rs/Cargo.toml`, a dep `theodb_lexical` sob a feature `spike-lexical`, e trocar
`crate::lexical::pg_directory::` por `theodb_lexical::` em `pg_backing.rs`/`mod.rs`.

#### Why this step (action + reasoning)
1. **What this step does** — liga o `theodb_rs` ao crate núcleo (workspace + dep + imports).
2. **Why it is necessary now** — sem isso, o núcleo extraído fica órfão; a camada pgrx precisa consumi-lo (DoD-4).

#### Evidence
`theodb_rs/Cargo.toml` (features `spike-lexical = ["dep:tantivy"]` — vira `["dep:theodb_lexical"]`);
`pg_backing.rs:13,75` (os `use crate::lexical::pg_directory::` a trocar); `mod.rs:12` (o re-export).

#### Files to edit
```
theodb_rs/Cargo.toml — [workspace] members=[".","lexical_core"]; feature spike-lexical -> dep:theodb_lexical (path, optional)
theodb_rs/src/lexical/pg_backing.rs — use theodb_lexical::{MemStore, SegmentStore, PgDirectory}
theodb_rs/src/lexical/mod.rs — re-exporta de theodb_lexical; remove `pub mod pg_directory`
```

#### Deep file dependency analysis
- `Cargo.toml`: `spike-lexical` passa a habilitar `dep:theodb_lexical` (path dep opcional) em vez de `dep:tantivy` direto (o tantivy vem transitivo do núcleo). O default (shipado) continua sem a feature → sem tantivy.
- `pg_backing.rs`: troca o caminho de import; a lógica SPI não muda.
- `mod.rs`: `pg_directory` deixa de ser submódulo (foi para o crate); re-exporta de `theodb_lexical`.
- Downstream: `pg_backing.rs` continua sendo o único consumidor do núcleo.

#### Deep Dives
- Invariante: o build default (`cargo build`, sem `--features spike-lexical`) **não** puxa tantivy nem theodb_lexical (a dep é opcional sob a feature).
- Edge case: `cargo pgrx` deve operar sobre o membro `theodb_rs` mesmo com o workspace — validar no T3.

#### Tasks
1. Adicionar `[workspace]` + membros ao `theodb_rs/Cargo.toml`.
2. Trocar a feature `spike-lexical` para `dep:theodb_lexical`.
3. Atualizar imports em `pg_backing.rs`/`mod.rs`.
4. `cargo check -p theodb_lexical` (núcleo) + (T3) build pgrx.

#### TDD
```
RED:  cargo check --features spike-lexical falha se os imports/dep estiverem errados
GREEN: workspace + dep + imports corretos
REFACTOR: None expected
VERIFY: cd theodb_rs && cargo check -p theodb_lexical && cargo pgrx check --features spike-lexical (T3, toolchain)
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] `cargo build -p theodb_lexical` compila (o núcleo isolado).
- [ ] `grep -c "theodb_lexical" theodb_rs/Cargo.toml` ≥ 1 (a dep entrou).
- [ ] O default (sem feature) não lista tantivy: `cargo tree --no-default-features 2>/dev/null | grep -c tantivy` ≈ 0 (validar no T3 com toolchain).

#### DoD
- [ ] `cd theodb_rs && cargo check -p theodb_lexical` exit code 0.
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Phase 3: ADR + validação do build pgrx + CI

**Objective:** o ADR de reconciliação e a prova de que o build shipado + CI seguem verdes.

### T3.1 — ADR-0053 + validação do build pgrx completo

#### Objective
Escrever `docs/adr/0053-m140-2-lexical-core-crate.md` (reconcilia com ADR-0009) e validar que
`cargo pgrx check --features spike-lexical` compila e o build default (shipado) segue verde.

#### Why this step (action + reasoning)
1. **What this step does** — documenta a decisão (D1/D2) e prova o build pgrx (DoD-5, DoD-6).
2. **Why it is necessary now** — é a evidência de que o consumo pelo cdylib funciona (não só o núcleo isolado).

#### Evidence
`docs/adr/0009-theodb-rs-api-surface-single-module.md` (o ADR a reconciliar); o toolchain pgrx 0.19+PG18 (provisionado localmente para validar).

#### Files to edit
```
docs/adr/0053-m140-2-lexical-core-crate.md — (NEW) reconcilia com ADR-0009 (D1/D2)
```

#### Deep file dependency analysis
- Documento; sem downstream de código. M140.3 o cita (constrói sobre o núcleo).

#### Deep Dives
- Estrutura ADR: Contexto, Decisão (D1+D2), Alternativas, Consequências, relação com ADR-0009.
- Invariante: `cargo pgrx check --features spike-lexical` compila; `cargo build` (default) compila sem tantivy.
- Edge case: se `cargo pgrx` quebrar com o workspace (Q1), o ADR registra a estrutura final adotada.

#### Tasks
1. Escrever o ADR-0053.
2. `cargo pgrx check --features spike-lexical` (build pgrx com o núcleo consumido).
3. `cargo build` (default, shipado) verde.
4. Confirmar CI verde no push (self-hosted runner, M136).

#### TDD
```
RED:  cargo pgrx check --features spike-lexical falha se o consumo estiver errado
GREEN: build verde nas duas configs (spike + default)
REFACTOR: None expected
VERIFY: cd theodb_rs && cargo pgrx check --features spike-lexical && cargo build
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] ADR-0053 tem Decisão + ≥1 alternativa + Consequências + relação explícita com ADR-0009.
- [ ] `cargo pgrx check --features spike-lexical` exit code 0.
- [ ] `cargo build` (default, shipado) exit code 0 e sem tantivy no default.
- [ ] `python3 scripts/check_xrefs.py` retorna Overall PASS (ou o equivalente em `.claude/scripts/`).

#### DoD
- [ ] `cd theodb_rs && cargo pgrx check --features spike-lexical` exit code 0.
- [ ] `cd theodb_rs && cargo build` exit code 0.
- [ ] ADR-0053 escrito; `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Coverage Matrix

| # | Gap / Requirement (DoD ROADMAP M140.2) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Crate núcleo sem dependência de pgrx no Cargo.toml; testes puros rodam em cargo test | T1.1 | `theodb_lexical` (dep só tantivy), 6 testes via `cargo test -p theodb_lexical` |
| 2 | ADR-1 reconcilia com o ADR-0009 (por que o núcleo merece crate separado) | T3.1 | ADR-0053 (D1: outra camada, zero externs) |
| 3 | `theodb_rs` consome o crate núcleo atrás da feature; build shipado + CI verdes | T2.1, T3.1 | workspace + dep sob spike-lexical + imports; `cargo pgrx check` + `cargo build` verdes |

**Coverage: 3/3 gaps covered (100%)**

## Global Definition of Done

- [ ] Todas as fases completas.
- [ ] `cd theodb_rs && cargo test -p theodb_lexical` exit code 0 (6 testes do núcleo).
- [ ] `cargo tree -p theodb_lexical | grep -c pgrx` retorna 0 (o gate objetivo do pgrx-free).
- [ ] `cargo pgrx check --features spike-lexical` exit code 0 (o cdylib consome o núcleo).
- [ ] `cargo build` (default shipado) exit code 0, sem tantivy no default.
- [ ] `ruff`/lint — n/a (Rust; `cargo clippy` cobre, rodado no CI M136).
- [ ] File-size budget respeitado (núcleo ≤ 320 LoC; ADR ≤ 500).
- [ ] CHANGELOG.md atualizado sob `[Unreleased]`.
- [ ] Backward compatibility — os 6 testes byte-idênticos; `pg_backing.rs` compila.
- [ ] Plan-specific: CI (self-hosted runner M136) verde no push.
- [ ] Plan archived após merge.

## Failure scenarios (I/O external)

(none — no external I/O touched)

O milestone é refactor de estrutura de crate + move de código; não toca HTTP/DB/queue. O `cargo pgrx check` é
build-time, não runtime I/O.

## Final Phase: Integration Validation (MANDATORY)

**Objective:** validar que o núcleo testa isolado E o cdylib consome o núcleo, nas duas configs.

### Execution
```
cd theodb_rs
cargo test -p theodb_lexical                      # 6 testes do núcleo, stock cargo (sem pgrx)
cargo tree -p theodb_lexical | grep -c pgrx       # deve ser 0
cargo pgrx check --features spike-lexical          # o cdylib consome o núcleo (toolchain 0.19+PG18)
cargo build                                        # build default shipado (sem tantivy)
cargo clippy --features spike-lexical -- -D warnings   # gate M136
python3 ../.claude/scripts/check_xrefs.py 2>&1 | tail -3
```

### Acceptance Criteria
- [ ] `cargo test -p theodb_lexical` verde (6 testes).
- [ ] `cargo tree -p theodb_lexical` sem pgrx (grep -c = 0).
- [ ] `cargo pgrx check --features spike-lexical` exit 0.
- [ ] `cargo build` (default) exit 0.
- [ ] `cargo clippy` sem warnings (gate M136).
- [ ] CI verde no push.

### If Validation Fails
1. Separar falhas causadas pelo plano vs pré-existentes.
2. Se `cargo pgrx` quebrar com o workspace (Q1) → reverter o D2 para workspace no repo root via ADR-amenda.
3. Re-rodar a cadeia.
