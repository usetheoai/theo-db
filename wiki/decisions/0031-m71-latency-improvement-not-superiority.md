---
type: Decision
title: ADR 0031 — M71: o critério vira "melhoria de latência medida", não superioridade iso-recall
description: O build multi-entry entrega +29% de QPS recall-neutro e é embarcado, mas a iso-recall o TheoDB precisa de ~2× o ef — a superioridade fica gated na navegabilidade do grafo.
resource: git:f7c7b93:docs/adr/0031-m71-latency-improvement-not-superiority.md
tags: [adr, latencia, hnsw, multi-entry, qps, m71, measurement-first]
adr_id: "0031"
adr_status: Accepted
decision_date: 2026-07-10
milestone: M71
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0031
    resource: git:f7c7b93:docs/adr/0031-m71-latency-improvement-not-superiority.md
    title: ADR-0031 — M71 latency improvement
    last_modified: 2026-07-10
---

Segue o mesmo padrão do [ADR 0030](/decisions/0030-m60-recall-parity-not-absolute-099.md): entregar o
ganho real medido e reenquadrar o critério que a evidência não sustenta.

# Contexto medido

**O que foi entregue de fato:** multi-entry `ep←W` no build, seguindo o algoritmo original do
[HNSW](/technologies/hnsw.md) e o precedente do pgvector — **+29% de QPS a 500k × 768d**,
recall-neutro (0,972 contra 0,974), com toda a suíte verde. Melhoria de throughput medida e
embarcada ([m71](/benchmarks/m71-scan-latency.md)).

**Veredito iso-recall, honesto:** a recall 0,996 e 100k, o pgvector faz 2,13 ms com `ef=100`, e o
TheoDB faz 3,16 ms com `ef=200`. O TheoDB **não é superior nem está em paridade** de latência —
precisa de ~2× o `ef` (e ~5× a 500k). A causa é a mesma lacuna de **navegabilidade do grafo** que
gateia o recall, com sete alavancas já refutadas
([root blocker](/benchmarks/p0-vector-superiority-root-blocker.md)). Cortes de custo por candidato
reduzem o p50 absoluto mas **não mudam a razão iso-recall**.

# Decisão

1. O critério passa a ser **"melhoria de latência medida"**, e não superioridade iso-recall, que
   está empiricamente gateada na navegabilidade.
2. Superioridade ou paridade iso-recall vira **follow-up autorizado**, gated na mesma lacuna
   compartilhada com o recall.
3. **Sem claim de superioridade:** é melhoria medida mais paridade de recall.

# Alternativas rejeitadas

**Manter o critério de superioridade iso-recall** — perseguir sem resolver a raiz é anti
measurement-first. **Não embarcar o multi-entry** — é ganho real, recall-neutro e testado; segurá-lo
seria descartar valor comprovado.

# Consequências

Entrega um ganho de latência medido e correto, com rastreabilidade honesta entre o que é **melhoria**
(medida) e o que é **superioridade** (gated).

**Trade-off honesto:** o build multi-entry é **mais lento** — mais trabalho por insert —, troca
aceitável num índice focado em latência de query.[^adr0031]

A causa-raiz seria atacada no [ADR 0034](/decisions/0034-hnsw-extend-candidates-navigability.md).

[^adr0031]: ADR-0031 — M71: DoD vira "melhoria de latência medida"
