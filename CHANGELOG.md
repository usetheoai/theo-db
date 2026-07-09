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