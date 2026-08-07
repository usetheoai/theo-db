---
type: Decision
title: ADR 0016 — Vectorizer worker: BackgroundWorker pgrx in-process
type_note: decisão de mecanismo
description: A fila de embeddings do vectorizer é drenada por um BackgroundWorker in-process registrado via shared_preload_libraries, e não por um processo externo como fazem pgai e Supabase.
resource: git:f7c7b93:docs/adr/0016-m54-vectorizer-worker-mechanism.md
tags: [adr, vectorizer, background-worker, pgrx, async, m54]
adr_id: "0016"
adr_status: Accepted
decision_date: 2026-07-07
milestone: M54
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0016
    resource: git:f7c7b93:docs/adr/0016-m54-vectorizer-worker-mechanism.md
    title: ADR 0016 — Vectorizer worker
    last_modified: 2026-07-07
---

O caso em que **divergir da SOTA é a decisão certa**, porque o modelo de deployment é diferente do
dela.

# Contexto

O [vectorizer declarativo](/features/16-vectorizer.md) mantém uma coluna de embedding
automaticamente em INSERT e UPDATE — como o pgai vectorizer da Timescale, o automatic embeddings
do Supabase e o [AlloyDB](/technologies/alloydb.md). Diferente de `theodb.embed(text)`, que é
síncrono e roda *dentro* da transação do usuário por decisão do
[ADR 0007](/decisions/0007-synchronous-per-row-model-http.md), a **manutenção da coluna** não pode
bloquear o COMMIT do escritor pela latência do modelo: precisa ser assíncrona, fora da transação.

A questão arquitetural não é o embedding — já havia `embed_batch` e retry limitado. É **quem drena
a fila**, num produto que é edição *downloadable* que roda em qualquer lugar.

# Decisão

**BackgroundWorker pgrx in-process**, registrado via `shared_preload_libraries`.

O racional inverte o argumento da SOTA: o TheoDB **já** exige carregar seu `.so`, por ser uma
extensão [pgrx](/technologies/pgrx.md). O custo operacional — `shared_preload_libraries`,
superuser, restart — **já está pago**. Logo o principal argumento pró-externo ("não exigir
instalação server-side") **não se aplica** ao modelo self-hosted.

In-process entrega: artefato único, sem nada a supervisionar fora do postmaster; crash-safety de
graça, porque o postmaster reinicia o worker via `bgw_restart_time`; reuso direto de `embed_batch`
e do retry, sem hop de rede nem HTTP para si mesmo; e menor footprint operacional.

# Alternativas rejeitadas

**Worker externo — o padrão SOTA.** É o que pgai e Supabase fazem, e é *superior* quando o alvo é
rodar contra um Postgres gerenciado que você não controla. **Esse não é o nosso alvo.** Adotá-lo
imporia um segundo deployable, contradizendo "download único", em troca de portabilidade que este
modelo não precisa. **Gatilho de reabertura:** surgir uma edição TheoDB gerenciada, sem superuser
nem `.so`.

**pg_cron.** Dependência extra e granularidade grosseira (~1 min de latência entre trigger e
visibilidade), e ainda exigiria a lógica de processamento. Mantida como fallback documentado para
ambientes que proíbem bgworkers customizados.

# O que revisita do ADR 0007

**Preserva:** `theodb.embed(text)` e todo `ai.*` permanecem síncronos, `VOLATILE`, por linha e
compatíveis com o AlloyDB. O contrato SQL do chamador não muda.

**Revisita, cirurgicamente:** o ADR 0007 adiara a opção assíncrona "até um gargalo medido
justificar", listando como gatilho explícito "um cliente rodando embeds sobre grandes corpora". **O
vectorizer é esse gatilho.** Este ADR levanta o adiamento **somente para a manutenção de coluna do
vectorizer** — não retroage assincronia sobre a função `embed()`. Assíncrono passa a coexistir com
o contrato síncrono: caminhos diferentes, decisões separadas.[^adr0016]

# Consequências

**Bom:** artefato único; crash-safety via postmaster; reuso in-process; fora da transação, então o
COMMIT não bloqueia.

**Risco alto, mitigado:** nova superfície FFI — ciclo de vida do bgworker, sinais, SPI. Os riscos
nomeados: um longjmp do embed matando o worker (contido por `PgTryBuilder`), duplo-processamento
(fencing por token de `owner`), lock atravessando HTTP (desenho em 3 fases) e poison-pill
(contagem de tentativas na claim mais dead-letter).

**Testabilidade:** a lógica da fila (`claim_batch`, `mark_done`, `mark_failed`, `renew_lease`) é
fatiada como `#[pg_extern]` testável sem preload, worker ou provedor de modelo; o main do worker é
um loop de ~20 linhas sem regra de negócio.

**Performance:** não medida — latência entre trigger e visibilidade, e throughput de drenagem,
viram benchmark antes de qualquer claim. Ver [m132](/benchmarks/m132-vectorizer-diagnosability.md).

[^adr0016]: ADR 0016 — Vectorizer worker: pgrx BackgroundWorker in-process
