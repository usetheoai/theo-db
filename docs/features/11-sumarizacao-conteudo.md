# Sumarização de conteúdo

> **Status:** 📋 Especificação (planejado) — recurso-alvo do milestone **M7 — IA avançada** ([ROADMAP](../../ROADMAP.md)).
> Esta página documenta a **API-alvo do TheoDB**. As funcionalidades aqui descritas **ainda não estão
> implementadas** na release atual (M0 entrega PostgreSQL 17 + `pgvector`). Nenhum número de desempenho
> nesta página é um benchmark — benchmarks reproduzíveis vivem em `docs/benchmarks/` quando publicados
> (CLAUDE.md, regra TheoDB 5).

Esta página cobre as funções `ai.summarize()` e `ai.agg_summarize()` — consultas SQL, parâmetros e
modos de execução (escalar, baseado em arrays, baseado em cursor e agregado) para gerar resumos de texto.

---

# 1. Verificar versão da extensão

```sql
SELECT extversion
FROM pg_extension
WHERE extname = 'theodb_ml';
```

Verifica se a extensão `theodb_ml` está na versão **1.5.7** ou superior.

---

# 2. Atualizar para versão Preview

```sql
CALL theodb_ml.upgrade_to_preview_version();
```

Atualiza a extensão para a versão Preview que disponibiliza `ai.summarize()`.

---

# 3. Habilitar funções Preview

```sql
SET theodb_ml.enable_preview_ai_functions = 'on';
```

Habilita as funções experimentais de IA para a sessão atual.

---

# 4. Criar tabela de avaliações

```sql
CREATE TABLE movie_reviews (
    id INT PRIMARY KEY,
    movie_id INT,
    review TEXT
);
```

Cria a tabela utilizada nos exemplos.

---

# 5. Inserir dados de exemplo

```sql
INSERT INTO movie_reviews (id, movie_id, review)
VALUES (...);
```

Popula a tabela com avaliações de filmes.

---

# 6. Resumir um único texto

```sql
SELECT ai.summarize(
    prompt => 'TEXT_CONTENT'
);
```

Executa a versão escalar da função para um único texto.

---

# 7. Resumir utilizando modelo específico

```sql
SELECT ai.summarize(
    prompt => 'TEXT_CONTENT',
    model_id => 'theodb-text-lite'
);
```

Permite escolher explicitamente o modelo.

---

# 8. Modelo padrão

```sql
model_id => 'theodb-text-lite'
```

Modelo utilizado quando `model_id` não é informado.

---

# 9. Resumir uma coluna

```sql
SELECT ai.summarize(review)
FROM movie_reviews;
```

Resume individualmente cada linha da tabela.

---

# 10. Retornar ID e resumo

```sql
SELECT
    id,
    ai.summarize(review)
FROM movie_reviews;
```

Retorna o resumo associado a cada registro.

---

# 11. Assinatura baseada em arrays

```sql
SELECT ai.summarize(
    prompts => ARRAY[
        'TEXT_1',
        'TEXT_2'
    ]
);
```

Processa vários textos em uma única chamada.

---

# 12. Configurar `batch_size`

```sql
batch_size => 15
```

Define quantos textos serão enviados em cada lote.

---

# 13. Agrupar avaliações

```sql
ARRAY_AGG(review ORDER BY id)
```

Converte múltiplas linhas em um array.

---

# 14. Adicionar prompt personalizado

```sql
ARRAY_AGG(
    'Please summarize this in max 10 words, review: '
    || review
    ORDER BY id
)
```

Permite controlar o estilo e o tamanho do resumo.

---

# 15. Resumir em lote

```sql
SELECT ai.summarize(
    prompts => ARRAY_AGG(review)
)
FROM movie_reviews;
```

Executa processamento em lote.

---

# 16. Agrupar IDs

```sql
ARRAY_AGG(id ORDER BY id)
```

Mantém a relação entre os resumos e seus registros.

---

# 17. Recuperar posições

```sql
generate_series(
    1,
    array_length(ids,1)
)
```

Percorre os arrays retornados.

---

# 18. Correlacionar resultados

```sql
SELECT
    ids[i] AS id,
    summaries[i] AS summary
FROM summarized_results,
generate_series(
    1,
    array_length(ids,1)
) AS i;
```

Relaciona cada resumo ao seu registro.

---

# 19. Fazer JOIN com a tabela

```sql
SELECT
    movie_reviews.id,
    correlated_results.summary
FROM movie_reviews
JOIN correlated_results
ON movie_reviews.id =
   correlated_results.id;
```

Retorna os resumos vinculados às linhas originais.

---

# 20. Ordenar resultados

```sql
ORDER BY movie_reviews.id DESC;
```

Ordena os resultados.

---

# 21. Assinatura baseada em cursor

```sql
CREATE OR REPLACE FUNCTION ai.summarize(
    prompt TEXT,
    input_cursor REFCURSOR,
    batch_size INT DEFAULT NULL,
    model_id VARCHAR(100) DEFAULT NULL
)
RETURNS REFCURSOR;
```

Versão destinada ao processamento de grandes volumes.

---

# 22. Criar tabela de resultados

```sql
CREATE TABLE IF NOT EXISTS review_summaries (
    review_id INT,
    summary_text TEXT
);
```

Tabela para armazenar os resumos produzidos.

---

# 23. Abrir cursor

```sql
OPEN review_cursor
FOR
SELECT review AS prompt
FROM movie_reviews
ORDER BY id;
```

Cria o cursor de entrada.

---

# 24. Executar `ai.summarize` com cursor

```sql
cursor_response := ai.summarize(
    prompt => 'Please summarize the following review in max 10 words:',
    input_cursor => review_cursor
);
```

Executa o resumo em streaming.

---

# 25. Obter IDs

```sql
SELECT ARRAY_AGG(id ORDER BY id)
INTO id_array
FROM movie_reviews;
```

Armazena os identificadores para manter a correspondência.

---

# 26. Ler resultados

```sql
FETCH cursor_response
INTO result_record;
```

Obtém um resumo por vez.

---

# 27. Inserir resumo

```sql
INSERT INTO review_summaries (
    review_id,
    summary_text
)
VALUES (
    id_array[idx],
    result_record.output
);
```

Persiste o resumo no banco.

---

# 28. Fechar cursor de entrada

```sql
CLOSE review_cursor;
```

Libera recursos.

---

# 29. Fechar cursor de saída

```sql
CLOSE cursor_response;
```

Finaliza o processamento.

---

# 30. Executar bloco PL/pgSQL

```sql
DO $$
...
$$;
```

Permite automatizar todo o fluxo com cursores.

---

# 31. Consultar resultados

```sql
SELECT *
FROM review_summaries;
```

Verifica os resumos gerados.

---

# 32. Resumo agregado

```sql
SELECT ai.agg_summarize(review)
FROM movie_reviews
GROUP BY movie_id;
```

Produz um único resumo consolidado para cada grupo.

---

# 33. Agrupar por filme

```sql
GROUP BY movie_id;
```

Cada grupo gera um resumo independente.

---

# 34. Resumir todas as avaliações de um filme

```sql
SELECT
    movie_id,
    ai.agg_summarize(review)
FROM movie_reviews
GROUP BY movie_id;
```

Resume todas as avaliações pertencentes ao mesmo filme.

---

# 35. Diferença entre `ai.summarize` e `ai.agg_summarize`

```text
ai.summarize      → resumo por linha
ai.agg_summarize  → resumo consolidado de várias linhas
```

`ai.summarize()` trabalha registro a registro, enquanto `ai.agg_summarize()` combina vários registros em uma única entrada para gerar um resumo único.

---

# 36. Fluxo completo (resumo individual)

```sql
SELECT
    id,
    ai.summarize(review)
FROM movie_reviews;
```

Resume individualmente cada avaliação.

---

# 37. Fluxo completo (processamento em lote)

```sql
WITH summarized_results AS (
    SELECT
        ARRAY_AGG(id ORDER BY id) AS ids,
        ai.summarize(
            prompts => ARRAY_AGG(review ORDER BY id),
            batch_size => 15
        ) AS summaries
    FROM movie_reviews
)
SELECT *
FROM summarized_results;
```

Executa resumos em lote utilizando arrays.

---

# 38. Fluxo completo (cursores)

```sql
DO $$
DECLARE
    review_cursor REFCURSOR;
    cursor_response REFCURSOR;
BEGIN
    OPEN review_cursor
    FOR
    SELECT review
    FROM movie_reviews;

    cursor_response := ai.summarize(
        prompt => 'Summarize:',
        input_cursor => review_cursor
    );

    CLOSE review_cursor;
    CLOSE cursor_response;
END;
$$;
```

Fluxo recomendado para grandes volumes de dados.

---

# 39. Fluxo completo (resumo agregado)

```sql
SELECT
    movie_id,
    ai.agg_summarize(review)
FROM movie_reviews
GROUP BY movie_id;
```

Gera um resumo consolidado para cada grupo de registros.

---

# 40. Modos de execução suportados

As funções de sumarização do TheoDB oferecem quatro formas principais de processamento:

* **Scalar (`ai.summarize`)**: resume um único texto por chamada.
* **Array-based (`ai.summarize`)**: resume múltiplos textos em lote usando arrays.
* **Cursor-based (`ai.summarize`)**: processa grandes volumes utilizando `REFCURSOR`, reduzindo o consumo de memória.
* **Aggregate (`ai.agg_summarize`)**: combina várias linhas em uma única entrada para produzir um resumo consolidado por grupo.
