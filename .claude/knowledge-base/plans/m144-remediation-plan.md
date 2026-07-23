---
slug: m144-remediation
milestone_id: M144
created_at: 2026-07-23
goal: Fechar os 3 HIGH + 4 MEDIUMs P1 do code-review de theodb_rs sob TDD, cada fix provado por um teste de regressão que falha antes e passa depois.
---

# Plan — M144 Remediação P0+P1 do code-review

## Goal

Fechar os 3 HIGH + 4 MEDIUMs P1 do audit loop-code-review de `theodb_rs` (2026-07-23) sob TDD, **measured by** `cargo pgrx test` (crate `theodb_rs`, feature `pg_test`) retornando 0 falhas com ≥ 7 testes de regressão novos (cada RED antes do fix) **e** `FROM_VER=1.1.0 TO_VER=1.2.0 scripts/test-upgrade.sh` imprimindo `SCENARIO_A_OK`/`CONVERGENCIA_OK`/`IDEMPOTENCIA_OK` no droplet.

## Context

Origem: `.claude/knowledge-base/audits/theodb-rs-code-review-2026-07-23.md` (100 findings, 0 CRITICAL, 90/90 arquivos). Blueprint de discovery (SHIPPABLE_WITH_CAVEATS 89): `.claude/knowledge-base/discoveries/blueprints/m144-remediation-blueprint.md` — resolveu, com citação a peers PG maduros, o padrão de cada fix: upgrade **delta-only** (pgvector/pg_trgm), **gate-out** de spike da superfície shipada (Cargo feature — padrão do repo com `spike-lexical`), **propagação de erro** para o dead-letter M122. Grill: `.claude/knowledge-base/grills/review-findings-remediation-feature-grill.md`. Edge-case review: `.claude/knowledge-base/reviews/m144-remediation-edge-cases-2026-07-23.md`. Milestone: ROADMAP M144 (gated M143 `[x]`). Cumpre `rules/error-handling.md` (fail-fast typed — o fix C é a materialização exata), `rules/parsimony-ladder.md` rung 4 (reusar M122, zero dep), `rules/testing.md` § 4.1 (edge + negative), `rules/git-safety.md` (develop).

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Último commit | Papel | Fix |
|---|---|---|---|---|
| `theodb_rs/src/vectorizer.rs` | 1633 | `c6025d3` 2026-07-21 | fila crash-safe + bgworker + delete/upsert | T1.3, T2.2, T2.3 |
| `theodb_rs/src/bench_symqg.rs` | 105 | `c6025d3` 2026-07-21 | spike SymphonyQG (`std::fs::read`) | T1.2 |
| `theodb_rs/src/parquet.rs` | 329 | `c897d12` 2026-07-22 | superfície lakehouse M143 | T1.1 (fonte dos CREATEs) |
| `theodb_rs/src/am/columnar.rs` | 1897 | `c6025d3` 2026-07-21 | TableAM colunar + xact callback | T2.1 |
| `theodb_rs/src/graph.rs` | 1103 | `c6025d3` 2026-07-21 | CSR builder (cast `as u32`) | T2.4 |
| `theodb_rs/theodb_rs.control` | 14 | `3cdff45` 2026-07-21 | `default_version = '1.1.0'` | T1.1 (bump 1.2.0) |
| `theodb_rs/sql/theodb_rs--1.1.0--1.2.0.sql` | (NEW) | — | delta-only upgrade | T1.1 |
| `theodb_rs/Cargo.toml` | — | — | `[features]` (já gateia `spike-lexical`) | T1.2 |

### Current callers / dependents

- `_vectorizer_process_delete` (`vectorizer.rs:455`): chamado pelo worker via SPI em `vectorizer.rs:899`; testes happy-path existentes em `:1337` e `:1405`. O subtxn do worker em `:896-905` converte `Err` do SPI → `_vectorizer_mark_failed` (`:276`) → dead-letter (`:633`). Verificado 2026-07-23: `:460` e `:469` ainda têm `let _ =`.
- `symqg_spike_bench` (`bench_symqg.rs:47-48`): `#[pg_extern]`; criado no SQL shipado (`theodb_rs--1.0.0--1.1.0.sql:340`); loop de REVOKE em `:1105-1112` cobre só `^_vectorizer_`. Nenhum caller de produção.
- `read_parquet`/`write_parquet`/`olap` (`parquet.rs:76,122,169`): `#[pg_extern]` + REVOKE via `extension_sql!` em `parquet.rs:320-329`; presentes no fresh-install, ausentes do script de upgrade.
- `flush_pending` no xact callback (`columnar.rs:193`): itera `WRITE_STATES` (thread-local) e faz `relation_open` no OID.

### Domain glossary

- **delta-only upgrade script** — `theodb_rs--X--Y.sql` que emite só objetos novos/alterados (padrão pgvector/pg_trgm), não o schema inteiro.
- **dead-letter (M122)** — job com `attempts >= max_attempts` vira `state='failed'` (`vectorizer.rs:276`) e é purgado (`:633`); não é reclamado.
- **gate-out** — remover um símbolo do binário/SQL shipado via `#[cfg(feature=...)]`, não só revogar EXECUTE.
- **CONV/IDEM oracle** (`scripts/test-upgrade.sh`) — CONV: catálogo incompleto converge para o completo; IDEM: rodar o script 2× não erra nem muda o schema.

### Architecture boundaries affected

- `rules/error-handling.md` § 2 — erro tipado, nunca engolir `Result` (fix T1.3 materializa isso).
- `rules/parsimony-ladder.md` rung 4 — reusar dead-letter M122 (zero dep nova).
- Fronteira C/pgrx — todo callback é `#[pg_guard] extern "C-unwind"`; propagar via `error!` é o caminho tipado (não panic cruzando C).
- `rules/git-safety.md` — trabalho em `develop`, `main` release-only.

## Prior Art & Related Work

- Blueprint `m144-remediation-blueprint.md`: pgvector `vector--0.7.4--0.8.0.sql:4-26`, pg_trgm `pg_trgm--1.5--1.6.sql:6-10` (delta-only); postgres `system_functions.sql:688,704` (REVOKE de fs-reading); paradedb `pg_search/src/index/writer/index.rs:44-86` (propagação via `?`).
- In-repo: M137 (`scripts/test-upgrade.sh`), M122 (dead-letter), M143 (`extension_sql!` REVOKE), feature-gate `spike-lexical` (`Cargo.toml`).

## ADRs

### ADR-1 — `symqg_spike_bench`: gate-out via Cargo feature (não apenas REVOKE)

**Decisão.** `#[cfg(feature = "spike-symqg")]` (não-default) no `#[pg_extern]`, removendo-o do `.so` e da superfície SQL shipada.

**Rationale.** `rules/parsimony-ladder.md` rung 1 (spike não precisa existir na superfície prod) + CLAUDE.md "Esforço≠Complexidade". Padrão vivo no repo (`spike-lexical`).

**Alternativa rejeitada.** *REVOKE-only* (padrão `parquet.rs:320-329`): correto para segurança, mas mantém uma primitiva de leitura de filesystem onde um `GRANT` futuro reabre o buraco. Mitigação documentada se a função precisar ficar shipada.

### ADR-2 — Upgrade delta-only `1.1.0→1.2.0` + bump `default_version`

**Decisão.** `theodb_rs--1.1.0--1.2.0.sql` delta-only (`CREATE OR REPLACE`) com os 3 CREATEs lakehouse + 3 REVOKEs; `default_version='1.2.0'`.

**Rationale.** SOTA de 2 peers (pgvector, pg_trgm) — Rule 9. Oráculo = `scripts/test-upgrade.sh` (M137).

**Alternativa rejeitada.** Re-emitir full-schema (estilo 1.0.0→1.1.0): diff maior, risco de drift, contra o SOTA.

### ADR-3 — Fix delete propaga para o dead-letter existente (zero dep nova)

**Decisão.** As 2 armas de `_vectorizer_process_delete` trocam `let _ =` por propagação (`err_input`, shape do upsert `:447-448`); o subtxn (`:896-905`) já converte `Err`→`mark_failed`→dead-letter.

**Rationale.** `rules/error-handling.md` (fail-fast, typed) + parsimony rung 4 (reusar M122).

**Alternativa rejeitada.** Novo mecanismo de retry dedicado ao delete — YAGNI; o dead-letter M122 já cobre falha permanente.

## Dependency Graph

```
T1.1 (upgrade) ─┐
T1.2 (gate-out) ┼─▶ T3.1 (Integration Validation)
T1.3 (delete)  ─┤
T2.1 T2.2 T2.3 T2.4 (MEDIUMs) ─┘
```

T1.1/T1.2/T1.3 independentes (arquivos disjuntos). T2.1–T2.4 independentes entre si. T3.1 gated por todas.

## Phase 1 — HIGH P0

### T1.1 — Upgrade delta-only 1.1.0→1.2.0

#### Files to edit
- `theodb_rs/sql/theodb_rs--1.1.0--1.2.0.sql` (NEW) — 3 `CREATE OR REPLACE FUNCTION` (`olap`, `read_parquet`, `write_parquet`) + 3 REVOKEs, verbatim do bloco gerado. `CREATE OR REPLACE` obrigatório (EC-1: IDEM roda 2×).
- `theodb_rs/theodb_rs.control` — `default_version = '1.2.0'`.

#### Deep file dependency analysis
Assinaturas de `parquet.rs:76,122,169`; REVOKEs de `parquet.rs:320-329`. Nenhum outro objeto muda entre 1.1.0 e 1.2.0.

#### Why this step
**Ação:** criar o script que expõe a superfície M143 a quem faz `ALTER EXTENSION UPDATE`, e avançar `default_version`. **Raciocínio:** README:102/132 promete o upgrade in-place e a superfície least-privilege; hoje só o fresh-install a tem (Baseline: callers de parquet). Delta-only é o SOTA de 2 peers (ADR-2, blueprint Q1).

#### TDD
- **RED:** `scripts/test-upgrade.sh` com `FROM_VER=1.1.0 TO_VER=1.2.0` retorna exit≠0 (versão 1.2.0 inexistente). Teste literal `test_upgrade_1_1_0_to_1_2_0_exposes_parquet_surface` asserta `SELECT count(*) FROM pg_proc WHERE proname IN ('read_parquet','write_parquet','olap')` = 3 pós-upgrade.
- **GREEN:** criar o script + bump; `scripts/test-upgrade.sh` sai 0.
- **REFACTOR:** garantir o CREATE idêntico ao fresh (oráculo CONV cobre drift).

#### Concurrency tests
(none — single-threaded) — DDL de upgrade roda sob lock de extensão.

#### Acceptance criteria
- `scripts/test-upgrade.sh` outputs `SCENARIO_A_OK` e `CONVERGENCIA_OK` e `IDEMPOTENCIA_OK`.
- `SELECT has_function_privilege('nonsuper','read_parquet(text)','EXECUTE')` returns `false`.
- `cat theodb_rs/theodb_rs.control` outputs `default_version = '1.2.0'`.

#### DoD
`FROM_VER=1.1.0 TO_VER=1.2.0 scripts/test-upgrade.sh` returns exit 0 no droplet; log salvo em `docs/benchmarks/m144-upgrade-1.2.0.md`.

### T1.2 — Gate-out do symqg_spike_bench

#### Files to edit
- `theodb_rs/Cargo.toml` — feature `spike-symqg` (não-default), análoga a `spike-lexical`.
- `theodb_rs/src/bench_symqg.rs` — `#[cfg(feature = "spike-symqg")]` no `#[pg_extern]` (`:47`).
- `theodb_rs/src/lib.rs` — `#[cfg(feature="spike-symqg")] mod bench_symqg;`.

#### Deep file dependency analysis
`bench_symqg.rs:47-48` é o único `#[pg_extern]` do módulo; faz `std::fs::read` (`:12,:28`). Nenhum caller de produção; não afeta a superfície `ai.*`/`theodb.*`.

#### Why this step
**Ação:** tirar a função de spike do binário/SQL default. **Raciocínio:** hoje é PUBLIC-executável e lê path arbitrário do servidor (Baseline: loop de REVOKE não a cobre). ADR-1: gate-out > REVOKE.

#### TDD
- **RED:** `symqg_spike_bench_absent_from_default_surface` asserta `SELECT count(*) FROM pg_proc WHERE proname='symqg_spike_bench'` = 0. Falha hoje (returns 1).
- **GREEN:** aplicar o cfg-gate; recompilar default; teste retorna 0.
- **REFACTOR:** confirmar que `cargo pgrx schema` default não emite a função.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- Feature ligada por engano em prod: mitigação = default explícito não a inclui; CI builda só default.

#### Acceptance criteria
- `cargo build` (default) outputs a binary sem `symqg_spike_bench` em `pg_proc`.
- `cargo check --features spike-symqg` returns exit 0 (não quebramos a função, só gateamos).
- `grep symqg_spike_bench theodb_rs/sql/schema_snapshot.sql` outputs nada.

#### DoD
`cargo pgrx test` (default) returns 0 com o teste de ausência; `cargo check --features spike-symqg` returns 0.

### T1.3 — Delete propaga erro para o dead-letter

#### Files to edit
- `theodb_rs/src/vectorizer.rs` — `:460` e `:469`: trocar `let _ = Spi::run_with_args(...)` por propagação no shape do upsert (`:447-448`, `err_input`).

#### Deep file dependency analysis
O worker chama o delete via SPI em `:899` dentro de um subtxn (`:896-905`) que converte `Err`→`_vectorizer_mark_failed` (`:276`)→dead-letter. Hoje o `let _ =` engole o erro, o delete "sucede" vazio, o job vira `mark_done` (`:917`), e o embedding permanece. Testes happy-path `:1337,:1405` continuam válidos.

#### TDD
- **RED:** `process_delete_failure_does_not_mark_done` cria vectorizer com target inválida (coluna inexistente força o `UPDATE ... SET %I` a falhar), enfileira um delete, roda o worker 1×, asserta `SELECT state FROM theodb.vectorizer_queue WHERE job_id=$1` returns `pending` ou `failed` (não `done`). Falha hoje (returns `done`).
- **GREEN:** substituir os 2 `let _ =` por `.unwrap_or_else(|e| crate::pg::err_input(...))`.
- **REFACTOR:** extrair helper `run_or_fail` se os 3 sites (447,460,469) compartilharem o shape (Regra de 3).
- **EDGE (EC-2):** `process_delete_of_absent_doc_marks_done` — delete de `source_pk` inexistente: SPI returns `Ok` 0 linhas; asserta `state` = `done`. Prova que só `Err` é propagado.

#### Concurrency tests
Sinal presente (worker + lease). Posture: reusa a invariante **atomic** owner-guarded do lease (`mark_done`, `:254`) — um **atomic-counter invariant** (compare-and-set do owner) provado pelos **concurrent test** de lease do M122; o fix não muda o protocolo de lease, só o caminho de erro, sem nova corrida introduzida.

#### Failure scenarios
- SPI DELETE falha (target inválida): reproduzido pelo teste RED; esperado = job NÃO marca done → retry → dead-letter após `max_attempts`.
- Falha permanente: dead-letter M122 após N tentativas (não retry infinito).

#### Acceptance criteria
- `cargo pgrx test process_delete_failure_does_not_mark_done` returns 0.
- `cargo pgrx test process_delete_nulls_target_embedding` returns 0 (regressão happy-path).
- `git diff --stat` outputs zero nova dependência em `Cargo.toml`.

#### DoD
`cargo pgrx test` returns 0 com os 2 testes novos + os 2 happy-path pré-existentes.

## Phase 2 — MEDIUM P1

### T2.1 — PRE_COMMIT flush não aborta COMMIT com OID dropado

#### Files to edit
- `theodb_rs/src/am/columnar.rs` — `:191-199`: antes do `relation_open`, checar existência do OID; se dropado na mesma txn, pular o flush e limpar `WRITE_STATES`.

#### Deep file dependency analysis
`WRITE_STATES` (thread-local) acumula OIDs pendentes; no `XACT_EVENT_PRE_COMMIT` o callback faz `relation_open(relid)`. Se a tabela foi `DROP`ada na mesma txn, o OID não resolve → `relation_open` erra → aborta o COMMIT do usuário.

#### Why this step
**Ação:** guard de existência do OID antes do flush. **Raciocínio:** review MEDIUM `columnar.rs:193` — INSERT+DROP na mesma txn deve commitar limpo (caso negativo de invariante transacional).

#### TDD
- **RED:** `columnar_insert_then_drop_same_txn_commits` roda `BEGIN; INSERT INTO col_tbl ...; DROP TABLE col_tbl; COMMIT;` e asserta que o COMMIT returns sucesso. Falha hoje (aborta).
- **GREEN:** pular OIDs dropados no loop de flush.
- **REFACTOR:** limpar a entrada de `WRITE_STATES` do OID dropado.

#### Concurrency tests
(none — single-threaded) — `WRITE_STATES` é thread-local por-backend, sem estado cross-thread; teste single-backend determinístico prova a invariante.

#### Acceptance criteria
- `cargo pgrx test columnar_insert_then_drop_same_txn_commits` returns 0.
- `cargo pgrx test` (suíte columnar existente) returns 0 (INSERT sem DROP ainda faz flush).

#### DoD
`cargo pgrx test` returns 0 com o novo teste.

### T2.2 — sanitize_error_text robusto a Unicode length-changing

#### Files to edit
- `theodb_rs/src/vectorizer.rs` — `:738-752`: iterar sobre uma representação char-consistente do original (lowercase char-a-char para o match), não índice compartilhado entre `lower_chars` e `bytes`.

#### Deep file dependency analysis
`sanitize_error_text` compara `lower_chars[i..]` (do `to_lowercase()`) mas indexa `bytes` do original com o mesmo `i` — se o lowercase muda o comprimento, os índices desalinham e um `Bearer <token>`/`sk-...` escapa da redação para `last_error`.

#### Why this step
**Ação:** alinhar o match sobre uma única representação. **Raciocínio:** review MEDIUM `vectorizer.rs:742` — vazamento de credencial em `last_error` (caso negativo de segurança).

#### TDD
- **RED:** `sanitize_redacts_bearer_after_length_changing_unicode` (unit): input com char length-changing no lowercase seguido de `Bearer sk-secret`; asserta que o output `contains` `Bearer <redacted>` e NÃO `contains` `sk-secret`. Falha hoje.
- **GREEN:** corrigir o alinhamento (match char-a-char no original com `char::to_lowercase`).
- **REFACTOR:** cobrir `sk-` no mesmo teste.

#### Concurrency tests
(none — single-threaded) — função pura, sem estado compartilhado.

#### Acceptance criteria
- `cargo test sanitize_redacts_bearer_after_length_changing_unicode` returns 0.
- `cargo test` (casos ASCII existentes) returns 0 (regressão).

#### DoD
`cargo test` returns 0 com o novo teste unit.

### T2.3 — Retry com backoff (não re-enfileirar instantâneo)

#### Files to edit
- `theodb_rs/src/vectorizer.rs` — `:280-292`: no `mark_failed`, quando volta a `pending`, setar `lease_deadline = now() + backoff(attempts)` (backoff exponencial saturado) em vez de `NULL`.

#### Deep file dependency analysis
Hoje `mark_failed` seta `lease_deadline=NULL` → job imediatamente reclamável → numa queda transitória do endpoint, o backlog inteiro é re-tentado em loop apertado.

#### Why this step
**Ação:** backoff exponencial no re-enqueue. **Raciocínio:** review MEDIUM `vectorizer.rs:285` — retry sem backoff martela o endpoint. `rules/error-handling.md` § recuperáveis: retry COM backoff.

#### TDD
- **RED:** `retry_sets_backoff_deadline` força `mark_failed` com `attempts=1`, asserta `SELECT lease_deadline > now() FROM theodb.vectorizer_queue WHERE job_id=$1` returns `true` (não NULL). Falha hoje.
- **GREEN:** computar `lease_deadline = now() + least(2^attempts, cap) segundos`.
- **REFACTOR:** cap configurável via GUC existente se houver.
- **EDGE (EC-3):** `backoff_saturates_for_large_attempts` — `attempts=60`: asserta que o shift satura no cap sem overflow (`1i64.checked_shl(attempts.min(30)).unwrap_or(cap)`).

#### Concurrency tests
Sinal presente (fila/lease). Posture: `lease_deadline` é o mecanismo **atomic** de exclusão temporal; o teste asserta o valor, e a reclaim é um **atomic-counter invariant** já provado pelos **concurrent test** de lease do M122 — reusa a invariante existente.

#### Failure scenarios
- Endpoint 5xx transitório: reproduzido injetando erro; esperado = job re-agendado com deadline futura, não hammer.

#### Acceptance criteria
- `cargo pgrx test retry_sets_backoff_deadline` returns 0.
- `cargo pgrx test backoff_saturates_for_large_attempts` returns 0.
- Após `max_attempts`, `SELECT state ...` returns `failed` (dead-letter intacto).

#### DoD
`cargo pgrx test` returns 0 com os 2 testes de backoff.

### T2.4 — Guard no cast u32 do CSR

#### Files to edit
- `theodb_rs/src/graph.rs` — `:310-318`: `u32::try_from` fail-closed antes do `as u32`; erro tipado se exceder.

#### Deep file dependency analysis
`adj[cur[u] as usize] = v as u32` trunca silenciosamente se `v > u32::MAX`. Node-ids são `i64`/`usize` na origem; um id grande corrompe a CSR sem sinal.

#### Why this step
**Ação:** checagem de faixa fail-closed antes do cast. **Raciocínio:** review `graph.rs:314` — truncamento silencioso é corrupção de dado. `rules/error-handling.md`: falha explícita, não valor mágico.

#### TDD
- **RED+EDGE (EC-4):** `csr_build_guards_u32_boundary` (unit): (NEGATIVE) node-id `= u32::MAX as i64 + 1` → asserta que a função returns `Err` tipado (não truncamento); (EDGE) node-id `= u32::MAX` → asserta que a CSR builds OK. Falha hoje (trunca).
- **GREEN:** `u32::try_from(v).map_err(...)?` / `error!` antes do cast.
- **REFACTOR:** helper de guard se `src` e `dst` repetirem (2 sites).

#### Concurrency tests
(none — single-threaded) — build de CSR sequencial, sem estado compartilhado.

#### Acceptance criteria
- `cargo test csr_build_guards_u32_boundary` returns 0.
- `cargo test` (node-ids ≤ u32::MAX) returns 0 (regressão).

#### DoD
`cargo test` returns 0 com o teste de guard.

## Phase 3 — Integration Validation

### T3.1 — Suíte completa + gates

#### Files to edit
- (nenhum — validação)

#### Deep file dependency analysis
Gate final que exercita os 7 fixes juntos: a suíte `pg_test`, os gates de lint, e o harness de upgrade no droplet.

#### Why this step
**Ação:** provar a milestone inteira end-to-end. **Raciocínio:** theodb-evolution § "code merged ≠ capability exists" — a evidência é o gate, não o merge.

#### TDD
- **RED:** antes dos fixes, `cargo pgrx test` returns ≠0 (7 REDs falhando).
- **GREEN:** com todos os fixes, `cargo pgrx test --features pg_test` returns 0.
- **REFACTOR:** rodar `cargo clippy -- -D warnings` e `cargo fmt --check`.

#### Concurrency tests
(none — single-threaded) — orquestração de validação, sem estado compartilhado.

#### Acceptance criteria
- `cargo pgrx test --features pg_test` returns exit 0.
- `cargo clippy -- -D warnings` returns exit 0 e `cargo fmt --check` returns exit 0.
- `cargo build` (default) returns exit 0 (symqg fora do default, nada mais quebrado).
- `FROM_VER=1.1.0 TO_VER=1.2.0 scripts/test-upgrade.sh` returns exit 0; log em `docs/benchmarks/m144-upgrade-1.2.0.md`.
- `/code-quality` outputs verdict ∉ {FAIL_HARD, INVALID}.

#### DoD
Todos os 7 DoDs de tarefa + este gate verdes, com evidência (log do test-upgrade + saída do `cargo pgrx test`) linkada no implementation summary; CHANGELOG `[Unreleased]` atualizado.

## Coverage Matrix

| # | Requisito (finding) | Severidade | Task | Teste RED |
|---|---|---|---|---|
| 1 | Upgrade chain congelada (superfície M143) | HIGH | T1.1 | `test_upgrade_1_1_0_to_1_2_0_exposes_parquet_surface` |
| 2 | symqg_spike_bench PUBLIC fs-read | HIGH | T1.2 | `symqg_spike_bench_absent_from_default_surface` |
| 3 | Delete engolido (PII) | HIGH | T1.3 | `process_delete_failure_does_not_mark_done` |
| 4 | PRE_COMMIT flush vs DROP mesma-txn | MEDIUM | T2.1 | `columnar_insert_then_drop_same_txn_commits` |
| 5 | sanitize Unicode length-changing | MEDIUM | T2.2 | `sanitize_redacts_bearer_after_length_changing_unicode` |
| 6 | retry sem backoff | MEDIUM | T2.3 | `retry_sets_backoff_deadline` |
| 7 | cast u32 CSR silencioso | MEDIUM | T2.4 | `csr_build_guards_u32_boundary` |
| 8 | Suíte + upgrade + gates | INTEGRACAO | T3.1 | `cargo pgrx test` full |

100% dos findings P0+P1 mapeados a task + teste. Os 6 test-gaps da fase 4 do audit são os REDs de T1.3/T2.1/T2.2/T2.3/T2.4 (absorvidos por construção).

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `0.19.0` | rust | Extension framework (todo o crate). ADR-3/M135. |
| `datafusion` | (já no tree) | rust | Superfície lakehouse M143 que T1.1 apenas expõe — não muda a dep. |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | — | — | M144 não adiciona crate; a única mudança em `Cargo.toml` é uma **feature** (`spike-symqg`) que gateia código existente | Parsimony rung 4: fix T1.3 reusa dead-letter M122; zero dep |

### Removed

| Package | Last version | Why removed |
|---|---|---|
| (none) | — | — |

> `/deps-audit m144-remediation` (2026-07-23): PASS_WITH_CAVEATS. Zero dep nova. Advisory transitivo pré-existente/ortogonal: RUSTSEC-2026-0204 (`crossbeam-epoch` via rayon→tantivy[non-default]+criterion[dev]; fix = bump lockfile ≥0.9.20). Relatório: `.claude/knowledge-base/audits/m144-remediation-deps-audit-2026-07-23.md`.

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Script de upgrade sobre catálogos existentes (lição M137: pgrx não gera upgrade; regex-anchoring; corrupção silenciosa de shell type) | HIGH | delta-only mínimo (3 CREATEs) + oráculos CONV/IDEM do `test-upgrade.sh` | implementer |
| Propagar o erro do delete pode reter jobs em retry se a falha for permanente | MEDIUM | dead-letter M122 após `max_attempts` (não infinito); T2.3 adiciona backoff para não martelar | implementer |
| `cargo pgrx test` não linka em dev box (símbolos PG) | MEDIUM | validação in-PG via droplet/example (A/B); testes unit puros (T2.2, T2.4) rodam via `cargo test` stock | implementer |
| Gate-out muda a superfície SQL (schema_snapshot regenera sem symqg) | LOW | schema-drift-gate valida o snapshot; regenerar no mesmo commit | implementer |

## Unresolved Questions

- Q7: nenhum `.github/workflows/` roda o upgrade harness hoje (`schema-drift-gate.yml` só guarda o snapshot); a evidência do DoD-1 é run manual no droplet nesta milestone. Wire de CI do upgrade é candidato a milestone futuro (não bloqueia M144).
- E5: upgrade encadeado 1.0.0→1.1.0→1.2.0 é aplicado pelo PG automaticamente; a perna 1.0.0→1.1.0 já é provada pelo M137, a 1.1.0→1.2.0 é o alvo — sem teste `FROM=1.0.0 TO=1.2.0` dedicado (redundante). Documentado em `docs/benchmarks/m144-upgrade-1.2.0.md`.

## Failure scenarios

- SPI DELETE falha (T1.3): reproduzido por target inválida; esperado = job não-done → retry → dead-letter.
- Endpoint 5xx transitório (T2.3): reproduzido por injeção; esperado = re-agendar com backoff, não hammer.
- OID dropado no PRE_COMMIT (T2.1): reproduzido por INSERT+DROP mesma-txn; esperado = COMMIT limpo.
- Node-id > u32 (T2.4): reproduzido por id gigante; esperado = erro tipado, não truncamento.

## Global Definition of Done

- `cargo pgrx test --features pg_test` returns exit 0 com 7 testes RED→GREEN novos; `cargo clippy -- -D warnings` e `cargo fmt --check` returns exit 0.
- `FROM_VER=1.1.0 TO_VER=1.2.0 scripts/test-upgrade.sh` returns exit 0 no droplet, log em `docs/benchmarks/m144-upgrade-1.2.0.md`.
- `SELECT count(*) FROM pg_proc WHERE proname='symqg_spike_bench'` returns 0 no default; superfície lakehouse alcançável por upgrade E superuser-only.
- CHANGELOG `[Unreleased]` atualizado; arquivos ≤ 500 LoC de delta; `/code-quality` outputs verdict ∉ {FAIL_HARD, INVALID}.
- Zero dep nova (parsimony rung 4).
