# ADR 0016 — Vectorizer worker: pgrx BackgroundWorker in-process

**Status:** Accepted · **Date:** 2026-07-07 · **Milestone:** M54
**Relacionado:** ADR `0007` (síncrono per-row — **revisitado** aqui, com escopo), ADR `0006` (own-code Rust), ADR `0001` (no engine fork)
**Blueprint:** `.claude/knowledge-base/discoveries/blueprints/m54-vectorizer-declarative-blueprint.md`

## Contexto e problema

O vectorizer declarativo (M54) mantém uma coluna de embedding automaticamente em INSERT/UPDATE — como pgai
vectorizer (Timescale), Supabase automatic embeddings, AlloyDB. Diferente de `theodb.embed(text)` (síncrono,
per-row, DENTRO da transação do usuário — fixado pelo ADR 0007), a **manutenção da coluna** não pode bloquear o
COMMIT do escritor pela latência do modelo. Precisa de execução **assíncrona, fora da transação do escritor**.

A questão arquitetural não é o embedding (já temos `embed_batch` + retry bounded em `http.rs`) — é **quem drena
a fila de jobs**, num produto que é **edição downloadable que roda em qualquer lugar**.

## Drivers da decisão

1. **Portabilidade "roda em qualquer lugar"** (CLAUDE.md North Star) — MAS o TheoDB **já** exige carregar seu
   `.so` (é uma extensão pgrx). O custo "precisa de `shared_preload_libraries`/superuser/restart" **já está pago**.
2. **Artefato único** — a promessa é um download, não "banco + processo satélite para supervisionar".
3. **Crash-safety** — worker morto não pode perder nem duplicar embeddings.
4. **Reuso (Regra 9)** — o worker chama `embed::run_batch` + retry `http.rs` **in-process**, sem HTTP-para-si-mesmo.
5. **Esforço ≠ Complexidade** — FFI de bgworker é esforço alto **essencial** (o problema exige out-of-txn);
   aceitável porque elimina um moving-part operacional.

## Opções consideradas

- **(A) pgrx `BackgroundWorker` in-process** — worker registrado no load; postmaster-supervisionado; SPI + `SKIP LOCKED`; chama `run_batch` direto. ← **escolhida**
- **(B) processo externo** (o padrão pgai/Supabase) — binário separado faz polling via libpq.
- **(C) pg_cron** — `cron.schedule` chama uma função SQL de processamento.

## Decisão

**Escolhida: (A) pgrx BackgroundWorker in-process**, registrado via `shared_preload_libraries`.

Racional: o TheoDB já carrega seu `.so`, então o principal argumento pró-externo do SOTA ("não exigir instalação
server-side") **não se aplica** ao nosso modelo self-hosted/downloadable. In-process entrega: (a) artefato único
(nada a supervisionar fora do postmaster); (b) crash-safety grátis (postmaster reinicia o worker via
`bgw_restart_time`); (c) reuso direto de `embed_batch`/retry sem hop de rede; (d) menor footprint operacional.

**Rejeitada (B) worker externo (o padrão SOTA), com motivo:** é o que pgai e Supabase fazem e é *superior* quando
o alvo é "rodar contra um Postgres gerenciado que você não controla". **Não é o nosso alvo.** Adotá-la imporia um
segundo deployable (contradiz "download único") em troca de portabilidade que nosso modelo não precisa —
complexidade acidental. **Re-open trigger:** surgir uma edição TheoDB managed sem superuser/`.so` → reabrir para (B).

**Rejeitada (C) pg_cron:** dependência extra + granularidade grosseira (~1min de latência trigger→visível) e ainda
exige a lógica de processamento. Mantida como fallback documentado p/ ambientes que proíbem bgworkers customizados.

## O que revisita / preserva do ADR 0007 (explícito)

- **PRESERVA (não toca):** `theodb.embed(text)` e todo `ai.*` permanecem síncronos, `VOLATILE`, per-row,
  AlloyDB-compatíveis (ADR 0007 Decision Outcome). O contrato SQL do chamador não muda; `embed_batch` continua
  o acelerador N→1.
- **REVISITA (escopo cirúrgico):** o ADR 0007 **adiou** a Opção 2 (async/queued worker) "até um gargalo medido
  de exhaustion de backends justificar", listando como re-open trigger explícito "um cliente rodando embeds sobre
  grandes corpora". **O vectorizer M54 É esse trigger.** Este ADR **levanta o defer da Opção 2 — SOMENTE para a
  manutenção-de-coluna do vectorizer**, não retroage async sobre a função `embed()`. Async passa a coexistir com
  o contrato síncrono: caminhos diferentes, decisões separadas.

## Consequências

- **Bom:** artefato único; crash-safety via postmaster; reuso in-process; out-of-txn (COMMIT não bloqueia).
- **Ruim / risco ALTO (mitigado no plano):** nova superfície FFI (lifecycle do bgworker, sinais, SPI); exige
  `shared_preload_libraries` (já exigido). Riscos B1 (embed longjmp mata worker → `PgTryBuilder...catch`), H1
  (duplo-processamento → fencing `owner` token), H2 (lock atravessando HTTP → design 3-fases), H3 (poison-pill →
  attempts-on-claim + dead-letter). Ver o blueprint § Riscos.
- **Testabilidade:** a LÓGICA da fila (`claim_batch`/`mark_done`/`mark_failed`/`renew_lease`) é fatiada como
  `#[pg_extern]` testável via `#[pg_test]` SEM preload/worker/OpenAI; o worker main é um loop de ~20 linhas sem
  business logic.
- **Performance:** UNBENCHMARKED — latência trigger→visível e throughput de drenagem viram benchmark em
  `docs/benchmarks/` antes de qualquer claim (PRD D3, `public-copy.md §4`).

## Quando este ADR pode mudar

Novo ADR. Re-open trigger de arquitetura: edição managed sem superuser (→ worker externo) OU benchmark mostrando
que pgmq maduro vale a dependência (→ trocar a fila mínima própria por pgmq).
