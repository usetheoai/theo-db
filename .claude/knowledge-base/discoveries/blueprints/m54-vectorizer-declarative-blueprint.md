# Blueprint M54 — Vectorizer declarativo (auto-embedding em INSERT/UPDATE)

Slug: `vectorizer-declarative` · milestone_id: M54 · Date: 2026-07-07 · Verdict: SHIPPABLE_WITH_CAVEATS

Síntese de dois discovery agents (council-research-adr: prior-art + decisão de worker; council-rust-pgrx: FFI do bgworker + crash-safety da fila). Documento de design — não código.

## Coverage Corner 1 — Integration Tests (como provar o pipeline)

Obrigações de teste derivadas de pgai/Supabase, adaptadas ao nosso CI (MEMORY m46: `cargo pgrx test` NÃO seta `shared_preload_libraries`; CI não roda pgrx test):

- **Lógica da fila (via `#[pg_test]`, SEM worker/preload/OpenAI):** INSERT → job `pending`; `claim_batch` → `processing` + `lease_deadline` futuro; owner morto (back-date `lease_deadline`) → job re-elegível; duplo-claim (2 owners) → `mark_done` do owner obsoleto afeta **0 rows** (fencing); `attempts >= max` → `failed` (dead-letter, nunca loop).
- **e2e (container c/ `shared_preload_libraries=theodb_rs` + stub determinístico):** INSERT → embedding aparece; UPDATE → re-embed idempotente; endpoint 500 persistente → retry bounded → `failed` tipado (nunca swallow, Rule 8).
- **Concurrency:** 2 workers via `SKIP LOCKED` nunca pegam o mesmo job.

## Coverage Corner 2 — Dependencies (blocos permissivos)

| Dep | Licença | Papel | Decisão |
|---|---|---|---|
| pgmq 1.5.1 (tembo-io) | PostgreSQL ✅ | fila visibility-timeout madura | **NÃO adotar em v1** — o design mínimo próprio (uma tabela + lease + `SKIP LOCKED`) espelha o visibility-timeout do pgmq sem trazer o msg-broker inteiro; adota-se pgmq se o benchmark justificar (ADR 0016). |
| theodb embed/http (próprio) | Apache 2.0 | `run_batch` + retry bounded — reuso in-process | **reuso direto** (`embed.rs:55-124`, `http.rs:41-98`) |

Zero AGPL (D1). Evidência: `references/supabase-postgres/nix/ext/pgmq/default.nix:39,96` (pgmq PostgreSQL License).

## Coverage Corner 3 — Tools (mecanismo de execução)

- **pgrx `BackgroundWorker`** (pgrx 0.16.1 `bgworkers.rs`): `BackgroundWorkerBuilder::new().set_library("theodb_rs").set_function(...).enable_spi_access().set_restart_time(...).load()` → `RegisterBackgroundWorker`. **Requer `shared_preload_libraries`** (o `.load()` é no-op-WARNING de um backend; `_PG_init` hoje roda em backend via CREATE EXTENSION). Escape: `.load_dynamic()` de um `#[pg_extern]` (sem preload). **Decisão M54: static preload** (documentado como install step; o e2e container seta o preload) — ver ADR 0016.
- Entrypoint: `#[pg_guard] pub extern "C-unwind" fn theodb_embed_worker_main(pg_sys::Datum)`.
- SPI no worker: `BackgroundWorker::connect_worker_to_spi(Some(db), None)` (bind a 1 DB).
- Loop: `while BackgroundWorker::wait_latch(Some(poll)) { ... }`; SIGTERM cooperativo (`attach_signal_handlers(SIGHUP|SIGTERM)`; `sigterm_received()`); saída limpa = return do main.

## Coverage Corner 4 — Techniques (crash-safety + reuso)

**Design de 3 fases (evita segurar lock/txn atravessando HTTP — H2):**
1. **txn1:** `claim_batch` — UPDATE atômico + COMMIT (libera locks).
2. **sem txn:** `embed_batch` sobre HTTP (o lease *committado*, não um lock, protege o job).
3. **txn2:** write owner-guarded (`mark_done`/`mark_failed`).

**Claim SQL (com fencing token — correção HIGH-1 obrigatória):**
```sql
UPDATE theodb.vectorizer_queue
SET state='processing', owner=$my_uuid, lease_deadline=now()+$vt, attempts=attempts+1
WHERE job_id IN (
  SELECT job_id FROM theodb.vectorizer_queue
  WHERE (state='pending' OR (state='processing' AND lease_deadline < now()))
    AND attempts < $max_attempts
  ORDER BY enqueued_at
  FOR UPDATE SKIP LOCKED
  LIMIT $batch)
RETURNING job_id, owner, source_pk, op;
```
- `attempts+1` **no claim** (não na falha): um job que mata o worker antes de reportar ainda queima tentativa → poison-pill é bounded → dead-letter.
- **fencing:** `mark_done`/`mark_failed`/`renew_lease` guardam `AND owner=$my_uuid AND state='processing'` → 0 rows = perdi o lease (worker lento reclamado) → descarto, NÃO sobrescrevo o novo owner. **Sem isto o design NÃO é crash-safe sob worker-lento-mas-vivo.**
- índice parcial `(state, enqueued_at)` p/ o claim.
- `vt >= (MAX_RETRIES+1)×HTTP_TIMEOUT` (`http.rs:16,18` ⇒ ≥ 90s) + upsert idempotente por PK no destino.

**B1 (BLOCKER) — embed_batch faz `ereport(ERROR)`/longjmp (`pg.rs:8-39`) que MATA o worker.** Envolver a chamada embed em `PgTryBuilder::new(|| embed::run_batch(...)).catch_others(...).execute()` (precedente in-repo `nl.rs:128`) → converter erro em `mark_failed` tipado. NÃO confiar no `transaction()` do pgrx (re-raise, `bgworkers.rs:307-309`). Catch no call-site do embed, não na fronteira de txn (também bounda o leak de heap Rust no longjmp — M1).

## Schema

```
theodb.vectorizer            id, source_table, source_pk_col, content_col, target_table, target_col, model, dims
theodb.vectorizer_queue      job_id bigserial, vectorizer_id, source_pk, op('upsert'|'delete'),
                             state('pending'|'processing'|'failed'), attempts, owner uuid,
                             lease_deadline timestamptz, last_error, enqueued_at
```
Chunking helper SQL (DoD item 3): função independente `theodb.chunk_text(text, size, overlap)` (recursive character split) — não acoplada ao vectorizer v1 (1 row → 1 embedding; chunking avançado é YAGNI).

## Fatia de testabilidade (§5 council-rust-pgrx) — a chave do CI

| Símbolo | Tipo | Testável CI? |
|---|---|---|
| `claim_batch(batch, vt, max_attempts) -> Vec<Job>` | `#[pg_extern]` (Spi, precedente `migrate.rs:49`) | **SIM** (`#[pg_test]` em tabela temp) |
| `mark_done(job_id, owner, ...)` / `mark_failed(job_id, owner, err)` | `#[pg_extern]` | **SIM** (fencing) |
| `renew_lease(ids, owner, vt)` | `#[pg_extern]` | **SIM** |
| `theodb_embed_worker_main(Datum)` | `#[pg_guard] extern "C-unwind"` | NÃO (preload) — ~20 linhas, zero business logic; compõe claim→embed→mark |
| `theodb.create_vectorizer(...)` + trigger | SQL/`#[pg_extern]` | trigger testável via `#[pg_test]` (só enfileira) |
| métrica jobs processados/falhados | contador consultável (`#[pg_extern]` view/função) | **SIM** |

## ADRs

Ver `docs/adr/0016-m54-vectorizer-worker-mechanism.md` (draft neste ciclo): **pgrx BackgroundWorker in-process** escolhido; alternativas rejeitadas (worker externo = padrão SOTA pgai/Supabase, mas p/ Postgres managed — não é nosso alvo self-hosted; pg_cron = granularidade grosseira). Revisita ADR 0007 (levanta o defer da Opção 2 SÓ p/ manutenção-de-coluna; `embed()` continua síncrono). Re-open trigger: edição managed sem superuser → worker externo.

## Riscos ALTOS (mitigação)

| # | Risco | Mitigação |
|---|---|---|
| B1 | embed_batch longjmp mata worker | `PgTryBuilder...catch` per-job → `failed` tipado |
| B2 | static registration exige preload | documentar install step; e2e container seta preload; lógica testável separada |
| H1 | duplo-processamento (worker lento) | fencing `owner` token em todas as transições |
| H2 | lock/txn atravessando HTTP | design 3-fases (claim commit → embed → write) |
| H3 | poison-pill crash loop | attempts-on-claim + dead-letter |
| R5 | trigger na hot-path do INSERT | trigger só INSERT na fila (barato, sem HTTP) |

## Caveats honestos (Rule 3)

- Corner pgai é conhecimento + docs a confirmar (sem clone local) — dívida consciente.
- Todo número de performance (latência trigger→visível, throughput de drenagem) é **UNBENCHMARKED** — vira benchmark em `docs/benchmarks/` antes de qualquer claim (PRD D3).
- Incertezas pgrx 0.16 a confirmar no código: fate exato de ERROR não-capturado no bgworker (mitigado por catch per-job de qualquer forma); `set_library` basename = `theodb_rs`.
