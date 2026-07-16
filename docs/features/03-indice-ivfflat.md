# Criar um índice IVFFlat

> **✅ Validado (M9, 2026-06-28):** o índice IVFFlat do `pgvector` está disponível na imagem e foi
> **validado + benchmarkado** no harness recall@k (`--index ivfflat`). Evidência medida (recall × QPS
> vs HNSW, build-time, tamanho) em [`docs/benchmarks/m9-ivfflat.md`](../benchmarks/m9-ivfflat.md).
> A capacidade IVFFlat está disponível via `CREATE INDEX … USING ivfflat (…) WITH (lists = N)` +
> `SET ivfflat.probes`. A superfície literal abaixo permanece como API-alvo do TheoDB.

> **Status:** ✅ **Entregue (M9 + M21 + M34).** Duas superfícies: (a) `USING ivfflat` do pgvector nativo na imagem
> (benchmark em [`docs/benchmarks/m9-ivfflat.md`](../benchmarks/m9-ivfflat.md)); e (b) o **access method próprio**
> `theodb_ivfflat` em Rust (`theodb_rs/src/am/mod.rs:35`, opclass `theodb_ivfflat_l2_ops` `theodb_rs/src/am/mod.rs:164`,
> reloption `WITH (lists=N)` `theodb_rs/src/am/options.rs`, GUC `theodb_ivfflat.probes` `theodb_rs/src/am/guc.rs:30`).
> Provado por `benchmarks/tests/test_index_am.py` (criação/persistência/scan) + `benchmarks/tests/test_ann_index.py`
> (`test_ivfflat_knn_recall_high_vs_bruteforce`, `test_recall_parity_gate`) + `benchmarks/tests/test_reloption.py`.
> A sintaxe literal `WITH (lists)` do pgvector e o AM próprio coexistem.

Esta página cobre a criação de índices `IVFFlat` no TheoDB para busca aproximada de vizinhos mais próximos sobre colunas vetoriais, incluindo as métricas de distância suportadas, o parâmetro `lists` e exemplos de consulta.

---

# 1. Instalar extensão `vector`

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

Instala a extensão `pgvector`, necessária para criar índices `ivfflat` e executar consultas vetoriais.

---

# 2. Criar índice IVFFlat básico (AM próprio `theodb_ivfflat`)

```sql
CREATE INDEX products_ivfflat_idx
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
)
WITH (
    lists = 100
);
```

Cria um índice `IVFFlat` para busca aproximada de vizinhos mais próximos usando o
access method **próprio** do TheoDB (`theodb_ivfflat`). Opclass default é
`theodb_ivfflat_l2_ops`; use `theodb_ivfflat_cosine_ops` / `theodb_ivfflat_ip_ops`
para cosseno / produto interno.

> **Coexistência:** o TheoDB também expõe o IVFFlat do pgvector (`USING ivfflat (…
> vector_cosine_ops)`). Essa superfície mira o AM do pgvector, **não** o AM próprio
> `theodb_ivfflat`.

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
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_l2_ops
)
WITH (
    lists = 100
);
```

Cria índice usando distância Euclidiana (opclass default).

---

# 7. Índice IVFFlat com Inner Product

```sql
CREATE INDEX products_ivfflat_ip
ON products
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_ip_ops
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
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
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
USING theodb_ivfflat (
    CAST(description_embedding AS vector(768))
    theodb_ivfflat_cosine_ops
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
<=> theodb.embed('running shoes', 'text-embedding-3-small')
LIMIT 10;
```

Gera embedding a partir de texto e compara com os embeddings armazenados.

---

# 22. `theodb.embed()` já retorna `vector`

```sql
theodb.embed('running shoes', 'text-embedding-3-small')
```

`theodb.embed(content, model)` retorna o tipo `vector` diretamente — não é preciso cast.

---

# 23. Consulta com filtro SQL

```sql
SELECT *
FROM products
WHERE category_id = 3
ORDER BY description_embedding
<=> theodb.embed('comfortable shoes', 'text-embedding-3-small')
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
<=> theodb.embed('casual hoodie', 'text-embedding-3-small')
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
    <=> theodb.embed('casual hoodie', 'text-embedding-3-small') AS distance
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
USING theodb_ivfflat (
    description_embedding theodb_ivfflat_cosine_ops
)
WITH (
    lists = 100
);

SELECT
    product_id,
    name,
    description_embedding
    <=> theodb.embed('wireless headphones', 'text-embedding-3-small') AS distance
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
