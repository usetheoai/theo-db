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
- **M64 — Fases 1-2: RAG-sobre-SQL unificado (pg_test recall/read-your-writes + harness unified-vs-app-layer)** (`theodb_rs/src/am/hnsw_page.rs`, `benchmarks/run_m64_rag_over_sql.py` + `benchmarks/tests/test_run_m64_rag_over_sql.py`): a "query única" de RAG (filtro relacional + retrieval vetorial + context-assembly numa SQL só) MEDIDA vs a orquestração app-layer. 2 `#[pg_test]`: `rag_unified_query_preserves_recall` (a query composta `WITH retrieved AS (WHERE cat ORDER BY emb <=> $q LIMIT k) SELECT string_agg(content) ...` recupera EXATAMENTE o top-k filtrado do oráculo exato — recall preservado + context-assembly concatena exatamente K docs) e `rag_unified_read_your_writes` (linha INSERTada na txn é recuperável pela RAG-query na MESMA txn via pending region — a consistência transacional que o app-layer não tem de graça). Harness 2-braços (A_unified 1 round-trip / B_app_layer 2 round-trips: retrieve+hydrate) — métrica primária round-trips/query (medido, não hardcoded), recall-match gate ANTES de comparar latência (fairness), aritmética claim-bearing pura (round_trip_delta, recall_match_gate, verdict honesto por-eixo com UNCOMPARABLE se o gate falha) unit-testada container-free (15/15 pytest GREEN, ruff clean). Zero código de produção novo (composição — rung-1 parsimony, precedente ADR-0022). **Achado honesto (blueprint + ADR-0023):** o DoD pede "agregação columnar planner-integrada" mas é inalcançável (pg_duckdb proíbe DuckDB em função, ADR-0021; row-store + Parquet são 2 engines que 1 planner não unifica) — entregamos Path 1 (uma query row-store real) + Path 2 (columnar = 2 statements) documentado honestamente. **Validado (droplet, stack real theodb_rs+vector+vectorscale+theodb):** os 2 `#[pg_test]` GREEN (`cargo pgrx test pg17 rag_unified` — 2 passed); benchmark (n=5000, dim 128, k 10, 3 runs): braço A unified **1 round-trip** p50 **6.721 ms** vs braço B app-layer **2 round-trips** p50 **7.284 ms**, **recall-match gate PASS (jaccard 1.0)** — unified ~8% mais rápido co-located (o custo do 2º round-trip, que amplifica sobre rede), a vitória estrutural é round_trips 1 vs 2. Correção de honestidade: tabela ganhou PRIMARY KEY (sem ela o hydrate do braço B seqscanearia 5000 linhas — straw-man). `docs/benchmarks/m64-rag-over-sql.{md,json}` + ADR-0023.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.53.0] - 2026-07-09
### Added
- **M63 — Fases 1-3: vector JOIN via LATERAL-index-scan (pg_test EXPLAIN prova Index Scan + benchmark)** (`theodb_rs/src/am/hnsw_page.rs`, `benchmarks/run_m63_vector_join.py` + `benchmarks/tests/test_run_m63_vector_join.py`, `docs/benchmarks/m63-vector-join.md`, `docs/adr/0022-m63-vector-join-lateral-not-node.md`): **achado central (R1) POSITIVO e provado por `EXPLAIN`** — o `a CROSS JOIN LATERAL (SELECT b.id FROM b ORDER BY b.emb <=> a.emb LIMIT k) j` planeja o ramo **interno** como `Index Scan using vjb_idx ... Order By: (b.emb <=> a.emb)` no índice `theodb_hnsw` (o `Seq Scan` é só do lado externo `a`, o driver do LATERAL), **não** o nested-loop O(n·m). 4 `#[pg_test]` GREEN (`cargo pgrx test pg17 vector_join` — 4 passed): `vector_join_uses_index_scan` (o gate estrutural, + a forma dedup `WHERE b.id<>a.id` mantém o Index Scan, Q1), `vector_join_recall_matches_exact_within_tol` (join-recall per-row min+mean vs GT exato O(n·m), edges k=1/k≥|b|), `vector_join_threshold_correct` (τ∈{0,mid,large}), `vector_join_negative_threshold_returns_empty` (caso negativo: τ<0 → conjunto vazio documentado, sem crash na fronteira C). Harness 3-braços (T1 LATERAL-index / T2 naive cross-join+sort O(n·m) / T3 pgvector controle) + dedup self-join com duplicatas plantadas → precisão/recall; aritmética claim-bearing pura (join_recall min surface recall-0, dedup_metrics, verdict honesto por-eixo) unit-testada container-free (16/16 pytest GREEN, ruff clean). **Helper `theodb.vector_join` REJEITADO (D2/ADR-0022):** raw-LATERAL-only — o LATERAL já é o idioma first-class index-served (parsimony rung 1, YAGNI); o SQL dinâmico do helper arriscaria o pushdown (R5) para açúcar puro; zero novo código de produção. **Benchmark medido (droplet, imagem com pgvector, 200×5000, dim 128, k 10, 3 runs):** T1 LATERAL-index p50 **0.452 ms** / join-recall mean **0.9948** (min 0.80) — **2.16× mais rápido que T2 naive O(n·m)** (0.977 ms) e em **paridade de latência com o controle pgvector** (0.42 ms, recall 1.0); dedup e2e **recall 1.0** (20/20 duplicatas plantadas achadas), precisão 0.115 (função do τ). `docs/benchmarks/m63-vector-join.{md,json}`.
- **M63 — plan: vector JOIN (LATERAL-index-scan)** (`.claude/knowledge-base/plans/m63-vector-join-plan.md`): 4 fases/6 tasks TDD (validar LATERAL usa Index Scan via EXPLAIN → join-recall vs GT exato O(n·m) → benchmark 3-braços + dedup e2e → integration). 2 ADRs (LATERAL vs custom join node — Regra 9; helper `theodb.vector_join` só se EXPLAIN provar que preserva o Index Scan, senão raw-LATERAL-only). ADR=0022. Coverage 100%. Gate SHIPPABLE_WITH_CAVEATS (73.6). Honesto: o LATERAL-index já é vector-join first-class; M63 valida+mede+documenta, não constrói mecanismo.
- **M63 — discover: blueprint de vector JOIN** (`.claude/knowledge-base/discoveries/blueprints/m63-vector-join-blueprint.md`): deep research R0 (pgvector issues #812/#713/#703/#645, PG docs LATERAL §7.2.1.5, arXiv:2402.13397 Xling ANN-join, Milvus). Achado (maintainer pgvector @ankane): join column-vs-column NÃO usa índice (Nested Loop O(n·m)); o índice ANN serve **`CROSS JOIN LATERAL (… ORDER BY b.emb <=> a.emb LIMIT k)`** — o mesmo shape index-served do M52. Recomendação (ADR): LATERAL-index-scan, NÃO custom join node (PhD-level, sem ganho, Regra 9). M63 = validar+medir+documentar + helper opcional `theodb.vector_join`. Benchmark: join-recall (mean±std, GT exato) vs cross-join O(n·m) vs pgvector. O LATERAL já é vector-join first-class funcional; falta (fora do M63) push-down em join de topo + amortização de batch.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.52.0]