# Changelog

Todas as mudanças notáveis deste projeto são documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/),
e este projeto adere ao [Semantic Versioning](https://semver.org/).

> Nota: o projeto está em fase inicial de design (pré-código, sem release). O tracker
> de issues/PRs ainda não está configurado, por isso as entradas abaixo ainda não
> referenciam números de ticket. A partir da configuração do tracker, toda entrada
> passará a citar o issue/PR correspondente.

## [Unreleased]

### Added
- **M65 — Fases 1-3: `ai.rerank` own-code (cross-encoder via HTTP) + harness BEIR** (`theodb_rs/src/rerank.rs` (NEW), `theodb_rs/src/api.rs`, `theodb_rs/src/lib.rs`, `benchmarks/servers/rerank_server.py` (NEW), `benchmarks/run_m65_rerank.py` (NEW) + tests): superfície `ai.rerank(query text, docs text[], model text DEFAULT NULL, top_n int DEFAULT NULL) RETURNS TABLE(idx int, score real)` ordenada por relevância DESC (idx 0-based no array de entrada). `rerank.rs::run` espelha `embed.rs::run_batch` (reusa `http.rs::post_json` — SSRF max_redirects=0, retry, timeout, err tipado; Regra 9, zero client novo), parser N-in/N-out do shape cross-encoder `{"results":[{"index","relevance_score"}]}` (Cohere/BGE/TEI). GUCs livres `theodb.rerank_endpoint`/`_model`/`_api_key` (sem GucRegistry). REVOKE ALL FROM PUBLIC (interno+público, least-privilege HTTP outbound). Nome `rerank` distinto do `ai.rank` existente (LLM-scoring). pg_test offline (guards NULL query/doc, empty→no-HTTP, unset endpoint, SSRF non-http, connrefused tipado, parser align-by-index/size-mismatch/dup/out-of-range/non-numeric → 38000) + parser unit-tested; harness aritmética (mrr_at_k, rerank_verdict PASS/HONEST_NEGATIVE) 11/11 pytest GREEN container-free, ruff clean. `rerank_server.py` = cross-encoder REAL (sentence-transformers, Apache 2.0) espelhando `embedding_server.py`. **Gate real (resta droplet):** benchmark BEIR (SciFact) nDCG@10/MRR com vs sem rerank → `docs/benchmarks/m65-rerank.{md,json}` — honest-negative se não melhorar (literatura: cross-encoders off-the-shelf degradaram nDCG −0.3% a −3.1% fora de distribuição). ADR-0024.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.54.0] - 2026-07-09
### Added
- **M64 — Fases 1-2: RAG-sobre-SQL unificado (pg_test recall/read-your-writes + harness unified-vs-app-layer)** (`theodb_rs/src/am/hnsw_page.rs`, `benchmarks/run_m64_rag_over_sql.py` + `benchmarks/tests/test_run_m64_rag_over_sql.py`): a "query única" de RAG (filtro relacional + retrieval vetorial + context-assembly numa SQL só) MEDIDA vs a orquestração app-layer. 2 `#[pg_test]`: `rag_unified_query_preserves_recall` (a query composta `WITH retrieved AS (WHERE cat ORDER BY emb <=> $q LIMIT k) SELECT string_agg(content) ...` recupera EXATAMENTE o top-k filtrado do oráculo exato — recall preservado + context-assembly concatena exatamente K docs) e `rag_unified_read_your_writes` (linha INSERTada na txn é recuperável pela RAG-query na MESMA txn via pending region — a consistência transacional que o app-layer não tem de graça). Harness 2-braços (A_unified 1 round-trip / B_app_layer 2 round-trips: retrieve+hydrate) — métrica primária round-trips/query (medido, não hardcoded), recall-match gate ANTES de comparar latência (fairness), aritmética claim-bearing pura (round_trip_delta, recall_match_gate, verdict honesto por-eixo com UNCOMPARABLE se o gate falha) unit-testada container-free (15/15 pytest GREEN, ruff clean). Zero código de produção novo (composição — rung-1 parsimony, precedente ADR-0022). **Achado honesto (blueprint + ADR-0023):** o DoD pede "agregação columnar planner-integrada" mas é inalcançável (pg_duckdb proíbe DuckDB em função, ADR-0021; row-store + Parquet são 2 engines que 1 planner não unifica) — entregamos Path 1 (uma query row-store real) + Path 2 (columnar = 2 statements) documentado honestamente. **Validado (droplet, stack real theodb_rs+vector+vectorscale+theodb):** os 2 `#[pg_test]` GREEN (`cargo pgrx test pg17 rag_unified` — 2 passed); benchmark (n=5000, dim 128, k 10, 3 runs): braço A unified **1 round-trip** p50 **6.721 ms** vs braço B app-layer **2 round-trips** p50 **7.284 ms**, **recall-match gate PASS (jaccard 1.0)** — unified ~8% mais rápido co-located (o custo do 2º round-trip, que amplifica sobre rede), a vitória estrutural é round_trips 1 vs 2. Correção de honestidade: tabela ganhou PRIMARY KEY (sem ela o hydrate do braço B seqscanearia 5000 linhas — straw-man). `docs/benchmarks/m64-rag-over-sql.{md,json}` + ADR-0023.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.53.0]