# Ranquear resultados de busca

> **Status:** ✅ **Entregue (M7-S3).** A função `ai.rank(prompt text, model text DEFAULT NULL) RETURNS float4`
> (`theodb_rs/src/api.rs:338`, implementada em `theodb_rs/src/chat.rs:90` `ai_rank`) atribui um score de relevância
> via LLM, parseando a saída para float com erro tipado em saída malformada. Provado por
> `benchmarks/tests/test_ai_sql.py` (`test_rank_parses_float:265`, `test_rank_malformed_output_raises_typed:283`).
> Para fusão keyword+vetor determinística sem LLM, ver também `ai.hybrid_search_rrf` (feature 06 — RRF).
> **Nota de honestidade:** a qualidade do ranking depende do modelo LLM configurado (modelo síncrono por-linha,
> ADR `docs/adr/0007-synchronous-per-row-model-http.md`); não há benchmark de qualidade de ranking publicado.

Esta página cobre duas funções **distintas** entregues no TheoDB, incluindo o pipeline
híbrido que combina busca vetorial (`pgvector`) com reranking semântico para aplicações RAG:

- **`ai.rank(prompt text, model text DEFAULT NULL) RETURNS real`** — um scorer **escalar**:
  recebe **um** prompt e devolve **um** score de relevância (float). Um prompt → um score.
- **`ai.rerank(query text, documents text[], model text DEFAULT NULL, top_n int DEFAULT NULL) RETURNS TABLE(idx int, score real)`**
  — o reranker **em lote** (cross-encoder): recebe a query + um array de documentos e devolve
  `(idx, score)` por documento, onde **`idx` é 0-based** (o primeiro documento é `idx = 0`).

Não confunda as duas: `ai.rank` é escalar (1→1); `ai.rerank` é o batch (N→N com `idx`/`score`).

---

# 1. Instalar a extensão

```sql
CREATE EXTENSION IF NOT EXISTS theodb;
```

Instala a extensão `theodb`, que fornece as funções `ai.*` (incluindo `ai.rerank`/`ai.rank`) e o
schema `theodb_ml` (registro de modelos). `theodb_ml` é um **schema** dentro da extensão `theodb`,
não uma extensão separada.

---

# 3. Assinatura básica de `ai.rerank` (batch)

```sql
SELECT idx, score
FROM ai.rerank(
    'SEARCH_STRING',
    ARRAY[
        'DOCUMENT_1',
        'DOCUMENT_2',
        'DOCUMENT_3'
    ]
);
```

Assinatura entregue: `ai.rerank(query text, documents text[], model text DEFAULT NULL, top_n int DEFAULT NULL)`.
Classifica documentos de acordo com sua relevância para uma consulta, retornando
`(idx, score)` por documento — `idx` é **0-based**.

---

# 8. Definir o modelo (opcional)

```sql
ai.rerank('query', ARRAY['a','b'], model => 'theodb-ranker-default-003')
```

O 3º argumento (`model`) é opcional; quando `NULL` usa o modelo padrão configurado.

---

# 9. Definir a query

```sql
'Affordable family-friendly vacation spots'
```

Texto utilizado como critério de busca (1º argumento posicional).

---

# 10. Definir documentos

```sql
ARRAY[
    'Documento A',
    'Documento B',
    'Documento C'
]
```

Lista de documentos que serão classificados (2º argumento).

---

# 11. Consultar índice e score

```sql
SELECT
    idx,
    score
FROM ai.rerank(
    'Affordable family-friendly vacation spots',
    ARRAY[
        'Luxury resorts in South Korea',
        'Family vacation packages for Vietnam',
        'Budget beaches in Thailand'
    ]
);
```

Retorna a posição **0-based** (`idx`) e o score de relevância de cada documento.

---

# 12. Exemplo de ranking simples

```sql
SELECT
    idx,
    score
FROM ai.rerank(
    'TheoDB AI database',
    ARRAY[
        'Alloys are combinations of metals',
        'Enterprise-ready PostgreSQL database',
        'Apartment heating systems'
    ]
);
```

Ordena documentos conforme sua relevância para a consulta.

---

# 12b. Score escalar com `ai.rank`

```sql
SELECT ai.rank(
    'A busca "PostgreSQL vetorial" é relevante para: Enterprise-ready PostgreSQL database'
);
```

`ai.rank(prompt text, model text DEFAULT NULL) RETURNS real` avalia **um** prompt e
devolve **um** float de relevância. Distinta de `ai.rerank` (que é batch, N→N).

---

# 13. Recuperar Top-N após ranking vetorial

```sql
WITH initial_ranking AS (
    SELECT
        id,
        description,
        ROW_NUMBER() OVER () - 1 AS idx
    FROM product
    ORDER BY embedding
        <=> theodb.embed(
            'personal fitness equipment',
            'theodb-embedding-001'
        )::vector
    LIMIT 10
)
SELECT *
FROM initial_ranking;
```

Obtém inicialmente os 10 documentos mais próximos via busca vetorial. Note o
`ROW_NUMBER() OVER () - 1` — alinhado ao `idx` **0-based** de `ai.rerank`.

---

# 14. Gerar numeração dos documentos

```sql
ROW_NUMBER() OVER () - 1 AS idx
```

Cria uma referência **0-based** utilizada posteriormente no reranking (casa com o
`idx` retornado por `ai.rerank`).

---

# 15. Buscar embedding

```sql
theodb.embed(
    'personal fitness equipment',
    'theodb-embedding-001'
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
ARRAY_AGG(description ORDER BY idx)
```

Converte os candidatos em um array para envio ao modelo.

---

# 19. Reranking com `ai.rerank`

```sql
SELECT
    idx,
    score
FROM ai.rerank(
    'personal fitness equipment',
    (
        SELECT ARRAY_AGG(description ORDER BY idx)
        FROM initial_ranking
    ),
    top_n => 5
);
```

Reordena semanticamente os candidatos da busca vetorial. O `idx` retornado é
**0-based** e casa com o `idx` do CTE.

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
WHERE initial_ranking.idx =
      reranked_results.idx
ORDER BY
      reranked_results.score DESC;
```

Fluxo completo de reranking.

---

# 22. Relacionar índices

```sql
WHERE initial_ranking.idx =
      reranked_results.idx
```

Relaciona os documentos originais ao ranking produzido pela IA. Ambos os lados são
**0-based** (`ROW_NUMBER() OVER () - 1` no CTE, `idx` de `ai.rerank`) — sem off-by-one.

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
        ROW_NUMBER() OVER () - 1 AS idx
    FROM user_reviews
    ORDER BY
        review_desc_embedding
        <=>
        theodb.embed(
            'good desserts',
            'theodb-embedding-001'
        )::vector
    LIMIT 10
)
```

Obtém candidatos utilizando embeddings das avaliações (`idx` **0-based**).

---

# 25. Reranquear avaliações

```sql
SELECT
    idx,
    score
FROM ai.rerank(
    'good desserts',
    (
        SELECT ARRAY_AGG(review ORDER BY idx)
        FROM initial_ranking
    ),
    model => 'theodb-ranker-512',
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
WHERE initial_ranking.idx =
      reranked_results.idx
ORDER BY
      reranked_results.score DESC;
```

Retorna os produtos associados aos melhores documentos.

---

# 27. Utilizar `theodb-ranker-default-003`

```sql
ai.rerank('query', ARRAY[...], model => 'theodb-ranker-default-003')
```

Modelo de ranking semântico padrão.

---

# 28. Utilizar `theodb-ranker-512`

```sql
ai.rerank('query', ARRAY[...], model => 'theodb-ranker-512')
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
    <=> theodb.embed(
        'fitness equipment',
        'theodb-embedding-001'
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
        ROW_NUMBER() OVER () - 1 AS idx
    FROM product
    ORDER BY
        embedding
        <=> theodb.embed(
            'fitness equipment',
            'theodb-embedding-001'
        )::vector
    LIMIT 10
),
reranked_results AS (
    SELECT
        idx,
        score
    FROM ai.rerank(
        'fitness equipment',
        (
            SELECT ARRAY_AGG(description ORDER BY idx)
            FROM initial_ranking
        ),
        model => 'theodb-ranker-default-003',
        top_n => 5
    )
)
SELECT
    id,
    description
FROM initial_ranking,
     reranked_results
WHERE initial_ranking.idx = reranked_results.idx
ORDER BY reranked_results.score DESC;
```

Fluxo completo recomendado para aplicações RAG (Retrieval-Augmented Generation):

1. gera o embedding da consulta;
2. executa busca vetorial (`pgvector`);
3. recupera os candidatos mais próximos;
4. envia os candidatos ao reranker em lote `ai.rerank`;
5. reranqueia semanticamente (`idx` **0-based** casa com `ROW_NUMBER() OVER () - 1`);
6. retorna os documentos em ordem de maior relevância.
