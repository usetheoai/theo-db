---
type: Guide
title: Embeddings a partir do SQL (theodb.embed)
description: Gera vetores direto do SQL chamando um endpoint configurável — o banco não embarca modelo, o que mantém a imagem enxuta e o modelo trocável.
resource: git:f7c7b93:docs/sql-embeddings.md
tags: [guia, embeddings, model-agnostic, guc, sql]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: sqlemb
    resource: git:f7c7b93:docs/sql-embeddings.md
    title: SQL embeddings — theodb.embed()
---

**O banco chama um endpoint de modelo; ele não embarca um modelo.** Essa é a escolha de desenho central,
e ela é o que mantém a imagem enxuta — sem torch nem ONNX dentro do Postgres — e o modelo **totalmente
trocável**.

# Contrato

```sql
theodb.embed(content text, model text DEFAULT NULL) RETURNS vector
```

**Conteúdo primeiro, modelo depois** — a ordem inversa é o erro mais comum.

| GUC | Obrigatória | Significado |
|---|---|---|
| `theodb.embedding_endpoint` | **sim** | URL compatível com `/v1/embeddings` |
| `theodb.embedding_model` | não | modelo default quando a chamada omite |
| `theodb.embedding_api_key` | não | enviada como `Authorization: Bearer …` |

**Endpoint não configurado produz erro tipado fail-fast, nunca um `NULL` silencioso** — a disciplina de
error handling do projeto aplicada na borda.

# Duas formas de prover o modelo

**Modelo local self-hosted**, sem nuvem e sem credencial:

```bash
python benchmarks/servers/embedding_server.py --host 0.0.0.0 --port 8088 \
  --model BAAI/bge-small-en-v1.5
```

```sql
SET theodb.embedding_endpoint = 'http://host.docker.internal:8088/v1/embeddings';
SELECT theodb.embed('the cat sat on the mat');   -- vector(384)

SELECT id FROM docs
ORDER BY embedding <=> theodb.embed('a feline on a rug')
LIMIT 5;
```

**Provedor em nuvem**, apontando a mesma GUC para qualquer API compatível:

```sql
SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings';
SET theodb.embedding_api_key  = '…';
SELECT theodb.embed('hello', 'text-embedding-3-small');   -- vector(1536)
```

# Validação contra provedores reais

Ambos os caminhos são exercitados de verdade, **sem mock**: o modelo local de 384 dimensões serve de
oráculo nos testes de integração, e o provedor em nuvem retorna 1536 dimensões com semântica genuína —
paráfrase medindo muito mais próxima que texto não relacionado. A imagem embarca `ca-certificates` para
que a verificação de TLS funcione com provedores HTTPS.

# Notas honestas

O servidor local usa um modelo real, em ONNX, **sem GPU e sem torch** — é opção de dependência zero,
não um stub.

**A chamada é síncrona dentro do backend**, o mesmo padrão do [AlloyDB](/technologies/alloydb.md) — e a
decisão que fixa isso, com suas consequências de escala, é o
[ADR 0007](/decisions/0007-synchronous-per-row-model-http.md). Para embedar tabelas grandes, use
`theodb.embed_batch` ou, melhor, o [vectorizer](/features/16-vectorizer.md), que tira a latência do
modelo da transação de quem escreve.

# Relacionados

O uso em consultas está em [busca por similaridade](/features/01-busca-similaridade-vetorial.md); as
estratégias de fatiamento de texto, no [ADR 0025](/decisions/0025-m66-chunking-strategies.md); e a razão
de não haver cache de embedding, no [ADR 0008](/decisions/0008-no-embedding-chat-cache.md).
