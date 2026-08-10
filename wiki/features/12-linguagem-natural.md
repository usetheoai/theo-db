---
type: Feature
title: Consultas em linguagem natural (ai.nl_to_sql e ai.nl_query)
description: Gera e executa SQL a partir de linguagem natural sob defesa em quatro camadas — statement único de leitura, denylist, allowlist verificada por EXPLAIN e sandbox read-only.
resource: git:f7c7b93:docs/features/12-linguagem-natural.md
tags: [feature, nl2sql, seguranca, prompt-injection, sandbox, sql]
feature_status: entregue (MVP seguro)
milestone: M19
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat12
    resource: git:f7c7b93:docs/features/12-linguagem-natural.md
    title: Consultas em linguagem natural
---

**Status: entregue** como MVP seguro. A guarda anti-prompt-injection **é o gate** desta feature — não um
detalhe de implementação.

# Assinaturas

```sql
ai.nl_to_sql(question TEXT, allowed_relations TEXT[], model TEXT DEFAULT NULL) RETURNS TEXT
ai.nl_query(question TEXT, allowed_relations TEXT[], model TEXT DEFAULT NULL,
            max_rows INT DEFAULT NULL) RETURNS JSONB
```

A primeira **gera e valida** o SQL; a segunda **gera, valida e executa** num sandbox somente-leitura,
devolvendo JSONB.

# A postura de segurança — quatro camadas, fail-closed

1. **Statement único, apenas `SELECT`/`WITH`.** Só uma instrução de leitura é aceita.
2. **Denylist de tokens** — palavras-chave de DDL, DML e funções administrativas são bloqueadas.
3. **Allowlist de relações verificada por `EXPLAIN`.** Apenas as relações declaradas podem ser
   referenciadas — e a verificação é feita **sobre o plano**, não por parsing de texto, que seria
   frágil e contornável.
4. **Sandbox read-only** com `statement_timeout`.

A camada 3 é a mais importante conceitualmente: validar pelo **plano** significa que o mecanismo não
depende de antecipar todas as formas sintáticas que um LLM adversarialmente induzido poderia produzir.

# Uso

```sql
-- só gerar o SQL
SELECT ai.nl_to_sql(
  'What is the population of the United States?',
  ARRAY['public.countries']
);

-- gerar, validar e executar
SELECT ai.nl_query(
  'List the 5 countries with the largest population',
  ARRAY['public.countries'],
  max_rows => 5
);
```

# Configuração, templates e índice de valores

```sql
SELECT ai.nl_add_config('my_app_cfg', ARRAY['public.countries']);   -- o ARRAY é obrigatório
SELECT ai.nl_add_template('my_app_cfg', 'Você traduz perguntas sobre países em SQL.');
SELECT ai.nl_set_template_enabled('my_app_cfg', TRUE);
SELECT ai.nl_set_value_index('my_app_cfg', 'public.countries', 'name', ARRAY['Brazil','China']);
SELECT ai.nl_refresh_value_index('my_app_cfg', 'public.countries', 'name');

-- atenção à ordem: a PERGUNTA vem primeiro, o config depois
SELECT ai.nl_query_cfg('Which country has the most people?', 'my_app_cfg');
```

# O que NÃO existe

Toda a superfície de extensão separada no estilo do [AlloyDB](/technologies/alloydb.md) — com funções de
criação de configuração, contexto de schema e associação de tipos conceituais — **não está
implementada**. Material de roadmap que a descreve não é código executável. A superfície entregue é a
acima, no schema `ai`.

# Ressalva honesta

**A qualidade da geração depende do modelo, e não há benchmark de acurácia NL→SQL publicado** — nem
contra suítes acadêmicas conhecidas. O que está provado é a **segurança** (as quatro camadas) e o
contrato, não a taxa de acerto.

# Relacionados

O restante da superfície em [funções de IA em SQL](/features/07-funcoes-ia-sql.md), e a decisão que
fixa a semântica síncrona por linha em
[ADR 0007](/decisions/0007-synchronous-per-row-model-http.md).
