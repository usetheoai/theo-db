# Criar um índice IVFFlat

> **Status:** 📋 Especificação (planejado) — recurso-alvo do milestone **M2 — Vetorial / IA** ([ROADMAP](../../ROADMAP.md)).
> Esta página documenta a **API-alvo do TheoDB**. As funcionalidades aqui descritas **ainda não estão
> implementadas** na release atual (M0 entrega PostgreSQL 17 + `pgvector`). Nenhum número de desempenho
> nesta página é um benchmark — benchmarks reproduzíveis vivem em `docs/benchmarks/` quando publicados
> (CLAUDE.md, regra TheoDB 5).

Esta página cobre a criação de índices `IVFFlat` no TheoDB para busca aproximada de vizinhos mais próximos sobre colunas vetoriais, incluindo as métricas de distância suportadas, o parâmetro `lists` e exemplos de consulta.

---

# 1. Instalar extensão `vector`

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

Instala a extensão `pgvector`, necessária para criar índices `ivfflat` e executar consultas vetoriais.

---

# 2. Criar índice IVFFlat básico

```sql
CREATE INDEX products_ivfflat_idx
ON products
USING ivfflat (
    description_embedding vector_cosine_ops
)
WITH (
    lists = 100
);
```

Cria um índice `IVFFlat` para busca aproximada de vizinhos mais próximos.

---

# 3. Definir nome do índice

```sql
CREATE INDEX products_ivfflat_idx ...
```

Define o identificador do índice dentro do banco.

---

# 4. Definir tabela indexada

```sql
ON products
```

Indica a tabela que contém os embeddings armazenados.

---

# 5. Definir coluna vetorial

```sql
description_embedding
```

Coluna que armazena embeddings no tipo `vector`.

---

# 6. Índice IVFFlat com distância L2

```sql
CREATE INDEX products_ivfflat_l2
ON products
USING ivfflat (
    description_embedding vector_l2_ops
)
WITH (
    lists = 100
);
```

Cria índice usando distância Euclidiana.

---

# 7. Índice IVFFlat com Inner Product

```sql
CREATE INDEX products_ivfflat_ip
ON products
USING ivfflat (
    description_embedding vector_ip_ops
)
WITH (
    lists = 100
);
```

Cria índice usando produto interno.

---

# 8. Índice IVFFlat com Cosine Distance

```sql
CREATE INDEX products_ivfflat_cosine
ON products
USING ivfflat (
    description_embedding vector_cosine_ops
)
WITH (
    lists = 100
);
```

Cria índice usando distância cosseno.

---

# 9. Configurar parâmetro `lists`

```sql
WITH (
    lists = 100
)
```

Define o número de listas/partições usadas pelo índice.

Valores maiores tendem a melhorar recall, mas podem aumentar custo de busca e criação.

---

# 10. Criar IVFFlat em coluna `real[]`

```sql
CREATE INDEX products_ivfflat_real_array
ON products
USING ivfflat (
    CAST(description_embedding AS vector(768))
    vector_cosine_ops
)
WITH (
    lists = 100
);
```

Permite criar índice quando os embeddings estão armazenados como `real[]`.

---

# 11. Definir dimensão do vetor

```sql
CAST(description_embedding AS vector(768))
```

Converte o array `real[]` para `vector` com dimensão explícita.

---

# 12. Consultar dimensão do vetor

```sql
SELECT vector_dims(description_embedding::vector)
FROM products
LIMIT 1;
```

Retorna a quantidade de dimensões do embedding.

---

# 13. Consultar progresso da indexação

```sql
SELECT *
FROM pg_stat_progress_create_index;
```

Mostra o andamento da criação do índice.

---

# 14. Consultar fase atual

```sql
SELECT phase
FROM pg_stat_progress_create_index;
```

Mostra a fase atual da indexação.

---

# 15. Consulta vetorial genérica

```sql
SELECT *
FROM products
ORDER BY description_embedding DISTANCE_FUNCTION_QUERY '[...]'
LIMIT ROW_COUNT;
```

Executa busca por vizinhos mais próximos usando a métrica compatível com o índice.

---

# 16. Consulta com L2

```sql
SELECT *
FROM products
ORDER BY description_embedding
<-> '[0.12,0.45,0.81]'::vector
LIMIT 10;
```

Busca usando distância Euclidiana.

---

# 17. Consulta com Inner Product

```sql
SELECT *
FROM products
ORDER BY description_embedding
<#> '[0.12,0.45,0.81]'::vector
LIMIT 10;
```

Busca usando produto interno.

---

# 18. Consulta com Cosine Distance

```sql
SELECT *
FROM products
ORDER BY description_embedding
<=> '[0.12,0.45,0.81]'::vector
LIMIT 10;
```

Busca usando distância cosseno.

---

# 19. Retornar apenas o melhor resultado

```sql
LIMIT 1;
```

Retorna somente o vizinho mais próximo.

---

# 20. Retornar Top-K resultados

```sql
LIMIT 20;
```

Retorna os 20 vetores mais semelhantes.

---

# 21. Consulta com embedding textual

```sql
SELECT *
FROM products
ORDER BY description_embedding
<=> embedding(
    'theodb-embedding-005',
    'running shoes'
)::vector
LIMIT 10;
```

Gera embedding a partir de texto e compara com os embeddings armazenados.

---

# 22. Cast obrigatório de `embedding()` para `vector`

```sql
embedding(
    'theodb-embedding-005',
    'running shoes'
)::vector
```

Necessário porque `embedding()` retorna `real[]`.

---

# 23. Consulta com filtro SQL

```sql
SELECT *
FROM products
WHERE category_id = 3
ORDER BY description_embedding
<=> embedding(
    'theodb-embedding-005',
    'comfortable shoes'
)::vector
LIMIT 5;
```

Combina filtro relacional com busca vetorial.

---

# 24. Selecionar colunas específicas

```sql
SELECT
    product_id,
    name,
    price
FROM products
ORDER BY description_embedding
<=> embedding(
    'theodb-embedding-005',
    'casual hoodie'
)::vector
LIMIT 5;
```

Evita retornar colunas desnecessárias.

---

# 25. Exibir score de distância

```sql
SELECT
    product_id,
    name,
    description_embedding
    <=> embedding(
        'theodb-embedding-005',
        'casual hoodie'
    )::vector AS distance
FROM products
ORDER BY distance
LIMIT 5;
```

Retorna a distância calculada.

---

# 26. Ordenar por menor distância

```sql
ORDER BY distance
```

Menor distância representa maior similaridade.

---

# 27. Fluxo completo recomendado

```sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE INDEX products_ivfflat_idx
ON products
USING ivfflat (
    description_embedding vector_cosine_ops
)
WITH (
    lists = 100
);

SELECT
    product_id,
    name,
    description_embedding
    <=> embedding(
        'theodb-embedding-005',
        'wireless headphones'
    )::vector AS distance
FROM products
ORDER BY distance
LIMIT 10;
```

Fluxo completo:

1. habilita `vector`;
2. cria índice `IVFFlat`;
3. gera embedding textual;
4. executa busca por distância;
5. retorna os itens mais semelhantes.
