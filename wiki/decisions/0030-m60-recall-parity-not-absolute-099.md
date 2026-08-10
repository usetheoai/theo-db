---
type: Decision
title: ADR 0030 — O critério de recall vira PARIDADE com o pgvector, não 0,99 absoluto
description: A medição refutou a premissa do milestone — o próprio pgvector só atinge 0,988 neste corpus, então 0,99 era artefato do dado, e o critério passa a ser paridade medida.
resource: git:f7c7b93:docs/adr/0030-m60-recall-parity-not-absolute-099.md
tags: [adr, recall, hnsw, paridade, sbq, m60, measurement-first]
adr_id: "0030"
adr_status: Accepted
decision_date: 2026-07-10
milestone: M60
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0030
    resource: git:f7c7b93:docs/adr/0030-m60-recall-parity-not-absolute-099.md
    title: ADR-0030 — M60 recall parity
    last_modified: 2026-07-10
---

Outro caso de critério corrigido por medição: o alvo não foi afrouxado por conveniência, foi
**refutado como artefato do dado**.

# Contexto medido

O milestone nasceu com o critério `recall@10 ≥ 0,99` a 500k × 768d, sob a premissa de que o
`theodb_hnsw` tinha um gap de recall **específico** de 2 a 3 pontos contra o pgvector, e de que 0,99
era alcançável. O head-to-head no **mesmo corpus** refuta a premissa
([m60](/benchmarks/m60-hnsw-recall.md)):

| Índice (ef=1000, 500k × 768d, mesmo corpus, GT exato) | recall@10 |
|---|---|
| pgvector hnsw (m=16, efc=64) | **0,988** |
| theodb_hnsw **SBQ** (over_fetch=32, rerank) | **0,986** |
| theodb_hnsw f32 | 0,974 |

Dois fatos saem daí:

1. **O gate de 0,99 é artefato do dado** — *o próprio pgvector só chega a 0,988*. São 256 clusters
   gaussianos apertados em 768 dimensões, o que produz muitos décimos-vizinhos quase equidistantes e
   um teto de `recall@10` abaixo de 0,99 para índices da classe [HNSW](/technologies/hnsw.md).
   Perseguir 0,99 absoluto é perseguir um número que o SOTA permissivo não atinge nesta distribuição.
2. **O caminho SBQ já está em paridade** — 0,986 contra 0,988, dentro do ruído de um slot de ground
   truth sobre 500. O caminho f32 puro fica ~1,4 ponto atrás.

# Decisão

1. O critério de recall passa a ser **paridade com o pgvector** — o oráculo de controle —, medida no
   mesmo corpus, e **não** o valor absoluto. Isso alinha com a moldura de recall-parity que o
   north-star já usa.
2. O milestone é **fechado pelo caminho SBQ**, com a paridade medida como artefato.
3. O gap de ~1,4 ponto do caminho f32 fica registrado como **follow-up autorizado**, sem bloquear
   nada. **Cinco alavancas de recall já foram refutadas por medição** — aumentar `ef_construction`,
   MERGE de back-links, aumentar `m`, descida de beam com `ef=1`, e multi-entry —, e a causa do
   resíduo é um detalhe sutil de implementação que exigiria investigação profunda e incerta.[^adr0030]

# Alternativas rejeitadas

**Manter o critério em 0,99 e continuar caçando o gap f32** — perseguir um alvo que o pgvector não
atinge, com cinco alavancas já derrubadas; violaria measurement-first e a regra anti-sunk-cost.
**Marcar o milestone como pronto sem ADR nem evidência** — seria fabricar conclusão.

# Consequências

**Positivas:** fecha com evidência medida e honesta, e destrava os milestones seguintes. O critério
passa a ser alcançável e comparável.

**Trade-off honesto:** a paridade de recall é do caminho **SBQ**, que tem custo de QPS contra f32,
conforme o [ADR 0018](/decisions/0018-m57-sbq-inline-not-superior.md). Portanto entrega-se
**paridade de recall** — não superioridade de recall nem de latência. A latência é escopo do
milestone seguinte, que herda um achado medido: o grafo multi-entry rende +29% de QPS a recall igual.

[^adr0030]: ADR-0030 — M60: DoD do recall vira PARIDADE-pgvector
