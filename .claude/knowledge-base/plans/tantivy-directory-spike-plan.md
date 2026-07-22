---
slug: tantivy-directory-spike
milestone_id: M139
created_at: 2026-07-21
goal: Provar (ou refutar) por protótipo mínimo que um `Directory` do Tantivy sobre storage do Postgres indexa e busca, sobrevivendo a crash real.
---

# Plano — M139: spike do `Directory` do Tantivy sobre block storage do Postgres

## Goal

Entregar um protótipo que responde a pergunta-gate do spike, avançando pelos 4 gates do blueprint EM ORDEM, e
emitir um ADR **GO/NO-GO** ao fim. Métrica observável do gate 1 (o desta iteração): **um teste que indexa N
documentos num `Directory` custom e recupera o doc certo por uma busca de termo — sem tocar o filesystem**.

## Context

Consome `discoveries/blueprints/tantivy-directory-spike-blueprint.md` (veredito GO para o spike). O risco não é
o BM25 — é fazer o Tantivy viver no PG com MVCC+WAL+crash. Tantivy 0.26 é MIT (regra-de-licença-do-PRD-limpo). O molde regra-de-licença-do-PRD-limpo é o
lancedb/tantivy-object-store (Apache-2.0): um `Directory` não-fs que stuba `watch`/`lock` sob single-writer e
versiona para atomicidade. A parte cara (além do object-store) é a integração transacional PG — que entra por gates.

## Baseline Context

### Files that will be touched

| Arquivo | LoC (medido) | Papel |
|---|---|---|
| `theodb_rs/Cargo.toml` | ~90 | adicionar `tantivy = "0.26"` (MIT, regra-de-licença-do-PRD-limpo — gate do deny.toml) |
| `theodb_rs/src/lexical/mod.rs` | — | **NEW** — módulo do spike (isolado; não toca a superfície shipada) |
| `theodb_rs/src/lexical/pg_directory.rs` | — | **NEW** — o `Directory` custom |
| `theodb_rs/src/am/hnsw_page.rs` | 1502 | **precedente** de page-native storage + WAL (lido, não editado) |
| `theodb_rs/isolation/` | — | **precedente** dos harnesses crash*.sh (molde do gate 3) |

### Current callers / dependents

- Nenhum — o spike é um módulo isolado (`src/lexical/`), sem caller na superfície `ai.*`/`theodb.*` shipada.
  Isso é deliberado: um spike não altera produto até o veredito GO (então o M140 wira).

### Architecture boundaries affected

Nenhuma na superfície shipada. O módulo `lexical` é novo e auto-contido. Se o gate 1 exigir só um backend
in-memory/blob (não páginas PG ainda), isso é aceitável para o gate 1 e declarado — os gates 2/3 sobem para
páginas PG + WAL.

## Prior Art & Related Work

- Blueprint `tantivy-directory-spike-blueprint.md` (contrato do `Directory`, lancedb, ParadeDB AGPL study-only).
- Precedente próprio: `am/hnsw_page.rs` (page-native + WAL M35), `isolation/crash*.sh` (#46/#47), `am/columnar.rs` (MVCC-via-catalog M99).

## ADRs

### ADR-1 — Protótipo por gates incrementais, em ordem; crash-real é obrigatório

**Decisão:** avançar os 4 gates do DoD EM ORDEM (indexa+busca → MVCC → crash-real → custo), cada um um passo
verificável. O gate 1 (esta iteração) pode usar um backend de storage mínimo (blob em memória/página) desde que
**não toque o filesystem** e prove o caminho Tantivy-sobre-Directory-custom.
**Alternativa rejeitada:** ir direto ao Directory-sobre-páginas-PG-com-WAL (gate 3) — esconde o risco de
integração do Tantivy (nested tokio runtime, contrato do trait) atrás de um build gigante. Incremental de-risca.

### ADR-2 — Tantivy stock (MIT), sem fork até o protótipo doer

**Decisão:** usar o Tantivy 0.26 upstream. Só forkar se um gate provar que o upstream não basta dentro do PG (Política-de-Fork-do-PRD).
**Alternativa rejeitada:** copiar o `pg_search` do ParadeDB — AGPL (regra-de-licença-do-PRD barra), 105k LoC. Estudar, não copiar.

## Dependencies

| Dep | Versão | Regra 9 / regra-de-licença-do-PRD |
|---|---|---|
| `tantivy` | =0.26.0 | MIT, regra-de-licença-do-PRD-limpo (verificado no Cargo.toml upstream); o `deny.toml` (M136) barra AGPL transitiva automaticamente |

## Phase 1 — Gate 1: `Directory` custom indexa + busca (sem filesystem)

### T1.1 — Adicionar Tantivy e provar o build sob o deny.toml (regra-de-licença-do-PRD)

#### Why this step

Antes de qualquer código, provar que a dep entra limpa: build compila E o `cargo deny check licenses` (gate do
M136) passa com o Tantivy + sua árvore transitiva. Se algo transitivo for AGPL, o gate barra aqui — a hora certa.

#### TDD
```
RED: cargo deny check licenses FALHA se houver AGPL transitiva do tantivy
GREEN: cargo add tantivy@0.26; cargo deny check licenses → "licenses ok"; cargo build --features pg18 → exit 0
```

#### Files to edit
- `theodb_rs/Cargo.toml`

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- `cargo deny check licenses` retorna exit 0 ("licenses ok") com o tantivy adicionado.
- `cargo build --features pg18` retorna exit 0.

#### DoD
- Build verde + regra-de-licença-do-PRD verde no droplet.

### T1.2 — `PgDirectory` mínimo + teste que indexa e busca

#### Why this step

É o gate 1 do spike: provar que o Tantivy roda sobre um `Directory` NOSSO (não `MmapDirectory`) — indexando N
docs e recuperando o certo por busca de termo, **sem tocar o filesystem**. Segue o molde do lancedb (stub de
`watch`/`lock` sob single-writer; `atomic_write` por versionamento; delete no-op). É o de-risk do contrato do trait
e do risco nested-tokio-runtime antes de subir para páginas PG (gates 2/3).

#### TDD
```
RED: test_pg_directory_indexes_and_searches
     Given um PgDirectory custom (backend de blob, SEM filesystem) e 3 docs {id, body}
     When  indexa via IndexWriter e busca o termo 'lazy'
     Then  o doc que contém 'lazy' é recuperado (top-1), e nenhum arquivo é criado no fs
```

#### Files to edit
- `theodb_rs/src/lexical/mod.rs` (NEW)
- `theodb_rs/src/lexical/pg_directory.rs` (NEW — impl do trait `Directory`)

#### Concurrency tests
(none — single-writer por design; watch/lock stubados como no lancedb)

#### Acceptance criteria
- `cargo test --features pg18 test_pg_directory_indexes_and_searches` retorna exit 0.
- O teste assevera que o doc com 'lazy' é o top-1 e que o storage é o `PgDirectory` (não fs) — ex.: um contador
  de bytes no backend > 0 e nenhuma escrita em disco.

#### DoD
- Teste verde no droplet (sob a rede cassert do M136).

## Failure scenarios

| Cenário | Reprodução | Esperado |
|---|---|---|
| Tantivy embute `tokio::Runtime` (nested-runtime panic dentro do PG) | rodar o índice num contexto síncrono | detectar cedo; usar backend síncrono / `spawn_blocking` (R3 do blueprint) |
| dep transitiva AGPL | `cargo deny check` | gate barra (regra-de-licença-do-PRD) |

## Coverage Matrix

| Afirmação do Goal | Tarefa |
|---|---|
| Directory custom indexa + busca sem filesystem | T1.2 |
| Tantivy entra regra-de-licença-do-PRD-limpo | T1.1 |
| (gates 2 MVCC, 3 crash-real, 4 custo) | **fora desta iteração — próximas fases do spike, declaradas** |

100% das afirmações do gate 1 mapeadas. Os gates 2–4 são fases seguintes do spike (não deste passo).

## Drawbacks & Risks

| # | Risco | Sev | Mitigação |
|---|---|---|---|
| R1 | Spike de semanas — esta iteração fecha só o gate 1 | ALTA | honestidade a cada gate; o gate 1 é o de-risk decisivo do contrato Tantivy↔Directory |
| R2 | nested tokio runtime dentro do PG | MÉDIA | backend síncrono; detectar no gate 1 |
| R3 | upstream Tantivy pode não bastar dentro do PG (ParadeDB forka) | ALTA | gate revela; Política-de-Fork-do-PRD fork condicional |

## Unresolved Questions

- Q1 — O gate 1 usa backend de blob (não páginas PG ainda). Subir para páginas PG + WAL é o gate 3; declarado.
- Q2 — `cargo pgrx test` no droplet historicamente teve problema de símbolos PG; o teste pode precisar rodar como
  teste Rust puro (o `PgDirectory` do gate 1 é pgrx-free por design, testável com `cargo test` stock — padrão M134/egress).

## Global DoD

- [ ] `tantivy = "0.26"` adicionado; `cargo deny check licenses` verde (regra-de-licença-do-PRD).
- [ ] `cargo build --features pg18` verde.
- [ ] `test_pg_directory_indexes_and_searches` verde — indexa + busca sem filesystem.
- [ ] CHANGELOG `[Unreleased]` atualizado.
- [ ] Honesto: esta iteração fecha o gate 1 do spike; gates 2–4 (MVCC, crash-real, custo) são as próximas fases.
