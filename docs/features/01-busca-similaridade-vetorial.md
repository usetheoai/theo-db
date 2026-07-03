# Busca por similaridade vetorial

> **Status:** ✅ **Entregue (M20).** A busca por similaridade vetorial está disponível: kernels de distância
> próprios do TheoDB — `theodb.l2_distance` / `theodb.inner_product` / `theodb.cosine_distance`
> (`theodb_rs/src/api.rs:483,487,491`, implementados em `theodb_rs/src/vec.rs` com paridade f32 vs pgvector) —
> operando sobre o tipo `vector`. Coexiste com os operadores `<->` / `<#>` / `<=>` do pgvector. Provado por
> `benchmarks/tests/test_vector_ops.py`. Números de desempenho reproduzíveis (recall/QPS) vivem em
> `docs/benchmarks/` (M31b/M32/M34/M35); nenhuma afirmação de desempenho nesta página sem link para esse artefato
> (CLAUDE.md, regra TheoDB 5).

Esta página cobre a execução de buscas por similaridade vetorial (KNN / nearest neighbor) no TheoDB, detalhando a consulta base, os operadores de distância e os parâmetros de cada consulta.

1. **Consulta base de similaridade vetorial**

```sql
SELECT *
FROM TABLE
ORDER BY EMBEDDING_COLUMN DISTANCE_FUNCTION_QUERY ['EMBEDDING']
LIMIT ROW_COUNT;
```

Executa uma busca KNN/nearest neighbor sobre uma coluna de embeddings.

2. **Parâmetro `TABLE`**

```sql
FROM TABLE
```

Representa a tabela onde os embeddings estão armazenados.

3. **Parâmetro `EMBEDDING_COLUMN`**

```sql
ORDER BY EMBEDDING_COLUMN ...
```

Representa a coluna vetorial usada para comparar similaridade.

4. **Operador L2 distance**

```sql
ORDER BY EMBEDDING_COLUMN <-> '[...]'
```

Usa distância euclidiana. Quanto menor o valor, maior a similaridade.

5. **Operador inner product**

```sql
ORDER BY EMBEDDING_COLUMN <#> '[...]'
```

Usa produto interno como métrica de comparação vetorial.

6. **Operador cosine distance**

```sql
ORDER BY EMBEDDING_COLUMN <=> '[...]'
```

Usa distância cosseno. É comum para embeddings de texto.

7. **Parâmetro `EMBEDDING`**

```sql
['EMBEDDING']
```

É o vetor alvo usado como entrada da comparação.

8. **Parâmetro `ROW_COUNT`**

```sql
LIMIT ROW_COUNT
```

Define quantos vizinhos mais próximos serão retornados.

9. **Retornar apenas o melhor match**

```sql
LIMIT 1
```

Retorna somente o item mais similar.

10. **Criar extensão `vector`**

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

Habilita o `pgvector`, chamado de `vector` no TheoDB.

11. **Busca vetorial com entrada textual**

```sql
SELECT *
FROM TABLE
ORDER BY EMBEDDING_COLUMN::vector
<=> theodb_ml.embedding('MODEL_IDVERSION_TAG', 'TEXT')
LIMIT ROW_COUNT;
```

Converte texto em embedding e compara com embeddings armazenados.

12. **Cast explícito para `vector`**

```sql
EMBEDDING_COLUMN::vector
```

Garante compatibilidade com operadores do `pgvector`.

13. **Gerar embedding a partir de texto**

```sql
theodb_ml.embedding('MODEL_IDVERSION_TAG', 'TEXT')
```

Transforma texto em vetor usando um modelo de embeddings.

14. **Modelo recomendado**

```sql
theodb_ml.embedding('theodb-embedding-005', 'TEXT')
```

Usa o modelo configurável de embeddings de texto.

15. **Modelo com version tag**

```sql
theodb_ml.embedding('theodb-embedding-005@VERSION_TAG', 'TEXT')
```

Fixa uma versão específica do modelo para evitar mudanças inesperadas.

16. **Busca com texto real**

```sql
SELECT *
FROM products
ORDER BY description_embedding::vector
<=> theodb_ml.embedding('theodb-embedding-005', 'running shoes')
LIMIT 5;
```

Retorna os 5 produtos semanticamente mais próximos de “running shoes”.

17. **Busca por similaridade cosseno**

```sql
SELECT *
FROM products
ORDER BY description_embedding::vector
<=> theodb_ml.embedding('theodb-embedding-005', 'waterproof backpack')
LIMIT 10;
```

Consulta típica para embeddings textuais.

18. **Busca por distância L2**

```sql
SELECT *
FROM products
ORDER BY description_embedding::vector
<-> '[0.12, 0.45, 0.33]'::vector
LIMIT 5;
```

Compara contra um vetor fornecido diretamente.

19. **Busca por produto interno**

```sql
SELECT *
FROM products
ORDER BY description_embedding::vector
<#> '[0.12, 0.45, 0.33]'::vector
LIMIT 5;
```

Usa inner product como métrica.

20. **Ordenação por menor distância**

```sql
ORDER BY description_embedding <=> query_vector
```

A ordenação crescente coloca os vetores mais semelhantes no topo.

21. **Consulta sem bulk search**

```sql
-- Não suportado pelo theodb_scann:
-- múltiplas buscas vetoriais em lote na mesma operação
```

O documento indica que bulk search não é suportado com `theodb_scann`.

22. **Uso consistente da métrica**

```sql
ORDER BY embedding_column <=> query_embedding
```

A métrica usada na consulta deve ser a mesma usada na criação do índice.

23. **Uso com filtro relacional**

```sql
SELECT *
FROM products
WHERE category_id = 3
ORDER BY description_embedding::vector
<=> theodb_ml.embedding('theodb-embedding-005', 'comfortable shoes')
LIMIT 5;
```

Combina filtro SQL tradicional com busca vetorial.

24. **Uso com seleção de colunas**

```sql
SELECT product_id, name, price
FROM products
ORDER BY description_embedding::vector
<=> theodb_ml.embedding('theodb-embedding-005', 'casual hoodie')
LIMIT 3;
```

Retorna apenas os campos necessários.

25. **Expor score de distância**

```sql
SELECT *,
       description_embedding::vector
       <=> theodb_ml.embedding('theodb-embedding-005', 'casual hoodie') AS distance
FROM products
ORDER BY distance
LIMIT 3;
```

Mostra explicitamente a distância calculada.

26. **Interpretação do score**

```sql
distance menor = maior similaridade
```

A métrica retorna distância, não similaridade direta.

27. **Consulta com embedding literal**

```sql
SELECT *
FROM items
ORDER BY item_embedding <=> '[0.01, 0.02, 0.03]'::vector
LIMIT 10;
```

Usa um vetor já conhecido, sem chamar modelo de embedding.

28. **Consulta com texto dinâmico**

```sql
SELECT *
FROM items
ORDER BY item_embedding::vector
<=> theodb_ml.embedding('theodb-embedding-005', :search_text)
LIMIT :limit;
```

Padrão para aplicações com parâmetros externos.

29. **Busca semântica de documentos**

```sql
SELECT document_id, title
FROM documents
ORDER BY content_embedding::vector
<=> theodb_ml.embedding('theodb-embedding-005', 'refund policy')
LIMIT 5;
```

Aplica o padrão em documentos textuais.

30. **Busca semântica de produtos**

```sql
SELECT product_id, name
FROM products
ORDER BY description_embedding::vector
<=> theodb_ml.embedding('theodb-embedding-005', 'lightweight running footwear')
LIMIT 5;
```

Aplica o padrão em catálogo de produtos.
