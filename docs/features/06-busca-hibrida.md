# Busca híbrida por similaridade vetorial

> **✅ Entregue (M7-S1 + M13):** capacidade RRF via `ai.hybrid_search_rrf(...)` (M7-S1) + a superfície JSON
> literal **`ai.hybrid_search(config jsonb)`** (M13, wrapper fino — paridade testada com o rrf). Ver
> [`docs/sql-ai-functions.md`](../sql-ai-functions.md) § "Packaged surface". As chaves JSON **realmente
> honradas** pelo código são: `table`, `id_col`, `content_tsv_col`, `content_text_col`, `vector_col`,
> `query_text`, `query_vector`, `k`, `per_leg_limit`, `result_limit`, `language`, `filter_sql`,
> `lexical_engine` (ver § 9). A fusão é **RRF pura (sem pesos)** com `<=>` cosseno + `ts_rank_cd` nativo.
> `theodb_scann` (índice) **não** é entregue (usamos DiskANN/HNSW — specs 02/05).

> **Status:** ✅ **Entregue (M7-S1 + M13 + M19).** A busca híbrida está disponível: `ai.hybrid_search_rrf(...)`
> (Rust `theodb_rs/src/hybrid.rs`, wrapper SQL `theodb_rs/src/api.rs:399`) e `ai.hybrid_search(config jsonb)`
> (`theodb_rs/src/api.rs:418`) — fusão RRF de vetorial + FTS nativo do PostgreSQL (`ts_rank_cd`/GIN), ambas
> `REVOKE`das de PUBLIC. Provado por `benchmarks/tests/test_hybrid.py` (`test_rrf_fuse_matches_handcalc`,
> `test_rrf_fuse_tie_break_is_id_asc`, `test_ndcg_at_k_*`) + `benchmarks/tests/test_hybrid_guard.py` +
> [`docs/benchmarks/m7-hybrid-recall.md`](../benchmarks/m7-hybrid-recall.md). **Honestidade:** a perna de texto usa
> FTS nativo; um BM25 permissivo (pg_search é AGPL, barrado por D1) é slice futura — não é a superfície entregue.

Esta página cobre a busca híbrida no TheoDB — combinação de busca vetorial (semântica) com Full Text Search — tanto pela função nativa `ai.hybrid_search()` quanto pela implementação manual via SQL com Reciprocal Rank Fusion (RRF).

> **Superfície implementada (M7-S1):** a primeira fatia entregue é a função SQL **`ai.hybrid_search_rrf(...)`**
> (`sql/40-theodb-hybrid.sql`) — o MVP manual-SQL da RRF (`score = Σ 1/(k+rank)`, k=60 default exposto como
> parâmetro; perna FTS `ts_rank_cd`/GIN + perna vetorial `pgvector` `<=>`; empty-leg via `FULL OUTER JOIN`+`COALESCE`).
> A API nativa `ai.hybrid_search()` com `search_inputs` JSON (abaixo) é um wrapper fino futuro sobre essa mesma
> função (uma única fonte de verdade da fusão). Recall medido (BEIR-style) em `docs/benchmarks/m7-hybrid-recall.md`.
> BM25 permissivo (`pg_search` é AGPL → barrado por D1) é a slice M7-S2.

---

# 1. Criar tabela de documentos

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

# 4. Instalar extensão de vetores

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

Habilita o mecanismo de busca vetorial (`pgvector`). O índice ANN entregue é
DiskANN/HNSW (specs 02/05), não ScaNN.

---

# 5. Criar índice vetorial (HNSW)

```sql
CREATE INDEX documents_text_embedding_idx
ON documents
USING hnsw (
    text_embedding vector_cosine_ops
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

# 9. Assinatura e contrato JSON de `ai.hybrid_search`

```sql
SELECT id, score
FROM ai.hybrid_search('{ ...config... }'::jsonb);
```

Assinatura entregue: `ai.hybrid_search(config jsonb) RETURNS TABLE(id text, score real)`.
Executa busca híbrida via fusão RRF pura (sem pesos) de uma perna vetorial (`<=>` cosseno)
com uma perna de texto (FTS nativo do PostgreSQL, `ts_rank_cd`). As chaves do `config`
realmente honradas pelo código são:

| Chave | Descrição |
|---|---|
| `table` | Tabela consultada. |
| `id_col` | Coluna identificadora do documento. |
| `content_tsv_col` | Coluna `tsvector` para a perna FTS. |
| `content_text_col` | Coluna de texto original (para `ts_rank_cd`/`plainto_tsquery`). |
| `vector_col` | Coluna de embeddings para a perna vetorial. |
| `query_text` | Texto da consulta (perna FTS). |
| `query_vector` | Embedding da consulta (perna vetorial). |
| `k` | Constante RRF (`score = Σ 1/(k+rank)`, default 60). |
| `per_leg_limit` | Máximo de candidatos por perna. |
| `result_limit` | Máximo de resultados finais. |
| `language` | Configuração de idioma do FTS (ex.: `english`). |
| `filter_sql` | Predicado SQL adicional aplicado às pernas. |
| `lexical_engine` | Motor lexical: `postgres` (default, entregue) ou `bm25`. |

---

# 10. Definir tabela e colunas

```json
{
  "table": "documents",
  "id_col": "doc_id",
  "content_tsv_col": "text_tsv",
  "content_text_col": "content",
  "vector_col": "text_embedding"
}
```

Aponta a tabela e as colunas usadas por cada perna da fusão.

---

# 11. Definir a consulta (texto + vetor)

```json
{
  "query_text": "managed database",
  "query_vector": "[0.12, 0.98, ...]"
}
```

`query_text` alimenta a perna FTS; `query_vector` é o embedding da consulta
(gere-o com `theodb.embed('theodb-embedding-001', 'managed database')`).

---

# 12. Ajustar RRF e limites

```json
{
  "k": 60,
  "per_leg_limit": 50,
  "result_limit": 5
}
```

`k` é a constante do RRF; `per_leg_limit` limita candidatos por perna;
`result_limit` corta o resultado final.

---

# 13. Idioma e filtro

```json
{
  "language": "english",
  "filter_sql": "status = 'published'"
}
```

`language` define a configuração de FTS; `filter_sql` restringe ambas as pernas.

---

# 14. Motor lexical (`lexical_engine`)

```json
{
  "lexical_engine": "postgres"
}
```

A perna de texto entregue é o **FTS nativo do PostgreSQL** (`ts_rank_cd`/GIN),
selecionado por `"postgres"` (default). Uma perna BM25 permissiva
(`"lexical_engine": "bm25"`) existe no código, porém está **desligada na imagem
entregue** (requer a extensão `pg_textsearch`). Ver a nota de honestidade no topo.

---

# 15. Utilizar `plainto_tsquery`

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
<=> theodb.embed(
    'managed database',
    'theodb-embedding-001'
)
LIMIT 10;
```

Primeira etapa da busca híbrida manual.

---

# 31. Ranking vetorial

```sql
RANK() OVER (
    ORDER BY embedding <=> theodb.embed(...)
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
SELECT id, score
FROM ai.hybrid_search(
    jsonb_build_object(
        'table',            'documents',
        'id_col',           'doc_id',
        'content_tsv_col',  'text_tsv',
        'content_text_col', 'content',
        'vector_col',       'text_embedding',
        'query_text',       'managed database',
        'query_vector',     theodb.embed('managed database', 'theodb-embedding-001'),
        'k',                60,
        'per_leg_limit',    50,
        'result_limit',     5,
        'language',         'english',
        'lexical_engine',   'postgres'
    )
);
```

Fluxo recomendado para busca híbrida utilizando a função nativa do TheoDB —
fusão RRF pura (sem pesos) de vetorial (`<=>`) + FTS nativo (`ts_rank_cd`).

---

# 41. Fluxo completo usando SQL puro

```sql
WITH vector_search AS (
    SELECT
        id,
        RANK() OVER (
            ORDER BY embedding
            <=> theodb.embed(
                'database',
                'theodb-embedding-001'
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

1. busca vetorial (`pgvector`: HNSW, DiskANN, IVF ou IVFFlat);
2. Full Text Search (`GIN` ou `RUM`);
3. cálculo do **Reciprocal Rank Fusion (RRF)**;
4. reranqueamento final dos documentos mais relevantes.

---

## 🎯 API-alvo / roadmap (não-shipped)

> As chaves abaixo aparecem em material antigo mas **não são honradas pelo código
> entregue**. A fusão atual é **RRF pura (sem pesos)**, então `weight` é ignorado; o
> operador de distância é fixo em `<=>` (cosseno) e a função de ranking textual é fixa
> em `ts_rank_cd` nativo. Estão documentadas aqui apenas como intenção futura — **não use
> em exemplos executáveis**. Use as chaves reais da § 9.

| Chave (não-shipped) | Intenção futura |
|---|---|
| `weight` | Fusão ponderada por perna (hoje: RRF sem pesos). |
| `distance_operator` | Operador de distância configurável (hoje: fixo `<=>` cosseno). |
| `ranking_function` | Função de ranking textual configurável (hoje: fixo `ts_rank_cd`). |
| `include_json_output` | Saída JSON detalhada do cálculo de ranking. |
| `id_type` | Cast automático do tipo do ID retornado (hoje: `id text`). |

O parser `g_to_tsquery` também **não existe** — a superfície entregue usa o
`plainto_tsquery` nativo do PostgreSQL.
