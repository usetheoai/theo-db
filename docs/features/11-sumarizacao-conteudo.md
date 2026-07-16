# Sumarização de conteúdo

> **Status:** ✅ **Entregue (M10 + M18).** A sumarização está disponível em duas formas:
> a função escalar `ai.summarize(content text, model text DEFAULT NULL) RETURNS text`
> (`sql/50-theodb-ai.sql:32`, plpgsql que chama o `ai._chat` **em Rust**, `theodb_rs/src/chat.rs`) e o agregado
> `ai.agg_summarize(text)` que colapsa várias linhas num único resumo (`sql/50-theodb-ai.sql:82`, com
> `_agg_summ_accum`/`_agg_summ_final`). Provado por `benchmarks/tests/test_ai_sql.py`
> (`test_summarize_returns_text`, `test_agg_summarize_over_rows`, `test_agg_summarize_empty_and_null_input_is_null`,
> `test_agg_summarize_finalfunc_is_volatile`, `test_agg_summarize_skips_null_and_empty_rows`,
> `test_agg_summarize_propagates_empty_completion_typed` — 6 verdes). **Nota de honestidade:** a qualidade do
> resumo depende do modelo LLM configurado (modelo síncrono por-linha, ADR
> `docs/adr/0007-synchronous-per-row-model-http.md`); não há benchmark de qualidade de sumarização — a validação é
> o teste de contrato contra o container. As seções abaixo (versionamento `theodb_ml`, flags de preview) descrevem
> a API-alvo estilo AlloyDB; a superfície entregue do TheoDB é `ai.summarize` / `ai.agg_summarize` no schema `ai`.

Esta página cobre as funções `ai.summarize()` (escalar) e `ai.agg_summarize()` (agregada) — assinaturas,
parâmetros e uso para gerar resumos de texto.

---

# 1. Assinaturas das funções

```sql
FUNCTION ai.summarize(
    content TEXT,
    model TEXT DEFAULT NULL
)
RETURNS TEXT;

AGGREGATE ai.agg_summarize(TEXT) RETURNS TEXT;
```

`ai.summarize` resume um único texto por chamada; `ai.agg_summarize` é um agregado que colapsa várias linhas
num único resumo. Não requerem `CREATE EXTENSION` nem flags de preview — as funções `ai.*` vivem no schema `ai`.

---

# 2. (opcional) Registrar/selecionar um modelo

```sql
SELECT theodb_ml.create_model('theodb-text-lite', '<your-llm-endpoint>', 'theodb-text-lite');
SELECT theodb_ml.apply_model('theodb-text-lite');
```

`theodb_ml` é um schema + registry de modelos (não uma extensão). Registrar/aplicar um modelo é opcional; sem
isso, o modelo padrão vem das GUCs de sessão (`theodb.llm_endpoint`/`theodb.llm_model`).

---

# 3. Criar tabela de avaliações

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
SELECT ai.summarize('TEXT_CONTENT');
```

Executa a versão escalar da função para um único texto.

---

# 7. Resumir utilizando modelo específico

```sql
SELECT ai.summarize(
    'TEXT_CONTENT',
    'theodb-text-lite'
);
```

Permite escolher explicitamente o modelo (segundo argumento `model`).

---

# 8. Modelo padrão

```sql
SELECT ai.summarize('TEXT_CONTENT');
```

Quando o segundo argumento `model` é omitido, o modelo padrão (das GUCs de sessão) é utilizado.

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

## 🎯 API-alvo / roadmap (não-shipped)

**As seções 11–31 abaixo descrevem os modos array-based e cursor-based estilo AlloyDB e NÃO estão
implementados.** A superfície entregue de `ai.summarize` é **escalar** (`content, model`); o modo **agregado**
`ai.agg_summarize` (seções 32+ abaixo) também está entregue. Não use os exemplos desta seção (arrays com
`prompts =>`/`batch_size =>`, `REFCURSOR`, `input_cursor =>`) como código executável.

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

## ✅ Superfície entregue (continuação): agregado `ai.agg_summarize`

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

# 37. Fluxo completo (processamento em lote) — 🎯 roadmap (não-shipped)

> Modo array-based não implementado — ver "API-alvo / roadmap" acima. Não executável.

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

# 38. Fluxo completo (cursores) — 🎯 roadmap (não-shipped)

> Modo cursor-based não implementado — ver "API-alvo / roadmap" acima. Não executável.

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

* **Scalar (`ai.summarize`)** — ✅ entregue: resume um único texto por chamada (`content, model`).
* **Aggregate (`ai.agg_summarize`)** — ✅ entregue: combina várias linhas em um único resumo consolidado por grupo.
* **Array-based / Cursor-based** — 🎯 roadmap: ver a seção "API-alvo / roadmap" acima; não implementados.
