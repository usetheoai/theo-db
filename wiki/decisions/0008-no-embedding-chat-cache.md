---
type: Decision
title: ADR 0008 — Sem cache de embedding/chat no v1 (YAGNI explícito)
description: Nenhum cache de resultado de modelo no v1; cache de embedding determinístico é a otimização futura sancionada, memoização de chat é rejeitada por semântica.
resource: git:f7c7b93:docs/adr/0008-no-embedding-chat-cache.md
tags: [adr, ai-surface, custo, yagni, cache]
adr_id: "0008"
adr_status: Accepted
decision_date: 2026-06-29
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0008
    resource: git:f7c7b93:docs/adr/0008-no-embedding-chat-cache.md
    title: ADR 0008 — No embedding/chat result cache in v1
    last_modified: 2026-06-29
---

Um ADR cuja única função é converter uma **decisão por omissão** em YAGNI registrado. O risco
que ele endereça não é de correção — é o de um trade-off intencional parecer descuido.

# Problema

`theodb.embed` e as funções `ai.*` batem no endpoint do modelo a cada chamada, mesmo para
entradas `(content, model)` idênticas. Embeddings de um modelo fixo são **determinísticos**, então
um cache `(content, model) -> vector` seria otimização legítima de custo e latência. Nenhum
existia, e o adiamento não estava registrado em lugar nenhum — um grep por `cache`, `memoize` e
`materialize` no código de produção não retornava nada.

# Direcionadores

YAGNI e KISS. Measurement-first — adicionar cache só quando um custo medido de re-embedding o
justificar. Consistência com o modelo `VOLATILE` por chamada do
[ADR 0007](/decisions/0007-synchronous-per-row-model-http.md): chamadas de LLM/chat são tratadas
como não-determinísticas e com efeito colateral, então cachear chat é **semanticamente errado**;
cachear embeddings determinísticos é o único candidato legítimo. E honestidade — nomear o YAGNI
para que seja decisão, não acidente.

# Decisão

**Sem cache no v1**, explicitamente. Embeddings e resultados de chat são tratados como chamadas
externas `VOLATILE`.

| Opção | Veredito | Razão |
|---|---|---|
| Sem cache (stateless por chamada) | **Escolhida** | mais simples; sem invalidação; sem staleness; casa com o contrato `VOLATILE` |
| Tabela de cache `(content, model) -> vector` | Otimização futura **sancionada** | segura porque determinística; gated em custo medido |
| Memoização de resultado de chat | **Rejeitada** | chat é não-determinístico e com efeito colateral — cachear muda a semântica |

# Consequências

**Bom:** nenhuma superfície de invalidação de cache, nenhum risco de vetor obsoleto, nenhum
estado novo a gerenciar, nenhuma complexidade prematura.

**Ruim:** embeds idênticos repetidos pagam latência e custo de endpoint integrais toda vez; um
workload pesado em re-embedding não tem alívio até o cache existir.

**Gatilho de reabertura:** custo medido de re-embedding — taxa de duplicatas `(content, model)`
multiplicada pelo custo e latência do endpoint pago — que justifique a tabela de cache.[^adr0008]

[^adr0008]: ADR 0008 — No embedding/chat result cache in v1
