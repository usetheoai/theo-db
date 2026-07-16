# Análise de sentimento de texto

> **Status:** ✅ **Entregue (M7-S3).** A função `ai.analyze_sentiment(content text, model text DEFAULT NULL)
> RETURNS text` (`theodb_rs/src/api.rs:334`, implementada em `theodb_rs/src/chat.rs:73` `ai_sentiment`) classifica
> o texto num rótulo de sentimento via LLM, com erro tipado em saída malformada. Provado por
> `benchmarks/tests/test_ai_sql.py` (`test_analyze_sentiment_in_label_set:101`,
> `test_sentiment_malformed_output_raises_typed:290`). **Nota de honestidade:** a acurácia depende do modelo LLM
> configurado (modelo síncrono por-linha, ADR `docs/adr/0007-synchronous-per-row-model-http.md`); não há benchmark
> de acurácia de sentimento publicado.

Esta página cobre a função `ai.analyze_sentiment()` — assinatura, parâmetros e uso escalar para classificar
o sentimento de textos.

---

# 1. Assinatura da função

```sql
FUNCTION ai.analyze_sentiment(
    content TEXT,
    model TEXT DEFAULT NULL
)
RETURNS TEXT;
```

Classifica o texto num rótulo de sentimento (`positive` / `negative` / `neutral`) via LLM. Não requer
`CREATE EXTENSION` nem flags de preview — as funções `ai.*` vivem no schema `ai`.

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
SELECT ai.analyze_sentiment('This movie is very good');
```

Executa a versão escalar da função para um único texto.

---

# 7. Informar modelo específico

```sql
SELECT ai.analyze_sentiment(
    'This movie is very good',
    'theodb-text-lite'
);
```

Permite utilizar um modelo diferente do padrão (segundo argumento `model`).

---

# 8. Utilizar modelo padrão

```sql
SELECT ai.analyze_sentiment('This movie is very good');
```

Quando o segundo argumento `model` é omitido, o modelo padrão (das GUCs de sessão) é utilizado automaticamente.

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

## 🎯 API-alvo / roadmap (não-shipped)

**As seções 12–38 abaixo descrevem os modos array-based e cursor-based estilo AlloyDB e NÃO estão
implementados.** A superfície entregue de `ai.analyze_sentiment` é **escalar** (`content, model`) — seções
1–11 acima. Não use os exemplos desta seção (arrays com `prompts =>`/`batch_size =>`, `REFCURSOR`,
`input_cursor =>`) como código executável.

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

* **Scalar** (entregue): processa um único texto por chamada — `ai.analyze_sentiment(content, model)`.
* **Array-based** / **Cursor-based** (🎯 roadmap): ver a seção "API-alvo / roadmap" acima; não implementados.

---

# 40. Fluxo geral recomendado (shipped)

```sql
-- (opcional) registrar e aplicar um modelo
SELECT theodb_ml.create_model('theodb-text-lite', '<your-llm-endpoint>', 'theodb-text-lite');
SELECT theodb_ml.apply_model('theodb-text-lite');

-- classificar sentimento linha-a-linha
SELECT
    id,
    ai.analyze_sentiment(review_content)
FROM reviews;
```

Fluxo entregue:

1. (opcional) registrar/aplicar um modelo no registry `theodb_ml`;
2. utilizar `ai.analyze_sentiment(content, model)` no modo escalar, linha a linha.
