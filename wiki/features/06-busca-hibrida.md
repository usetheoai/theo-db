---
type: Feature
title: Busca híbrida (vetorial + lexical por RRF)
description: Funde a perna vetorial com a lexical por Reciprocal Rank Fusion ponderável, com superfície de função direta ou por configuração JSON.
resource: git:f7c7b93:docs/features/06-busca-hibrida.md
tags: [feature, busca-hibrida, rrf, fts, bm25, sql]
feature_status: entregue
milestone: M7-S1+M13+M19
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat06
    resource: git:f7c7b93:docs/features/06-busca-hibrida.md
    title: Busca híbrida por similaridade vetorial
---

**Status: entregue.** A fusão é [RRF](/technologies/rrf.md) **ponderável**, combinando a perna vetorial
com a lexical. Recall medido em [m7](/benchmarks/m7-hybrid-recall.md).

# A fórmula

$$
\mathrm{score}(d) = \frac{w_{vec}}{k + \mathrm{rank}_{vec}(d)} + \frac{w_{txt}}{k + \mathrm{rank}_{txt}(d)}
$$

com `k = 60` por padrão e ambos os pesos em `1.0`, o que reduz à RRF pura. Documentos presentes numa só
perna entram por junção externa, sem serem penalizados por ausência na outra. Empates são desempatados
por id ascendente, o que torna o resultado determinístico.

# As duas superfícies

**Função direta:**

```sql
SELECT * FROM ai.hybrid_search_rrf(...);
```

**Por configuração JSON**, que é um wrapper fino sobre a mesma implementação — uma só fonte de verdade
da fusão:

```sql
SELECT * FROM ai.hybrid_search('{...}'::jsonb);
```

As chaves **efetivamente honradas** pelo código são: `table`, `id_col`, `content_tsv_col`,
`content_text_col`, `vector_col`, `query_text`, `query_vector`, `k`, `per_leg_limit`, `result_limit`,
`language`, `filter_sql`, `lexical_engine`, `vector_weight` e `text_weight`. Ambas as funções são
revogadas de PUBLIC.

# A tabela — e a armadilha do embedding gerado

```sql
CREATE TABLE documents (
    doc_id TEXT PRIMARY KEY,
    content TEXT,
    text_tsv tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED,
    text_embedding vector(3072)   -- NÃO pode ser GENERATED
);
```

**`theodb.embed` não pode aparecer numa coluna `GENERATED ALWAYS AS … STORED`.** Ela faz uma chamada
HTTP por linha e é `VOLATILE` por decisão registrada
([ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)), enquanto o PostgreSQL exige expressão
`IMMUTABLE` nessa posição.

Preencha explicitamente — **conteúdo primeiro, modelo depois**:

```sql
UPDATE documents SET text_embedding = theodb.embed(content, 'theodb-embedding-001')
WHERE text_embedding IS NULL;
```

Para manter atualizado automaticamente, use o [vectorizer](/features/16-vectorizer.md).

# A perna lexical — o que efetivamente embarca

**A perna de texto embarcada é o full-text search nativo do PostgreSQL (`ts_rank_cd` com GIN).** A peça
[BM25](/technologies/bm25.md) SOTA de mercado é AGPL e portanto barrada
([ADR 0003](/decisions/0003-permissive-bm25-pg-textsearch.md)).

Existe um [motor lexical BM25 próprio](/features/18-motor-lexical-bm25.md), decidido como a superfície
BM25 de produção pelo [ADR 0054](/decisions/0054-m140-3-bm25-supersede-textsearch.md) — mas ele é
compilado apenas sob feature flag e **não está no binário default**. A chave `lexical_engine` existe
para selecionar a perna quando ele está presente.

# Ressalva medida sobre a fusão

Esta é a parte que muda decisões: **na fusão RRF, trocar `ts_rank_cd` por BM25 não mede ganho** — e num
corpus lexical-pesado a troca mede **pior** ([m138](/benchmarks/m138-bm25-fusion.md), um
honest-negative). A vantagem do BM25 aparece em **lexical puro**, e a fusão a lava.

É exatamente por isso que o default embarcado permanece o nativo, apesar de o BM25 próprio existir e
ser melhor no eixo isolado.

A fusão também passou por teste de significância
([m123](/benchmarks/m123-hybrid-significance.md)), com a mesma lição de método do
[ADR 0012](/decisions/0012-benchmark-data-degeneracy.md): coeficiente de variação não é significância
pareada.

# Relacionados

Rerank de segunda ordem sobre o resultado em
[ranquear resultados](/features/09-ranquear-resultados.md), e a query unificada de RAG em
[funções de IA](/features/07-funcoes-ia-sql.md).
