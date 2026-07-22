# Consultas SQL inteligentes com funções de IA

> **✅ Entregue (M7-S3 + M10/M11 + M13):** funções `ai.*` (generate/if_batch/if_costly/analyze_sentiment/
> summarize/rank/rerank, agg_summarize, generate_batch) + o **registry `theodb_ml`** (M13: `create_model`/
> `apply_model`). Ver [`docs/sql-ai-functions.md`](../sql-ai-functions.md).
> **Divergência honesta (ADR D2):** o `theodb_ml` **não** persiste credenciais (sem coluna `api_key`) — as
> chaves permanecem GUC de sessão (`theodb.llm_api_key`); `apply_model` faz a ponte via GUCs em vez do
> `model_id =>` por-chamada / `CALL …(model_auth_type=>…)` literais do AlloyDB (deferidos).

> **Status:** ✅ **Entregue — núcleo escalar + registry (M7-S3 + M10/M11 + M13).** Funções `ai.*`: `ai.generate`,
> `ai.summarize`, `ai.agg_summarize` (`sql/50-theodb-ai.sql:21,32,82`); `ai.if_batch`, `ai.if_costly`,
> `ai.analyze_sentiment`, `ai.rank`, `ai.rerank`, `ai.generate_batch`, `ai._chat` (Rust
> `theodb_rs/src/api.rs:334-355` + `theodb_rs/src/chat.rs`); registry `theodb_ml`
> (`create_model`/`apply_model`/`drop_model`/`list_models`, `sql/70-theodb-ml.sql:26,68`), tudo `REVOKE`do de PUBLIC.
> Provado por `benchmarks/tests/test_ai_sql.py` (33 testes + 3 real-OpenAI) + `benchmarks/tests/test_theodb_ml.py`.
> **Honestidade:** os modos **cursor-based** desta página **não estão implementados** (follow-up YAGNI);
> o registry não persiste credencial (chave via GUC de sessão, ADR D2). O núcleo escalar está entregue e testado.

Esta página cobre as funções SQL de IA do TheoDB (`ai.if_batch`/`ai.if_costly`, `ai.generate`, `ai.rank`): suas assinaturas, parâmetros e casos de uso para filtragem, geração e ranking inteligentes em SQL.

> **Superfície implementada (M7-S3):** as funções **escalares** `ai.generate`/`ai.if_costly`/`ai.rank`/`ai.analyze_sentiment`/`ai.summarize`
> (mais a variante em lote `ai.if_batch`) estão entregues (`sql/50-theodb-ai.sql`) sobre um **endpoint chat-completions
> OpenAI-compatible configurável** (GUCs `theodb.llm_endpoint`/`theodb.llm_model`/`theodb.llm_api_key`),
> model-agnostic, fail-fast tipado e `REVOKE`das de PUBLIC. Doc operacional: `docs/sql-ai-functions.md`. O modo
> **via cursor** desta página é um **follow-up documentado** (não nesta fatia — KISS/YAGNI). O `theodb_ml` é um
> **schema + registry de modelos** (`theodb_ml.create_model`/`apply_model`), não uma extensão — as funções vivem no schema `ai`.

---

# 1. Registrar um modelo no registry `theodb_ml`

```sql
SELECT theodb_ml.create_model(
    'theodb-text-lite',                 -- nome lógico do modelo
    '<your-llm-endpoint>',              -- endpoint chat-completions OpenAI-compatible
    'theodb-text-lite'                  -- model_name enviado ao endpoint
);
```

`theodb_ml` é um **schema + registry de modelos** (não uma extensão): `theodb_ml.create_model` cadastra um
apelido de modelo sobre um endpoint configurável. Ver também `theodb_ml.apply_model`, `theodb_ml.drop_model`
e `theodb_ml.list_models`.

---

# 2. Listar modelos registrados

```sql
SELECT theodb_ml.list_models();
```

Lista os modelos cadastrados no registry.

---

# 3. Aplicar um modelo como padrão da sessão

```sql
SELECT theodb_ml.apply_model('theodb-text-lite');
```

Faz a ponte do modelo registrado para as GUCs de sessão (`theodb.llm_endpoint`/`theodb.llm_model`), de forma
que as funções `ai.*` passem a usá-lo quando `model` for omitido.

---

# 4. Remover um modelo do registry

```sql
SELECT theodb_ml.drop_model('theodb-text-lite');
```

Remove o modelo cadastrado.

---

# 5. Gerar texto usando modelo registrado

```sql
SELECT ai.generate(
    'What is TheoDB?',
    'theodb-text-lite'
);
```

Executa geração de texto utilizando um modelo previamente registrado (segundo argumento `model`).

---

# 6. Assinatura do `ai.if_costly`

```sql
FUNCTION ai.if_costly(
    condition TEXT,
    val TEXT,
    model TEXT DEFAULT NULL
)
RETURNS BOOLEAN;
```

Avalia uma condição em linguagem natural sobre um valor e retorna `TRUE` ou `FALSE`. (Formas disponíveis:
`ai.if(prompt)` — escalar de 1 argumento; `ai.if_costly(condition, value)` — escalar com COST alto para
push-down; `ai.if_batch(condition, values[])` — lote.)

---

# 7. Filtrar registros com `ai.if_costly`

```sql
SELECT name
FROM restaurant_reviews
WHERE ai.if_costly(
    'Is this a positive review?',
    review
);
```

Filtra registros utilizando conhecimento do modelo.

---

# 8. Filtrar usando modelo específico

```sql
SELECT name
FROM restaurant_reviews
WHERE ai.if_costly(
    'Is this a positive review?',
    location_city || ' ... ' || review,
    'theodb-text-lite'
);
```

Executa a mesma avaliação utilizando um modelo definido.

---

# 9. `GROUP BY` com `ai.if_costly`

```sql
SELECT
    name,
    location_city
FROM restaurant_reviews
WHERE ai.if_costly('Is this a positive review?', review)
GROUP BY
    name,
    location_city
HAVING COUNT(*) > 500;
```

Combina filtro inteligente com agregações SQL.

---

# 10. JOIN usando `ai.if_costly`

```sql
SELECT item_name,
       COUNT(*)
FROM menu_items
JOIN user_reviews
ON ai.if_costly(
    'Does this review mention the menu item?',
    user_reviews.review_text || ' item: ' || item_name
)
GROUP BY item_name;
```

Permite realizar joins semânticos.

---

# 11. Assinatura do `ai.if_batch`

```sql
FUNCTION ai.if_batch(
    condition TEXT,
    vals TEXT[],
    model TEXT DEFAULT NULL
)
RETURNS BOOLEAN[];
```

Avalia a mesma condição sobre um array de valores e retorna um array de booleanos — N valores em uma chamada.

---

# 12. Avaliar em lote com `ai.if_batch`

```sql
SELECT ai.if_batch(
    'Is this a positive review?',
    ARRAY_AGG(review),
    'theodb-text-lite'
)
FROM restaurant_reviews;
```

Executa várias avaliações em lote (N→1 round-trip HTTP).

---

# 13. Converter linhas em arrays

```sql
ARRAY_AGG(review)
```

Agrupa várias linhas para processamento em lote com `ai.if_batch` / `ai.generate_batch`.

---

# 14. Expandir resultado do array

```sql
SELECT UNNEST(
    ai.if_batch('Is this a positive review?', ARRAY_AGG(review))
)
FROM restaurant_reviews;
```

Transforma o array de resultados de volta em linhas SQL.

---

# 15. Assinatura do `ai.generate`

```sql
FUNCTION ai.generate(
    prompt TEXT,
    model TEXT DEFAULT NULL
)
RETURNS TEXT;
```

Gera texto baseado no prompt.

---

# 16. Gerar resumo

```sql
SELECT ai.generate(
    'Summarize in 20 words: ' || review
)
FROM user_reviews;
```

Resume textos individualmente.

---

# 17. Assinatura do `ai.generate_batch`

```sql
FUNCTION ai.generate_batch(
    prompts TEXT[],
    model TEXT DEFAULT NULL
)
RETURNS TEXT[];
```

Gera N respostas em UM round-trip HTTP (N-in/N-out) — evita o N+1 de chamadas por-linha.

---

# 18. Gerar múltiplos textos em lote

```sql
SELECT UNNEST(
    ai.generate_batch(
        ARRAY_AGG('Summarize in 20 words: ' || review)
    )
)
FROM user_reviews;
```

Gera múltiplos resumos em lote e expande o array de volta em linhas.

---

# 19. Assinatura do `ai.rank`

```sql
FUNCTION ai.rank(
    prompt TEXT,
    model TEXT DEFAULT NULL
)
RETURNS REAL;
```

Calcula um score baseado em linguagem natural.

---

# 20. Ordenar utilizando IA

```sql
SELECT review
FROM user_reviews
ORDER BY ai.rank(
    'Score this review: ' || review
) DESC
LIMIT 20;
```

Ordena resultados usando critérios definidos pelo prompt.

---

# 21. Assinatura do `ai.rerank`

```sql
FUNCTION ai.rerank(
    query TEXT,
    documents TEXT[],
    model TEXT DEFAULT NULL,
    top_n INT DEFAULT NULL
)
RETURNS TABLE(idx INT, score REAL);
```

Reordena um conjunto de documentos por relevância a uma consulta, retornando o índice original e o score.

---

# 22. Reordenar documentos

```sql
SELECT idx, score
FROM ai.rerank(
    'best pizza in town',
    ARRAY['great pasta', 'amazing pizza', 'good coffee'],
    top_n => 2
);
```

Retorna os `top_n` documentos mais relevantes.

---

# 23. Fluxo completo de filtragem inteligente

```sql
SELECT name
FROM restaurant_reviews
WHERE ai.if_costly(
    'Is this a positive review?',
    review,
    'theodb-text-lite'
);
```

Fluxo para filtrar registros semanticamente com `ai.if_costly`.

---

# 24. Fluxo completo de geração de texto

```sql
SELECT ai.generate(
    'Summarize the following review in 20 words: ' || review,
    'theodb-text-lite'
)
FROM user_reviews;
```

Fluxo para gerar resumos de texto utilizando um modelo configurável do TheoDB.

---

# 25. Fluxo completo de ranking inteligente

```sql
SELECT review
FROM user_reviews
ORDER BY ai.rank(
    'Score this review from 1 to 10 based on customer satisfaction: ' || review,
    'theodb-text-lite'
) DESC
LIMIT 20;
```

Fluxo para ranquear resultados utilizando critérios definidos em linguagem natural por um modelo configurável do TheoDB.

---

## 🎯 API-alvo / roadmap (não-shipped)

As formas abaixo (processamento **via cursor** para streaming de milhões de linhas) descrevem a superfície-alvo
estilo AlloyDB e **não estão implementadas** hoje. A superfície entregue é escalar + em lote (`ai.generate` /
`ai.generate_batch` / `ai.if_costly` / `ai.if_batch` / `ai.rank` / `ai.rerank`). Não use como código executável.

```sql
-- ROADMAP (não-shipped): processamento via cursor
summary_cursor := ai.generate('Summarize:', prompt_cursor);

score_cursor := ai.rank('Score this review:', prompt_cursor);

OPEN prompt_cursor FOR SELECT review FROM user_reviews;

LOOP
    FETCH score_cursor INTO rec;
    EXIT WHEN NOT FOUND;
    INSERT INTO scored_results VALUES (rec.input, rec.output);
END LOOP;

CLOSE score_cursor;
```

Categorias-alvo de funções AI (a categoria **Cursor-based** é roadmap; hoje só há Scalar + Array-based):

* **Scalar** — processa um único valor por chamada. ✅ entregue.
* **Array-based** — processa arrays inteiros em uma única chamada (N→1 round-trip). ✅ entregue.
* **Cursor-based** — processa grandes volumes via cursores e streaming. 🎯 roadmap.
