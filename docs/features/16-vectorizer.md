# Vectorizer — embeddings declarativos automáticos

> **✅ Entregue (M54 + M66 + M104 + M122 + M132).** O vectorizer é a superfície **AI-native declarativa**
> do TheoDB: você declara uma coluna de embedding e um **background worker** a mantém fresca conforme o
> conteúdo muda. A fila é **crash-safe** (`theodb.vectorizer_queue`, estados `pending`/`processing`/`failed`,
> lease com fencing por `owner`, dead-letter) — `theodb_rs/src/vectorizer.rs:42` (schema da fila),
> `theodb_rs/src/vectorizer.rs:105` (`theodb.create_vectorizer`, 10 args),
> `theodb_rs/src/vectorizer.rs:173` (`theodb.vectorizer_stats()`),
> `theodb_rs/src/vectorizer.rs:797` (worker `theodb_embed_worker_main`). Diagnosticabilidade medida em
> [`docs/benchmarks/m132-vectorizer-diagnosability.md`](../benchmarks/m132-vectorizer-diagnosability.md).

> **Status:** ✅ **Entregue (código own-code em Rust/plpgsql).** A validação **sustentada em produção real
> por ≥ 30 dias** é o **cenário-âncora do dogfood (M141)** — ainda **pré-1.0**
> (`.claude/rules/dogfood-golden-rule.md § 1`). Ou seja: a capacidade está entregue e é benchmarkada em
> comportamento (M132), mas a evidência de uso sustentado em produção é trabalho em aberto — não há claim de
> "production-ready" aqui (`.claude/rules/public-copy.md § 3`).

Esta página cobre como declarar um vectorizer, como o worker processa a fila crash-safe, o modo de chunking
opcional, e a observabilidade (stats, fila, dead-letter). O gatilho de enfileiramento é barato (só um `INSERT`
na fila, sem HTTP) — toda a latência do modelo fica no worker, fora da transação de quem escreve
(`theodb_rs/src/vectorizer.rs:73`).

---

# 1. Instalar a extensão `theodb`

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
```

Instala a extensão `theodb` (own-code) e sua base `theodb_rs` via `CASCADE` — o que provê o tipo `vector`, a
função `theodb.embed()`, o schema `theodb` e toda a superfície do vectorizer.

---

# 2. Configurar o endpoint de embeddings (operador)

```sql
ALTER SYSTEM SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings';
ALTER SYSTEM SET theodb.embedding_api_key  = 'sk-...';
ALTER SYSTEM SET theodb.embedding_model    = 'text-embedding-3-small';
SELECT pg_reload_conf();
```

O worker e `theodb.embed()` chamam este endpoint. `theodb.embedding_endpoint` e `theodb.embedding_api_key` são
**operator-only** (`GucContext::Suset`, valor oculto de não-superusers — `theodb_rs/src/am/guc.rs:475`), pois o
endpoint é o vetor de SSRF e a chave é credencial. `theodb.embedding_model` é **caller-settable**
(`theodb_rs/src/am/guc.rs:495`) — pode ser escolhido por sessão.

---

# 3. Habilitar o background worker

```sql
ALTER SYSTEM SET shared_preload_libraries = 'theodb_rs';
-- requer restart do servidor
```

O worker do vectorizer só é registrado quando a lib é carregada via `shared_preload_libraries` (no postmaster) —
o registro é guardado por `process_shared_preload_libraries_in_progress` (`theodb_rs/src/vectorizer.rs:667`).
Sem isso, a fila acumula jobs `pending` mas nada os drena.

> **Nota de honestidade (v1):** o worker é **um por servidor** e fixado ao banco `postgres`
> (`WORKER_DBNAME = "postgres"` — `theodb_rs/src/vectorizer.rs:662`). Um launcher multi-DB dirigido por GUC é
> follow-up. Ele processa em batches de 10 jobs, lease de 120s, até 5 tentativas por job
> (`theodb_rs/src/vectorizer.rs:657`).

---

# 4. Preparar a coluna de embedding (modo in-place)

```sql
ALTER TABLE articles ADD COLUMN body_embedding vector(1536);
```

No modo padrão (1 documento → 1 vetor, sem chunking) o embedding é gravado **na própria tabela**, na coluna
alvo. Ela precisa existir antes de declarar o vectorizer.

---

# 5. Declarar um vectorizer (forma mínima)

```sql
SELECT theodb.create_vectorizer(
    'articles',               -- source_table   regclass
    'id',                     -- source_pk_col  text
    'body',                   -- content_col    text
    'articles',               -- target_table   regclass
    'body_embedding',         -- target_col     text
    'text-embedding-3-small'  -- model          text
);
```

Registra o vectorizer e anexa um trigger `AFTER INSERT OR UPDATE OR DELETE` à tabela de origem
(`theodb_rs/src/vectorizer.rs:105`). Retorna o `id int` do vectorizer. A assinatura completa tem **10 argumentos**
— `model`, `dims`, `chunk_strategy` (todos `DEFAULT NULL`), `chunk_size` (`DEFAULT 512`) e `chunk_overlap`
(`DEFAULT 64`) são opcionais.

> **Segurança:** `theodb.create_vectorizer` tem `REVOKE ALL … FROM PUBLIC` (`theodb_rs/src/vectorizer.rs:160`) —
> não é chamável por qualquer role.

---

# 6. Assinatura completa de `theodb.create_vectorizer`

```sql
-- theodb_rs/src/vectorizer.rs:105
theodb.create_vectorizer(
    source_table   regclass,
    source_pk_col  text,
    content_col    text,
    target_table   regclass,
    target_col     text,
    model          text DEFAULT NULL,
    dims           int  DEFAULT NULL,
    chunk_strategy text DEFAULT NULL,
    chunk_size     int  DEFAULT 512,
    chunk_overlap  int  DEFAULT 64
) RETURNS int
```

`source_table`/`target_table` são `regclass` (validados na hora — uma tabela inexistente falha ali mesmo).
`source_pk_col` é a coluna de chave usada para casar origem↔alvo. `content_col` é o texto a embutir.

---

# 7. Como o gatilho enfileira

```sql
INSERT INTO articles (id, body) VALUES (1, 'running shoes review');
-- o trigger insere um job 'upsert' em theodb.vectorizer_queue (sem HTTP)
```

O trigger genérico (`theodb._vectorizer_enqueue`, `theodb_rs/src/vectorizer.rs:81`) faz só um `INSERT` barato na
fila: `upsert` em `INSERT`/`UPDATE`, `delete` em `DELETE`. Há **coalescing**: no máximo **um** job `pending` por
`(vectorizer_id, source_pk)` (índice único parcial — `theodb_rs/src/vectorizer.rs:65`), então um backfill em
massa ou updates repetidos da mesma linha não inundam o worker.

---

# 8. Como o worker processa a fila

```sql
-- inspecionar a profundidade da fila por estado
SELECT state, count(*)
FROM theodb.vectorizer_queue
GROUP BY state;
```

O worker roda em 3 fases por job: **claim** (transação própria — comita o lease), **process/embed** (embed em
sub-transação isolada, fora da txn de leitura desde o M122 para não fixar o `backend_xmin` durante o HTTP —
`theodb_rs/src/vectorizer.rs:963`), e **mark** (`mark_done`/`mark_failed`, guardado por `owner`). Cada transição é
protegida por um `owner` uuid único (fencing H1), então um worker lento cujo lease expirou não sobrescreve o novo
dono.

---

# 9. Estados da fila (`theodb.vectorizer_queue`)

```sql
-- theodb_rs/src/vectorizer.rs:42
-- state IN ('pending','processing','failed')  -- jobs 'done' são DELETEd
```

`pending` = a processar; `processing` = com lease ativo de um worker; `failed` = dead-letter (esgotou as
tentativas). Jobs concluídos são **removidos** (à la `pgmq.archive`), não retidos — a tabela fica enxuta.

---

# 10. Observabilidade — `theodb.vectorizer_stats()`

```sql
SELECT * FROM theodb.vectorizer_stats();
```

Retorna, numa linha (`theodb_rs/src/vectorizer.rs:173`):

```sql
-- RETURNS TABLE(processed bigint, failed bigint, last_run timestamptz,
--               pending bigint, processing bigint, failed_jobs bigint)
```

`processed`/`failed` são contadores cumulativos do worker; `last_run` é o último ciclo; `pending`/`processing`/
`failed_jobs` são a profundidade viva da fila por estado.

---

# 11. Inspecionar o dead-letter (por que um job falhou)

```sql
SELECT job_id, source_pk, attempts, last_error
FROM theodb.vectorizer_queue
WHERE state = 'failed'
ORDER BY job_id DESC;
```

Desde o M132, `last_error` carrega a **causa real** (SQLSTATE + mensagem), não mais um literal genérico —
foi exatamente essa cegueira que custou um dia de debug no #132
([`docs/benchmarks/m132-vectorizer-diagnosability.md`](../benchmarks/m132-vectorizer-diagnosability.md)). A
mensagem é **sanitizada no sink**: runs no formato `Bearer …`/`sk-…` são redigidas e o texto é truncado
(`theodb_rs/src/vectorizer.rs:728`), para nunca persistir uma credencial no dead-letter.

---

# 12. Limitar o dead-letter on-disk

```sql
ALTER SYSTEM SET theodb.vectorizer_dead_letter_max = 1000;
SELECT pg_reload_conf();
```

Mantém as `keep` linhas `failed` mais recentes e purga as mais antigas (`theodb_rs/src/am/guc.rs:438`, default
**1000**). Sem isso, uma linha-veneno persistente ou um endpoint mal configurado acumularia tombstones para sempre.
A manutenção periódica do worker aplica o corte (`_vectorizer_purge_dead_letters`,
`theodb_rs/src/vectorizer.rs:635`).

---

# 13. Consultar embeddings mantidos frescos

```sql
SELECT id, title
FROM articles
ORDER BY body_embedding <=> theodb.embed('comfortable running shoes', 'text-embedding-3-small')
LIMIT 10;
```

Como o worker mantém `body_embedding` sincronizado com `body`, a busca vetorial reflete o conteúdo atual sem
nenhum pipeline de embedding externo — é o AI-native declarativo: você declara, o banco mantém.

---

# 14. Modo de chunking (opt-in) — 1 documento → N chunks

```sql
SELECT theodb.create_vectorizer(
    source_table   => 'articles',
    source_pk_col  => 'id',
    content_col    => 'body',
    target_table   => 'articles',
    target_col     => 'body_embedding',
    model          => 'text-embedding-3-small',
    dims           => 1536,
    chunk_strategy => 'recursive',
    chunk_size     => 512,
    chunk_overlap  => 64
);
```

Com `chunk_strategy` não-nulo, cada documento é fatiado em N chunks e o vectorizer provisiona uma tabela irmã
`articles_chunks (source_pk, chunk_index, chunk_text, embedding vector(1536), PRIMARY KEY (source_pk, chunk_index))`
(`theodb_rs/src/vectorizer.rs:136`). As estratégias válidas são `fixed`, `sentence`, `recursive`
(`theodb_rs/src/vectorizer.rs:124`) — qualquer outra é rejeitada na hora (`RAISE EXCEPTION`, fail-fast). A config
é validada na borda: exige `chunk_size > 0` e `0 <= chunk_overlap < chunk_size`.

---

# 15. Consultar os chunks (modo chunking)

```sql
SELECT source_pk, chunk_index, chunk_text
FROM articles_chunks
ORDER BY embedding <=> theodb.embed('injury prevention', 'text-embedding-3-small')
LIMIT 10;
```

No modo chunking o embedding fica em `articles_chunks.embedding` — **não** na coluna alvo da tabela de origem.
Um re-embed é atômico: os chunks antigos do documento são apagados e os novos inseridos numa única passada
(`theodb_rs/src/vectorizer.rs:366`), sem órfãos.

---

# 16. Reprocessar em massa (backfill)

```sql
-- re-toca linhas existentes; o coalescing garante 1 job pending por PK
UPDATE articles SET body = body WHERE created_at < now() - interval '1 day';

SELECT pending FROM theodb.vectorizer_stats();
```

Um backfill re-enfileira via o trigger; graças ao índice único parcial (`theodb_rs/src/vectorizer.rs:65`), a
profundidade `pending` é limitada ao conjunto **distinto** de linhas alteradas — o worker único não é inundado
além do trabalho real.

---

# 17. Remover um vectorizer

```sql
-- descobrir o id e o trigger
SELECT id, source_table, target_table FROM theodb.vectorizer;

-- remover o trigger (nomeado por id) e a linha de config
DROP TRIGGER theodb_vectorizer_1 ON articles;
DELETE FROM theodb.vectorizer WHERE id = 1;
```

O trigger é nomeado `theodb_vectorizer_{id}` (`theodb_rs/src/vectorizer.rs:143`). Remover o trigger para o
enfileiramento; apagar a linha de `theodb.vectorizer` remove a config (a `FOREIGN KEY … ON DELETE CASCADE` limpa os
jobs pendentes daquele vectorizer — `theodb_rs/src/vectorizer.rs:44`).

---

# 18. Fluxo completo recomendado

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

ALTER SYSTEM SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings';
ALTER SYSTEM SET theodb.embedding_api_key  = 'sk-...';
ALTER SYSTEM SET shared_preload_libraries  = 'theodb_rs';
SELECT pg_reload_conf();
-- restart do servidor para carregar o worker

ALTER TABLE articles ADD COLUMN body_embedding vector(1536);

SELECT theodb.create_vectorizer(
    'articles', 'id', 'body', 'articles', 'body_embedding', 'text-embedding-3-small'
);

INSERT INTO articles (id, body) VALUES (1, 'running shoes review');

-- observar o worker drenar a fila
SELECT * FROM theodb.vectorizer_stats();

SELECT id, title
FROM articles
ORDER BY body_embedding <=> theodb.embed('comfortable shoes', 'text-embedding-3-small')
LIMIT 5;
```

Fluxo completo:

1. instala a extensão `theodb`;
2. configura o endpoint de embeddings + habilita o worker (`shared_preload_libraries` + restart);
3. cria a coluna alvo e declara o vectorizer;
4. escreve conteúdo (o trigger enfileira, o worker embute);
5. consulta os embeddings mantidos frescos.

> **Lembrete de dogfood (M141):** a prova de uso **sustentado em produção** deste fluxo é o cenário-âncora
> pré-1.0 — a capacidade está entregue e medida em comportamento, mas o claim de produção depende do M141
> (`.claude/rules/dogfood-golden-rule.md § 1`).
