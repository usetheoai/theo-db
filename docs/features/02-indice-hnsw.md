# Criar índices HNSW

> **Status:** ✅ **Entregue (M21 + M35).** O TheoDB tem um access method HNSW **próprio**: `theodb_hnsw`
> (`CREATE ACCESS METHOD theodb_hnsw` em `theodb_rs/src/am/mod.rs:58`, opclass `theodb_hnsw_l2_ops` em `:208`).
> Uso: `CREATE INDEX … USING theodb_hnsw (embedding theodb_hnsw_l2_ops)` + `SET theodb_hnsw.ef_search = N`. Desde o
> M35 a persistência é page-native com travessia on-demand (grafo em `theodb_rs/src/ann/hnsw.rs`, páginas em
> `theodb_rs/src/am/hnsw_page.rs`). Provado por `benchmarks/tests/test_hnsw_structured.py`. Benchmark medido:
> `docs/benchmarks/m35-hnsw-structured-scan.{md,json}` (~100 QPS @ recall 0.98 a 1M; O(N)→O(ef·M)). Coexiste com o
> HNSW do pgvector. Regra TheoDB 5: só há afirmação de desempenho com link para o artefato de benchmark.

Esta página cobre a criação de índices vetoriais HNSW no TheoDB — todas as consultas SQL, parâmetros e funcionalidades da indexação HNSW, das funções de distância aos parâmetros de construção do grafo.

---

# 1. Instalar a extensão `vector`

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

Instala a extensão `pgvector` utilizada pelo TheoDB para armazenar vetores e criar índices HNSW.

---

# 2. Criar índice HNSW (AM próprio `theodb_hnsw`)

```sql
CREATE INDEX my_hnsw_index
ON products
USING theodb_hnsw (
    description_embedding theodb_hnsw_cosine_ops
)
WITH (
    m = 16,
    ef_construction = 64
);
```

Cria um índice baseado no algoritmo **Hierarchical Navigable Small World (HNSW)**
usando o access method **próprio** do TheoDB (`theodb_hnsw`). Opclass default é
`theodb_hnsw_l2_ops`; use `theodb_hnsw_cosine_ops` / `theodb_hnsw_ip_ops` para
cosseno / produto interno.

> **Coexistência:** o TheoDB também expõe o HNSW do pgvector (`USING hnsw (…
> vector_cosine_ops)`). Essa superfície mira o AM do pgvector, **não** o AM próprio
> `theodb_hnsw`.

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
USING theodb_hnsw (
    description_embedding theodb_hnsw_l2_ops
);
```

Cria índice baseado em distância Euclidiana (opclass default).

---

# 7. Índice usando Inner Product

```sql
CREATE INDEX my_index
ON products
USING theodb_hnsw (
    description_embedding theodb_hnsw_ip_ops
);
```

Cria índice utilizando produto interno.

---

# 8. Índice usando Cosine Distance

```sql
CREATE INDEX my_index
ON products
USING theodb_hnsw (
    description_embedding theodb_hnsw_cosine_ops
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
USING theodb_hnsw (
    embedding theodb_hnsw_cosine_ops
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

# 22. Consulta usando `theodb.embed()`

```sql
SELECT *
FROM products
ORDER BY embedding
<-> theodb.embed('wireless headphones', 'text-embedding-3-small')
LIMIT 10;
```

Converte texto em embedding durante a consulta. `theodb.embed(content, model)`
recebe o conteúdo primeiro e retorna `vector` diretamente.

---

# 23. Consulta usando distância cosseno com texto

```sql
SELECT *
FROM products
ORDER BY embedding
<=> theodb.embed('running shoes', 'text-embedding-3-small')
LIMIT 10;
```

Busca semântica baseada em texto.

---

# 24. Consulta usando produto interno com texto

```sql
SELECT *
FROM products
ORDER BY embedding
<#> theodb.embed('smartphone', 'text-embedding-3-small')
LIMIT 10;
```

Executa busca usando Inner Product.

---

# 25. `theodb.embed()` já retorna `vector`

```sql
theodb.embed('shoe', 'text-embedding-3-small')
```

`theodb.embed(content, model)` retorna o tipo `vector` diretamente — não é preciso cast.

---

# 26. Consulta com filtro SQL

```sql
SELECT *
FROM products
WHERE category_id = 2
ORDER BY embedding
<=> theodb.embed('hoodie', 'text-embedding-3-small')
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
<=> theodb.embed('hoodie', 'text-embedding-3-small')
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
    theodb.embed('hoodie', 'text-embedding-3-small') AS distance
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
USING theodb_hnsw (
    embedding theodb_hnsw_cosine_ops
)
WITH (
    m = 16,
    ef_construction = 64
);

SELECT *
FROM products
ORDER BY embedding
<=> theodb.embed('wireless headphones', 'text-embedding-3-small')
LIMIT 10;
```

Fluxo completo de uso do HNSW no TheoDB:

1. instalar `pgvector`;
2. criar índice HNSW;
3. gerar embedding da consulta;
4. executar busca vetorial por similaridade.
