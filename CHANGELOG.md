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
- **M67 — auto-tune de índices vetoriais (`theodb.recommend_ef` + coletor de stats)** (`theodb_rs/src/am/autotune.rs` (NEW), `am/mod.rs`, `am/hnsw_page.rs`, `api.rs`, `benchmarks/run_m67_autotune.py` (NEW)): **recomendador determinístico** `theodb.recommend_ef(index, vec_col, samples, recall_target, k)` — bisecção monotônica sobre recall(ef) (monotônico, Malkov & Yashunin) contra GT exato amostrado (seqscan), retorna o menor ef que atinge o alvo (ctid como id estável; MAX_EF se inatingível). **Coletor** `theodb.scan_stats(tbl,col,query,ef,k)` — mede o **pages_read REAL** (thread_local que o traverse HNSW bumpa — 1 add in-memory, sem page write) + latência, persiste no catálogo heap `theodb._index_scan_stats` (FORA das páginas do índice — crash-safe, M35); `theodb.index_scan_stats(rel)` lê os agregados. REVOKE FROM PUBLIC. **5 pg_test GREEN** (stack real) + 12 pytest (MAE/RQUT/convergência). **Benchmark (10k sintético) — CONVERGED com nuance honesta:** o recomendador converge na média (recall 0.986 ≥ alvos), MAS (1) corpus fácil demais (baseline ef=64 dá recall 1.0; todos os alvos → ef=10 — não estressa a curva ef; SIFT1M mostraria o scaling), (2) RQUT 12% de cauda (mean-optimal, não tail-safe — v2). **NÃO auto-tune online** (deferido por evidência ADR-0026 — oscilação; SOTA é early-termination acadêmico DARTH/Ada-ef). **amcostestimate:** fórmula M48 (f(ef)) retida + auditabilidade via scan_stats; calibração-in-planning DEFERIDA por risco EC-3 (SPI no planning abortaria TODO o planejamento). `docs/benchmarks/m67-autotune.{md,json}`, ADR-0026.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.56.0] - 2026-07-09
### Added
- **M66 — Fase 1: chunker declarativo own-code (`theodb.chunk`)** (`theodb_rs/src/chunk.rs` (NEW), `api.rs`, `lib.rs`, `vectorizer.rs`): `theodb.chunk(content text, strategy text DEFAULT 'recursive', chunk_size int DEFAULT 512, overlap int DEFAULT 64) RETURNS text[]` — 3 estratégias (`fixed` janelas deslizantes / `sentence` agrupa sentenças `.!?` até size / `recursive` hierarquia de separadores `\n\n`→`\n`→`. `→` ` à la LangChain) + `overlap` ortogonal. **Lógica de string pura, char-based, Unicode-safe** (nunca corta grapheme multibyte — testado com emoji/CJK). 9 unit-tests puros (`cargo test`, sem DB) + 4 pg_test (error-paths: size≤0/overlap≥size/strategy desconhecida → 22023; smoke das 3 estratégias). **Bug pego em teste de mesa antes do commit:** o carry de overlap do `pack_with_overlap` acumulava chunks > size quando overlap=0 (`cur.len()>1` → `!cur.is_empty()`). Substitui o `theodb.chunk_text` plpgsql MORTO (fixed-only, nunca chamado no fluxo) — um só chunker (KISS). Own-code char-based é rung-1 (pgai também começou char-based; token-based/tiktoken é v2 rastreado). ADR-0025 (deferir `semantic` por evidência: arXiv:2410.13070 — ganho 0-4pp, freq negativo, 14× custo). **Fases 2-3 (chunk-table opt-in + benchmark):** modo `WITH (chunk_strategy=…)` no vectorizer — 1-doc→N-chunks numa `{target}_chunks` table (`upsert_chunks`: chunk→embed_batch→DELETE+INSERT sem órfãos), modo 1→1 in-place preservado (não-breaking). **pg_test GREEN (stack real): chunk 16/16 + vectorizer 13/13.** **Benchmark BEIR/NFCorpus (50 queries) — STRATEGY_MATTERS:** `sentence` nDCG@10 **0.397** > `recursive` 0.391 > `fixed` 0.372 (spread 0.025 > ruído; a estratégia move o recall, k-adaptativo iguala o budget de contexto; dependência de corpus honesta — não-universal). `docs/benchmarks/m66-chunking.{md,json}`, ADR-0025.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.55.0]