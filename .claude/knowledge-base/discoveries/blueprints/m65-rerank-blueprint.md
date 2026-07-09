# Blueprint M65 — `ai.rerank`: reranking de 2ª ordem por cross-encoder

**Milestone:** M65 · **Data:** 2026-07-09 · **Método:** R0 (WebSearch/WebFetch ativo — papers/OSS/blogs, ≥2 fontes por claim) + council-ai-in-db (mapa file:line da superfície ai.* a espelhar).

## Coverage Corner 1 — Integration Tests

- Padrão ai.* testa em 2 níveis (`benchmarks/tests/test_ai_sql.py:3`): (a) pg_test Rust offline (guards/parsers, sem rede — ex. `lib.rs:56-100` embed NULL/unset/SSRF/connrefused; `chat.rs:251-270` parsers); (b) oracle Python contra stub HTTP determinístico (`benchmarks/servers/chat_server.py`, round-trip contado via `/count`).
- **M65:** pg_test offline dos guards/parsers de `rerank.rs` (N-in/N-out, NULL element, SSRF, unset endpoint — copia `lib.rs:83-100`) + novo stub `benchmarks/servers/rerank_server.py` (shape `{"results":[{"index":i,"relevance_score":s}]}`) + `test_rerank_sql.py`.

## Coverage Corner 2 — Dependencies

- **Reusar** o HTTP client compartilhado `theodb_rs/src/http.rs::post_json` (`http.rs:41` — minreq blocking, retry 429/502/503 MAX_RETRIES=2, SSRF `with_max_redirects(0)` `http.rs:50`, timeout 30s, Bearer header, err tipado 38000). Regra 9 / parsimony rung-4: não reinventar client.
- GUCs livres de sessão (sem GucRegistry): `guc("theodb.rerank_endpoint")`/`_model`/`_api_key` (`pg.rs:50` = `current_setting`). Zero-config, espelha `embed.rs:129-150`.
- Reranker externo (não reinventar o modelo): default permissivo **BGE-reranker-v2-m3** (BAAI, **Apache 2.0**, self-host TEI/vLLM) ou **mxbai-rerank-v2** (Apache 2.0). Endpoints proprietários (Cohere/Voyage) configuráveis por GUC; Jina evitado como default (CC-BY-NC).

## Coverage Corner 3 — Tools

- Benchmark BEIR reusa o harness do M53 (`benchmarks/tests/test_m53_beir.py` / `theodb_bench`). Métricas nDCG@10 + MRR@10 + Recall@50 (reusar `pytrec_eval` — rung-2, não reinventar métricas).
- Reranker real para o benchmark: self-host BGE-reranker-v2-m3 via `sentence-transformers CrossEncoder` num servidor HTTP mínimo (CPU inference ok para subset pequeno: SciFact 300 queries × top-50).

## Coverage Corner 4 — Techniques

**Padrão SOTA (citado, ≥2 fontes):** dois estágios **retrieve (bi-encoder, recall) → rerank top-k (cross-encoder, precision)**. O cross-encoder concatena query+doc numa sequência e faz full-attention → escalar de relevância por par (mais preciso que bi-encoder, mas 1 inferência/par → só para top-k pequeno).
- monoBERT — Nogueira & Cho 2019 ([arXiv:1901.04085](https://arxiv.org/abs/1901.04085)); monoT5 — Nogueira 2020 ([arXiv:2003.06713](https://arxiv.org/abs/2003.06713)); "In Defense of Cross-Encoders" ([arXiv:2212.06121](https://arxiv.org/pdf/2212.06121), >20% P@20 vs bi-encoders).
- **API shape convergente** (Cohere/Jina/Voyage/TEI/vLLM): `POST /rerank {"query","documents[],"model"}` → `{"results":[{"index","relevance_score"}]}`. Cohere ([docs.cohere.com/reference/rerank](https://docs.cohere.com/reference/rerank)); BGE ([HF BAAI/bge-reranker-v2-m3](https://huggingface.co/BAAI/bge-reranker-v2-m3)); mxbai ([github.com/mixedbread-ai/mxbai-rerank](https://github.com/mixedbread-ai/mxbai-rerank)).
- **SOTA-anchor (AlloyDB `ai.rank`)** retorna `TABLE(index, score)`, NÃO reordena in-place ([AlloyDB rank-rerank](https://docs.cloud.google.com/alloydb/docs/ai/rank-rerank-search-results-rag)); pgai `ai.cohere_rerank` ([github.com/timescale/pgai](https://github.com/timescale/pgai)).
- **BEIR** (Thakur et al. NeurIPS 2021, [arXiv:2104.08663](https://arxiv.org/abs/2104.08663), [repo](https://github.com/beir-cellar/beir)): nDCG@10 é a métrica primária; metodologia retrieve-then-rerank.

## ADR-1 — Assinatura + nome `ai.rerank` (não `ai.rank`)

`ai.rerank(query text, docs text[], model text DEFAULT NULL, top_n int DEFAULT NULL) RETURNS TABLE(idx int, score real)`.
- Retorna `TABLE(idx, score)` (não reordena) — convergência AlloyDB/Cohere/Voyage/Jina; permite `ORDER BY score DESC` + join do idx de volta aos docs. Precedente exato: `_hybrid_search_rrf` usa `TableIterator` (`api.rs:108`).
- Nome **`rerank`** (não `rank`) — o repo JÁ tem `ai.rank` (LLM-scoring por-linha, `chat.rs:90`, semanticamente diferente: 1 prompt→1 float via generative). Divergimos do AlloyDB (que chama o dele `ai.rank`) de propósito para não colidir. Registrar no ADR.
- `idx` 0-based referenciando o array de entrada (igual Cohere) — documentar o off-by-one no join com `ROW_NUMBER()` (1-based).

## ADR-2 — Own-code mínimo (rung-1 parsimony)

Novo `rerank.rs::run(query, docs[]) → Vec<f32>` alinhado por índice, espelhando `embed.rs::run_batch` (`embed.rs:55-124`): guard docs vazio → `[]` sem HTTP; NULL element → 22023; `resolve_rerank_cfg`; payload `{"query","documents","model"}`; `post_json("ai.rerank",...)`; parse `results[].index`+`relevance_score` com invariante N-in/N-out (mismatch/duplicate/out-of-range → 38000). Novo `#[pg_extern] _ai_rerank` + wrapper `extension_sql!` + REVOKE FROM PUBLIC. Zero client novo (reusa http.rs), zero GUC registry.

## O gate REAL (o que importa) — o benchmark, não a superfície

**A superfície que roda ≠ ganho de retrieval provado.** O DoD do M65 é explícito: `ai.rerank` só é aceito se **melhorar nDCG@10/MRR mensuravelmente em BEIR** (`docs/benchmarks/m65-rerank.{md,json}`), com **honest-negative se não melhorar**. Protocolo honesto:
1. Mesmo top-k de entrada (retrieval theodb_hnsw top-50) alimenta os 2 braços.
2. Braço A (baseline): nDCG@10/MRR@10 sobre o top-50 ordenado por distância vetorial.
3. Braço B (+rerank): rerankear os 50 via `ai.rerank`, medir nDCG@10/MRR@10 sobre o novo top-10.
4. Reportar Recall@50 pré/pós (o rerank NÃO adiciona evidência — sanity check) + p95/p99 de latência (o custo é real).
5. ≥3 runs, mean±std, artefato em `docs/benchmarks/`.

## Honest-negative (evidência quantitativa — o DoD exige)

Cross-encoders off-the-shelf (ms-marco-MiniLM, BGE-reranker-base) **degradaram nDCG −0.3% a −3.1%** + **560-2100 ms de latência** em corpora fora da distribuição de treino ([pgai report](https://0deepresearch.com/posts/2025-07-22-an-expert-technical-report-on-pgai-architecture-implementation-and-future-of-in-database-ai-with-postgresql/)). 3 modos de falha ([ReRank or Not](https://medium.com/@sindhuja.codes/when-to-rerank-and-when-to-let-semantic-search-do-its-job-af3adddd602b), [Zilliz](https://zilliz.com/learn/optimize-rag-with-rerankers-the-role-and-tradeoffs)): (1) retrieval já bom (top-3 correto → rerank decorativo); (2) recall baixo (nada com que trabalhar); (3) piora ativa (distribution shift). **Se o delta nDCG for ≤0 ou dentro do ruído, declarar honest-negative com números — não spin.** O valor de M65 é fechar o lifecycle retrieve→rerank de forma mensurável e model-agnostic, não afirmar ganho universal.

## Débito honesto

- Latência do rerank HTTP síncrono pode ser o novo gargalo — p99 obrigatório.
- Escolha do reranker default (BGE vs mxbai) decidida por benchmark no corpus-alvo, não reputação.
- SSRF/timeout herda o fail-closed do `ai.embed` (invariante que o /review confere).
