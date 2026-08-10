---
type: Guide
title: Funções generativas em SQL — contratos, garantias e segurança
description: O documento operacional da superfície ai.*, com os erros tipados que cada função emite, a postura de segurança e os limites declarados de cada forma.
resource: git:f7c7b93:docs/sql-ai-functions.md
tags: [guia, ai-surface, seguranca, ssrf, erro-tipado, agregado, batch]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: sqlai
    resource: git:f7c7b93:docs/sql-ai-functions.md
    title: Generative-AI SQL functions (ai.*)
---

O documento **operacional** da superfície `ai.*` — o de features descreve o que ela faz; este descreve
**como ela se comporta quando algo dá errado**.

**O banco não embarca modelo.** Ele faz uma chamada HTTP do lado do servidor a um endpoint
chat-completions configurável, exatamente como faz para [embeddings](/guides/sql-embeddings.md).

# Configuração

```sql
SET theodb.llm_endpoint = 'https://api.openai.com/v1/chat/completions';  -- obrigatório
SET theodb.llm_model    = 'gpt-4o-mini';                                  -- default opcional
SET theodb.llm_api_key  = 'sk-...';                                       -- bearer opcional
```

O endpoint pode ser um **modelo local self-hosted** ou um provedor em nuvem — essa portabilidade é a
alavanca contra lock-in de IA gerenciada.

# Comportamento e garantias

**Fail-fast e tipado**, que é o contrato mais importante desta superfície:

| Situação | SQLSTATE |
|---|---|
| Endpoint não configurado | `22023` |
| Endpoint não-`http(s)` (guarda de SSRF) | `22023` |
| Conexão recusada, timeout, 5xx | `38000` |
| Modelo ignorou o formato — booleano não booleano, score sem número, rótulo fora do conjunto | `22023` |

**Nunca um valor errado silencioso.** Cada função envia um prompt de sistema que restringe a saída para
que ela parseie deterministicamente, e **o parser é a última linha de defesa** se o modelo se comportar
mal.

Todas as funções escalares são invólucros finos sobre um único helper privado — **uma fonte de verdade
de HTTP**.

# Segurança

**Least-privilege.** O helper e todas as funções públicas são **revogados de PUBLIC**, porque fazem saída
HTTP do lado do servidor. Concessão é explícita, por papel.

**Endurecimento contra SSRF.** Apenas `http(s)://` é aceito e **redirects estão desabilitados**, de modo
que uma GUC definida numa sessão **não consegue** fazer o servidor buscar URLs internas ou de metadata de
nuvem.

**Tratamento da chave — a ressalva que o operador precisa saber.** A chave é uma **GUC de sessão**,
portanto **visível a `SHOW` e capturável por `log_statement`**. Defina-a por sessão, fora de banda, nunca
em DDL logada. Ela **nunca é ecoada em mensagem de erro**.

# Agregado — o comportamento que surpreende

`ai.agg_summarize(text)` colapsa muitas linhas num resumo só, com **uma** chamada por grupo.

```sql
SELECT service, ai.agg_summarize(content) AS digest
FROM incidents GROUP BY service;
```

Quatro propriedades que mudam o uso:

- Linhas nulas e vazias são **puladas**; grupo vazio ou todo nulo devolve `NULL`, **sem chamar o modelo**.
- **A ordem das linhas é indeterminada** — um agregado comum não tem ordem definida —, então o resumo
  **não é reproduzível** entre execuções, a menos que se fixe com `ai.agg_summarize(content ORDER BY id)`.
- O prompt acumulado é **limitado a 12000 caracteres**, por custo e por tokens; grupos muito grandes são
  **truncados**. Limitação documentada.
- Nenhum agregado do PostgreSQL pode ser `VOLATILE`, então a chamada não determinística vive na função
  final volátil, que o executor reexecuta por query — **resultados nunca são cacheados**.

**Privilégio:** como as funções rodam como invocador e chamam o helper, um papel precisa de `EXECUTE`
**no helper além** do agregado.

# Limitações declaradas

**Síncrono** — uma chamada bloqueante por linha; para volume, use as formas em lote de
[acelerar consultas](/features/08-acelerar-consultas.md). O racional e as consequências de escala estão
no [ADR 0007](/decisions/0007-synchronous-per-row-model-http.md).

**A qualidade e o custo dependem do endpoint** para o qual você apontar.

# Teste

**Offline**, no CI: um stub determinístico compatível com a API serve de endpoint, então o contrato
SQL→HTTP→parse é testado **com zero chamadas externas**. **Real**, opcional: roda contra um provedor de
verdade quando as variáveis estão configuradas, asserindo **polaridade e forma, nunca texto exato** — a
única forma honesta de testar saída não determinística.

# Nota de drift

O documento de origem lista `ai.if` como predicado escalar. A superfície evoluiu no
[ADR 0043](/decisions/0043-m102-ai-operators-batched-pushdown.md) para o par `ai.if_batch` (lote, um
round-trip) e `ai.if_costly` (por linha, com custo declarado para o planner ordenar quais) — que é o que
está em [funções de IA em SQL](/features/07-funcoes-ia-sql.md).
