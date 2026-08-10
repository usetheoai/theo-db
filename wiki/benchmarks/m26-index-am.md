---
type: Measurement
title: m26 — access methods vetoriais persistidos: recall e latência
description: Evidência de que os índices deixaram de ser reconstruídos por query e passaram a ser access methods do PostgreSQL, com escopo declarado de opclass única.
resource: git:f7c7b93:docs/benchmarks/m26-index-am.md
tags: [benchmark, index-am, persistencia, pushdown, m26]
milestone: M26
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m26
    resource: git:f7c7b93:docs/benchmarks/m26-index-am.md
    title: M26 — Vector Index Access Method evidence
---

A medição que acompanha o [ADR 0010](/decisions/0010-m26-index-am-scope.md) — o momento em que os índices
próprios deixaram de ser funções que reconstroem o grafo a cada chamada e viraram **access methods
persistidos**.

# Cobertura do critério de pronto

| Item | Evidência |
|---|---|
| Rotina de access method registrada, com todos os hooks | ambos aparecem no catálogo |
| `CREATE INDEX … USING` persistindo em páginas, não reconstruindo por query | tamanho de relação maior que zero, mais a latência |
| Pushdown do planner para `ORDER BY … LIMIT k` | `EXPLAIN` mostra Index Scan |
| Manutenção incremental de INSERT, DELETE e VACUUM | teste dedicado |
| Coexistência com a forma chamável anterior, sem quebrar nada | 61 testes das fatias anteriores verdes |

**A coexistência testada é o que torna a migração segura**: o caminho antigo continua funcionando
enquanto o novo é validado.

# Escopo honesto

Ambos os access methods embarcam **apenas a opclass L2**. As de cosseno e produto interno são follow-up
documentado — a métrica **é gravada no blob persistido**, mas resolvê-la a partir da opclass no momento
do build exigiria um lookup de catálogo que a versão do ferramental não expõe.

**Declarar a limitação com a causa técnica** é o que permite a alguém depois avaliar se ela ainda vale.

# O número que virou baseline

O ganho medido foi **16× contra a reconstrução por query** — e essa mesma medição expôs a limitação
seguinte, o scan O(N) por blob, que o [m31](/benchmarks/m31-am-latency.md) fecharia com mais 45×.

# Relacionados

As decisões de escopo estão no [ADR 0010](/decisions/0010-m26-index-am-scope.md); as features
correspondentes são [HNSW](/features/02-indice-hnsw.md) e [IVFFlat](/features/03-indice-ivfflat.md).
