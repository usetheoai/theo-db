---
type: Decision
title: ADR 0007 — Chamadas HTTP síncronas por linha (embed + ai.*), com batch/async adiados
description: theodb.embed e as funções ai.* fazem uma chamada HTTPS bloqueante por linha, marcadas VOLATILE; a fila assíncrona fica adiada até haver gargalo medido.
resource: git:f7c7b93:docs/adr/0007-synchronous-per-row-model-http.md
tags: [adr, ai-surface, data-flow, escalabilidade, n+1, alloydb-compat]
adr_id: "0007"
adr_status: Accepted
decision_date: 2026-06-29
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0007
    resource: git:f7c7b93:docs/adr/0007-synchronous-per-row-model-http.md
    title: ADR 0007 — Synchronous per-row model HTTP calls
    last_modified: 2026-06-29
---

Converte em registro durável um desenho que existia apenas em comentários de função SQL — e,
com ele, o seu footgun de escala conhecido.

# Problema

`theodb.embed(text)` e toda função generativa `ai.*` (`ai.generate`, `ai.summarize`, `ai.if`,
`ai.rank`, `ai.analyze_sentiment`) emitem **um round-trip HTTPS bloqueante por linha**. São
marcadas `VOLATILE` para que o planner não reaproveite uma única chamada em N linhas.

Consequência direta: `SELECT ai.generate(col) FROM t` explode em N chamadas HTTP sequenciais de
timeout 30s, cada uma segurando **um backend PostgreSQL inteiro** pela latência completa do
modelo. Não havia ADR registrando isso como escolha deliberada — apenas comentários inline, que
não são registro de decisão.

# Direcionadores

Compatibilidade de API com o [AlloyDB](/technologies/alloydb.md) — `embedding()` e
`ai.generate()` são funções SQL por-linha, e a superfície precisa casar para que a migração seja
paridade, que é a north-star metric do
[ADR 0005](/decisions/0005-unification-as-differentiator.md). Correção — `VOLATILE` impede o
planner de espalhar um resultado de modelo por N linhas. Honestidade — o footgun de escala
precisa ser decisão registrada, não conhecimento tribal. E measurement-first: máquina de fila é
complexidade essencial apenas depois de um gargalo medido.

# Opções e decisão

1. **Síncrono, uma chamada por linha, `VOLATILE`** (escolhida), tendo `ai.generate_batch`
   (N prompts → 1 round-trip) como único acelerador.
2. **Embedding assíncrono/enfileirado** via background worker — **explicitamente adiada**.
3. **Batching client-side por padrão** — empurra complexidade a todo chamador e diverge da
   forma de chamada do AlloyDB.

A opção 1 é a única que preserva semântica por-linha compatível com o AlloyDB e a correção do
planner na escala atual (pré-GA, nó único).

# Follow-ups aceitos como parte da decisão

- **ENTREGUE:** `theodb.embed_batch(text[]) RETURNS vector[]`, espelhando o
  `ai.generate_batch`. O endpoint de embeddings aceita array de entrada, então é mitigação de
  baixo esforço e alta alavanca que fecha o N+1 de embed — a operação em massa mais comum — sem
  mudança de desenho. Entregue com benchmark reproduzível de latência N→1 em
  [audit-remediation-embed-batch](/benchmarks/audit-remediation-embed-batch.md). Um retry
  limitado para classe recuperável também foi adicionado ao cliente de embed e ao `ai._chat`.
- Documentar `LIMIT` e tamanho de lote recomendados no COMMENT de cada função, e considerar um
  GUC de teto de concorrência no servidor.

# Consequências

**Bom:** paridade AlloyDB preservada; correção do planner garantida; nenhuma infraestrutura
assíncrona prematura; raio de dano limitado por `REVOKE … FROM PUBLIC`.

**Ruim:** uma chamada sobre a coluna inteira prende um backend por linha pela soma das latências,
de modo que **`max_connections` — não CPU nem RAM — é a primeira parede vertical** sob fan-out.

**Gatilho de reabertura:** workload medido de embedding ou geração em massa que exaure backends,
ou um cliente rodando embeds sobre corpora grandes numa única instrução.[^adr0007]

A evolução desta linha está no [ADR 0049](/decisions/0049-m122-three-phase-async-embed.md), que
introduz o embed assíncrono em três fases — exatamente o caminho que este ADR deixou gated.
O [ADR 0008](/decisions/0008-no-embedding-chat-cache.md) registra a ausência de cache, decisão
irmã desta.

[^adr0007]: ADR 0007 — Synchronous per-row model HTTP calls
