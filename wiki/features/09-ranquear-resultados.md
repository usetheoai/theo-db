---
type: Feature
title: Ranquear resultados de busca (ai.rank e ai.rerank)
description: Duas funções distintas — um scorer escalar por LLM e um reranker cross-encoder em lote — cuja diferença é a confusão mais provável desta superfície.
resource: git:f7c7b93:docs/features/09-ranquear-resultados.md
tags: [feature, rerank, ranking, cross-encoder, rag]
feature_status: entregue
milestone: M7-S3+M65
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat09
    resource: git:f7c7b93:docs/features/09-ranquear-resultados.md
    title: Ranquear resultados de busca
---

**Status: entregue.** Duas funções **distintas**, e não confundi-las é o ponto principal desta página.

# As duas funções

| | `ai.rank` | `ai.rerank` |
|---|---|---|
| Forma | **escalar**, 1 → 1 | **lote**, N → N |
| Assinatura | `(prompt text, model text) RETURNS float4` | `(query text, documents text[], model text, top_n int) RETURNS TABLE(idx int, score real)` |
| Mecanismo | julgamento por LLM, uma chamada por item | **cross-encoder**, query e documentos juntos |
| Custo | N round-trips | 1 round-trip |

```sql
SELECT idx, score
FROM ai.rerank('running shoes', ARRAY['doc A', 'doc B', 'doc C'])
ORDER BY score DESC;
```

**`idx` é 0-based** — o primeiro documento do array é `idx = 0`. É por isso que a função retorna índice
em vez de reordenar in-place: o `idx` é o que permite juntar de volta às linhas de origem, decisão
registrada no [ADR 0024](/decisions/0024-m65-ai-rerank-cross-encoder.md).

O nome `rerank` diverge do [AlloyDB](/technologies/alloydb.md), que chama o dele de `rank`, **de
propósito** — porque `ai.rank` aqui já existe e significa outra coisa.

# O veredito medido — leia antes de ativar

O benchmark em [BEIR](/technologies/beir.md)/SciFact deu **honest-negative**: o rerank
**degradou** o nDCG@10 em **−3,8%**, ao custo de ~1,96 s de p50 por query
([m65](/benchmarks/archive/m65-rerank.md)).

Isso é consistente com a literatura: cross-encoders off-the-shelf regridem em corpora **fora da
distribuição** de treino. A superfície embarca porque é **correta, model-agnostic e mensurável** — e
não porque entregue ganho universal.

**Consequências práticas:** rerank é **opt-in**, nunca default. O operador escolhe o reranker adequado
ao seu corpus por GUC, e **um reranker in-domain pode ganhar onde este perdeu — mas isso exige o
próprio benchmark, não extrapolação.**

# A alternativa determinística, sem LLM

Para fundir a perna de palavra-chave com a vetorial **sem** chamar modelo nenhum, use a
[busca híbrida](/features/06-busca-hibrida.md) por RRF. Ela é determinística, não custa round-trip e
tem recall medido — em muitos casos é a escolha certa antes de considerar rerank.

# Pipeline de RAG típico

1. recuperar candidatos com [busca vetorial](/features/01-busca-similaridade-vetorial.md) ou híbrida;
2. opcionalmente rerankear com `ai.rerank` — medindo se ganha no **seu** corpus;
3. montar o contexto e gerar com [funções de IA](/features/07-funcoes-ia-sql.md).

O padrão de query unificada que faz isso numa SQL só está no
[ADR 0023](/decisions/0023-m64-rag-unified-not-columnar-planner.md).

# Ressalva

Não há benchmark publicado de **qualidade de ranking** do `ai.rank` — a qualidade depende do modelo
configurado, e o modelo é síncrono por linha.
