---
type: Feature
title: Funções de IA em SQL (superfície ai.*)
description: Geração, predicados, sentimento, sumarização e rerank chamáveis de SQL sobre um endpoint configurável, mais um registry de modelos que não persiste credenciais.
resource: git:f7c7b93:docs/features/07-funcoes-ia-sql.md
tags: [feature, ai-surface, llm, sql, model-agnostic, seguranca]
feature_status: entregue (núcleo escalar + registry)
milestone: M7-S3+M10+M11+M13
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat07
    resource: git:f7c7b93:docs/features/07-funcoes-ia-sql.md
    title: Consultas SQL inteligentes com funções de IA
---

**Status: entregue** no núcleo escalar mais o registry. Todas as funções são **revogadas de PUBLIC** e
operam sobre um endpoint chat-completions **configurável**, o que é a propriedade de
**independência de modelo** que o [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md)
lista como superioridade estrutural.

# A superfície

| Função | O que faz |
|---|---|
| `ai.generate(prompt)` | geração de texto por linha |
| `ai.generate_batch(prompts[])` | N prompts, **1** round-trip |
| `ai.if_batch(condition, vals[])` | predicado booleano em lote — 1 round-trip |
| `ai.if_costly(condition, val)` | predicado por linha, com `COST` alto para o planner ordenar quais |
| `ai.analyze_sentiment(text)` | ver [análise de sentimento](/features/10-analise-sentimento.md) |
| `ai.summarize(text)` / `ai.agg_summarize(...)` | ver [sumarização](/features/11-sumarizacao-conteudo.md) |
| `ai.rank(...)` | scoring por LLM, uma chamada por item |
| `ai.rerank(query, docs[])` | cross-encoder em lote — ver [ranquear](/features/09-ranquear-resultados.md) |

**`ai.rank` e `ai.rerank` são coisas diferentes**, e o nome divergiu do AlloyDB de propósito para não
colidir: o primeiro é julgamento por LLM item a item; o segundo é um cross-encoder em lote
([ADR 0024](/decisions/0024-m65-ai-rerank-cross-encoder.md)).

# Predicados em SQL — o padrão que importa

O par de predicados é a entrega do [ADR 0043](/decisions/0043-m102-ai-operators-batched-pushdown.md):

```sql
-- o planner avalia o qual barato primeiro e só chama a IA nos sobreviventes
SELECT * FROM tickets
WHERE created_at > now() - interval '7 days'
  AND ai.if_costly('este ticket descreve uma falha de pagamento', body);
```

O `COST 100000` declarado faz a própria ordenação de quais do PostgreSQL cuidar do push-down — medido
em ~12× menos latência que o caminho por linha.

`ai.call_count()` e `ai.call_reset()` expõem a contagem de round-trips, o que permite **provar** em
tempo de query que N linhas custaram uma chamada.

# Registry de modelos

```sql
SELECT theodb_ml.create_model('theodb-text-lite', '<endpoint>', 'theodb-text-lite');
SELECT theodb_ml.apply_model('theodb-text-lite');   -- vira o default da sessão
SELECT theodb_ml.list_models();
SELECT theodb_ml.drop_model('theodb-text-lite');
```

`theodb_ml` é um **schema com registry**, não uma extensão; as funções vivem no schema `ai`.

**Divergência honesta declarada:** o registry **não persiste credenciais** — não há coluna de chave de
API. As chaves permanecem como GUC de sessão, e aplicar um modelo faz a ponte por GUC, em vez do
estilo de credencial por chamada do [AlloyDB](/technologies/alloydb.md).

# Segurança

Toda a superfície faz saída HTTP e recebe valores não confiáveis que viram entrada de prompt — uma
superfície de **prompt injection inerente**. Os controles reais, registrados no ADR 0043:

- endpoint restrito a `http(s)`, **sem seguir redirects** (para não alcançar metadata interno),
  timeout, e erro tipado;
- `REVOKE ALL FROM PUBLIC`, com comentário explícito de nunca conceder a papel isolado;
- raio de dano limitado ao valor da própria linha — uma resposta envenenada vira `NULL`, nunca
  escalação.

**Não existe quoting à prova de injeção para prompt de texto livre.** O controle honesto é
least-privilege, não escape.

# O que NÃO está implementado

Os modos **baseados em cursor** descritos em material de roadmap **não existem** — follow-up
deliberado. E não há cache de resultado, por decisão explícita no
[ADR 0008](/decisions/0008-no-embedding-chat-cache.md).

# Custo e escala

Cada chamada por linha é um round-trip bloqueante que segura um backend
([ADR 0007](/decisions/0007-synchronous-per-row-model-http.md)), então **`max_connections` — não CPU
nem RAM — é a primeira parede** sob fan-out. Use as variantes em lote para operações em massa.
