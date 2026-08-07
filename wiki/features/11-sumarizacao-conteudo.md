---
type: Feature
title: Sumarização de conteúdo (ai.summarize e ai.agg_summarize)
description: Uma função escalar que resume um texto e um agregado que colapsa várias linhas num único resumo, com tratamento explícito de linhas nulas e vazias.
resource: git:f7c7b93:docs/features/11-sumarizacao-conteudo.md
tags: [feature, ai-surface, sumarizacao, agregado, llm]
feature_status: entregue
milestone: M10+M18
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat11
    resource: git:f7c7b93:docs/features/11-sumarizacao-conteudo.md
    title: Sumarização de conteúdo
---

**Status: entregue**, em duas formas.

```sql
ai.summarize(content TEXT, model TEXT DEFAULT NULL) RETURNS TEXT
AGGREGATE ai.agg_summarize(TEXT) RETURNS TEXT
```

A primeira resume **um** texto por chamada; a segunda é um **agregado** que colapsa várias linhas num
**único** resumo — a diferença importa, porque a segunda não é a primeira aplicada linha a linha.

# Uso

```sql
-- um resumo por linha
SELECT id, ai.summarize(article_body) FROM articles;

-- um resumo de todas as linhas do grupo
SELECT category, ai.agg_summarize(article_body)
FROM articles
GROUP BY category;
```

# Comportamento de borda do agregado

Explicitamente testado, e é o que distingue um agregado bem-feito de um mal-feito:

- entrada **vazia ou toda nula** resulta em `NULL`, não em erro nem em string vazia;
- linhas **nulas ou vazias são puladas**, e não contribuem ruído ao resumo;
- uma completion vazia vinda do modelo **propaga como erro tipado**, em vez de virar um resumo em
  branco silencioso.

# Ressalvas

A **qualidade do resumo depende do modelo configurado, e não há benchmark de qualidade publicado** — a
validação é de contrato contra o container, não de qualidade de saída.

E o material de roadmap que descreve versionamento de modelo e flags de preview no estilo do
[AlloyDB](/technologies/alloydb.md) **não corresponde à superfície entregue**, que é a descrita acima,
no schema `ai`.

# Custo

Como todas as funções da superfície, cada chamada é um round-trip bloqueante
([ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)). Para volume, use o batching de
[acelerar consultas](/features/08-acelerar-consultas.md).
