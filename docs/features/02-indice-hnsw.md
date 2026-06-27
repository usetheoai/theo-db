# Criar índices HNSW

> **Status:** 📋 Especificação (planejado) — recurso-alvo do milestone **M2 — Vetorial / IA** ([ROADMAP](../../ROADMAP.md)).
> Esta página documenta a **API-alvo do TheoDB**. As funcionalidades aqui descritas **ainda não estão
> implementadas** na release atual (M0 entrega PostgreSQL 17 + `pgvector`). Nenhum número de desempenho
> nesta página é um benchmark — benchmarks reproduzíveis vivem em `docs/benchmarks/` quando publicados
> (CLAUDE.md, regra TheoDB 5).

Esta página cobre a criação de índices vetoriais HNSW no TheoDB — todas as consultas SQL, parâmetros e funcionalidades da indexação HNSW, das funções de distância aos parâmetros de construção do grafo.

---

# 1. Instalar a extensão `vector`

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

Instala a extensão `pgvector` utilizada pelo TheoDB para armazenar vetores e criar índices HNSW.

---

# 2. Criar índice HNSW

```sql
CREATE INDEX my_hnsw_index
ON products
USING hnsw (
    description_embedding vector_cosine_ops
)
WITH (
    m = 16,
    ef_construction = 64
);
```

Cria um índice baseado no algoritmo **Hierarchical Navigable Small World (HNSW)**.

---

# 3. Definir nome do índice

```sql
CREATE INDEX my_hnsw_index ...
```

Define um identificador único para o índice.

---

# 4. Definir tabela indexada

```sql
ON products
```

Especifica a tabela que contém os embeddings.

---

# 5. Definir coluna vetorial

```sql
description_embedding
```

Coluna que armazena os vetores (`vector`).

---

# 6. Índice usando distância L2

```sql
CREATE INDEX my_index
ON products
USING hnsw (
    description_embedding vector_l2_ops
);
```

Cria índice baseado em distância Euclidiana.

---

# 7. Índice usando Inner Product

```sql
CREATE INDEX my_index
ON products
USING hnsw (
    description_embedding vector_ip_ops
);
```

Cria índice utilizando produto interno.

---

# 8. Índice usando Cosine Distance

```sql
CREATE INDEX my_index
ON products
USING hnsw (
    description_embedding vector_cosine_ops
);
```

Cria índice baseado em similaridade cosseno.

---

# 9. Configurar parâmetro `m`

```sql
WITH (
    m = 16
)
```

Define o número máximo de conexões por nó no grafo.

Quanto maior:

* maior recall;
* maior uso de memória;
* maior tempo de construção.

---

# 10. Configurar `ef_construction`

```sql
WITH (
    ef_construction = 64
)
```

Define o tamanho da lista de candidatos durante a construção do grafo.

Valores maiores:

* aumentam qualidade do índice;
* aumentam tempo de criação.

---

# 11. Exemplo completo

```sql
CREATE INDEX products_hnsw
ON products
USING hnsw (
    embedding vector_cosine_ops
)
WITH (
    m = 32,
    ef_construction = 128
);
```

Cria um índice HNSW otimizado para maior recall.

---

# 12. Consultar progresso da indexação

```sql
SELECT *
FROM pg_stat_progress_create_index;
```

Mostra todos os índices sendo criados.

---

# 13. Consultar apenas a fase

```sql
SELECT phase
FROM pg_stat_progress_create_index;
```

Mostra a etapa atual da construção do índice.

---

# 14. Verificar fase "building graph"

```text
building graph
```

Indica que o algoritmo está construindo o grafo HNSW.

Após terminar, essa fase desaparece da view.

---

# 15. Consulta vetorial básica

```sql
SELECT *
FROM products
ORDER BY description_embedding
<=> '[...]'::vector
LIMIT 10;
```

Realiza busca dos vetores mais próximos.

---

# 16. Consulta usando L2

```sql
SELECT *
FROM products
ORDER BY description_embedding
<-> '[...]'::vector
LIMIT 10;
```

Pesquisa utilizando distância Euclidiana.

---

# 17. Consulta usando Inner Product

```sql
SELECT *
FROM products
ORDER BY description_embedding
<#> '[...]'::vector
LIMIT 10;
```

Pesquisa utilizando produto interno.

---

# 18. Consulta usando Cosine Distance

```sql
SELECT *
FROM products
ORDER BY description_embedding
<=> '[...]'::vector
LIMIT 10;
```

Pesquisa utilizando distância cosseno.

---

# 19. Buscar apenas o melhor resultado

```sql
LIMIT 1;
```

Retorna somente o vizinho mais próximo.

---

# 20. Buscar Top-K

```sql
LIMIT 50;
```

Retorna os cinquenta vetores mais próximos.

---

# 21. Consulta usando embedding já calculado

```sql
SELECT *
FROM documents
ORDER BY embedding
<=> '[0.12,0.45,0.81,...]'::vector
LIMIT 5;
```

Utiliza um vetor conhecido.

---

# 22. Consulta usando `embedding()`

```sql
SELECT *
FROM products
ORDER BY embedding
<-> embedding(
    'theodb-embedding-005',
    'wireless headphones'
)::vector
LIMIT 10;
```

Converte texto em embedding durante a consulta.

---

# 23. Consulta usando distância cosseno com texto

```sql
SELECT *
FROM products
ORDER BY embedding
<=> embedding(
    'theodb-embedding-005',
    'running shoes'
)::vector
LIMIT 10;
```

Busca semântica baseada em texto.

---

# 24. Consulta usando produto interno com texto

```sql
SELECT *
FROM products
ORDER BY embedding
<#> embedding(
    'theodb-embedding-005',
    'smartphone'
)::vector
LIMIT 10;
```

Executa busca usando Inner Product.

---

# 25. Converter `embedding()` para `vector`

```sql
embedding(
    'theodb-embedding-005',
    'shoe'
)::vector
```

Como `embedding()` retorna `real[]`, é obrigatório fazer o cast para `vector`.

---

# 26. Consulta com filtro SQL

```sql
SELECT *
FROM products
WHERE category_id = 2
ORDER BY embedding
<=> embedding(
    'theodb-embedding-005',
    'hoodie'
)::vector
LIMIT 5;
```

Combina filtro relacional com busca vetorial.

---

# 27. Selecionar apenas algumas colunas

```sql
SELECT
    product_id,
    name,
    price
FROM products
ORDER BY embedding
<=> embedding(
    'theodb-embedding-005',
    'hoodie'
)::vector
LIMIT 5;
```

Evita retornar colunas desnecessárias.

---

# 28. Exibir score da distância

```sql
SELECT
    *,
    embedding
    <=>
    embedding(
        'theodb-embedding-005',
        'hoodie'
    )::vector AS distance
FROM products
ORDER BY distance;
```

Retorna também a distância calculada.

---

# 29. Ordenar pela distância

```sql
ORDER BY distance;
```

Os menores valores representam maior similaridade.

---

# 30. Fluxo completo recomendado

```sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE INDEX products_hnsw
ON products
USING hnsw (
    embedding vector_cosine_ops
)
WITH (
    m = 16,
    ef_construction = 64
);

SELECT *
FROM products
ORDER BY embedding
<=> embedding(
    'theodb-embedding-005',
    'wireless headphones'
)::vector
LIMIT 10;
```

Fluxo completo de uso do HNSW no TheoDB:

1. instalar `pgvector`;
2. criar índice HNSW;
3. gerar embedding da consulta;
4. executar busca vetorial por similaridade.
