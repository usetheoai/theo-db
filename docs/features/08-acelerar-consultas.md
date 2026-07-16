# Acelerar consultas com funções otimizadas

> **Status:** ✅ **Entregue (M11 + M18).** A aceleração de funções de IA está disponível: `ai.generate_batch(prompts
> text[], model text DEFAULT NULL) RETURNS text[]` responde N prompts em UM round-trip HTTP
> (`theodb_rs/src/api.rs`, lógica em `theodb_rs/src/chat.rs` `ai_generate_batch`) — o padrão N-in/N-out que evita o
> N+1 de chamadas por-linha; e `ai.if_batch(condition text, vals text[], model text DEFAULT NULL) RETURNS
> boolean[]` para classificação booleana em massa (mais o escalar `ai.if_costly`), em
> `theodb_rs/src/chat.rs`. Provado por `benchmarks/tests/test_ai_sql.py` (9 testes de `generate_batch` + testes
> de `ai.if_batch`/`ai.if_costly`). **Nota de honestidade:** a aceleração é o batching de round-trips (N→1);
> o "Proxy Model local" descrito abaixo (estilo AlloyDB) não é a superfície entregue — a nossa é `ai.generate_batch`
> / `ai.if_batch`. Qualidade/latência dependem do LLM configurado (ADR `docs/adr/0007-synchronous-per-row-model-http.md`).

Esta página cobre a aceleração de funções de IA do TheoDB. A superfície **entregue** é o batching de
round-trips com `ai.generate_batch` (N prompts → 1 chamada HTTP), documentado primeiro. A seção
**🎯 API-alvo / roadmap (não-shipped)** ao final descreve o "Proxy Model local" estilo AlloyDB, que **não**
está implementado.

---

# 1. A aceleração entregue: `ai.generate_batch`

```sql
FUNCTION ai.generate_batch(
    prompts TEXT[],
    model TEXT DEFAULT NULL
)
RETURNS TEXT[];
```

`ai.generate_batch` responde N prompts em **um único round-trip HTTP** (N-in/N-out), eliminando o N+1 de
chamadas por-linha. É o mecanismo de aceleração real do TheoDB. Chamadas grandes são fatiadas automaticamente
em blocos de `theodb.ai_max_batch`.

---

# 2. Acelerar geração linha-a-linha com batching

```sql
SELECT UNNEST(
    ai.generate_batch(
        ARRAY_AGG('Summarize in 20 words: ' || review)
    )
)
FROM restaurant_reviews;
```

Colapsa N gerações por-linha numa só chamada ao endpoint e reexpande o array de respostas em linhas.

---

# 3. Acelerar classificação em lote com `ai.if_batch`

```sql
SELECT UNNEST(
    ai.if_batch(
        'Is this a positive review?',
        ARRAY_AGG(review)
    )
)
FROM restaurant_reviews;
```

Para classificação booleana em massa, `ai.if_batch(condition TEXT, vals TEXT[], model TEXT DEFAULT NULL)`
retorna um array de booleanos numa única chamada — o análogo de classificação do `ai.generate_batch`.

---

# 4. Configurar o tamanho de bloco (`theodb.ai_max_batch`)

```sql
SET theodb.ai_max_batch = 20;
```

Define quantos prompts por chamada HTTP são enviados; arrays maiores são fatiados em blocos deste tamanho.

---

## 🎯 API-alvo / roadmap (não-shipped)

**Tudo abaixo descreve a superfície-alvo estilo AlloyDB e NÃO está implementado.** O "Proxy Model local"
(2-arg `ai.if(prompt, embedding)`, `theodb_ml.enable_ai_query_engine`, `theodb_ml.runtime_accuracy_check`,
treinamento local disparado por `PREPARE`, e o DDL de extensão `CREATE EXTENSION`/`ALTER EXTENSION theodb_ml`)
é roadmap. A aceleração **entregue** é `ai.generate_batch` / `ai.if_batch` (seções 1–4 acima). Não use os
exemplos desta seção como código executável.

# 5. Criar tabela de exemplo

```sql
CREATE TABLE restaurant_reviews (
    id SERIAL,
    name VARCHAR(64),
    city VARCHAR(64),
    review TEXT,
    review_embedding VECTOR(768)
);
```

Cria uma tabela contendo:

* coluna textual (`review`);
* coluna de embeddings (`review_embedding`).

---

# 4. Definir coluna de conteúdo

```sql
review TEXT
```

Coluna utilizada como entrada do modelo LLM.

---

# 5. Definir coluna de embedding

```sql
review_embedding VECTOR(768)
```

Coluna utilizada pelo Proxy Model durante as consultas otimizadas.

---

# 6. Preparar consulta otimizada

```sql
PREPARE positive_reviews_query AS
SELECT
    r.name,
    r.city
FROM restaurant_reviews r
WHERE ai.if(
    'Is the following a positive review? Review: '
    || r.review,
    r.review_embedding
)
GROUP BY
    r.name,
    r.city
HAVING COUNT(*) > 500;
```

Cria uma consulta preparada e inicia, em segundo plano, o treinamento do Proxy Model.

---

# 7. Executar consulta preparada

```sql
EXECUTE positive_reviews_query;
```

Executa a consulta utilizando o modelo proxy treinado.

---

# 8. Executar a mesma consulta sem `EXECUTE`

```sql
SELECT
    r.name,
    r.city
FROM restaurant_reviews r
WHERE ai.if(
    'Is the following a positive review? Review: '
    || r.review,
    r.review_embedding
)
GROUP BY
    r.name,
    r.city
HAVING COUNT(*) > 500;
```

Outras conexões podem executar exatamente a mesma consulta e reutilizar o Proxy Model.

---

# 9. Utilizar `ai.if()` otimizado

```sql
ai.if(
    'Prompt...',
    review_embedding
)
```

Nesta modalidade, o segundo parâmetro é o embedding da linha.

Isso permite utilizar o Proxy Model local.

---

# 10. Assinatura otimizada de `ai.if`

```sql
ai.if(
    prompt,
    embedding_column
)
```

Executa classificação utilizando:

* prompt;
* embedding previamente armazenado.

---

# 11. Filtrar registros

```sql
SELECT *
FROM restaurant_reviews
WHERE ai.if(
    'Positive review: '
    || review,
    review_embedding
);
```

Filtra registros utilizando IA otimizada.

---

# 12. Agrupar resultados

```sql
GROUP BY
    name,
    city
```

Agrupa os registros classificados.

---

# 13. Filtrar grupos

```sql
HAVING COUNT(*) > 500;
```

Retorna apenas restaurantes com mais de 500 avaliações positivas.

---

# 14. Preparar novamente o modelo

```sql
PREPARE positive_reviews_query AS
SELECT ...
```

Executar novamente o `PREPARE` inicia um novo treinamento.

---

# 15. Desabilitar validação de acurácia

```sql
ALTER DATABASE my_database
SET theodb_ml.runtime_accuracy_check = off;
```

Desativa a validação automática do Proxy Model.

---

# 16. Habilitar AI Query Engine

```sql
SET theodb_ml.enable_ai_query_engine = on;
```

Ativa o mecanismo AI necessário para consultas inteligentes.

---

# 17. Verificar treinamento via `PREPARE`

```sql
PREPARE ...
```

O treinamento do modelo ocorre de forma assíncrona.

---

# 18. Executar modelo treinado

```sql
EXECUTE positive_reviews_query;
```

Utiliza o Proxy Model treinado localmente.

---

# 19. Reutilizar consulta

```sql
SELECT ...
WHERE ai.if(
    'Prompt',
    review_embedding
);
```

Consultas idênticas podem reutilizar o modelo.

---

# 20. Fluxo completo de treinamento

```sql
PREPARE positive_reviews_query AS
SELECT
    name
FROM restaurant_reviews
WHERE ai.if(
    'Positive review: '
    || review,
    review_embedding
);

EXECUTE positive_reviews_query;
```

Fluxo mínimo para criação e uso do Proxy Model.

---

# 21. Fluxo completo com agrupamento

```sql
PREPARE positive_reviews_query AS
SELECT
    name,
    city
FROM restaurant_reviews
WHERE ai.if(
    'Positive review: '
    || review,
    review_embedding
)
GROUP BY
    name,
    city
HAVING COUNT(*) > 500;

EXECUTE positive_reviews_query;
```

Exemplo completo de uso.

---

# 22. Reprocessar após alteração dos dados

```sql
PREPARE positive_reviews_query AS
...
```

Caso os dados mudem significativamente, um novo `PREPARE` treina um novo modelo.

---

# 23. Reprocessar após alterar o prompt

```sql
PREPARE another_query AS
SELECT ...
WHERE ai.if(
    'Is this an excellent review? '
    || review,
    review_embedding
);
```

Mudanças no prompt exigem novo treinamento.

---

# 24. Reprocessar após alterar embedding

```sql
ai.if(
    prompt,
    new_embedding_column
)
```

Trocar a coluna de embeddings invalida o Proxy Model existente.

---

# 25. Consulta usando apenas uma função `ai.if`

```sql
SELECT *
FROM restaurant_reviews
WHERE ai.if(
    'Positive review'
    || review,
    review_embedding
);
```

O treinamento só ocorre quando existe exatamente uma chamada `ai.if()`.

---

# 26. Consulta sem subquery

```sql
SELECT *
FROM restaurant_reviews
WHERE ai.if(...);
```

O `ai.if()` deve estar diretamente na consulta principal.

---

# 27. Consulta elegível para otimização

```sql
SELECT
    name
FROM restaurant_reviews
WHERE ai.if(
    prompt,
    review_embedding
);
```

Consulta simples compatível com o treinamento automático.

---

# 28. Consulta não elegível

```sql
SELECT *
FROM (
    SELECT *
    FROM restaurant_reviews
) t
WHERE ai.if(...);
```

`ai.if()` dentro de subconsultas não utiliza Proxy Model.

---

# 29. Fluxo de fallback para LLM

```text
Proxy Model
      ↓
Accuracy Check
      ↓
LLM (fallback)
```

Quando a acurácia do modelo proxy não atinge o limite esperado, a consulta é automaticamente executada pelo LLM remoto.

---

# 30. Fluxo completo recomendado

```sql
SELECT extversion
FROM pg_extension
WHERE extname = 'theodb_ml';

ALTER EXTENSION theodb_ml UPDATE;

PREPARE positive_reviews_query AS
SELECT
    name,
    city
FROM restaurant_reviews
WHERE ai.if(
    'Is the following a positive review? '
    || review,
    review_embedding
)
GROUP BY
    name,
    city
HAVING COUNT(*) > 500;

EXECUTE positive_reviews_query;
```

Fluxo completo recomendado para uso das **Optimized AI Functions**:

1. verificar a versão da extensão;
2. atualizar a extensão, se necessário;
3. preparar a consulta (`PREPARE`), iniciando o treinamento do Proxy Model;
4. executar a consulta (`EXECUTE`), utilizando o modelo local sempre que possível;
5. caso a validação de acurácia falhe ou o modelo não esteja disponível, o TheoDB faz automaticamente o fallback para um LLM remoto.
