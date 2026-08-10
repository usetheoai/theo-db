---
type: Feature
title: Análise de sentimento (ai.analyze_sentiment)
description: Classifica texto num rótulo de sentimento via LLM, com erro tipado em saída malformada; a acurácia depende do modelo configurado e não há benchmark publicado.
resource: git:f7c7b93:docs/features/10-analise-sentimento.md
tags: [feature, ai-surface, sentimento, llm]
feature_status: entregue
milestone: M7-S3
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat10
    resource: git:f7c7b93:docs/features/10-analise-sentimento.md
    title: Análise de sentimento de texto
---

**Status: entregue.**

```sql
ai.analyze_sentiment(content TEXT, model TEXT DEFAULT NULL) RETURNS TEXT
```

Classifica o texto num rótulo — `positive`, `negative` ou `neutral` — via LLM, com **erro tipado** em
saída malformada, em vez de devolver lixo silenciosamente. Não requer `CREATE EXTENSION` nem flag de
preview: as funções `ai.*` vivem no schema `ai`.

# Uso

```sql
SELECT id, ai.analyze_sentiment(review_content) AS sentimento
FROM reviews;
```

O modelo vem das GUCs de sessão. Registrar um modelo no registry é **opcional**:

```sql
SELECT theodb_ml.create_model('theodb-text-lite', '<endpoint>', 'theodb-text-lite');
SELECT theodb_ml.apply_model('theodb-text-lite');
```

# Custo em massa

Cada linha é um round-trip bloqueante. Para classificar uma coluna inteira, o caminho correto **não** é
esta função: é o predicado em lote de [acelerar consultas](/features/08-acelerar-consultas.md), que
colapsa N chamadas em uma. Ver também a decisão que fixa a semântica por linha em
[ADR 0007](/decisions/0007-synchronous-per-row-model-http.md).

# Ressalva honesta

**A acurácia depende do modelo configurado, e não há benchmark de acurácia de sentimento publicado.** A
validação existente é de **contrato** — que o rótulo pertence ao conjunto esperado e que saída
malformada vira erro tipado —, não de qualidade.

# Relacionados

Outras funções da mesma superfície em [funções de IA em SQL](/features/07-funcoes-ia-sql.md), e as
implicações de segurança de passar texto não confiável a um LLM estão descritas lá.
