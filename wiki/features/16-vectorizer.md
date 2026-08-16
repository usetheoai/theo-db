---
type: Feature
title: Vectorizer — embeddings declarativos automáticos
description: Declara-se a coluna de embedding e um background worker a mantém fresca; o gatilho de escrita é barato e toda a latência do modelo fica fora da transação de quem escreve.
resource: git:f7c7b93:docs/features/16-vectorizer.md
tags: [feature, vectorizer, background-worker, fila, crash-safe, ai-native]
feature_status: entregue (dogfood sustentado em aberto)
milestone: M54+M66+M104+M122+M132
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat16
    resource: git:f7c7b93:docs/features/16-vectorizer.md
    title: Vectorizer — embeddings declarativos automáticos
---

**Status: entregue** em código próprio. É a superfície **AI-native declarativa** do TheoDB: declara-se
uma coluna de embedding, e um **background worker** a mantém fresca conforme o conteúdo muda.

**A validação de uso sustentado em produção por 30 dias segue em aberto** — é o cenário-âncora do
dogfood, e o projeto é pré-1.0. A capacidade está entregue e tem comportamento medido
([m132](/benchmarks/m132-vectorizer-diagnosability.md)), mas **não há alegação de "production-ready"
aqui**.

# A propriedade que define o desenho

**O gatilho de enfileiramento é barato** — apenas um `INSERT` na fila, sem HTTP. Toda a latência do
modelo fica **no worker, fora da transação de quem escreve**. É por isso que escrever numa tabela com
vectorizer não fica refém da latência do endpoint.

# Configuração

```sql
CREATE EXTENSION IF NOT EXISTS theodb_rs CASCADE;

ALTER SYSTEM SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings';
ALTER SYSTEM SET theodb.embedding_api_key  = 'sk-...';
```

Depois declara-se o vectorizer com `theodb.create_vectorizer`, que aceita o modo de chunking opcional
descrito no [ADR 0025](/decisions/0025-m66-chunking-strategies.md).

# A fila é crash-safe

Os estados são `pending`, `processing` e `failed`, com **lease com fencing por posse** e **dead-letter**.
As propriedades que isso garante:

- um worker cuja lease expirou **não pode** marcar como concluído um job já re-reivindicado;
- um crash no meio do processamento leva a re-embed, com escrita idempotente por chave — o contrato
  **pelo menos uma vez**, decidido no [ADR 0049](/decisions/0049-m122-three-phase-async-embed.md);
- um job envenenado vai para dead-letter em vez de travar a fila.

# O worker roda in-process, e isso foi decisão

O [ADR 0016](/decisions/0016-m54-vectorizer-worker-mechanism.md) escolheu um BackgroundWorker
**in-process**, divergindo da SOTA — que usa processo externo — porque o modelo de deployment é
diferente: o TheoDB **já** exige carregar o seu `.so`, então o custo operacional já está pago, e
in-process entrega artefato único, crash-safety pelo postmaster e reuso direto do cliente HTTP.

# O detalhe de MVCC que quase passou

O embed roda **fora** de transação, em três fases, porque manter a transação aberta durante o HTTP
**prendia o `backend_xmin`** — e com ele o horizonte do autovacuum local — por toda a duração da
chamada ([ADR 0049](/decisions/0049-m122-three-phase-async-embed.md)). O modo com chunking ainda usa o
caminho de transação única, o que é desvantagem documentada.

# Observabilidade

`theodb.vectorizer_stats()` expõe o estado da fila, e há inspeção direta da fila e do dead-letter. A
diagnosticabilidade foi medida como milestone próprio, não assumida.

# Relacionados

A função de embedding por linha está em
[busca por similaridade](/features/01-busca-similaridade-vetorial.md); as estratégias de chunking, no
ADR 0025; e a razão de não haver cache, no
[ADR 0008](/decisions/0008-no-embedding-chat-cache.md).
