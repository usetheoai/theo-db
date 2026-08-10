---
type: Guide
title: Unificação — um sistema contra dois
description: Compara simplicidade operacional e consistência de dados entre fazer a busca filtrada e aumentada por IA numa SQL transacional ou colando dois sistemas na aplicação. Não é comparação de velocidade.
resource: git:f7c7b93:docs/unification-1-vs-2-systems.md
tags: [guia, unificacao, posicionamento, consistencia, etl, honestidade]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: unif
    resource: git:f7c7b93:docs/unification-1-vs-2-systems.md
    title: Unification — one system vs two
---

**Esta não é uma comparação de velocidade.** A performance vetorial do TheoDB é competitiva, não um
número de marketing — o que se compara aqui é **simplicidade operacional e consistência de dados**,
conforme o [ADR 0005](/decisions/0005-unification-as-differentiator.md).

# A tarefa

*"Retornar os 5 produtos mais similares, **em estoque**, da categoria 3, com um resumo por IA de cada
um."*

# Um sistema, uma transação

```sql
SELECT p.id, ai.summarize(p.description) AS gist
FROM products p
JOIN inventory i ON i.product_id = p.id
WHERE i.in_stock AND p.category_id = 3
ORDER BY p.embedding <=> '[0.1,0.2,...]'::vector
LIMIT 5;
```

Uma instrução. O vetor, o estado relacional de estoque e categoria, e a chamada de IA estão na **mesma
transação** sobre as **mesmas linhas** — sem janela de staleness e sem job de sincronização.

# Dois sistemas

```python
# 1. consultar o vector DB — só vetor, só filtro de metadado
res = pinecone_index.query(vector=q, top_k=50, filter={"category_id": 3})
ids = [m.id for m in res.matches]

# 2. buscar o estado relacional autoritativo — está REALMENTE em estoque AGORA?
rows = pg.execute("SELECT id, description FROM products p JOIN inventory i ON i.product_id=p.id "
                  "WHERE p.id = ANY(%s) AND i.in_stock", (ids,))

# 3. re-rankear e re-aplicar o top_k na aplicação, porque o filtro relacional derrubou parte
final = merge_and_take(rows, ids, k=5)

# 4. chamar o LLM por linha, na aplicação
summaries = [llm.summarize(r.description) for r in final]

# ...mais um job de ETL mantendo os vetores em dia com as escritas do Postgres
```

# O placar honesto

| Dimensão | 1 sistema | Vector DB + Postgres |
|---|---|---|
| Sistemas a operar | **1** | 2, mais um pipeline de sync |
| Peças móveis nesta query | 1 instrução SQL | 2 queries + merge na app + LLM por linha |
| Consistência vetor ↔ relacional | **transacional, staleness zero** | eventual — o vector DB pode estar atrás |
| Correção do filtro | `WHERE` relacional **no mesmo plano** | filtro de metadado num sistema, refiltrado na app; o `top_k` pode devolver menos após o filtro relacional |
| Dados a sincronizar | **nenhum** — são as mesmas linhas | embeddings duplicados, ETL obrigatório |

# A leitura

O moat é **menos sistemas, sem ETL e consistência transacional** — **não** uma alegação de velocidade.

Vale notar que o ganho estrutural também foi **medido**, e com honestidade sobre seu tamanho: em
[m64](/benchmarks/m64-rag-over-sql.md), a query unificada economiza um round-trip (1 contra 2), com
ganho de latência **modesto quando co-localizado (~8%)** e que amplifica sobre rede real. E o benchmark
recebeu uma correção deliberada — dar chave primária à tabela — justamente para **não** inflar o braço
rival com um espantalho.

# Ressalva de drift

O documento de origem sugere `SET hnsw.iterative_scan` para preservar recall sob filtro seletivo. Essa
GUC vinha de uma extensão **removida** ([ADR 0029](/decisions/0029-m70-drop-pgvector.md)); o mecanismo
atual é o **filtro inline** do [ADR 0040](/decisions/0040-m90-inline-label-filter-verdict.md).
