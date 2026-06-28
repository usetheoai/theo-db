# Consultas SQL inteligentes com funções de IA

> **Status:** 📋 Especificação (planejado) — recurso-alvo do milestone **M7 — IA avançada** ([ROADMAP](../../ROADMAP.md)).
> Esta página documenta a **API-alvo do TheoDB**. As funções escalares `ai.generate`/`ai.if`/`ai.rank`/
> `ai.analyze_sentiment`/`ai.summarize` **estão implementadas** desde M7-S3 (ver nota abaixo); os modos
> array/cursor e a extensão empacotada `theodb_ml` permanecem alvo. Nenhum número de desempenho
> nesta página é um benchmark — benchmarks reproduzíveis vivem em `docs/benchmarks/` quando publicados
> (CLAUDE.md, regra TheoDB 5).

Esta página cobre as funções SQL de IA do TheoDB (`ai.if`, `ai.generate`, `ai.rank`): suas assinaturas, parâmetros, modos de processamento (escalar, em lote e via cursor) e casos de uso para filtragem, geração e ranking inteligentes em SQL.

> **Superfície implementada (M7-S3):** as funções **escalares** `ai.generate`/`ai.if`/`ai.rank`/`ai.analyze_sentiment`/`ai.summarize`
> estão entregues (`sql/50-theodb-ai.sql`) sobre um **endpoint chat-completions OpenAI-compatible configurável**
> (GUCs `theodb.llm_endpoint`/`theodb.llm_model`/`theodb.llm_api_key`), model-agnostic, fail-fast tipado e
> `REVOKE`das de PUBLIC. Doc operacional: `docs/sql-ai-functions.md`. Os modos **em lote (array) e via cursor**
> ("aceleradas") desta página são um **follow-up documentado** (não nesta fatia — KISS/YAGNI). A extensão
> empacotada `theodb_ml` e os comandos `CALL theodb_ml.*` são a forma-alvo; hoje as funções vivem no schema `ai`.

---

# 1. Verificar versão da extensão

```sql
SELECT extversion
FROM pg_extension
WHERE extname = 'theodb_ml';
```

Consulta a versão instalada da extensão `theodb_ml`.

---

# 2. Instalar extensão

```sql
CREATE EXTENSION IF NOT EXISTS theodb_ml;
```

Instala a extensão responsável pela integração com modelos configuráveis do TheoDB.

---

# 3. Atualizar extensão

```sql
ALTER EXTENSION theodb_ml UPDATE;
```

Atualiza a extensão para a versão mais recente.

---

# 4. Habilitar AI Query Engine na sessão

```sql
SET theodb_ml.enable_ai_query_engine = on;
```

Ativa as funções `ai.*` apenas para a sessão atual.

---

# 5. Habilitar AI Query Engine para o banco

```sql
ALTER DATABASE my_database
SET theodb_ml.enable_ai_query_engine = 'on';
```

Ativa permanentemente para um banco específico.

---

# 6. Habilitar AI Query Engine para um usuário

```sql
ALTER ROLE postgres
SET theodb_ml.enable_ai_query_engine = 'on';
```

Ativa para todas as sessões desse usuário.

---

# 7. Registrar endpoint de modelo

```sql
CALL theodb_ml.create_model(
    model_id => 'theodb-text-lite-global',
    model_type => 'llm',
    model_provider => 'theodb',
    model_qualified_name => 'theodb-text-lite',
    model_request_url => 'https://...',
    model_auth_type => 'theodb_service_agent'
);
```

Registra um endpoint remoto para uso pelas funções AI.

---

# 8. Registrar modelo de texto avançado

```sql
CALL theodb_ml.create_model(
    model_id => 'theodb-text-pro-preview-model',
    model_request_url => 'https://...',
    model_qualified_name => 'theodb-text-pro-preview',
    model_provider => 'theodb',
    model_type => 'llm',
    model_auth_type => 'theodb_service_agent'
);
```

Registra um modelo disponível apenas via endpoint global.

---

# 9. Gerar texto usando modelo registrado

```sql
SELECT ai.generate(
    prompt => 'What is TheoDB?',
    model_id => 'theodb-text-pro-preview-model'
);
```

Executa geração de texto utilizando um modelo previamente registrado.

---

# 10. Assinatura do `ai.if`

```sql
FUNCTION ai.if(
    prompt TEXT,
    model_id VARCHAR DEFAULT NULL
)
RETURNS BOOLEAN;
```

Avalia uma condição em linguagem natural e retorna `TRUE` ou `FALSE`.

---

# 11. Filtrar registros com `ai.if`

```sql
SELECT name
FROM restaurant_reviews
WHERE ai.if(
    location_city ||
    ' has a population greater than 100000 and this is a positive review: '
    || review
);
```

Filtra registros utilizando conhecimento do modelo.

---

# 12. Filtrar usando modelo específico

```sql
SELECT name
FROM restaurant_reviews
WHERE ai.if(
    prompt => location_city || ' ... ' || review,
    model_id => 'theodb-text-lite'
);
```

Executa a mesma avaliação utilizando um modelo definido.

---

# 13. `GROUP BY` com `ai.if`

```sql
SELECT
    name,
    location_city
FROM restaurant_reviews
WHERE ai.if(...)
GROUP BY
    name,
    location_city
HAVING COUNT(*) > 500;
```

Combina filtro inteligente com agregações SQL.

---

# 14. JOIN usando `ai.if`

```sql
SELECT item_name,
       COUNT(*)
FROM menu_items
JOIN user_reviews
ON ai.if(
    prompt =>
        'Does this review mention the menu item? '
        || user_reviews.review_text
        || item_name
)
GROUP BY item_name;
```

Permite realizar joins semânticos.

---

# 15. `ai.if` baseado em arrays

```sql
ai.if(
    prompts => ARRAY_AGG(prompt),
    model_id => 'theodb-text-lite',
    batch_size => 20
)
```

Executa várias avaliações em lote.

---

# 16. Configurar `batch_size`

```sql
batch_size => 20
```

Define quantos prompts serão enviados por chamada.

---

# 17. Converter linhas em arrays

```sql
ARRAY_AGG(review)
```

Agrupa várias linhas para processamento em lote.

---

# 18. Expandir resultado do array

```sql
generate_series(
    1,
    array_length(review_ids,1)
)
```

Reconstrói as linhas após o processamento.

---

# 19. `ai.if` usando cursor

```sql
result_cursor := ai.if(
    'Is the statement true?',
    prompt_cursor,
    model_id => 'theodb-text-lite'
);
```

Executa processamento de grandes volumes utilizando cursores.

---

# 20. Ler resultados do cursor

```sql
FETCH result_cursor INTO rec;
```

Recupera uma linha do cursor.

---

# 21. Inserir resultado do cursor

```sql
INSERT INTO filtered_results
VALUES(rec.input, rec.output);
```

Armazena resultados processados.

---

# 22. Assinatura do `ai.generate`

```sql
FUNCTION ai.generate(
    prompt TEXT,
    model_id VARCHAR DEFAULT NULL
)
RETURNS TEXT;
```

Gera texto baseado no prompt.

---

# 23. Gerar resumo

```sql
SELECT ai.generate(
    prompt =>
        'Summarize in 20 words: '
        || review
)
FROM user_reviews;
```

Resume textos individualmente.

---

# 24. `ai.generate` baseado em arrays

```sql
SELECT UNNEST(
    ai.generate(
        prompts => ARRAY_AGG(review)
    )
);
```

Gera múltiplos resumos em lote.

---

# 25. Expandir respostas

```sql
UNNEST(...)
```

Transforma o array retornado em linhas SQL.

---

# 26. `ai.generate` usando cursor

```sql
summary_cursor := ai.generate(
    'Summarize:',
    prompt_cursor
);
```

Processa milhões de linhas utilizando streaming.

---

# 27. Salvar resumos

```sql
INSERT INTO summary_results
VALUES(rec.output);
```

Persiste os textos gerados.

---

# 28. Assinatura do `ai.rank`

```sql
FUNCTION ai.rank(
    prompt TEXT,
    model_id VARCHAR DEFAULT NULL
)
RETURNS REAL;
```

Calcula um score baseado em linguagem natural.

---

# 29. Ordenar utilizando IA

```sql
SELECT review
FROM user_reviews
ORDER BY ai.rank(
    'Score this review: '
    || review
) DESC
LIMIT 20;
```

Ordena resultados usando critérios definidos pelo prompt.

---

# 30. `LIMIT`

```sql
LIMIT 20;
```

Retorna somente os itens melhor classificados.

---

# 31. `ai.rank` baseado em arrays

```sql
SELECT UNNEST(
    ai.rank(
        ARRAY_AGG(review)
    )
);
```

Calcula vários scores em uma única chamada.

---

# 32. `ai.rank` usando cursor

```sql
score_cursor := ai.rank(
    'Score this review:',
    prompt_cursor
);
```

Executa ranking sobre grandes conjuntos de dados.

---

# 33. Armazenar score

```sql
INSERT INTO scored_results
VALUES(
    rec.input,
    rec.output
);
```

Persiste os valores calculados.

---

# 34. Cursor de entrada

```sql
OPEN prompt_cursor
FOR
SELECT review
FROM user_reviews;
```

Abre o cursor que alimentará a função AI.

---

# 35. Loop de leitura

```sql
LOOP
    FETCH score_cursor INTO rec;
    EXIT WHEN NOT FOUND;
END LOOP;
```

Processa todos os resultados do cursor.

---

# 36. Fechar cursor

```sql
CLOSE score_cursor;
```

Libera os recursos do cursor.

---

# 37. Executar bloco PL/pgSQL

```sql
DO $$
...
$$;
```

Permite executar fluxos completos envolvendo cursores e funções AI.

---

# 38. Categorias de funções AI

As funções AI do TheoDB são divididas em três categorias:

* **Scalar**: processa um único valor por chamada. Indicado para menos de 50 chamadas.
* **Array-based**: processa arrays inteiros em uma única chamada, oferecendo maior throughput.
* **Cursor-based**: processa grandes volumes (milhares ou milhões de linhas) usando cursores e streaming.

---

# 39. Fluxo completo de filtragem inteligente

```sql
SET theodb_ml.enable_ai_query_engine = on;

SELECT name
FROM restaurant_reviews
WHERE ai.if(
    prompt => 'Is this a positive review? '
              || review,
    model_id => 'theodb-text-lite'
);
```

Fluxo para habilitar o AI Query Engine e filtrar registros semanticamente.

---

# 40. Fluxo completo de geração de texto

```sql
SELECT ai.generate(
    prompt => 'Summarize the following review in 20 words: '
              || review,
    model_id => 'theodb-text-lite'
)
FROM user_reviews;
```

Fluxo para gerar resumos de texto utilizando um modelo configurável do TheoDB.

---

# 41. Fluxo completo de ranking inteligente

```sql
SELECT review
FROM user_reviews
ORDER BY ai.rank(
    prompt => 'Score this review from 1 to 10 based on customer satisfaction: '
              || review,
    model_id => 'theodb-text-lite'
) DESC
LIMIT 20;
```

Fluxo para ranquear resultados utilizando critérios definidos em linguagem natural por um modelo configurável do TheoDB.
