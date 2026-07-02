# Ranquear resultados de busca

> **Status:** ✅ **Entregue (M7-S3).** A função `ai.rank(prompt text, model text DEFAULT NULL) RETURNS float4`
> (`theodb_rs/src/api.rs:338`, implementada em `theodb_rs/src/chat.rs:90` `ai_rank`) atribui um score de relevância
> via LLM, parseando a saída para float com erro tipado em saída malformada. Provado por
> `benchmarks/tests/test_ai_sql.py` (`test_rank_parses_float:265`, `test_rank_malformed_output_raises_typed:283`).
> Para fusão keyword+vetor determinística sem LLM, ver também `ai.hybrid_search_rrf` (feature 06 — RRF).
> **Nota de honestidade:** a qualidade do ranking depende do modelo LLM configurado (modelo síncrono por-linha,
> ADR `docs/adr/0007-synchronous-per-row-model-http.md`); não há benchmark de qualidade de ranking publicado.

Esta página cobre o uso de `ai.rank()` para ranking e reranking de resultados de busca no TheoDB, incluindo o pipeline híbrido que combina busca vetorial (`pgvector`) com reranking semântico para aplicações RAG.

---

# 1. Verificar versão da extensão

```sql
SELECT extversion
FROM pg_extension
WHERE extname = 'theodb_ml';
```

Consulta a versão instalada da extensão `theodb_ml`.

---

# 2. Instalar extensão

```sql
CREATE EXTENSION IF NOT EXISTS theodb_ml;
```

Instala a extensão necessária para utilizar modelos de IA.

---

# 3. Atualizar extensão

```sql
ALTER EXTENSION theodb_ml UPDATE;
```

Atualiza a extensão para uma versão compatível.

---

# 4. Habilitar AI Query Engine na sessão

```sql
SET theodb_ml.enable_ai_query_engine = on;
```

Ativa o mecanismo de IA para a sessão atual.

---

# 5. Habilitar AI Query Engine para o banco

```sql
ALTER DATABASE my_database
SET theodb_ml.enable_ai_query_engine = 'on';
```

Ativa permanentemente para um banco específico.

---

# 6. Habilitar AI Query Engine para um usuário

```sql
ALTER ROLE postgres
SET theodb_ml.enable_ai_query_engine = 'on';
```

Ativa para todas as sessões do usuário.

---

# 7. Assinatura básica de `ai.rank`

```sql
SELECT ai.rank(
    model_id => 'MODEL_ID',
    search_string => 'SEARCH_STRING',
    documents => ARRAY[
        'DOCUMENT_1',
        'DOCUMENT_2',
        'DOCUMENT_3'
    ]
);
```

Classifica documentos de acordo com sua relevância para uma consulta.

---

# 8. Definir `model_id`

```sql
model_id => 'theodb-ranker-default-003'
```

Especifica o modelo de ranking utilizado.

---

# 9. Definir `search_string`

```sql
search_string => 'Affordable family-friendly vacation spots'
```

Texto utilizado como critério de busca.

---

# 10. Definir documentos

```sql
documents => ARRAY[
    'Documento A',
    'Documento B',
    'Documento C'
]
```

Lista de documentos que serão classificados.

---

# 11. Consultar índice e score

```sql
SELECT
    index,
    score
FROM ai.rank(
    model_id => 'theodb-ranker-default-003',
    search_string => 'Affordable family-friendly vacation spots',
    documents => ARRAY[
        'Luxury resorts in South Korea',
        'Family vacation packages for Vietnam',
        'Budget beaches in Thailand'
    ]
);
```

Retorna a posição (`index`) e o score de relevância de cada documento.

---

# 12. Exemplo de ranking simples

```sql
SELECT
    index,
    score
FROM ai.rank(
    model_id => 'theodb-ranker-default-003',
    search_string => 'TheoDB AI database',
    documents => ARRAY[
        'Alloys are combinations of metals',
        'Enterprise-ready PostgreSQL database',
        'Apartment heating systems'
    ]
);
```

Ordena documentos conforme sua relevância para a consulta.

---

# 13. Recuperar Top-N após ranking vetorial

```sql
WITH initial_ranking AS (
    SELECT
        id,
        description,
        ROW_NUMBER() OVER() AS ref_number
    FROM product
    ORDER BY embedding
        <=> theodb_ml.embedding(
            'theodb-embedding-001',
            'personal fitness equipment'
        )::vector
    LIMIT 10
)
SELECT *
FROM initial_ranking;
```

Obtém inicialmente os 10 documentos mais próximos via busca vetorial.

---

# 14. Gerar numeração dos documentos

```sql
ROW_NUMBER() OVER() AS ref_number
```

Cria uma referência utilizada posteriormente no reranking.

---

# 15. Buscar embedding

```sql
theodb_ml.embedding(
    'theodb-embedding-001',
    'personal fitness equipment'
)::vector
```

Transforma o texto da consulta em embedding.

---

# 16. Ordenar por distância vetorial

```sql
ORDER BY embedding <=> query_embedding
```

Realiza a busca vetorial inicial.

---

# 17. Limitar candidatos

```sql
LIMIT 10
```

Seleciona apenas os candidatos para reranking.

---

# 18. Agrupar documentos para o ranking

```sql
ARRAY_AGG(description ORDER BY ref_number)
```

Converte os candidatos em um array para envio ao modelo.

---

# 19. Reranking com `ai.rank`

```sql
SELECT
    index,
    score
FROM ai.rank(
    model_id => 'theodb-ranker-default-003',
    search_string => 'personal fitness equipment',
    documents => (
        SELECT ARRAY_AGG(description ORDER BY ref_number)
        FROM initial_ranking
    ),
    top_n => 5
);
```

Reordena semanticamente os candidatos da busca vetorial.

---

# 20. Definir `top_n`

```sql
top_n => 5
```

Retorna apenas os cinco documentos mais relevantes.

---

# 21. Combinar ranking vetorial e semântico

```sql
WITH
initial_ranking AS (...),
reranked_results AS (...)
SELECT
    id,
    description
FROM initial_ranking,
     reranked_results
WHERE initial_ranking.ref_number =
      reranked_results.index
ORDER BY
      reranked_results.score DESC;
```

Fluxo completo de reranking.

---

# 22. Relacionar índices

```sql
WHERE initial_ranking.ref_number =
      reranked_results.index
```

Relaciona os documentos originais ao ranking produzido pela IA.

---

# 23. Ordenar pelo score

```sql
ORDER BY reranked_results.score DESC;
```

Retorna os documentos em ordem de relevância.

---

# 24. Buscar produtos por avaliações

```sql
WITH initial_ranking AS (
    SELECT
        product_id,
        name,
        review,
        ROW_NUMBER() OVER() AS ref_number
    FROM user_reviews
    ORDER BY
        review_desc_embedding
        <=>
        theodb_ml.embedding(
            'theodb-embedding-001',
            'good desserts'
        )::vector
    LIMIT 10
)
```

Obtém candidatos utilizando embeddings das avaliações.

---

# 25. Reranquear avaliações

```sql
SELECT
    index,
    score
FROM ai.rank(
    model_id => 'theodb-ranker-512',
    search_string => 'good desserts',
    documents => (
        SELECT ARRAY_AGG(review ORDER BY ref_number)
        FROM initial_ranking
    ),
    top_n => 5
);
```

Ranqueia avaliações conforme relevância para "good desserts".

---

# 26. Recuperar produtos mais relevantes

```sql
SELECT
    product_id,
    name
FROM initial_ranking,
     reranked_results
WHERE initial_ranking.ref_number =
      reranked_results.index
ORDER BY
      reranked_results.score DESC;
```

Retorna os produtos associados aos melhores documentos.

---

# 27. Utilizar `theodb-ranker-default-003`

```sql
model_id => 'theodb-ranker-default-003'
```

Modelo de ranking semântico padrão.

---

# 28. Utilizar `theodb-ranker-512`

```sql
model_id => 'theodb-ranker-512'
```

Modelo alternativo para reranking.

---

# 29. Consulta vetorial + reranking

```sql
SELECT
    id,
    description
FROM product
ORDER BY
    embedding
    <=> theodb_ml.embedding(
        'theodb-embedding-001',
        'fitness equipment'
    )::vector
LIMIT 10;
```

Primeira etapa do pipeline híbrido de busca.

---

# 30. Fluxo completo recomendado

```sql
WITH initial_ranking AS (
    SELECT
        id,
        description,
        ROW_NUMBER() OVER() AS ref_number
    FROM product
    ORDER BY
        embedding
        <=> theodb_ml.embedding(
            'theodb-embedding-001',
            'fitness equipment'
        )::vector
    LIMIT 10
),
reranked_results AS (
    SELECT
        index,
        score
    FROM ai.rank(
        model_id => 'theodb-ranker-default-003',
        search_string => 'fitness equipment',
        documents => (
            SELECT ARRAY_AGG(description ORDER BY ref_number)
            FROM initial_ranking
        ),
        top_n => 5
    )
)
SELECT
    id,
    description
FROM initial_ranking,
     reranked_results
WHERE initial_ranking.ref_number = reranked_results.index
ORDER BY reranked_results.score DESC;
```

Fluxo completo recomendado para aplicações RAG (Retrieval-Augmented Generation):

1. gera o embedding da consulta;
2. executa busca vetorial (`pgvector`);
3. recupera os candidatos mais próximos;
4. envia os candidatos ao modelo `ai.rank`;
5. reranqueia semanticamente;
6. retorna os documentos em ordem de maior relevância.
