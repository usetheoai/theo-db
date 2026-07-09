# Blueprint M64 — RAG-sobre-SQL unificado ("the one query")

**Milestone:** M64 · **Data:** 2026-07-09 · **Método:** R0 (WebSearch/WebFetch ativo — papers/OSS/blogs, ≥2 fontes primárias por claim) + council-ai-in-db (inventário file:line do código local).

## Coverage Corner 1 — Integration Tests

Como o campo testa "retrieval + filtro numa query só"?
- **pgvector-python** hybrid: RRF via CTE + FULL OUTER JOIN, testado com fixtures ([rrf.py](https://github.com/pgvector/pgvector-python/blob/master/examples/hybrid_search/rrf.py)).
- **Supabase** `hybrid_search()` função SQL testável ([hybrid-search](https://supabase.com/docs/guides/ai/hybrid-search)).
- TheoDB local: `filtered_scan_preserves_recall_via_iterative` (`hnsw_page.rs:2283`) prova recall==exact seqscan sob filtro; `hybrid_search_accepts_filter_and_language` (`hnsw_page.rs:2337`) prova filtro aplicado nas 2 pernas.
- **Gap de teste do M64:** a query composta de referência (WHERE + ORDER BY vec + LIMIT + `string_agg` contexto) numa única SQL; + invariante read-your-writes (escreve linha na txn → RAG-query na mesma txn → recuperável).

## Coverage Corner 2 — Dependencies

Nenhuma dependência nova. Composição do que existe:
- Filtered ANN (M52): `theodb_hnsw` `amcanorderbyop=true` (`am/mod.rs:78`), iterative scan (`am/scan.rs:127-316`).
- Híbrida RRF (M53): `ai.hybrid_search_rrf` (`hybrid.rs:90`), `filter_sql` inlineado nas 2 pernas (`hybrid.rs:34,42,66,74`).
- embed/chat/nl in-SQL: `theodb.embed` (`api.rs:306`), `theodb.chat` (`chat.rs:19`), `ai.rank` (`api.rs:375`).
- Vector JOIN (M63): raw-LATERAL Index Scan (`hnsw_page.rs:2976`).
- Columnar codegen (M61/M62): `theodb.olap_sql`/`htap_refresh_sql` (`sql/85-theodb-htap.sql:73,126`) — **codegen, cliente executa** (pg_duckdb proíbe DuckDB em função, ADR-0021).

## Coverage Corner 3 — Tools

- Benchmark harness próprio (Python, espelha `benchmarks/run_m63_vector_join.py`): braço unified-single-SQL vs braço app-orchestrated multi-call (referência LangChain PGVector / LlamaIndex Postgres).
- Métrica: recall@k **igualado ANTES** de comparar latência (disciplina BEIR/`theodb_bench`); p50/p95/p99; round-trips/query instrumentado; bytes app↔DB.

## Coverage Corner 4 — Techniques

**Padrão SOTA (citado, ≥2 fontes):** o núcleo é `SELECT … [JOIN] WHERE <filtro> ORDER BY embedding <op> <qvec> LIMIT k`. O que cada sistema "vende" é quanto dobra no statement:
- **AlloyDB AI** — `embedding()` in-SQL + `ai.rank()` rerank RAG in-SQL num CTE (um statement): [work-with-embeddings](https://cloud.google.com/alloydb/docs/ai/work-with-embeddings), [rank-rerank-search-results-rag](https://docs.cloud.google.com/alloydb/docs/ai/rank-rerank-search-results-rag).
- **pgai (Timescale)** — `ai.openai_embed` + `ai.openai_chat_complete` in-SQL: [pgai](https://github.com/timescale/pgai).
- **Supabase** — `match_documents()` / `hybrid_search()`: [semantic-search](https://supabase.com/docs/guides/ai/semantic-search).
- **RRF:** Cormack et al., SIGIR 2009, `RRFscore=Σ1/(k+r(d))`, k=60 ([PDF](http://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf), DOI 10.1145/1571941.1572114).
- **Consistência:** content+embedding na mesma txn ([Supabase](https://supabase.com/blog/openai-embeddings-postgres-vector)); "don't go through the app layer" ([AWS Aurora ML](https://aws.amazon.com/blogs/database/leverage-pgvector-and-amazon-aurora-postgresql-for-natural-language-processing-chatbots-and-sentiment-analysis/)).
- **A lacuna genuína do campo:** ninguém publica (i) um shape canônico de **context-assembly** (top-k → `string_agg` → contexto), nem (ii) o head-to-head **"1 SQL vs N app-calls"** (round-trips economizados). É o que o M64 contribui.

## A leg columnar — veredito honesto (BLOCKER de honestidade no DoD)

Combinar retrieval row-store (index-served) + agregação columnar (Parquet/DuckDB) **numa query planner-integrada NÃO é alcançável hoje**: ADR-0021 — pg_duckdb `ERROR: DuckDB execution is not supported inside functions`; o índice `theodb_hnsw` é row-store, o Parquet vive no DuckDB — **duas engines, um planner não as unifica**. SOTA que faz first-class (AlloyDB in-memory columnar, TiDB/TiFlash) tem **uma engine + um planner** dono de ambos os stores ([AlloyDB columnar](https://cloud.google.com/alloydb/docs/columnar-engine/about), [TiFlash](https://github.com/pingcap/docs/blob/master/tiflash/tiflash-overview.md), [HTAP survey arXiv:2404.15670](https://arxiv.org/abs/2404.15670)).

Dois caminhos honestos:
- **Path 1 (uma query, real, row-store):** `WHERE … ORDER BY vec LIMIT k` como CTE → `GROUP BY … AVG/COUNT` sobre o top-k. Planner-integrado hoje; a agregação corre no executor PG sobre k linhas — a engine columnar é **irrelevante** nessa escala (chamar de "columnar RAG" seria desonesto).
- **Path 2 (agregar retrieved-set contra fato columnar Parquet grande):** **dois statements** (o retrieval + o `SELECT olap_sql()` que o cliente roda). Reusa M62.

## ADR-1 — Own-code vs composição (rung 1 parsimony)

**M64 é ~90% composição+medição+documentação.** Rung-1 ("isto precisa existir?") diz **NÃO** construir `theodb.rag_query(...)` — precedente ADR-0022 (M63 rejeitou `theodb.vector_join` pelo mesmo motivo: helper com SQL dinâmico açucara o first-class e arrisca o pushdown). Deliverables: (1) guia + query de referência (Path 1 real, Path 2 honesto); (2) benchmark harness unified-vs-app-layer; (3) veredito honesto. Own-code opcional: um **template SQL de context-assembly** só se a medição mostrar que a costura manual dói.

## ADR-2 — Segurança do filter_sql

`filter_sql` (RRF) é RAW caller-privilege SQL com confinamento sintático, **não** injection-proof (`hybrid.rs:8-13,132-137`). Qualquer exemplo do padrão RAG que construa `filter_sql` de input (ex.: NL→SQL) passa por council-security antes de endossar.

## Débito honesto (fora do M64)

- Cross-encoder rerank de 2ª ordem → M65 (`ai.rerank`, não existe ainda; o rerank disponível hoje é `ai.rank` LLM-scoring ou o score RRF).
- Qualidade de chunking → M66.
- Path 2 columnar first-class exigiria uma engine única (fora de escopo D2).
