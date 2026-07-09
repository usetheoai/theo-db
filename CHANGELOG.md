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
- **M66 — Fase 1: chunker declarativo own-code (`theodb.chunk`)** (`theodb_rs/src/chunk.rs` (NEW), `api.rs`, `lib.rs`, `vectorizer.rs`): `theodb.chunk(content text, strategy text DEFAULT 'recursive', chunk_size int DEFAULT 512, overlap int DEFAULT 64) RETURNS text[]` — 3 estratégias (`fixed` janelas deslizantes / `sentence` agrupa sentenças `.!?` até size / `recursive` hierarquia de separadores `\n\n`→`\n`→`. `→` ` à la LangChain) + `overlap` ortogonal. **Lógica de string pura, char-based, Unicode-safe** (nunca corta grapheme multibyte — testado com emoji/CJK). 9 unit-tests puros (`cargo test`, sem DB) + 4 pg_test (error-paths: size≤0/overlap≥size/strategy desconhecida → 22023; smoke das 3 estratégias). **Bug pego em teste de mesa antes do commit:** o carry de overlap do `pack_with_overlap` acumulava chunks > size quando overlap=0 (`cur.len()>1` → `!cur.is_empty()`). Substitui o `theodb.chunk_text` plpgsql MORTO (fixed-only, nunca chamado no fluxo) — um só chunker (KISS). Own-code char-based é rung-1 (pgai também começou char-based; token-based/tiktoken é v2 rastreado). ADR-0025 (deferir `semantic` por evidência: arXiv:2410.13070 — ganho 0-4pp, freq negativo, 14× custo). **Fases 2-3 (chunk-table opt-in + benchmark):** modo `WITH (chunk_strategy=…)` no vectorizer — 1-doc→N-chunks numa `{target}_chunks` table (`upsert_chunks`: chunk→embed_batch→DELETE+INSERT sem órfãos), modo 1→1 in-place preservado (não-breaking). **pg_test GREEN (stack real): chunk 16/16 + vectorizer 13/13.** **Benchmark BEIR/NFCorpus (50 queries) — STRATEGY_MATTERS:** `sentence` nDCG@10 **0.397** > `recursive` 0.391 > `fixed` 0.372 (spread 0.025 > ruído; a estratégia move o recall, k-adaptativo iguala o budget de contexto; dependência de corpus honesta — não-universal). `docs/benchmarks/m66-chunking.{md,json}`, ADR-0025.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.55.0] - 2026-07-09
### Added
- **M65 — Fases 1-3: `ai.rerank` own-code (cross-encoder via HTTP) + harness BEIR** (`theodb_rs/src/rerank.rs` (NEW), `theodb_rs/src/api.rs`, `theodb_rs/src/lib.rs`, `benchmarks/servers/rerank_server.py` (NEW), `benchmarks/run_m65_rerank.py` (NEW) + tests): superfície `ai.rerank(query text, docs text[], model text DEFAULT NULL, top_n int DEFAULT NULL) RETURNS TABLE(idx int, score real)` ordenada por relevância DESC (idx 0-based no array de entrada). `rerank.rs::run` espelha `embed.rs::run_batch` (reusa `http.rs::post_json` — SSRF max_redirects=0, retry, timeout, err tipado; Regra 9, zero client novo), parser N-in/N-out do shape cross-encoder `{"results":[{"index","relevance_score"}]}` (Cohere/BGE/TEI). GUCs livres `theodb.rerank_endpoint`/`_model`/`_api_key` (sem GucRegistry). REVOKE ALL FROM PUBLIC (interno+público, least-privilege HTTP outbound). Nome `rerank` distinto do `ai.rank` existente (LLM-scoring). pg_test offline (guards NULL query/doc, empty→no-HTTP, unset endpoint, SSRF non-http, connrefused tipado, parser align-by-index/size-mismatch/dup/out-of-range/non-numeric → 38000) + parser unit-tested; harness aritmética (mrr_at_k, rerank_verdict PASS/HONEST_NEGATIVE) 11/11 pytest GREEN container-free, ruff clean. `rerank_server.py` = cross-encoder REAL (sentence-transformers, Apache 2.0) espelhando `embedding_server.py`. **Gate medido (droplet, stack real) — VEREDITO HONEST-NEGATIVE:** 14 `#[pg_test]` GREEN (`cargo pgrx test pg17 rerank`); benchmark BEIR/SciFact (100 queries, 3 runs determinísticos) mostrou o rerank (BGE-reranker-base) **degradando** o nDCG@10 em **−3.8%** (baseline 0.7327 → rerank 0.6947), custo ~1.96 s p50/query, Recall@50 conservado (0.92, sanity ✓) — exatamente o previsto pela literatura (cross-encoder off-the-shelf regride em corpus fora de distribuição). **Decisão:** `ai.rerank` embarca (superfície model-agnostic correta e medível), NÃO se afirma ganho de qualidade (public-copy §4), rerank é opt-in (custo sem ganho garantido; o operador escolhe o reranker por GUC). `docs/benchmarks/m65-rerank.{md,json}`, ADR-0024.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.54.0]