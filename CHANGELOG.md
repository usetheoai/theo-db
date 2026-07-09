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

## [0.54.0] - 2026-07-09
### Added
- **M64 — Fases 1-2: RAG-sobre-SQL unificado (pg_test recall/read-your-writes + harness unified-vs-app-layer)** (`theodb_rs/src/am/hnsw_page.rs`, `benchmarks/run_m64_rag_over_sql.py` + `benchmarks/tests/test_run_m64_rag_over_sql.py`): a "query única" de RAG (filtro relacional + retrieval vetorial + context-assembly numa SQL só) MEDIDA vs a orquestração app-layer. 2 `#[pg_test]`: `rag_unified_query_preserves_recall` (a query composta `WITH retrieved AS (WHERE cat ORDER BY emb <=> $q LIMIT k) SELECT string_agg(content) ...` recupera EXATAMENTE o top-k filtrado do oráculo exato — recall preservado + context-assembly concatena exatamente K docs) e `rag_unified_read_your_writes` (linha INSERTada na txn é recuperável pela RAG-query na MESMA txn via pending region — a consistência transacional que o app-layer não tem de graça). Harness 2-braços (A_unified 1 round-trip / B_app_layer 2 round-trips: retrieve+hydrate) — métrica primária round-trips/query (medido, não hardcoded), recall-match gate ANTES de comparar latência (fairness), aritmética claim-bearing pura (round_trip_delta, recall_match_gate, verdict honesto por-eixo com UNCOMPARABLE se o gate falha) unit-testada container-free (15/15 pytest GREEN, ruff clean). Zero código de produção novo (composição — rung-1 parsimony, precedente ADR-0022). **Achado honesto (blueprint + ADR-0023):** o DoD pede "agregação columnar planner-integrada" mas é inalcançável (pg_duckdb proíbe DuckDB em função, ADR-0021; row-store + Parquet são 2 engines que 1 planner não unifica) — entregamos Path 1 (uma query row-store real) + Path 2 (columnar = 2 statements) documentado honestamente. **Validado (droplet, stack real theodb_rs+vector+vectorscale+theodb):** os 2 `#[pg_test]` GREEN (`cargo pgrx test pg17 rag_unified` — 2 passed); benchmark (n=5000, dim 128, k 10, 3 runs): braço A unified **1 round-trip** p50 **6.721 ms** vs braço B app-layer **2 round-trips** p50 **7.284 ms**, **recall-match gate PASS (jaccard 1.0)** — unified ~8% mais rápido co-located (o custo do 2º round-trip, que amplifica sobre rede), a vitória estrutural é round_trips 1 vs 2. Correção de honestidade: tabela ganhou PRIMARY KEY (sem ela o hydrate do braço B seqscanearia 5000 linhas — straw-man). `docs/benchmarks/m64-rag-over-sql.{md,json}` + ADR-0023.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.53.0]