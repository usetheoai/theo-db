---
slug: m144-remediation
milestone_id: M144
created_at: 2026-07-23
goal: Fechar os 3 HIGH + 4 MEDIUMs P1 do code-review de theodb_rs sob TDD, cada fix provado por um teste de regressão que falha antes e passa depois.
---

# Plan — M144 Remediação P0+P1 do code-review

## Goal

Fechar os 3 HIGH + 4 MEDIUMs P1 do audit loop-code-review de `theodb_rs` (2026-07-23) sob TDD, cada fix provado por evidência comportamental medida.

**Nota de validação (honestidade Regra 3 — reconciliada pós-review 2026-07-23).** `cargo pgrx test` é **estruturalmente inexecutável** neste ambiente (o test-binary do crate não linka símbolos PG standalone — memória do projeto). Portanto **measured by**: (1) `cargo check --features pg18,pg_test --tests` exit 0 (os `#[pg_test]` COMPILAM); (2) smokes SQL comportamentais contra o `.so` instalado no droplet PG18.4 (a prova comportamental real); (3) `FROM_VER=1.1.0 TO_VER=1.2.0 scripts/test-upgrade.sh` imprimindo `SCENARIO_A_OK`/`CONVERGENCIA_OK`/`IDEMPOTENTE_OK` no droplet; (4) `rustc --test` standalone para as funções puras (T2.2, fórmula de backoff). Os corpos dos `#[pg_test]` são compilados-não-executados; a asseveração comportamental vem dos smokes paralelos. Ver `m144-remediation-implementation.md`.

## Context

Origem: `.claude/knowledge-base/audits/theodb-rs-code-review-2026-07-23.md` (100 findings, 0 CRITICAL, 90/90 arquivos). Blueprint de discovery (SHIPPABLE_WITH_CAVEATS 89): `.claude/knowledge-base/discoveries/blueprints/m144-remediation-blueprint.md` — resolveu, com citação a peers PG maduros, o padrão de cada fix: upgrade **delta-only** (pgvector/pg_trgm), **REVOKE ALL FROM PUBLIC** do spike fs-reading (least-privilege canônico do PG core; upgrade-safe), **propagação de erro** para o dead-letter M122. Grill: `.claude/knowledge-base/grills/review-findings-remediation-feature-grill.md`. Edge-case review: `.claude/knowledge-base/reviews/m144-remediation-edge-cases-2026-07-23.md`. Milestone: ROADMAP M144 (gated M143 `[x]`). Cumpre `rules/error-handling.md` (fail-fast typed — o fix C é a materialização exata), `rules/parsimony-ladder.md` rung 4 (reusar M122, zero dep), `rules/testing.md` § 4.1 (edge + negative), `rules/git-safety.md` (develop).

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
- **REVOKE ALL FROM PUBLIC** — least-privilege canônico do PG (revoga EXECUTE de PUBLIC; a função fica no `.so`, upgrade-safe). O `gate-out` (remover o símbolo via `#[cfg]`) foi rejeitado por quebrar a cadeia de upgrade (ADR-1).
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

### ADR-1 — `symqg_spike_bench`: REVOKE ALL FROM PUBLIC (least-privilege upgrade-safe)

**Decisão (revisada na implementação — ver nota).** `REVOKE ALL ON FUNCTION symqg_spike_bench(...) FROM PUBLIC` via `extension_sql!` (fresh install) + no script de upgrade `1.1.0→1.2.0` (installs existentes). A função permanece no `.so`.

**Rationale.** Least-privilege canônico do PostgreSQL core — `system_functions.sql:688,704` faz exatamente isto para `pg_read_file`/`lo_import` (blueprint Q2), e o próprio crate já o faz para as primitivas Parquet (`parquet.rs:320-329`). Fecha o achado de segurança (sem fs-read por PUBLIC) **sem quebrar a cadeia de upgrade**.

**Nota de implementação (2026-07-23, honestidade Regra 3).** O plano original escolheu *gate-out* (remover o símbolo do `.so` via Cargo feature). A validação real do harness (`scripts/test-upgrade.sh`) provou que o gate-out **quebra a cadeia de upgrade**: o script fresh `theodb_rs--1.1.0.sql` (já shipado) referencia `symqg_spike_bench_wrapper`, que o `.so` novo removeria → `CREATE EXTENSION VERSION '1.1.0'` falha com símbolo dangling, e installs 1.1.0 existentes ficam com a função quebrada. Extension-upgrade é UM subsistema (theodb-evolution): remover um símbolo exige o DROP no script de upgrade E deixa versões antigas ininstaláveis com o `.so` novo. REVOKE é o alternativo já documentado no ADR e é upgrade-safe (o `.so` mantém o wrapper; o catálogo antigo continua válido).

**Alternativa rejeitada.** *Gate-out via Cargo feature*: menor superfície shipada, mas quebra a cadeia de upgrade (símbolo dangling nos scripts de versão antigos) e não é validável pelo harness — provado na implementação. Descartado.

### ADR-2 — Upgrade full-schema self-healing `1.1.0→1.2.0` + bump `default_version`

**Decisão (revisada na implementação — ver nota).** `theodb_rs--1.1.0--1.2.0.sql` **full-schema self-healing**: re-emite o schema 1.1.0 inteiro de forma idempotente (corpo do `1.0.0→1.1.0`, `CREATE OR REPLACE`/`IF NOT EXISTS`/guards) + ACRESCENTA a superfície lakehouse (3 `CREATE OR REPLACE` parquet + REVOKEs + symqg REVOKE); `default_version='1.2.0'`.

**Rationale.** Convenção IN-REPO do projeto (M137 + o oráculo **CONV** de `scripts/test-upgrade.sh`) — Rule 9 (o padrão local vence o externo). Provado: `SCENARIO_A_OK` + `CONVERGENCIA_OK` (280→290) + `IDEMPOTENTE_OK` no droplet.

**Nota de implementação (2026-07-23, honestidade Regra 3).** O plano/blueprint original escolheu *delta-only* (do SOTA pgvector/pg_trgm). A validação real do harness provou que o oráculo **CONV** do projeto EXIGE full-schema self-healing: ele dropa `embed`/`embed_batch`/`rerank` e verifica que o UPDATE os RESTAURA (catálogo incompleto converge para o completo). Um delta-only não restaura objetos dropados → CONV falha. O projeto (M137) usa scripts full-schema idempotentes exatamente por essa propriedade self-healing. Segui a convenção do projeto, não o SOTA externo.

**Alternativa rejeitada.** *Delta-only* (só os 3 objetos novos): menor diff, mas falha o oráculo CONV do projeto (não é self-healing) — provado na implementação. Descartado.

### ADR-3 — Fix delete propaga para o dead-letter existente (zero dep nova)

**Decisão.** As 2 armas de `_vectorizer_process_delete` trocam `let _ =` por propagação (`err_input`, shape do upsert `:447-448`); o subtxn (`:896-905`) já converte `Err`→`mark_failed`→dead-letter.

**Rationale.** `rules/error-handling.md` (fail-fast, typed) + parsimony rung 4 (reusar M122).

**Alternativa rejeitada.** Novo mecanismo de retry dedicado ao delete — YAGNI; o dead-letter M122 já cobre falha permanente.

**Nota de implementação (2026-07-23, honestidade Regra 3 — provado no droplet).** A validação real revelou que, no pgrx 0.19, `Spi::run_with_args` faz **longjmp** de um `elog(ERROR)` de DML (`pgrx-0.19.0/src/spi.rs:400-427` — "Postgres will do that for us automatically"); só retorna `Err(SpiError(code))` para status-code negativo (uso malformado do SPI), que um template `UPDATE … WHERE …` com args ligados nunca produz. Consequência: (a) o `.unwrap_or_else(err_input)` só dispara no caminho raro do SpiError-code — é defensivo e consistente com o braço upsert, **não** o caminho primário; (b) para erros SQL o `let _ =` antigo **também** já propagava (o longjmp o pula), então o finding #76 é **defense-in-depth** (o audit o marcou `heuristic`), não bug explorável no pgrx atual — a propriedade de segurança (delete falho nunca vira `done` → o `in_subtxn_msg`/M132 do worker registra `last_error`) já valia. O smoke T1.3b prova a propriedade: `process_delete` **diverge** (`column "emb" does not exist`) num delete quebrado, nunca retorna `Ok`. O unit test foi corrigido para asseverar o substring real (`does not exist`), não o prefixo inalcançável.

**Nota de implementação T2.4 (2026-07-23, honestidade Regra 3 — provado no droplet).** O CSR é **denso, indexado pelo node id cru** (`graph.rs:311` `vec![0u64; nn+1]`, `offsets[node as usize]`), então `nn == max_id+1`. Construir com o id literal u32::MAX (4294967295) aloca ~34 GB → OOM (não relacionado ao guard). Por isso o teste EDGE de ACEITE usa um id grande-mas-factível (1_000_000): prova que o guard (`>`, não `>=`) não dá falso-positivo em id válido; o REJECT no limite exato (u32::MAX+1) é provado factível pelo teste NEGATIVE (o guard aborta **antes** da alocação). Smokes: EDGE 1M constrói; NEGATIVE u32::MAX+1 → `node ids must fit in u32`.

## Dependency Graph

```
T1.1 (upgrade) ─┐
T1.2 (REVOKE)   ┼─▶ T3.1 (Integration Validation)
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
- RED shape: `test_upgrade_1_1_0_to_1_2_0_exposes_parquet_surface()` -> assert proc_count == 3 AND has_function_privilege == false
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

### T1.2 — REVOKE do symqg_spike_bench (least-privilege)

#### Files to edit
- `theodb_rs/src/bench_symqg.rs` — `extension_sql!` com `REVOKE ALL ON FUNCTION symqg_spike_bench(text, bigint, bigint, int) FROM PUBLIC`, `requires = [symqg_spike_bench]` (mirror de `parquet.rs:320-329`).
- `theodb_rs/sql/theodb_rs--1.1.0--1.2.0.sql` — mesmo REVOKE (installs 1.1.0 existentes ao dar UPDATE).

#### Deep file dependency analysis
`bench_symqg.rs:47-48` é o único `#[pg_extern]` do módulo; faz `std::fs::read` (`:12,:28`). Nenhum caller de produção. A função permanece no `.so` (o wrapper `symqg_spike_bench_wrapper` continua exportado) — crítico para a cadeia de upgrade (o script fresh `theodb_rs--1.1.0.sql` referencia esse wrapper; removê-lo quebraria `CREATE EXTENSION VERSION '1.1.0'`).

#### Why this step
**Ação:** revogar EXECUTE de PUBLIC na função (superuser-only). **Raciocínio:** hoje é PUBLIC-executável e lê path arbitrário do servidor (Baseline: loop de REVOKE em `:1105-1112` cobre só `^_vectorizer_`). ADR-1: REVOKE é o least-privilege canônico do PG core (`system_functions.sql:704`) e é upgrade-safe (mantém o wrapper no `.so`).

#### TDD
- RED shape: `test_symqg_revoked_from_public()` -> assert public_has_execute == false
- **RED:** `symqg_spike_bench_revoked_from_public` asserta `has_function_privilege('public','symqg_spike_bench(text,bigint,bigint,int)','EXECUTE')` = false. Falha hoje (returns true — PUBLIC tem EXECUTE).
- **GREEN:** adicionar o `extension_sql!` REVOKE (fresh) + o REVOKE no script de upgrade; teste retorna false.
- **REFACTOR:** confirmar que o REVOKE roda APÓS o CREATE (`requires`).

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- Role comum tenta executar a função: mitigação = REVOKE fecha o EXECUTE; só superuser chama.

#### Acceptance criteria
- `SELECT has_function_privilege('public','symqg_spike_bench(text,bigint,bigint,int)','EXECUTE')` returns `false` (fresh install 1.2.0).
- Pós-upgrade de um install 1.1.0: mesmo REVOKE aplicado (o script de upgrade).
- O wrapper permanece no `.so` (cadeia de upgrade intacta) — `CREATE EXTENSION VERSION '1.1.0'` continua funcionando.

#### DoD
Smoke SQL: `has_function_privilege(...)` = false na extensão 1.2.0 instalada; harness de upgrade passa (o wrapper presente não quebra a cadeia).

### T1.3 — Delete propaga erro para o dead-letter

#### Files to edit
- `theodb_rs/src/vectorizer.rs` — `:460` e `:469`: trocar `let _ = Spi::run_with_args(...)` por propagação no shape do upsert (`:447-448`, `err_input`).

#### Deep file dependency analysis
O worker chama o delete via SPI em `:899` dentro de um subtxn (`:896-905`) que converte `Err`→`_vectorizer_mark_failed` (`:276`)→dead-letter. Hoje o `let _ =` engole o erro, o delete "sucede" vazio, o job vira `mark_done` (`:917`), e o embedding permanece. Testes happy-path `:1337,:1405` continuam válidos.

#### TDD
- RED shape: `test_process_delete_failure_does_not_mark_done()` -> assert state != done
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
- RED shape: `test_columnar_insert_then_drop_same_txn_commits()` -> assert commit_ok == true
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
- RED shape: `test_sanitize_redacts_bearer_after_length_changing_unicode()` -> assert output_contains_secret == false
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
- RED shape: `test_retry_sets_backoff_deadline()` -> assert lease_deadline > now
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
- RED shape: `test_csr_build_guards_u32_boundary()` -> assert result is Err for over_max AND is Ok for exactly_max
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
- RED shape: `test_full_suite_and_upgrade_harness()` -> assert exit_code == 0
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

| # | Requisito (finding) | Severidade | Task | Prova comportamental (medida no droplet salvo nota) |
|---|---|---|---|---|
| 1 | Upgrade chain congelada (superfície M143) | HIGH | T1.1 | Harness `test-upgrade.sh` 1.1.0→1.2.0: SCENARIO_A/CONV/IDEM/B1, exit 0 |
| 2 | symqg_spike_bench PUBLIC fs-read (REVOKE) | HIGH | T1.2 | `has_function_privilege('public', …, EXECUTE)`=false + pg_test `symqg_spike_bench_revoked_from_public` |
| 3 | Delete engolido (PII) | HIGH | T1.3 | Smoke: ausente→limpo, quebrado→diverge. pg_test `process_delete_failure_does_not_mark_done` é **behavior-lock** (não RED distinguível — o `let _=` antigo também divergia via longjmp; ver ADR-3) |
| 4 | PRE_COMMIT flush vs DROP mesma-txn | MEDIUM | T2.1 | **Smoke apenas** (`INSERT;DROP;COMMIT`→sem crash + controle lê): PRE_COMMIT **não é pg_test-ável** (o harness dá rollback, o callback nunca dispara). Sem `#[pg_test]` por design |
| 5 | sanitize Unicode length-changing | MEDIUM | T2.2 | pg_test `sanitize_redacts_credential_cleanly_after_length_changing_unicode` (RED→GREEN real, provado standalone via rustc) |
| 6 | retry sem backoff | MEDIUM | T2.3 | Smoke fila (deadline +4s / NULL@dead-letter / cap 300s / fencing) + pg_tests `retry_sets_backoff_deadline`/`backoff_saturates_for_large_attempts` |
| 7 | cast u32 CSR silencioso | MEDIUM | T2.4 | Smoke EDGE 1M + NEGATIVE u32::MAX+1. pg_tests `csr_build_accepts_large_valid_u32_id`/`csr_build_guards_u32_boundary` |
| 8 | Suíte + upgrade + gates | INTEGRACAO | T3.1 | `cargo check --features pg18,pg_test --tests` exit 0 + smokes + harness (cargo pgrx test inexecutável — ver Goal) |

100% dos findings P0+P1 mapeados a task + prova. **Honestidade:** 6/7 fixes têm `#[pg_test]` commitado (compilado, não executado); T2.1 é smoke-only (PRE_COMMIT não pg_test-ável); T1.3 é behavior-lock. A prova comportamental de todos vem dos smokes/harness no droplet.

## Dependencies

### Existing — use as-is

| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `pgrx` | `0.19.0` | rust | Extension framework (todo o crate). ADR-3/M135. |
| `datafusion` | (já no tree) | rust | Superfície lakehouse M143 que T1.1 apenas expõe — não muda a dep. |

### New — to be introduced

| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| (none) | — | — | M144 não adiciona crate. Cargo.toml: só `test = false` no `[[bench]]` (fix de test-infra pré-existente) | Parsimony rung 4: fix T1.3 reusa dead-letter M122; zero dep |

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

- `cargo check --features pg18,pg_test --tests` returns exit 0 (os `#[pg_test]` compilam; `cargo pgrx test` é inexecutável neste ambiente — ver Goal). 8 `#[pg_test]` novos commitados (6/7 fixes; T2.1 é smoke-only, T1.3 é behavior-lock). Prova comportamental de todos os 7 via smokes SQL no droplet + `rustc --test` (funções puras).
- `FROM_VER=1.1.0 TO_VER=1.2.0 scripts/test-upgrade.sh` returns exit 0 no droplet (SCENARIO_A/CONV/IDEM/B1).
- `SELECT count(*) FROM pg_proc WHERE proname='symqg_spike_bench'` returns 0 no default; superfície lakehouse alcançável por upgrade E superuser-only.
- CHANGELOG `[Unreleased]` atualizado; arquivos ≤ 500 LoC de delta; `/code-quality` outputs verdict ∉ {FAIL_HARD, INVALID}.
- Zero dep nova (parsimony rung 4).
