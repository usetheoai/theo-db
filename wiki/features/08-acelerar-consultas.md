---
type: Feature
title: Acelerar consultas de IA por batching de round-trips
description: N prompts numa única chamada HTTP via ai.generate_batch e ai.if_batch, com fatiamento automático — a aceleração entregue é essa, e não um proxy model local.
resource: git:f7c7b93:docs/features/08-acelerar-consultas.md
tags: [feature, ai-surface, batching, n+1, performance]
feature_status: entregue
milestone: M11+M18
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat08
    resource: git:f7c7b93:docs/features/08-acelerar-consultas.md
    title: Acelerar consultas com funções otimizadas
---

**Status: entregue.** A aceleração real do TheoDB é o **batching de round-trips** — colapsar N chamadas
por linha numa só. O "proxy model local" no estilo do [AlloyDB](/technologies/alloydb.md) descrito em
material de roadmap **não é a superfície entregue**.

# O mecanismo

```sql
ai.generate_batch(prompts TEXT[], model TEXT DEFAULT NULL) RETURNS TEXT[]
ai.if_batch(condition TEXT, vals TEXT[], model TEXT DEFAULT NULL) RETURNS BOOLEAN[]
```

Ambas respondem N entradas em **um único round-trip**, no padrão N-in/N-out, eliminando o N+1 que o
caminho por linha impõe por decisão registrada
([ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)). Chamadas grandes são **fatiadas
automaticamente** em blocos configuráveis por `theodb.ai_max_batch`, cujo default é 256 — o bound que
o [ADR 0047](/decisions/0047-m104-scaling-tradeoffs-deliberate.md) instalou.

# Os dois padrões

**Geração em massa:**

```sql
SELECT UNNEST(
  ai.generate_batch(ARRAY_AGG('Summarize in 20 words: ' || review))
)
FROM restaurant_reviews;
```

**Classificação booleana em massa:**

```sql
SELECT UNNEST(
  ai.if_batch('Is this a positive review?', ARRAY_AGG(review))
)
FROM restaurant_reviews;
```

O idioma é o mesmo: agregar as entradas num array, uma chamada, e reexpandir o array de respostas em
linhas.

# O ganho medido

Em [m102](/benchmarks/archive/m102-ai-operators.md): **1 round-trip contra N** (1 contra 1000 no modelo
determinístico), e latência **≈12× menor** contra um modelo real. Além disso, `ai.call_count()` permite
**provar** o número de round-trips em tempo de query, em vez de confiar na documentação.

# O outro eixo de aceleração: não chamar

Para predicados, o push-down do
[ADR 0043](/decisions/0043-m102-ai-operators-batched-pushdown.md) é frequentemente mais eficaz que o
batching, porque **evita a chamada em vez de agrupá-la**:

```sql
SELECT * FROM tickets
WHERE status = 'open'                          -- barato, avaliado primeiro
  AND ai.if_costly('descreve falha de pagamento', body);
```

O `COST` alto declarado no predicado faz a ordenação de quais do PostgreSQL cuidar disso — a IA só roda
nos sobreviventes do filtro barato.

# Ressalva honesta

Qualidade e latência dependem do modelo configurado. E o batch tem um efeito estatístico próprio: as N
perguntas compartilham uma mensagem, o que produz *context bleed*. As respostas **não são asseridas
byte-idênticas** às do caminho por linha num modelo real — a correção do mecanismo é provada com um
modelo determinístico, e o modelo real é o benchmark.
