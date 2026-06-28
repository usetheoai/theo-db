# Busca híbrida por similaridade vetorial

> **Status:** 📋 Especificação (planejado) — recurso-alvo do milestone **M7 — IA avançada** ([ROADMAP](../../ROADMAP.md)).
> Esta página documenta a **API-alvo do TheoDB**. As funcionalidades aqui descritas **ainda não estão
> implementadas** na release atual (M0 entrega PostgreSQL 17 + `pgvector`). Nenhum número de desempenho
> nesta página é um benchmark — benchmarks reproduzíveis vivem em `docs/benchmarks/` quando publicados
> (CLAUDE.md, regra TheoDB 5).

Esta página cobre a busca híbrida no TheoDB — combinação de busca vetorial (semântica) com Full Text Search — tanto pela função nativa `ai.hybrid_search()` quanto pela implementação manual via SQL com Reciprocal Rank Fusion (RRF).

> **Superfície implementada (M7-S1):** a primeira fatia entregue é a função SQL **`ai.hybrid_search_rrf(...)`**
> (`sql/40-theodb-hybrid.sql`) — o MVP manual-SQL da RRF (`score = Σ 1/(k+rank)`, k=60 default exposto como
> parâmetro; perna FTS `ts_rank_cd`/GIN + perna vetorial `pgvector` `<=>`; empty-leg via `FULL OUTER JOIN`+`COALESCE`).
> A API nativa `ai.hybrid_search()` com `search_inputs` JSON (abaixo) é um wrapper fino futuro sobre essa mesma
> função (uma única fonte de verdade da fusão). Recall medido (BEIR-style) em `docs/benchmarks/m7-hybrid-recall.md`.
> BM25 permissivo (`pg_search` é AGPL → barrado por D1) é a slice M7-S2.

---

# 1. Habilitar funções Preview

```sql
SET theodb_ml.enable_preview_ai_functions = true;
```

Ativa as funções experimentais necessárias para `ai.hybrid_search()`.

---

# 2. Criar tabela de documentos

```sql
CREATE TABLE documents (
    doc_id TEXT PRIMARY KEY,
    content TEXT,
    text_tsv tsvector GENERATED ALWAYS AS (
        to_tsvector('english', content)
    ) STORED,
    text_embedding vector(3072) GENERATED ALWAYS AS (
        embedding('theodb-embedding-001', content)
    ) STORED
);
```

Cria uma tabela contendo:

* identificador;
* texto original;
* coluna `tsvector` para Full Text Search;
* embedding vetorial gerado automaticamente.

---

# 3. Inserir documentos

```sql
INSERT INTO documents (doc_id, content)
VALUES (...);
```

Insere os documentos que participarão da busca híbrida.

---

# 4. Instalar extensão ScaNN

```sql
CREATE EXTENSION IF NOT EXISTS theodb_scann;
```

Habilita o mecanismo de indexação vetorial ScaNN.

---

# 5. Criar índice ScaNN

```sql
CREATE INDEX documents_text_embedding_idx
ON documents
USING scann (
    text_embedding cosine
)
WITH (
    num_leaves = 10,
    quantizer = 'SQ8'
);
```

Cria um índice vetorial para acelerar a busca semântica.

---

# 6. Criar índice GIN

```sql
CREATE INDEX documents_text_tsv_idx
ON documents
USING GIN (
    text_tsv
);
```

Cria um índice Full Text Search.

---

# 7. Criar índice GIN diretamente

```sql
CREATE INDEX my_gin_index
ON documents
USING GIN (
    to_tsvector('english', content)
);
```

Alternativa para tabelas sem coluna `tsvector` persistida.

---

# 8. Criar índice RUM

```sql
CREATE INDEX idx_documents_rum
ON documents
USING rum (
    text_tsv rum_tsvector_ops
);
```

Índice otimizado para Full Text Search.

---

# 9. Assinatura básica de `ai.hybrid_search`

```sql
SELECT *
FROM ai.hybrid_search(
    search_inputs => ARRAY[
        ...json...
    ]
);
```

Executa busca híbrida utilizando múltiplos mecanismos de pesquisa.

---

# 10. Definir componente vetorial

```json
{
  "data_type":"vector"
}
```

Indica que o componente utiliza busca vetorial.

---

# 11. Definir componente textual

```json
{
  "data_type":"text"
}
```

Indica que o componente utiliza Full Text Search.

---

# 12. Definir peso (`weight`)

```json
"weight":0.5
```

Peso relativo utilizado no cálculo do score final.

---

# 13. Definir tabela

```json
"table_name":"documents"
```

Tabela consultada.

---

# 14. Definir chave

```json
"key_column":"doc_id"
```

Coluna identificadora do documento.

---

# 15. Definir coluna vetorial

```json
"vec_column":"text_embedding"
```

Coluna contendo embeddings.

---

# 16. Definir operador vetorial

```json
"distance_operator":"public.<=>"
```

Operador utilizado para distância cosseno.

---

# 17. Definir limite

```json
"limit":5
```

Quantidade máxima de resultados por componente.

---

# 18. Definir vetor da consulta

```json
"query_vector":
"ai.embedding('theodb-embedding-001','managed database')::vector"
```

Embedding gerado dinamicamente.

---

# 19. Definir coluna textual

```json
"text_column":"text_tsv"
```

Coluna utilizada pelo Full Text Search.

---

# 20. Definir função de ranking

```json
"ranking_function":"ts_rank"
```

Função utilizada para calcular relevância textual.

---

# 21. Definir texto pesquisado

```json
"query_text_input":"database"
```

Texto utilizado na pesquisa Full Text.

---

# 22. Executar busca híbrida

```sql
SELECT *
FROM ai.hybrid_search(
    search_inputs => ARRAY[
        ...
    ],
    include_json_output => false
);
```

Retorna apenas:

* id
* score

---

# 23. Incluir JSON detalhado

```sql
include_json_output => true
```

Inclui um JSON contendo o cálculo completo do ranking.

---

# 24. Estrutura do JSON

```json
{
  "component_1": {...},
  "component_2": {...},
  "final_score": ...
}
```

Mostra:

* ranking vetorial;
* ranking textual;
* peso;
* score individual;
* score final;
* tempo de execução.

---

# 25. Definir tipo do ID

```sql
id_type => NULL::INTEGER
```

Converte automaticamente o identificador retornado.

---

# 26. Exemplo de cast

```sql
SELECT
    id,
    pg_typeof(id)
FROM ai.hybrid_search(
    ...,
    id_type => NULL::INTEGER
);
```

Retorna IDs do tipo `INTEGER`.

---

# 27. Utilizar `g_to_tsquery`

```sql
g_to_tsquery(...)
```

Parser padrão recomendado pelo TheoDB para buscas textuais.

---

# 28. Utilizar `plainto_tsquery`

```sql
plainto_tsquery('database')
```

Parser padrão do PostgreSQL.

---

# 29. Utilizar `to_tsquery`

```sql
to_tsquery('database')
```

Parser avançado do PostgreSQL.

---

# 30. Busca vetorial manual

```sql
SELECT
    id
FROM products
ORDER BY embedding
<=> ai.embedding(
    'theodb-embedding-001',
    'managed database'
)
LIMIT 10;
```

Primeira etapa da busca híbrida manual.

---

# 31. Ranking vetorial

```sql
RANK() OVER (
    ORDER BY embedding <=> ai.embedding(...)
)
```

Atribui posição aos resultados vetoriais.

---

# 32. Busca textual manual

```sql
SELECT
    id
FROM products
WHERE
    to_tsvector(
        'english',
        description
    )
@@
to_tsquery('database');
```

Executa Full Text Search.

---

# 33. Ranking textual

```sql
RANK() OVER (
    ORDER BY ts_rank(...)
)
```

Calcula posição dos documentos textuais.

---

# 34. Calcular `ts_rank`

```sql
ts_rank(
    to_tsvector(...),
    to_tsquery(...)
)
```

Calcula relevância textual.

---

# 35. Combinar resultados

```sql
FULL OUTER JOIN
```

Une os resultados das duas buscas.

---

# 36. Selecionar documento

```sql
COALESCE(
    vector_search.id,
    text_search.id
)
```

Obtém o ID presente em qualquer uma das buscas.

---

# 37. Calcular RRF

```sql
COALESCE(
    1.0/(60+vector_rank),
    0
)
+
COALESCE(
    1.0/(60+text_rank),
    0
)
```

Calcula o **Reciprocal Rank Fusion**.

---

# 38. Ordenar por RRF

```sql
ORDER BY rrf_score DESC;
```

Retorna os documentos mais relevantes.

---

# 39. Limitar resultados

```sql
LIMIT 5;
```

Retorna apenas os cinco melhores documentos.

---

# 40. Fluxo completo usando `ai.hybrid_search`

```sql
SELECT *
FROM ai.hybrid_search(
    search_inputs => ARRAY[
        '{
            "data_type":"vector",
            "weight":0.5,
            "table_name":"documents",
            "key_column":"doc_id",
            "vec_column":"text_embedding",
            "distance_operator":"public.<=>",
            "query_vector":"ai.embedding(''theodb-embedding-001'',''managed database'')::vector",
            "limit":5
        }'::jsonb,
        '{
            "data_type":"text",
            "weight":0.5,
            "table_name":"documents",
            "key_column":"doc_id",
            "text_column":"text_tsv",
            "ranking_function":"ts_rank",
            "query_text_input":"database",
            "limit":5
        }'::jsonb
    ],
    include_json_output => false
);
```

Fluxo recomendado para busca híbrida utilizando a função nativa do TheoDB.

---

# 41. Fluxo completo usando SQL puro

```sql
WITH vector_search AS (
    SELECT
        id,
        RANK() OVER (
            ORDER BY embedding
            <=> ai.embedding(
                'theodb-embedding-001',
                'database'
            )
        ) AS rank
    FROM products
    LIMIT 10
),
text_search AS (
    SELECT
        id,
        RANK() OVER (
            ORDER BY ts_rank(
                to_tsvector(
                    'english',
                    description
                ),
                to_tsquery('database')
            ) DESC
        ) AS rank
    FROM products
    WHERE
        to_tsvector(
            'english',
            description
        )
        @@
        to_tsquery('database')
    LIMIT 10
)
SELECT
    COALESCE(
        vector_search.id,
        text_search.id
    ) AS id,
    COALESCE(
        1.0/(60+vector_search.rank),
        0
    )
    +
    COALESCE(
        1.0/(60+text_search.rank),
        0
    ) AS rrf_score
FROM vector_search
FULL OUTER JOIN text_search
ON vector_search.id = text_search.id
ORDER BY rrf_score DESC
LIMIT 5;
```

Fluxo completo da implementação manual de busca híbrida utilizando:

1. busca vetorial (`pgvector`/ScaNN, HNSW, IVF ou IVFFlat);
2. Full Text Search (`GIN` ou `RUM`);
3. cálculo do **Reciprocal Rank Fusion (RRF)**;
4. reranqueamento final dos documentos mais relevantes.
