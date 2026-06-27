# Análise de sentimento de texto

> **Status:** 📋 Especificação (planejado) — recurso-alvo do milestone **M7 — IA avançada** ([ROADMAP](../../ROADMAP.md)).
> Esta página documenta a **API-alvo do TheoDB**. As funcionalidades aqui descritas **ainda não estão
> implementadas** na release atual (M0 entrega PostgreSQL 17 + `pgvector`). Nenhum número de desempenho
> nesta página é um benchmark — benchmarks reproduzíveis vivem em `docs/benchmarks/` quando publicados
> (CLAUDE.md, regra TheoDB 5).

Esta página cobre a função `ai.analyze_sentiment()` — consultas SQL, parâmetros e modos de execução
(escalar, baseado em arrays e baseado em cursor) para classificar o sentimento de textos.

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

Atualiza a extensão para a versão Preview que contém `ai.analyze_sentiment()`.

---

# 3. Habilitar funções Preview

```sql
SET theodb_ml.enable_preview_ai_functions = 'on';
```

Ativa as funções experimentais de IA para a sessão atual.

---

# 4. Criar tabela de avaliações

```sql
CREATE TABLE IF NOT EXISTS reviews (
    id INT PRIMARY KEY,
    review_content TEXT
);
```

Cria a tabela utilizada para armazenar avaliações textuais.

---

# 5. Inserir dados de exemplo

```sql
INSERT INTO reviews (id, review_content)
VALUES
(1,'This movie is very good'),
(2,'The actors play the parts well'),
(3,'I like the music in this film'),
(4,'The story is easy to follow'),
(5,'Many people will enjoy this show'),
(6,'The film is too long'),
(7,'I do not like the ending'),
(8,'This movie is very boring'),
(9,'The story is okay'),
(10,'Some parts are fine');
```

Popula a tabela para testes de análise de sentimento.

---

# 6. Analisar sentimento de um texto

```sql
SELECT ai.analyze_sentiment(
    prompt => 'This movie is very good'
);
```

Executa a versão escalar da função para um único texto.

---

# 7. Informar modelo específico

```sql
SELECT ai.analyze_sentiment(
    prompt => 'This movie is very good',
    model_id => 'theodb-text-lite'
);
```

Permite utilizar um modelo diferente do padrão.

---

# 8. Utilizar modelo padrão

```sql
model_id => 'theodb-text-lite'
```

Caso `model_id` seja omitido, este modelo é utilizado automaticamente.

---

# 9. Analisar uma coluna

```sql
SELECT
    ai.analyze_sentiment(review_content)
FROM reviews;
```

Executa análise linha a linha.

---

# 10. Analisar juntamente com o ID

```sql
SELECT
    id,
    ai.analyze_sentiment(review_content)
FROM reviews;
```

Retorna cada registro juntamente com seu sentimento.

---

# 11. Retorno esperado

```text
positive
negative
neutral
```

A função retorna uma destas três classificações.

---

# 12. Assinatura baseada em arrays

```sql
SELECT ai.analyze_sentiment(
    prompts => ARRAY[
        'TEXT_1',
        'TEXT_2'
    ]
);
```

Processa vários textos em uma única chamada.

---

# 13. Configurar `batch_size`

```sql
batch_size => 15
```

Define quantos textos serão enviados por lote.

---

# 14. Agrupar textos

```sql
ARRAY_AGG(review_content)
```

Transforma várias linhas em um único array.

---

# 15. Analisar em lote

```sql
SELECT ai.analyze_sentiment(
    prompts => ARRAY_AGG(review_content)
)
FROM reviews;
```

Executa análise em lote.

---

# 16. Adicionar prompt personalizado

```sql
SELECT ai.analyze_sentiment(
    prompts => ARRAY_AGG(
        'Please analyze the sentiment of this review: '
        || review_content
    )
)
FROM reviews;
```

Permite contextualizar melhor cada texto.

---

# 17. Agrupar IDs

```sql
ARRAY_AGG(id ORDER BY id)
```

Mantém a correspondência entre resultados e registros.

---

# 18. Recuperar posições

```sql
generate_series(
    1,
    array_length(ids,1)
)
```

Percorre os arrays retornados.

---

# 19. Correlacionar resultados

```sql
SELECT
    ids[i] AS id,
    sentiments[i] AS sentiment
FROM sentiment_results,
generate_series(
    1,
    array_length(ids,1)
) AS i;
```

Relaciona cada sentimento ao seu registro original.

---

# 20. Fazer JOIN com a tabela

```sql
SELECT
    reviews.id,
    correlated_results.sentiment
FROM reviews
JOIN correlated_results
ON reviews.id =
   correlated_results.id;
```

Retorna os sentimentos associados aos registros.

---

# 21. Ordenar resultados

```sql
ORDER BY reviews.id DESC;
```

Ordena o resultado final.

---

# 22. Assinatura baseada em cursor

```sql
CREATE OR REPLACE FUNCTION ai.analyze_sentiment(
    prompt TEXT,
    input_cursor REFCURSOR,
    batch_size INT DEFAULT NULL,
    model_id VARCHAR(100) DEFAULT NULL
)
RETURNS REFCURSOR;
```

Versão destinada ao processamento de grandes volumes.

---

# 23. Iniciar transação

```sql
BEGIN;
```

Inicia o processamento utilizando cursores.

---

# 24. Declarar cursor

```sql
DECLARE review_cursor REFCURSOR;
```

Declara o cursor de entrada.

---

# 25. Abrir cursor

```sql
OPEN review_cursor
FOR
SELECT review_content
FROM reviews;
```

Carrega os dados que serão analisados.

---

# 26. Declarar cursor de saída

```sql
DECLARE result_cursor REFCURSOR;
```

Recebe os resultados produzidos pela IA.

---

# 27. Executar análise via cursor

```sql
SELECT ai.analyze_sentiment(
    prompt => 'Analyze the sentiment of the following movie review:',
    input_cursor => review_cursor,
    batch_size => 5
)
INTO result_cursor;
```

Executa análise em streaming.

---

# 28. Buscar resultados

```sql
FETCH ALL
FROM result_cursor;
```

Recupera todos os registros processados.

---

# 29. Fechar cursor de entrada

```sql
CLOSE review_cursor;
```

Libera recursos.

---

# 30. Fechar cursor de saída

```sql
CLOSE result_cursor;
```

Finaliza o processamento.

---

# 31. Encerrar transação

```sql
COMMIT;
```

Confirma a execução da transação.

---

# 32. Estrutura do retorno

```text
review_content
sentiment
score
```

O cursor retorna:

* texto original;
* classificação;
* score de confiança.

---

# 33. Exemplo de score positivo

```text
Positive | 0.9
```

Representa alta confiança em sentimento positivo.

---

# 34. Exemplo de score negativo

```text
Negative | -0.8
```

Representa alta confiança em sentimento negativo.

---

# 35. Exemplo de score neutro

```text
Neutral | 0.1
```

Representa baixa polarização.

---

# 36. Fluxo completo (análise individual)

```sql
SELECT
    id,
    ai.analyze_sentiment(review_content)
FROM reviews;
```

Analisa cada linha individualmente.

---

# 37. Fluxo completo (processamento em lote)

```sql
WITH sentiment_results AS (
    SELECT
        ARRAY_AGG(id ORDER BY id) AS ids,
        ai.analyze_sentiment(
            prompts => ARRAY_AGG(
                review_content
                ORDER BY id
            ),
            batch_size => 15
        ) AS sentiments
    FROM reviews
)
SELECT *
FROM sentiment_results;
```

Processa múltiplos registros em uma única chamada ao modelo.

---

# 38. Fluxo completo (cursor)

```sql
BEGIN;

DECLARE review_cursor REFCURSOR;

OPEN review_cursor
FOR
SELECT review_content
FROM reviews;

SELECT ai.analyze_sentiment(
    prompt => 'Analyze sentiment',
    input_cursor => review_cursor,
    batch_size => 5
)
INTO result_cursor;

FETCH ALL
FROM result_cursor;

CLOSE review_cursor;
CLOSE result_cursor;

COMMIT;
```

Fluxo recomendado para grandes volumes de dados utilizando cursores.

---

# 39. Modos de execução suportados

A função `ai.analyze_sentiment()` possui **três modos de processamento**:

* **Scalar**: processa um único texto por chamada.
* **Array-based**: processa múltiplos textos em lote utilizando arrays.
* **Cursor-based**: processa grandes conjuntos de dados utilizando `REFCURSOR`, permitindo streaming e menor consumo de memória.

---

# 40. Fluxo geral recomendado

```sql
SELECT extversion
FROM pg_extension
WHERE extname = 'theodb_ml';

CALL theodb_ml.upgrade_to_preview_version();

SET theodb_ml.enable_preview_ai_functions = 'on';

SELECT
    id,
    ai.analyze_sentiment(review_content)
FROM reviews;
```

Fluxo completo recomendado:

1. verificar a versão da extensão;
2. atualizar para a versão Preview (se necessário);
3. habilitar as funções Preview;
4. utilizar `ai.analyze_sentiment()` no modo escalar, em lote ou com cursores conforme o volume de dados.
