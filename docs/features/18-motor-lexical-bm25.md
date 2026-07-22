# Busca lexical BM25 (motor own-code)

> **✅ Entregue como núcleo medido, ⚠️ NÃO no binário default:** o motor BM25 own-code (`bm25_build` /
> `bm25_search`, sobre o crate pgrx-free `theodb_lexical` + Tantivy 0.26, MIT) existe e foi **medido**
> (`theodb_rs/src/lexical/engine.rs:110` e `:156`), mas é compilado **apenas** com `--features spike-lexical`
> (`theodb_rs/src/lib.rs:57`; `default = ["pg18"]` em `theodb_rs/Cargo.toml:23`) — **não** está na extensão
> `theodb` default. O default lexical da busca híbrida **continua `ts_rank_cd`** (`theodb_rs/src/api.rs:723`).
> Veredito medido: a engine own-code bate `ts_rank_cd` em lexical puro por margem **modesta e
> contexto-dependente** ([`docs/benchmarks/m140-1-lexical-measurement.md`](../benchmarks/m140-1-lexical-measurement.md)),
> e fica **~4% abaixo** do `pg_textsearch` em nDCG@10 num regime
> ([`docs/benchmarks/m140-3-bm25-engine.md`](../benchmarks/m140-3-bm25-engine.md)); na fusão RRF **não há ganho**
> mensurável e no corpus lexical-heavy a troca mede **pior**
> ([`docs/benchmarks/m138-bm25-fusion.md`](../benchmarks/m138-bm25-fusion.md)). Gate original de adoção:
> [`docs/benchmarks/m7-bm25-vs-tsrank.md`](../benchmarks/m7-bm25-vs-tsrank.md). Provado pelos `#[pg_test]`
> `test_bm25_build_indexes_and_bumps_generation`, `test_bm25_search_returns_matching_id` e
> `test_bm25_search_sees_new_generation_after_rebuild` (`theodb_rs/src/lexical/engine.rs`).

Esta página cobre o motor de busca lexical **BM25 own-code** do TheoDB: como construir e consultar o índice
Tantivy persistido no heap do PostgreSQL, e como (e quando) a busca híbrida usa a perna lexical BM25. A leitura
mais importante é o **trade-off honesto**: o BM25 é melhor em lexical puro por uma margem modesta, mas a fusão
RRF lava essa diferença — por isso o default embarcado permanece `ts_rank_cd`. Nenhuma afirmação de performance
aqui é sem um artefato medido em `docs/benchmarks/`.

---

# 1. Instalar a extensão `theodb`

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
```

Instala a extensão `theodb` (own-code), que provê a busca híbrida `ai.hybrid_search_rrf` (perna lexical default
`ts_rank_cd`). As funções `bm25_build`/`bm25_search` só aparecem se a extensão foi compilada com
`--features spike-lexical` (ver seção 3).

---

# 2. Busca híbrida com a perna lexical default (`ts_rank_cd`)

```sql
SELECT id, score
FROM ai.hybrid_search_rrf(
    'documents', 'id', 'content_tsv', 'embedding',
    query_text => 'wireless noise cancelling headphones'
);
```

`ai.hybrid_search_rrf(tbl regclass, id_col text, content_tsv_col text, vector_col text, ...)` funde a perna
lexical (`ts_rank_cd` sobre a coluna `tsvector`) e a perna vetorial (`<=>`) via Reciprocal Rank Fusion. O default
`lexical_engine => 'ts_rank_cd'` é o motor lexical embarcado. Assinatura verificada em `theodb_rs/src/api.rs:711`.

---

# 3. Compilar o motor BM25 own-code (feature `spike-lexical`)

> ⚠️ **API-alvo / não-shipped no binário default.** As funções `bm25_build`/`bm25_search` são compiladas
> **apenas** sob a feature `spike-lexical`.

```bash
cargo pgrx install --features spike-lexical
```

O motor BM25 own-code vive atrás da feature `spike-lexical` (`theodb_rs/Cargo.toml:32`, gate em
`theodb_rs/src/lib.rs:57`). O binário default (`default = ["pg18"]`) **não** o inclui — é o núcleo de retrieval
lexical do consumidor `theo-lens`, não uma superfície SQL de produção da extensão default.

---

# 4. Construir um índice BM25 sobre uma tabela (`bm25_build`)

> ⚠️ **API-alvo / não-shipped no binário default** (requer `--features spike-lexical`, ver seção 3).

```sql
SELECT bm25_build(1, 'documents', 'id', 'body');
```

`bm25_build(index_id bigint, table text, id_col text, text_col text) -> bigint` indexa
`SELECT id_col, text_col FROM table` no Tantivy, faz flush ao heap `theodb.lexical_files` (drop+reinsere atômico
na mesma txn) e bumpa a geração; retorna o nº de documentos indexados. Verificado em
`theodb_rs/src/lexical/engine.rs:110`. Rodar de novo com o mesmo `index_id` substitui o índice.

---

# 5. Buscar no índice BM25 (`bm25_search`)

> ⚠️ **API-alvo / não-shipped no binário default** (requer `--features spike-lexical`, ver seção 3).

```sql
SELECT id, score FROM bm25_search(1, 'wireless headphones', 10);
```

`bm25_search(index_id bigint, query text, k int) -> TABLE(id bigint, score float8)` busca BM25 sobre o índice,
usando o cache do Directory (rebuild só se a geração visível sob o snapshot mudou — MVCC-correto), e retorna
`(id, score)` ordenado por score desc, top-`k`. Verificado em `theodb_rs/src/lexical/engine.rs:156`.

---

# 6. `k` deve ser positivo (fail-fast)

> ⚠️ **API-alvo / não-shipped no binário default** (requer `--features spike-lexical`, ver seção 3).

```sql
SELECT id, score FROM bm25_search(1, 'headphones', 0);   -- ERRO: k must be > 0
```

`bm25_search` valida `k <= 0` na fronteira e falha com erro tipado (`theodb_rs/src/lexical/engine.rs:161`) — sem
valor mágico, sem retorno silencioso de zero linhas para um `k` inválido.

---

# 7. Query vazia retorna zero linhas (não erro)

> ⚠️ **API-alvo / não-shipped no binário default** (requer `--features spike-lexical`, ver seção 3).

```sql
SELECT count(*) FROM bm25_search(1, '   ', 10);   -- 0
```

Uma query só de espaços/operadores é sanitizada para vazia e retorna **zero linhas** — estado válido, não erro
(`theodb_rs/src/lexical/engine.rs:164-167`). Um índice sem `bm25_build` (geração 0) também retorna zero linhas.

---

# 8. Busca híbrida com a perna lexical BM25 (`lexical_engine => 'bm25'`)

> ⚠️ **API-alvo / não-shipped na imagem default.** Esta perna BM25 da híbrida usa a extensão externa
> `pg_textsearch` (índice `USING bm25` + operador `<@>`), **não** o motor own-code das seções 4–5, e o
> `pg_textsearch` **não está na imagem shipada** — a chamada abaixo falha com `0A000` até instalá-lo.

```sql
SELECT id, score
FROM ai.hybrid_search_rrf(
    'documents', 'id', 'content_tsv', 'embedding',
    query_text       => 'wireless headphones',
    lexical_engine   => 'bm25',
    content_text_col => 'body'
);
```

Com `lexical_engine => 'bm25'`, a perna lexical passa a usar `pg_textsearch` sobre a coluna TEXT indexada
`USING bm25` (`content_text_col` é obrigatória). O guard fail-fast exige a extensão `pg_textsearch` (checa
`pg_extension`) e, ausente na imagem shipada, surface um `0A000` claro em vez de um `42883` críptico —
`theodb_rs/src/hybrid.rs:160-183`.

---

# 9. BM25 sem `content_text_col` falha rápido (negativo)

> ⚠️ **API-alvo / não-shipped na imagem default** (requer `pg_textsearch`, ver seção 8).

```sql
SELECT id, score
FROM ai.hybrid_search_rrf(
    'documents', 'id', 'content_tsv', 'embedding',
    query_text     => 'headphones',
    lexical_engine => 'bm25'          -- ERRO 22023: requires content_text_col
);
```

A perna `bm25` sem `content_text_col` falha com erro tipado `22023` nomeando o argumento faltante
(`theodb_rs/src/hybrid.rs:164`) — nunca faz fallback silencioso para `ts_rank_cd` (um fallback silencioso deixaria
o caller medir `ts_rank_cd` acreditando estar medindo BM25). Provado por `hybrid_bm25_requires_content_text_col`.

---

# 10. `lexical_engine` inválido é rejeitado (negativo)

```sql
SELECT id, score
FROM ai.hybrid_search_rrf(
    'documents', 'id', 'content_tsv', 'embedding',
    query_text     => 'headphones',
    lexical_engine => 'okapi'         -- ERRO 22023: must be 'ts_rank_cd' or 'bm25'
);
```

Qualquer valor de `lexical_engine` fora de `{ts_rank_cd, bm25}` falha com `22023` nomeando o valor recebido
(`theodb_rs/src/hybrid.rs:181`). Provado por `hybrid_invalid_lexical_engine_errors`.

---

# 11. Fluxo completo recomendado (motor BM25 own-code)

> ⚠️ **API-alvo / não-shipped no binário default** (requer `--features spike-lexical`, ver seção 3).

```sql
-- (extensão instalada com --features spike-lexical)
SELECT bm25_build(1, 'documents', 'id', 'body');   -- indexa a tabela no Tantivy

SELECT d.id, d.title, s.score
FROM bm25_search(1, 'wireless noise cancelling', 10) AS s
JOIN documents d ON d.id = s.id
ORDER BY s.score DESC;
```

Fluxo completo do motor own-code:

1. instala a extensão com `--features spike-lexical`;
2. `bm25_build` indexa a tabela (id + corpo) no heap;
3. `bm25_search` retorna `(id, score)` top-`k`;
4. um `JOIN` recupera as colunas de interesse.

---

# Notas de honestidade (trade-off medido)

- **Lexical puro (o caso do `theo-lens`):** a engine BM25 own-code bate `ts_rank_cd` em dois eixos
  independentes — BEIR (scifact 0,661 vs 0,072; nfcorpus 0,308 vs 0,206) e logs HDFS known-item — mas a
  **magnitude é modesta e contexto-dependente** no regime justo (m=1: +13%), não um "5× universal"
  ([`docs/benchmarks/m140-1-lexical-measurement.md`](../benchmarks/m140-1-lexical-measurement.md)).
- **nDCG@10 in-PG:** a engine de produção own-code reproduz o M140.1 (0,6611 scifact) e fica **~4% abaixo** do
  `pg_textsearch` (0,688; atribuído a impl/tokenização, não significance-tested)
  ([`docs/benchmarks/m140-3-bm25-engine.md`](../benchmarks/m140-3-bm25-engine.md)).
- **Storage:** o índice Tantivy é **~3,5×** menor no footprint enxuto justo (M140.1) — direção robusta.
- **Fusão RRF (por que o default é `ts_rank_cd`):** apesar de a perna BM25 ser **9,8× mais forte isolada** em
  scifact, o RRF funde por rank e lava a diferença: a fusão com BM25 **não vence** com significância (p=0,51) e
  no NFCorpus lexical-heavy mede **significativamente pior** (p=0,0168). Trocar o default embarcado seria zero
  ganho mensurável + `shared_preload_libraries` + reinício + uma dependência hoje quebrada (issue #146). Por
  isso o default lexical **continua `ts_rank_cd`** — um honest-negative, não um fracasso
  ([`docs/benchmarks/m138-bm25-fusion.md`](../benchmarks/m138-bm25-fusion.md)).
