# Consultar um grafo nativo (travessia + GraphRAG)

> **✅ Entregue (M108 + M110 + M111 + M112):** o motor de grafo nativo do TheoDB é um **CSR persistido**
> (Compressed Sparse Row) serializado como `bytea` no catálogo `theodb.graph_csr` — WAL-logged, crash-safe e
> MVCC de graça pelo próprio PostgreSQL. As funções `theodb.graph_*` são **own-code Rust** (`#[pg_extern]` em
> `theodb_rs/src/graph.rs:334-493`, wrappers `theodb.*` em `theodb_rs/src/graph.rs:551-589`); a superfície de
> extração/GraphRAG vive em `theodb_rs/src/graph_extract.rs` e `theodb_rs/src/graph_rag.rs`. Todas compilam no
> binário **default** (`theodb_rs/src/lib.rs:51-54`, sem feature-gate). Benchmarks medidos em
> [`docs/benchmarks/m107-graph-spike.md`](../benchmarks/m107-graph-spike.md) (traversal CSR+BFS 106–738× vs
> recursive-CTE), [`docs/benchmarks/m108-persisted-csr.json`](../benchmarks/m108-persisted-csr.json),
> [`docs/benchmarks/m111-graphrag-flow.json`](../benchmarks/m111-graphrag-flow.json) e
> [`docs/benchmarks/fu1-samegraph-scan-microbench.md`](../benchmarks/fu1-samegraph-scan-microbench.md). Provado
> pelos testes `#[pg_test]` `m108_build_persists_and_expand_reads` (graph.rs), `m110_e2e_extract_to_expand`
> (graph_extract.rs), `m111_flow_structural_set` / `m111_flow_multihop_adds_recall` e `m112_eval_hotpot_llm_ppr`
> (graph_rag.rs).

Esta página cobre o motor de grafo nativo do TheoDB: como construir e refoldar o CSR persistido de uma tabela de
arestas, expandir vizinhanças em ≤H hops (single-source e batched multi-source), rodar Personalized PageRank e
compor o fluxo completo de **GraphRAG** (extração de entidades → grafo → travessia → reranking de chunks) sem
sair do SQL. O engine é medição-primeiro: o gate de traversal (`docs/benchmarks/m107-graph-spike.md`) foi
provado antes de qualquer código de produção.

---

# 1. Instalar a extensão `theodb`

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;
```

Instala a extensão `theodb` (own-code), que provê os catálogos `theodb.graph_csr` / `theodb.graph_nodes` /
`theodb.graph_edges` e todas as funções `theodb.graph_*`, `ai.extract_graph` e `ai.extract_entities`.

---

# 2. Preparar uma tabela de arestas

```sql
CREATE TABLE friendship (src bigint, dst bigint);
INSERT INTO friendship VALUES (0,1),(1,2),(2,3),(3,4),(0,2),(0,3);
```

O engine consome qualquer tabela com duas colunas `bigint` (origem/destino); as arestas são tratadas como
**não-direcionadas** na travessia.

---

# 3. Construir o CSR persistido (`graph_build`)

```sql
SELECT theodb.graph_build('friendship', 'src', 'dst');
```

`graph_build(edge_rel text, src_col text, dst_col text) -> bigint` monta o CSR **uma vez** e o persiste como
`bytea` em `theodb.graph_csr` (WAL-safe). Retorna a contagem de arestas. Assinatura verificada em
`theodb_rs/src/graph.rs:334` (wrapper `theodb.graph_build` em `theodb_rs/src/graph.rs:553`).

---

# 4. Refoldar após inserir novas arestas (`graph_refold`)

```sql
SELECT theodb.graph_refold('friendship');
```

`graph_refold(edge_rel text) -> bigint` reconstrói o CSR persistido a partir das arestas atuais (fold-on-demand),
reusando as colunas registradas no `graph_build` anterior. Falha com erro tipado se nenhum CSR foi construído
antes. Verificado em `theodb_rs/src/graph.rs:352` (wrapper `theodb_rs/src/graph.rs:555`).

---

# 5. Expandir a vizinhança em ≤H hops (`graph_expand`)

```sql
SELECT t.node FROM theodb.graph_expand('friendship', ARRAY[4]::bigint[], 1) AS t(node);
```

`graph_expand(edge_rel text, seeds bigint[], max_hops int) -> SETOF bigint` carrega o CSR persistido (sem
rebuild) e retorna o conjunto alcançável a partir de `seeds` dentro de `max_hops` (BFS de fronteira
não-direcionada). Verificado em `theodb_rs/src/graph.rs:409` (wrapper `theodb_rs/src/graph.rs:557`).

---

# 6. Contar o conjunto alcançável (`graph_expand_card`)

```sql
SELECT theodb.graph_expand_card('friendship', ARRAY[0]::bigint[], 2);
```

`graph_expand_card(edge_rel text, seeds bigint[], max_hops int) -> bigint` retorna apenas a **cardinalidade** do
conjunto alcançável (contagem calculada em Rust, uma linha de saída) — útil como sinal de alcance por entidade
sem materializar todos os nós. Verificado em `theodb_rs/src/graph.rs:423`.

---

# 7. Expansão batched multi-source (`graph_expand_multi`)

```sql
SELECT set_id, node
FROM theodb.graph_expand_multi(
    'friendship',
    ARRAY[1, 1, 2]::int[],       -- set_ids (lane de cada seed)
    ARRAY[0, 4, 2]::bigint[],    -- seeds (paralelo a set_ids)
    2
);
```

`graph_expand_multi(edge_rel text, set_ids int[], seeds bigint[], max_hops int) -> TABLE(set_id int, node
bigint)` faz Multi-Source BFS: `set_ids`/`seeds` são arrays **paralelos** (o seed `i` pertence à lane
`set_ids[i]`) — um único sweep avança até 64 lanes de uma vez. Verificado em `theodb_rs/src/graph.rs:457`.

---

# 8. Cardinalidade batched multi-source (`graph_expand_multi_card`)

```sql
SELECT set_id, card
FROM theodb.graph_expand_multi_card(
    'friendship',
    ARRAY[1, 2]::int[],
    ARRAY[0, 4]::bigint[],
    2
);
```

`graph_expand_multi_card(edge_rel text, set_ids int[], seeds bigint[], max_hops int) -> TABLE(set_id int, card
bigint)` retorna, por lane, apenas a cardinalidade alcançável — N linhas de saída (uma por lane), sem confundir
um benchmark de travessia com row-streaming. Verificado em `theodb_rs/src/graph.rs:478`.

---

# 9. Personalized PageRank a partir de sementes (`graph_ppr`)

```sql
SELECT node, score
FROM theodb.graph_ppr('friendship', ARRAY[0]::bigint[], 0.5, 20);
```

`graph_ppr(edge_rel text, seeds bigint[], damping float8 DEFAULT 0.5, iters int DEFAULT 20) -> TABLE(node
bigint, score float8)` roda Personalized PageRank a partir de `seeds` sobre o CSR persistido (ranking estilo
HippoRAG), retornando só nós com score > 0, maior primeiro. Verificado em `theodb_rs/src/graph.rs:433` (wrapper
com defaults em `theodb_rs/src/graph.rs:573`).

---

# 10. PPR com defaults (`damping`/`iters` omitidos)

```sql
SELECT node, score
FROM theodb.graph_ppr('friendship', ARRAY[0, 3]::bigint[]);
```

Os parâmetros `damping` (0.5) e `iters` (20) têm default no wrapper SQL (`theodb_rs/src/graph.rs:573`), então
podem ser omitidos. As sementes podem ser múltiplas (rank personalizado por um conjunto de entidades).

---

# 11. Extrair entidades de um texto (`ai.extract_entities`)

```sql
SELECT name, normalized_name, entity_type, mention_count
FROM ai.extract_entities('Alice met Bob in Paris. Alice works at Acme Corp.');
```

`ai.extract_entities(text text, use_llm boolean DEFAULT false, model text DEFAULT NULL) -> TABLE(name text,
normalized_name text, entity_type text, mention_count int)` extrai entidades (heurística por default; caminho LLM
opcional). Verificado em `theodb_rs/src/graph_extract.rs:366` (pg_extern `_extract_entities` em
`theodb_rs/src/graph_extract.rs:273`).

---

# 12. Extrair arestas de co-ocorrência (`ai.extract_graph`)

```sql
SELECT src_normalized, dst_normalized, weight, description
FROM ai.extract_graph('Alice met Bob in Paris. Alice works at Acme Corp.');
```

`ai.extract_graph(text text, use_llm boolean DEFAULT false, model text DEFAULT NULL) -> TABLE(src_normalized
text, dst_normalized text, weight int, description text)` retorna as arestas (entidade↔entidade) extraídas do
texto. Verificado em `theodb_rs/src/graph_extract.rs:369`.

---

# 13. Ingerir um chunk no grafo de conhecimento (`graph_upsert`)

```sql
SELECT theodb.graph_upsert('ws-1', 'coll-1', 'chunk-42',
       'Alice met Bob in Paris. Alice works at Acme Corp.');
```

`graph_upsert(workspace_id text, collection_id text, source_chunk_id text, text text, use_llm boolean DEFAULT
false) -> bigint` extrai e faz upsert **idempotente** de nós/arestas em `theodb.graph_nodes` / `theodb.graph_edges`
(isolado por `workspace_id`/`collection_id`); reingerir o mesmo chunk acumula contagens/pesos, nunca duplica.
Retorna o nº de arestas escritas. Verificado em `theodb_rs/src/graph_extract.rs:372` (pg_extern `_graph_upsert`
em `theodb_rs/src/graph_extract.rs:313`).

---

# 14. Construir o CSR sobre o grafo de conhecimento

```sql
SELECT theodb.graph_build('theodb.graph_edges', 'src_id', 'dst_id');
```

Depois de ingerir chunks com `graph_upsert`, o CSR do GraphRAG é construído diretamente sobre `theodb.graph_edges`
(colunas `src_id`/`dst_id`, ambas `bigint` — ADR-4). É a mesma `graph_build` da seção 3.

---

# 15. Gerar embeddings dos nós do grafo (`graph_embed_nodes`)

```sql
SELECT theodb.graph_embed_nodes('ws-1', 'coll-1');
```

`graph_embed_nodes(workspace_id text, collection_id text, model text DEFAULT NULL) -> bigint` preenche a coluna
`embedding` dos nós ainda não embeddados do workspace/coleção (entrada vetorial do GraphRAG). Verificado em
`theodb_rs/src/graph_rag.rs:71` (pg_extern `_graph_embed_nodes` em `theodb_rs/src/graph_rag.rs:29`).

---

# 16. Fluxo GraphRAG completo (`graph_rag_search`)

```sql
SELECT chunk_id, score
FROM theodb.graph_rag_search(
    theodb.embed('Where does Alice work?', 'text-embedding-3-small'),
    'ws-1', 'coll-1',
    5,   -- k_entry: entidades de entrada por similaridade vetorial
    2    -- max_hops: raio da travessia
);
```

`graph_rag_search(query_embedding vector, workspace_id text, collection_id text, k_entry int DEFAULT 5, max_hops
int DEFAULT 2) -> TABLE(chunk_id text, score float8)` compõe o fluxo: entrada por `<=>` vetorial nas entidades →
`graph_expand` da vizinhança → ranking dos chunks pelas arestas alcançadas. Verificado em
`theodb_rs/src/graph_rag.rs:74`. O valor central: a travessia **adiciona recall** — surfaça chunks de vizinhos a
≤H hops que uma busca vetorial só sobre entidades nunca alcançaria (provado por `m111_flow_multihop_adds_recall`).

---

# 17. GraphRAG com defaults (`k_entry`/`max_hops` omitidos)

```sql
SELECT chunk_id, score
FROM theodb.graph_rag_search(
    theodb.embed('Where does Alice work?', 'text-embedding-3-small'),
    'ws-1', 'coll-1'
);
```

`k_entry` (5) e `max_hops` (2) têm default no wrapper SQL (`theodb_rs/src/graph_rag.rs:74`), então podem ser
omitidos para o caso comum.

---

# 18. Inspecionar o CSR persistido

```sql
SELECT edge_rel::regclass, src_col, dst_col, nnodes, nedges, built_at
FROM theodb.graph_csr;
```

O catálogo `theodb.graph_csr` (definido em `theodb_rs/src/graph.rs:29`) guarda um CSR serializado por relação de
arestas, com estatísticas (`nnodes`/`nedges`) e o timestamp `built_at` que também versiona o cache por-backend
(um `graph_refold` invalida o cache transparentemente).

---

# 19. Fluxo completo recomendado (GraphRAG de ponta a ponta)

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

-- 1. ingerir chunks → grafo de conhecimento (idempotente, isolado por tenant)
SELECT theodb.graph_upsert('ws-1', 'coll-1', 'chunk-42',
       'Alice met Bob in Paris. Alice works at Acme Corp.');

-- 2. construir o CSR + embeddar os nós
SELECT theodb.graph_build('theodb.graph_edges', 'src_id', 'dst_id');
SELECT theodb.graph_embed_nodes('ws-1', 'coll-1');

-- 3. consultar: vetor-de-entrada → travessia → chunks rankeados
SELECT chunk_id, score
FROM theodb.graph_rag_search(
    theodb.embed('Where does Alice work?', 'text-embedding-3-small'),
    'ws-1', 'coll-1', 5, 2
);
```

Fluxo completo:

1. instala a extensão `theodb`;
2. ingere chunks no grafo de conhecimento (`graph_upsert`);
3. constrói o CSR persistido e embedda os nós;
4. consulta via GraphRAG (entrada vetorial → travessia ≤H hops → reranking de chunks).

---

# Notas de honestidade (estado medido)

- **O que é entregue e medido:** o primitivo de travessia (CSR+BFS) foi validado no gate M107
  (`docs/benchmarks/m107-graph-spike.md`, GO), persistido no M108 (`m108-persisted-csr.json`), e o fluxo
  GraphRAG composto foi provado no M111/M112 (`m111-graphrag-flow.json` + testes `#[pg_test]`). Todas as
  funções desta página existem no binário default.
- **`use_llm`:** o caminho de extração LLM (`use_llm => true` em `ai.extract_graph` / `graph_upsert`) faz
  fail-soft para a heurística de co-ocorrência quando o LLM não responde (provado por
  `m110_llm_path_fails_soft_to_heuristic`); a extração default (`use_llm => false`) é puramente heurística.
- **Escopo:** o número "106–738×" é sobre o **primitivo de travessia isolado** vs recursive-CTE, medido no
  spike M107 — não é uma afirmação de vitória end-to-end sobre um produto de grafo dedicado. A superfície
  `SQL/PGQ` (`theodb.pgq_match`, M113) é adjacente e não é coberta aqui.
