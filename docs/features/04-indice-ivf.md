# Criar um índice IVF

> **✅ Validado (M9, 2026-06-28):** no `pgvector`, o índice da família IVF **é o IVFFlat** (não há um
> access method "IVF" distinto). Foi **validado + benchmarkado** no harness recall@k — evidência medida
> em [`docs/benchmarks/m9-ivfflat.md`](../benchmarks/m9-ivfflat.md). Ver também
> [`03-indice-ivfflat.md`](./03-indice-ivfflat.md). A superfície literal abaixo permanece como API-alvo.

> **Status:** 📋 Especificação (planejado) — recurso-alvo do milestone **M2 — Vetorial / IA** ([ROADMAP](../../ROADMAP.md)).
> Esta página documenta a **API-alvo do TheoDB**. As funcionalidades aqui descritas **ainda não estão
> implementadas** na release atual (M0 entrega PostgreSQL 17 + `pgvector`). Nenhum número de desempenho
> nesta página é um benchmark — benchmarks reproduzíveis vivem em `docs/benchmarks/` quando publicados
> (CLAUDE.md, regra TheoDB 5).

Esta página cobre a criação de índices vetoriais IVF no TheoDB — consultas SQL, parâmetros e funcionalidades, das funções de distância à configuração de listas e quantizers.

---

# 1. Instalar extensão `vector`

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

Instala a extensão `pgvector`, necessária para armazenar embeddings e criar índices IVF.

---

# 2. Criar índice IVF básico

```sql
CREATE INDEX my_ivf_index
ON products
USING ivf (
    description_embedding vector_cosine_ops
)
WITH (
    lists = 100,
    quantizer = 'SQ8'
);
```

Cria um índice IVF para busca aproximada de vizinhos mais próximos.

---

# 3. Definir nome do índice

```sql
CREATE INDEX my_ivf_index ...
```

Define o identificador único do índice no banco.

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

Coluna que armazena valores do tipo `vector`.

---

# 6. Índice IVF com distância L2

```sql
CREATE INDEX products_ivf_l2
ON products
USING ivf (
    description_embedding vector_l2_ops
)
WITH (
    lists = 100,
    quantizer = 'SQ8'
);
```

Cria índice IVF usando distância Euclidiana.

---

# 7. Índice IVF com Inner Product

```sql
CREATE INDEX products_ivf_ip
ON products
USING ivf (
    description_embedding vector_ip_ops
)
WITH (
    lists = 100,
    quantizer = 'SQ8'
);
```

Cria índice IVF usando produto interno.

---

# 8. Índice IVF com Cosine Distance

```sql
CREATE INDEX products_ivf_cosine
ON products
USING ivf (
    description_embedding vector_cosine_ops
)
WITH (
    lists = 100,
    quantizer = 'SQ8'
);
```

Cria índice IVF usando distância cosseno.

---

# 9. Configurar `lists`

```sql
WITH (
    lists = 100
)
```

Define o número de listas/partições do índice IVF.

Valores maiores tendem a melhorar recall, mas podem aumentar custo de busca e criação.

---

# 10. Configurar quantizer `SQ8`

```sql
WITH (
    quantizer = 'SQ8'
)
```

Ativa scalar quantization de 8 bits.

É recomendado para consultas mais rápidas, com alguma perda de recall.

---

# 11. Configurar quantizer `FLAT`

```sql
WITH (
    quantizer = 'FLAT'
)
```

Usa representação não quantizada.

Tem maior uso de memória e consulta mais lenta, mas perda de recall quase nula.

---

# 12. Criar índice IVF com `real[]`

```sql
CREATE INDEX products_ivf_real_array
ON products
USING ivf (
    CAST(description_embedding AS vector(768))
    vector_cosine_ops
)
WITH (
    lists = 100,
    quantizer = 'SQ8'
);
```

Permite criar índice em coluna armazenada como `real[]`.

---

# 13. Definir dimensão do vetor

```sql
CAST(description_embedding AS vector(768))
```

Converte `real[]` para `vector` com dimensão explícita.

---

# 14. Consultar dimensões do vetor

```sql
SELECT vector_dims(description_embedding::vector)
FROM products
LIMIT 1;
```

Obtém a dimensionalidade do embedding.

---

# 15. Consultar progresso da indexação

```sql
SELECT *
FROM pg_stat_progress_create_index;
```

Mostra o andamento da criação do índice.

---

# 16. Consultar fase atual

```sql
SELECT phase
FROM pg_stat_progress_create_index;
```

Retorna a etapa atual da criação.

---

# 17. Fase `building postings`

```text
building postings
```

Indica que a criação do índice IVF está próxima da conclusão.

---

# 18. Consulta vetorial genérica

```sql
SELECT *
FROM products
ORDER BY description_embedding DISTANCE_FUNCTION_QUERY '[...]'
LIMIT ROW_COUNT;
```

Executa nearest-neighbor search usando o operador compatível com o índice.

---

# 19. Consulta com L2

```sql
SELECT *
FROM products
ORDER BY description_embedding
<-> '[0.12,0.45,0.81]'::vector
LIMIT 10;
```

Consulta usando distância Euclidiana.

---

# 20. Consulta com Inner Product

```sql
SELECT *
FROM products
ORDER BY description_embedding
<#> '[0.12,0.45,0.81]'::vector
LIMIT 10;
```

Consulta usando produto interno.

---

# 21. Consulta com Cosine Distance

```sql
SELECT *
FROM products
ORDER BY description_embedding
<=> '[0.12,0.45,0.81]'::vector
LIMIT 10;
```

Consulta usando distância cosseno.

---

# 22. Retornar apenas o melhor resultado

```sql
LIMIT 1;
```

Retorna somente o vizinho mais próximo.

---

# 23. Retornar Top-K resultados

```sql
LIMIT 20;
```

Retorna os 20 vetores mais próximos.

---

# 24. Consulta com embedding textual

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

Gera embedding a partir de texto e compara com os vetores armazenados.

---

# 25. Cast obrigatório de `embedding()` para `vector`

```sql
embedding(
    'theodb-embedding-005',
    'running shoes'
)::vector
```

Necessário porque `embedding()` retorna `real[]`.

---

# 26. Consulta com filtro SQL

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

Combina busca vetorial com filtro relacional.

---

# 27. Consulta selecionando colunas específicas

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

# 28. Exibir score de distância

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

Mostra a distância calculada entre o embedding salvo e o embedding da consulta.

---

# 29. Ordenar por menor distância

```sql
ORDER BY distance
```

Quanto menor o valor, maior a similaridade.

---

# 30. Fluxo completo recomendado

```sql
CREATE EXTENSION IF NOT EXISTS vector;

CREATE INDEX products_ivf_idx
ON products
USING ivf (
    description_embedding vector_cosine_ops
)
WITH (
    lists = 100,
    quantizer = 'SQ8'
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
2. cria índice IVF;
3. usa embedding textual;
4. ordena por distância;
5. retorna os itens mais similares.
