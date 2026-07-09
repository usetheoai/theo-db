---
slug: own-vector-type-drop-pgvector
generated_by: roadmap-feature
date: 2026-07-09
status: completed
milestones_added: [M69, M70]
source_of_truth: .claude/knowledge-base/discoveries/blueprints/own-vector-type-drop-pgvector-blueprint.md
grill_mode: derived-from-blueprint
---

# Feature grill — Independência do pgvector (own vector type)

> **Nota (Regra 1 / grill-me skip condition):** as 4 perguntas do grill foram respondidas
> pelo **blueprint de discovery SHIPPABLE (99.7)** — uma spec detalhada de rigor superior a
> um grill de 4 perguntas (discover-plan → edge-cases → plan-confidence 89 → execute[4 agentes
> R0 web] → confidence 99.7). O grill interativo foi dispensado por já haver 95%+ de confiança
> com evidência. Abaixo, o mapeamento das respostas ao blueprint.

## Q1 — O que é e por que AGORA?

Remover a dependência do pgvector **totalmente**, substituindo o tipo `vector` (hoje do pgvector,
consumido via `::real[]`) por um tipo próprio own-code. **Por que agora:** pedido explícito do
usuário (2026-07-09) + objetivo LOCKED do North Star ("substituir pgvector/pgvectorscale por
código próprio é o objetivo") + fecho dos milestones v2 M20→M22 gated em paridade. Fonte: blueprint
§ Context.

## Q2 — Dependências

M68 (roadmap v3 completo) para M69; M69 para M70. Blueprint § ADR-D2 (decomposição 2 milestones
com gate de paridade entre eles).

## Q3 — Definition of Done (verificável)

Ver os DoDs de M69 (spike pgrx gate + tipo próprio + gate de paridade byte-a-byte + coexistência)
e M70 (opclasses religadas + migração cast-binário + requires/Dockerfile zerados + gate set-equal
de recall + pgvector ausente) no ROADMAP.md. Todos com gate executável (blueprint § Corner 3 corpus
de paridade; § Corner 1 gate set-equal-vs-seqscan já existente em `hnsw_page.rs:2201-2214`).

## Q4 — Top 2 riscos NOVOS

1. **Spike pgrx (MÉDIO-ALTO):** nenhum peer permissivo shipa tipo `vector` próprio em pgrx —
   definir um tipo denso de dimensão-variável em pgrx 0.16.1 é território novo (blueprint § Corner 4
   Q2 "GAP honesto / UNKNOWN"). Mitigação: spike D3 como gate de continuação ANTES de qualquer build.
2. **Regressão silenciosa de recall (MÉDIO):** ao religar opclasses ao tipo próprio (M70), um bug
   no binding tipo↔AM pode degradar recall sem quebrar teste. Mitigação: o gate set-equal-vs-seqscan
   (top-k índice == top-k exato) + coexistência de M69 como oráculo/rollback (anti-sunk-cost).

## Out-of-scope cross-check

Sem conflito: "own vector type / remover pgvector" NÃO está em `## Fora de escopo do v2` (que trata
de control-plane/K8s/Go operator + reabertura de columnar). É explicitamente IN-scope pelo North Star
(M20→M22). Nenhum item de out-of-scope removido.

## SOTA delta

Não necessário — as referências `pgvector`, `vectorchord`, `pgvectorscale`, `postgres` já existem em
`.claude/knowledge-base/references/` e foram consumidas pelo blueprint. Nenhum peer novo clonado.
