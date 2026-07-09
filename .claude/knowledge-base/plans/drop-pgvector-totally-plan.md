---
slug: drop-pgvector-totally
milestone_id: M70
created_at: 2026-07-09
goal: Remover o pgvector totalmente movendo o tipo próprio para public.vector, provado pelos 55 AM pg_tests set-equal GREEN sem pgvector instalado.
---

# Plano de Implementação: Remover o pgvector totalmente (M70)

## Goal

Remover o pgvector (e pgvectorscale) totalmente — mover o tipo próprio de `theodb.vector` para `public.vector` (drop-in), flipar a dependência (`theodb_rs` vira base), e provar pelos **55 pg_tests `set-equal-vs-seqscan` do AM GREEN sobre o tipo próprio, com `CREATE EXTENSION theodb_rs` SEM pgvector instalado**.

- **Métrica observável:** os 55 pg_tests do AM (`hnsw_page.rs`) passam 100% GREEN em pg17 real com a extensão `theodb_rs` criada **sem** o pgvector (nem vectorscale) — o tipo `public.vector` é 100% own-code.

## Context

Fecha o roadmap v4 e o ROADMAP_COMPLETED. O **M69** (v0.59.0) entregou o tipo `theodb.vector` own-code byte-idêntico ao pgvector, coexistindo. O **M70** remove o pgvector: move o tipo para `public.vector` (o drop-in do veredito A do blueprint SHIPPABLE), resolve a dependência circular via **flip**, e remove o pgvector/pgvectorscale da distribuição.

Decisões resolvidas a montante:
- **Blueprint SHIPPABLE 99.7** (`own-vector-type-drop-pgvector-blueprint.md`) — veredito A + a receita de migração (Corner 2).
- **M69 já provou** o layout byte-idêntico + a coexistência + o cast binário `WITHOUT FUNCTION` (`docs/adr/0028`, `theodb_rs/src/dtype.rs`).
- **Dependência circular (ADR-D1 abaixo):** o umbrella `theodb` usa o tipo `vector`; o `theodb_rs` provê o tipo próprio mas hoje requer `theodb`. O pgvector era o 3º que quebrava o ciclo. FLIP: `theodb_rs` vira base.
- **Licença (D1, INQUEBRÁVEL):** código ORIGINAL; VectorChord AGPL (`[[vectorchord-agpl-study-only]]`) só estudo.

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Último commit | Papel / por que |
|---|---|---|---|
| `theodb_rs/src/dtype.rs` | 583 | 651925b | O tipo (M69). Mudar `theodb.vector`→`vector` (public); `SqlMapping::As("vector")`; DDL casts/operadores; `CREATE SCHEMA IF NOT EXISTS theodb`; reworkar o teste de coexistência para migração. |
| `theodb_rs/theodb_rs.control` | — | 2b436df | `requires = 'vector, theodb'` → `requires` VAZIO (o tipo/schema passam a ser próprios). |
| `theodb.control` (umbrella) | — | 2b436df | `requires = 'vector, vectorscale'` → `requires = 'theodb_rs'` (flip — precisa do tipo+schema do theodb_rs). |
| `Dockerfile` | ~150 | — | Remover stage pgvectorscale (`:10-26`), pgvector `ADD`+install (`:70-81`), `COPY vectorscale*` (`:87-89`), CASCADE (`:124,137,140,145`). **pg_duckdb INTOCADO** (`:50-64,147-149`). |
| `docs/ops/pgvector-migration.md` | (NEW) | — | Playbook de migração de tabelas existentes (cast binário + ALTER COLUMN + REINDEX). |

### Current callers / dependents

- **O tipo `vector`** — usado por: as opclasses do AM (`theodb_rs/src/am/mod.rs:253-284`, `FOR TYPE vector` — 6×), os ~44 `::vector` em `hybrid.rs`/`embed.rs`/`vectorizer.rs`/`pq.rs`/`api.rs`/`sbq.rs`, e as funções SQL do umbrella (`sql/30-embed`, `sql/40-hybrid`). **Todos referenciam o nome `vector`** — ao mover o tipo próprio para `public.vector`, resolvem AUTOMATICAMENTE ao tipo próprio (mudança de código mínima; só o schema do tipo muda). Confirmado: `am/mod.rs` tem 6 `FOR TYPE vector`.
- **O schema `theodb`** — criado hoje pelo umbrella (`sql/30-theodb-embed.sql:16`). Após o flip, criado por `theodb_rs` (bootstrap).
- **GUCs `theodb.*`** (`theodb.llm_endpoint`) — session-GUCs livres lidos em runtime (`chat.rs:136` via `guc(...)`), NÃO registrados — o theodb_rs não depende do umbrella para eles.

### Domain glossary

- **binary-coercible cast** — `CREATE CAST ... WITHOUT FUNCTION`: reinterpreta bytes sem função (layout idêntico).
- **opclass rebind** — trocar o `FOR TYPE` de uma operator class do AM.
- **dependency flip** — inverter a direção `requires` entre `theodb_rs` e o umbrella `theodb`.
- **set-equal-vs-seqscan** — o gate de recall do AM: o top-k via índice == o top-k exato (seqscan).

### Architecture boundaries affected

Per `.claude/rules/architecture.md § 1-2` (composition root, dependency direction): o **flip** inverte a fronteira `theodb_rs ↔ theodb` — `theodb_rs` passa a ser a base (provê tipo+AM+schema), o umbrella `theodb` a depender dele. É uma mudança de fronteira LEGÍTIMA (o tipo own-code em Rust é o alicerce; o umbrella SQL é a camada de cima) — registrada no ADR-D1. Budget: `dtype.rs` já está em ~425 LoC prod (M70 não adiciona muito).

## Prior Art & Related Work

- **Blueprint** `own-vector-type-drop-pgvector-blueprint.md` (Corner 2 — migração; Corner 6 — o que remover).
- **M69** `docs/adr/0028` + `theodb_rs/src/dtype.rs` — o tipo + o cast binário byte-idêntico (a base da migração).
- **pgvector** (`.claude/knowledge-base/references/pgvector/sql/vector.sql`) — precedente de que o layout nunca mudou (migração binária segura).
- Postgres docs (ALTER TABLE binary coercibility, CREATE CAST WITHOUT FUNCTION) — o mecanismo de migração grátis.

## ADRs

### D1 — Flip da dependência: `theodb_rs` vira a base (provê o tipo + schema); o umbrella requer `theodb_rs`

**Decisão:** `theodb_rs.control requires` ZERA; `theodb_rs` cria `CREATE SCHEMA IF NOT EXISTS theodb` (bootstrap) + provê `public.vector` + o AM. O umbrella `theodb.control requires` vira `theodb_rs`.

**Rationale:** o tipo own-code (Rust, com I/O no `.so` do theodb_rs) SÓ pode ser criado pelo theodb_rs (o umbrella é SQL puro, sem `.so`). Como o umbrella USA o tipo, ele deve depender do theodb_rs. Removido o pgvector (que quebrava o ciclo por ser um 3º que ambos requeriam), o flip é a única topologia sem ciclo. **Alternativa rejeitada:** tipo no umbrella — impossível (I/O funcs vivem no `.so` do theodb_rs). **Alternativa rejeitada:** manter `theodb_rs requires theodb` + tipo no theodb_rs — ciclo (umbrella usa tipo do theodb_rs, theodb_rs requer umbrella). Cita `architecture.md § 2` (direção de dependência).

### D2 — Tipo próprio em `public.vector` (não `theodb.vector`) — o drop-in do veredito A

**Decisão:** o tipo é `public.vector` (schema public, nome `vector`). Como o pgvector é removido, não há colisão. `::vector` do usuário e o `FOR TYPE vector` das opclasses resolvem ao tipo próprio SEM mudança.

**Rationale:** o veredito A do blueprint quer `::vector` drop-in. Com o pgvector fora, `public.vector` own é o alvo. **Alternativa rejeitada:** manter `theodb.vector` namespaced — quebra `::vector` (não drop-in), contra o veredito A. Precedente: a transição de naming planejada no M69 (public.vector no M70).

### D3 — Migração de instalações existentes via cast binário (não reescrita)

**Decisão:** o upgrade de uma instalação com colunas `vector` (do pgvector) migra via `CREATE CAST (old_vector AS public.vector) WITHOUT FUNCTION` + `ALTER TABLE ... ALTER COLUMN TYPE` (cast binário grátis, byte-idêntico M69) + REINDEX dos índices ANN. Documentado em `docs/ops/pgvector-migration.md`.

**Rationale:** o layout byte-idêntico (provado M69) torna a migração de dados grátis (sem reescrita de heap). **Alternativa rejeitada:** dump/restore — downtime + reescrita O(N) desnecessária. Cita blueprint Corner 2.

### D4 — Gate de não-regressão: os 55 AM pg_tests set-equal sobre o tipo próprio, SEM pgvector

**Decisão:** a prova de que o tipo próprio substitui 100% o pgvector é os 55 pg_tests `set-equal-vs-seqscan` do AM (`hnsw_page.rs`) GREEN com `CREATE EXTENSION theodb_rs` SEM pgvector/vectorscale.

**Rationale:** o set-equal (top-k índice == top-k exato) é o gate executável de recall que já existe; rodá-lo sobre `public.vector` own prova a substituição sem regressão. **Alternativa rejeitada:** benchmark de performance — M70 é correção; a não-regressão de recall é o dado. Cita blueprint Corner 1/5.

## Dependency Graph

```
Fase 1 (tipo → public.vector, dtype.rs) ──▶ Fase 2 (flip requires + schema)
        │                                            │
        └──────────────────┬─────────────────────────┘
                           ▼
              Fase 3 (Dockerfile — remover pgvector/vectorscale)
                           ▼
              Fase 4 (migração documentada + testada)
                           ▼
              Fase 5 (GATE: 55 AM pg_tests set-equal, sem pgvector) ──▶ Fase 6 (integration validation)
```

Fase 1 é a fundação (o tipo em public). Fase 2 (flip) depende de 1. Fase 3 (Dockerfile) depende de 2 (requires). Fase 5 é o gate crítico.

## Phase 1 — Mover o tipo próprio para `public.vector`

### T1.1 — `dtype.rs`: `theodb.vector` → `vector` (public) + schema bootstrap

#### TDD
- RED: `#[pg_test] fn own_vector_in_public` — `SELECT '[1,2,3]'::vector::text` == `[1,2,3]` com `CREATE EXTENSION theodb_rs` (sem pgvector) — o tipo `vector` é o próprio.
- RED: `#[pg_test] fn own_vector_operators` — `'[0,0]'::vector <-> '[3,4]'::vector` == 5 (operadores sobre o tipo próprio public).
- GWT: Given theodb_rs criado sem pgvector, When `::vector`, Then resolve ao tipo próprio; operadores/casts funcionam.
- GREEN: em `dtype.rs`: `CREATE TYPE theodb.vector`→`CREATE TYPE vector`; `SqlMapping::As("theodb.vector")`→`SqlMapping::As("vector")`; casts/operadores DDL `theodb.vector`→`vector`; adicionar `CREATE SCHEMA IF NOT EXISTS theodb` no bloco bootstrap; reworkar `binary_compat_with_pgvector` → `migration_binary_compat` (gated: só roda se pgvector presente, senão SKIP — a paridade byte já foi provada em M69).

#### Files to edit
- `theodb_rs/src/dtype.rs` — o rename de schema + o schema bootstrap.

#### Deep file dependency analysis
`dtype.rs` tem 110 refs a "theodb.vector" (muitas em comentário); as FUNCIONAIS são `SqlMapping::As` (2), `CREATE TYPE`, as DDL de cast/operador, e os testes. O rename `theodb.vector`→`vector` nos contextos SQL/SqlMapping. Os callers (opclasses `am/mod.rs`, ~44 `::vector`) já usam `vector` — não mudam.

#### Why this step
**Ação:** mover o tipo para public (o drop-in). **Raciocínio:** o veredito A quer `::vector` resolvendo ao tipo próprio; com o pgvector saindo, `public.vector` own é o alvo (D2). Cita a transição de naming planejada no M69 (public.vector no M70).

#### Concurrency tests
(none — single-threaded). Manipulação de varlena in-memory, sem locks/atomics/threads.

#### Acceptance criteria
- `CREATE EXTENSION theodb_rs` (sem pgvector) cria `public.vector`; `'[1,2,3]'::vector::text` == `[1,2,3]`; operadores `<->`/`<#>`/`<=>` funcionam.
- `grep -c "theodb.vector" theodb_rs/src/dtype.rs` só em comentários/docs (nenhuma DDL/SqlMapping funcional).

#### DoD
- `cargo pgrx test pg17 own_vector` GREEN (droplet, sem pgvector no CASCADE).
- `git commit` `feat(m70): T1.1 tipo proprio em public.vector + schema bootstrap`.

## Phase 2 — Flip da dependência

### T2.1 — `theodb_rs.control` zera requires; umbrella requer `theodb_rs`

#### TDD
- RED: `#[pg_test] fn extension_loads_standalone` — `CREATE EXTENSION theodb_rs` sozinho (sem CASCADE) cria o tipo + o AM + o schema `theodb`, e um índice `USING theodb_hnsw` sobre `vector` funciona.
- GREEN: `theodb_rs.control`: `requires = 'vector, theodb'` → remover a linha (ou `requires = ''`); `theodb.control`: `requires = 'vector, vectorscale'` → `requires = 'theodb_rs'`.

#### Files to edit
- `theodb_rs/theodb_rs.control`, `theodb.control`.

#### Deep file dependency analysis
O umbrella (`sql/*.sql`) usa o tipo `vector` + o schema `theodb` — ambos agora do theodb_rs. `CREATE EXTENSION theodb CASCADE` cria theodb_rs primeiro. O theodb_rs não usa nada do umbrella em tempo de CREATE (os GUCs são runtime).

#### Why this step
**Ação:** flipar a direção da dependência. **Raciocínio:** o tipo own-code vive no `.so` do theodb_rs; o umbrella que o usa deve depender dele (D1). Removido o pgvector, é a única topologia acíclica. Cita `architecture.md § 2`.

#### Concurrency tests
(none — single-threaded). Só metadados de extensão (`.control`).

#### Acceptance criteria
- `CREATE EXTENSION theodb_rs` sozinho carrega (tipo+AM+schema); `CREATE EXTENSION theodb CASCADE` cria theodb_rs→theodb sem pgvector/vectorscale.
- Nenhum `requires` menciona `vector` ou `vectorscale`.

#### DoD
- `cargo pgrx test pg17 extension_loads_standalone` GREEN.
- `git commit` `feat(m70): T2.1 flip da dependencia (theodb_rs base)`.

## Phase 3 — Remover pgvector + pgvectorscale do Dockerfile

### T3.1 — Dockerfile sem pgvector/pgvectorscale (pg_duckdb intocado)

#### TDD
- RED (verificação, não pg_test): `grep -c "pgvector\|pgvectorscale\|vectorscale" Dockerfile` (fora de comentários históricos) == 0 para os stages de build/install; `grep -c "pg_duckdb" Dockerfile` inalterado.
- GREEN: remover o stage 1 pgvectorscale (`:10-26`), o `ADD pgvector.git`+install (`:70-81`), os `COPY vectorscale*` (`:87-89`), os CASCADE que puxam vector/vectorscale (`:124,137,140,145`). Mover diskann para benchmark-only (doc). NÃO tocar pg_duckdb (`:50-64,147-149`).

#### Files to edit
- `Dockerfile`.

#### Deep file dependency analysis
O Dockerfile monta a imagem de distribuição. Remover pgvector/pgvectorscale reduz a imagem e as dependências. O `theodb_rs` agora provê o tipo. pg_duckdb (columnar, MIT) é independente do pilar vetorial — intocado.

#### Why this step
**Ação:** tirar o pgvector/pgvectorscale do produto. **Raciocínio:** o objetivo do M70 é "remover totalmente"; o Dockerfile é a distribuição. Cita blueprint Corner 6 + o pedido do usuário.

#### Concurrency tests
(none — single-threaded). Build declarativo.

#### Acceptance criteria
- Dockerfile sem stage/install de pgvector/pgvectorscale; pg_duckdb intacto; `CREATE EXTENSION theodb_rs` (+`theodb` CASCADE) na imagem sem CASCADE de terceiros.

#### DoD
- `grep` confirma remoção; build da imagem (ou dry-run do Dockerfile) sem erro de referência.
- `git commit` `feat(m70): T3.1 Dockerfile sem pgvector/pgvectorscale`.

## Phase 4 — Migração de tabelas existentes (documentada + testada)

### T4.1 — Playbook de migração + teste do cast binário

#### TDD
- RED: `#[pg_test] fn migration_binary_cast` — (gated se pgvector presente) cria uma coluna do tipo antigo, `CREATE CAST (old AS public.vector) WITHOUT FUNCTION` + `ALTER COLUMN TYPE`, e assere que os dados sobrevivem byte-idênticos (reusa a prova M69).
- GREEN: escrever `docs/ops/pgvector-migration.md` — passos: (1) instalar theodb_rs; (2) `CREATE CAST ... WITHOUT FUNCTION`; (3) `ALTER TABLE ... ALTER COLUMN emb TYPE public.vector USING emb::public.vector`; (4) REINDEX dos índices ANN; (5) `DROP EXTENSION vector`.

#### Files to edit
- `docs/ops/pgvector-migration.md` (NEW); `theodb_rs/src/dtype.rs` (o teste gated).

#### Deep file dependency analysis
A migração usa o cast binário (byte-idêntico M69). O REINDEX é necessário (as opfamilies mudam de pgvector p/ theodb_rs). Documentado; o teste prova o cast num ambiente com pgvector.

#### Why this step
**Ação:** dar o caminho de upgrade sem perda/downtime. **Raciocínio:** usuários existentes têm colunas `vector` do pgvector; o layout byte-idêntico permite migração grátis (D3). Cita blueprint Corner 2 + Postgres binary-coercibility.

#### Concurrency tests
(none — single-threaded). DDL de migração.

#### Failure scenarios
Ver `## Failure scenarios` — a migração toca DDL (ALTER/REINDEX), não I/O externo.

#### Acceptance criteria
- `docs/ops/pgvector-migration.md` com os 5 passos + o caveat do REINDEX; o teste `migration_binary_cast` GREEN (ou SKIP honesto se pgvector ausente no ambiente).

#### DoD
- `cargo pgrx test pg17 migration` GREEN/SKIP; doc existe.
- `git commit` `docs(m70): T4.1 playbook de migracao pgvector->own`.

## Phase 5 — GATE de não-regressão de recall (crítico)

### T5.1 — 55 AM pg_tests set-equal GREEN sobre `public.vector`, sem pgvector

#### TDD
- RED/GREEN: rodar `cargo pgrx test pg17` (a suíte completa do AM — `hnsw_page.rs` 55 pg_tests + `ann`/`sbq`/`ivfflat`) com `CREATE EXTENSION theodb_rs` SEM pgvector/vectorscale. Os testes criam `vector(N)` columns (agora o tipo próprio), buildam `theodb_hnsw`/`theodb_ivfflat`, e o **set-equal-vs-seqscan** (top-k índice == top-k exato) DEVE passar 100%.
- Se algum falhar: o tipo próprio diverge do pgvector em algum caminho que o AM usa (ex. `::real[]` cast, o `<->` operador) — corrigir em `dtype.rs`.

#### Files to edit
- (nenhum código novo esperado — é o gate; correções em `dtype.rs`/`am/mod.rs` se um teste revelar divergência).

#### Deep file dependency analysis
O AM lê colunas via `::real[]` (`ann_query.rs:71`) e ordena via `<->` — ambos existem no tipo próprio (M69). Os 55 pg_tests de `hnsw_page.rs` são o gate. Referência: `theodb_rs/src/am/hnsw_page.rs` (o set-equal em `:2201-2214` do M68).

#### Why this step
**Ação:** provar que o tipo próprio substitui 100% o pgvector no AM. **Raciocínio:** o set-equal é o gate de recall executável; GREEN sobre `public.vector` sem pgvector É a prova de "remoção total sem regressão" (D4). Cita blueprint Corner 1/5.

#### Concurrency tests
(none — single-threaded). O AM tem sua própria suíte de concorrência já validada em M48/M56, fora do escopo do M70.

#### Acceptance criteria
- `cargo pgrx test pg17` (suíte AM completa) 100% GREEN com theodb_rs SEM pgvector — os 55 `hnsw_page` set-equal + `ann`/`sbq`/`ivfflat`.
- `SELECT count(*) FROM pg_extension WHERE extname IN ('vector','vectorscale')` == 0 no ambiente de teste.

#### DoD
- Droplet: `cargo pgrx test pg17` GREEN sem pgvector instalado.
- `git commit` `test(m70): T5.1 gate 55 AM set-equal GREEN sobre tipo proprio`.

## Phase 6 — Integration Validation

### T6.1 — ADR + CHANGELOG + ROADMAP + validação final

#### TDD
- A suíte COMPLETA (`cargo pgrx test pg17`) 100% GREEN com theodb_rs standalone (sem pgvector/vectorscale) — o tipo + o AM + os testes de I/O + o AM set-equal.

#### Files to edit
- `docs/adr/00NN-m70-drop-pgvector.md` (NEW); `CHANGELOG.md`; `ROADMAP.md` (flip M70 [x]).

#### Deep file dependency analysis
O ADR registra D1-D4. O CHANGELOG é o contrato público. Nenhum código de produção novo.

#### Why this step
**Ação:** provar o milestone inteiro + fechar rastreabilidade. **Raciocínio:** "eat your own cooking" — a suíte GREEN sem pgvector prova a remoção total. Cita `cycle-plan.md`.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- Suíte completa GREEN sem pgvector; ADR novo; CHANGELOG + ROADMAP M70 [x]; `/code-quality` ∈ {PASS, PASS_WITH_CAVEATS}.

#### DoD
- Droplet: `cargo pgrx test pg17` GREEN sem pgvector.
- `git commit` `feat(m70): T6.1 integration validation + ADR + CHANGELOG`.

## Coverage Matrix

| Requisito (escopo M70) | Task(s) | Status |
|---|---|---|
| (1) tipo próprio → `public.vector` (drop-in) | T1.1 | Covered |
| (2) resolver dependência circular (flip) | T2.1 | Covered |
| (3) opclasses/`::vector` resolvem ao próprio | T1.1, T5.1 (verificado pelo gate) | Covered |
| (4) remover pgvector+pgvectorscale (Dockerfile) | T3.1 | Covered |
| (5) migração de tabelas existentes | T4.1 | Covered |
| (6) gate 55 AM set-equal sem pgvector | T5.1 | Covered |
| (7) ALTER TYPE SET SCHEMA (upgrade path) | T4.1 (no playbook de migração) | Covered |

**Coverage: 7/7 requisitos mapeados (100%)**

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Regressão de recall no AM ao trocar o tipo do pgvector pelo próprio (o P0) | ALTA | O gate T5.1 (55 set-equal-vs-seqscan) roda sobre o tipo próprio SEM pgvector — qualquer divergência de recall falha ANTES do merge. Rollback via a coexistência do M69 (o tipo próprio já provado byte-idêntico). | impl |
| O flip da dependência quebra o `CREATE EXTENSION` (ordem/ciclo) | MÉDIA | T2.1 tem o teste `extension_loads_standalone` + `CASCADE`; o schema/GUCs são próprios do theodb_rs (não dependem do umbrella). Validado no droplet. | impl |
| Algum `::vector` ou opclass NÃO resolve ao tipo próprio (ex. schema-qualified) | MÉDIA | T5.1 (gate AM) + verificação caso-a-caso dos ~44 `::vector`; o tipo em `public` resolve unqualified. | impl |
| Migração de tabela existente com índice ANN perde o índice (REINDEX) | BAIXA | Documentado no playbook (D3) — o REINDEX é esperado (opfamilies mudam); os dados sobrevivem via cast binário. | impl |

## Unresolved Questions

- **Os testes do AM referenciam funções do umbrella (`theodb.*`)?** Se sim, `CREATE EXTENSION theodb_rs` sozinho não basta para eles (precisariam do umbrella). T5.1 resolve empiricamente: se um teste do AM falhar por função ausente do umbrella, ou o teste é auto-contido (só tipo+AM) ou instala o umbrella também. A hipótese (baseada em `hnsw_page.rs` usar só `vector`+`theodb_hnsw`) é que são auto-contidos. Não bloqueia o design.
- Fora isso: (none — as demais decisões estão resolvidas pelo blueprint + M69).

## Failure scenarios

(none — no external I/O touched). O M70 é DDL (tipo, casts, opclasses, extensão) + Dockerfile + migração — nenhum HTTP/DB-driver/queue/socket novo. A migração (`ALTER`/`REINDEX`) é DDL local, coberta pelo teste do cast binário.

## Global Definition of Done

- [ ] Todas as tasks T1.1–T6.1 `committed`.
- [ ] **`cargo pgrx test pg17` 100% GREEN com `theodb_rs` SEM pgvector/vectorscale** (a métrica do Goal) — 55 AM set-equal + tipo + I/O.
- [ ] `SELECT count(*) FROM pg_extension WHERE extname IN ('vector','vectorscale')` == 0 no ambiente de teste (prova de remoção total).
- [ ] Dockerfile sem pgvector/pgvectorscale; pg_duckdb intacto (verificável: `grep`).
- [ ] Playbook de migração (`docs/ops/pgvector-migration.md`) com os passos + REINDEX.
- [ ] Código ORIGINAL (licença D1); ADR novo; CHANGELOG + ROADMAP M70 [x] (Regra 6).
- [ ] `/code-quality` ∈ {PASS, PASS_WITH_CAVEATS}.

## Final Phase: Integration Validation

A Fase 6 (T6.1) É a integration validation: a suíte completa em pg17 real com `theodb_rs` standalone (SEM pgvector). O plano NÃO está completo até: (a) `cargo pgrx test pg17` 100% GREEN sem pgvector, (b) os 55 AM set-equal GREEN sobre o tipo próprio, (c) pg_extension sem vector/vectorscale, (d) `/code-quality` PASS. Se qualquer um falhar, o plano falhou.
