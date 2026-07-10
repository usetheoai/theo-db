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
- Roadmap v5 "Superioridade vetorial P0 (MEDIDA)" definido (`ROADMAP-v5.md` + seção `# Roadmap v5` em `ROADMAP.md`): fecha o pilar P0 do North Star (`docs/adr/0002`) que segue parcial — superioridade vetorial comprovada por benchmark. Milestones: **M60** (fundação — recall HNSW ≥0.99 a escala, já aberto), **M71** (latência-superior do AM, scan hot-path v2), **M72** (QPS a 1M+ multi-cliente), **M73** (head-to-head MEDIDO vs ScaNN/AlloyDB — o veredito de superioridade), **M74** (CONDICIONAL — quantização SOTA só com lever não-refutado por M57/M59). Measurement-first + honesto (Regra 3/5): cada milestone tem gate executável e ACEITA honest-negative como conclusão; o v5 NÃO promete vencer o ScaNN (~25× gap de QPS medido no M33; M57 SBQ + M59 anisotrópica+AH já honest-negative) — promete o veredito medido de onde o TheoDB está vs o SOTA.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.60.0] - 2026-07-09
### Removed
- **M70 — pgvector e pgvectorscale REMOVIDOS totalmente** (`theodb_rs/src/dtype.rs`, `am/mod.rs`, `theodb_rs.control`, `theodb.control`, `sql/*.sql`, `Dockerfile`): o tipo `vector` do TheoDB agora é **100% own-code** — o pgvector e o pgvectorscale saíram da distribuição (Dockerfile sem o stage pgvectorscale, sem o `make install` do pgvector; **pg_duckdb intocado**). Fecha o roadmap v4 "Independência do pgvector" e o pilar do North Star.

### Changed
- **M70 — tipo `vector` own-code movido para `public.vector` (drop-in) + flip da dependência** (ADR-0029): o tipo próprio (M69) migrou de `theodb.vector` para `public.vector` — `::vector` do usuário e o `FOR TYPE vector` das opclasses do AM resolvem ao tipo own-code SEM mudança de código. **Flip (ADR-0029 D1):** `theodb_rs` vira a BASE da stack (provê o tipo `public.vector` + os AMs ANN + os schemas `theodb`/`ai` via o bloco `theodb_schema_bootstrap`); `theodb_rs.control requires` ZERADO; o umbrella `theodb.control requires` vira `theodb_rs` (antes ambos requeriam o pgvector, o 3º que quebrava o ciclo de dependência). **Migração** de instalações com pgvector via intermediário `real[]` (`docs/ops/pgvector-migration.md`, janela de manutenção — o byte-cast direto do M69 não se aplica ao upgrade por colisão de nome `public.vector`; honestidade Regra 3). **Validado pg17 real SEM pgvector:** 229/230 suíte completa GREEN standalone (a 1 falha é o teste de timing SIMD `pg_cosine_simd_per_candidate_speedup`, flaky sob carga — passa isolado, M70 não tocou `vec.rs`); os pg_tests do AM `set-equal-vs-seqscan` + 15/15 dtype + 13/13 HNSW GREEN sobre `public.vector`; **`CREATE EXTENSION theodb CASCADE` sem pgvector** → extensões `theodb` + `theodb_rs` (zero `vector`/`vectorscale`), `'[1,2,3]'::vector` resolve ao tipo próprio. Councils index-storage: greenfield SHIPPABLE (findings de migração corrigidos). Sem claim de performance (correção/paridade — o dado é o gate de não-regressão de recall). Código ORIGINAL (VectorChord AGPL só estudo). ADR-0029.

## [0.59.0] - 2026-07-09
### Added
- **M69 — tipo vetorial PRÓPRIO own-code `theodb.vector`** (`theodb_rs/src/dtype.rs` (NEW), `lib.rs`, `docs/adr/0028`): tipo `vector` own-code no schema `theodb`, com layout `#[repr(C)]` **byte-idêntico** ao `Vector` do pgvector (`varlena u32 · dim u16 · unused u16 · f32[]`; 8+4·dim bytes) — coexiste com `public.vector` (pgvector) SEM colisão (schemas distintos). I/O text (parse espelha `vector.c`, PostgreSQL License) + **typmod** (parse + enforce via length-coercion cast) + **recv/send binário** (wire big-endian, `unused`==0) + operadores `<->`/`<#>`/`<=>` (reuso dos kernels `vec.rs`) + casts `real[]`/`float8[]`/`text` + **cast binário `WITHOUT FUNCTION` bidirecional com o `vector` do pgvector** (habilita coexistência + a migração grátis do M70). Fundação para remover o pgvector (M70 fará `SET SCHEMA public` ⇒ drop-in). **Validado pg17 real:** 16/16 dtype pg_tests GREEN (paridade `vector_type`/`cast`/`copy` binário + byte-compat dim-variado + typmod + negative-cases + memória sem UAF) + 13/13 HNSW AM GREEN (**não tocou o AM, zero regressão P0**). Código ORIGINAL (VectorChord AGPL só estudo). Sem claim de performance (correção/paridade). Spike ADR-D3 (7/7). ADR-0028.
- Roadmap amended: added M69 Tipo vetorial próprio own-code (coexistindo com pgvector, gated por paridade) + M70 Remover pgvector (e pgvectorscale) totalmente (`/roadmap-feature own-vector-type-drop-pgvector`) — Roadmap v4 "Independência do pgvector"; decisão da fonte de verdade: blueprint SHIPPABLE `.claude/knowledge-base/discoveries/blueprints/own-vector-type-drop-pgvector-blueprint.md` (veredito A, decomposto em 2 milestones).

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.58.0] - 2026-07-09
### Added
- **M68 — observabilidade do query vetorial (`theodb.explain_scan` + `candidates_seen`)** (`theodb_rs/src/ann/scan_core.rs`, `am/hnsw_page.rs`, `am/autotune.rs`, `api.rs`, `docs/ops/vector-scan-diagnostics.md` (NEW)): fecha o pilar de operabilidade do scan ANN (opaco por natureza). **`theodb.explain_scan(index_table, vector_col, query, ef, k)`** — função diagnóstica que retorna, de UM scan real: `index_name`, `ef_effective`, `pages_read`, `candidates_seen`, `latency_us`, `results` (padrão Qdrant `/telemetry`/Milvus — **não** `amexplain`, que não existe no PG17/18). **`candidates_seen`** — tamanho do pool navegado no beam, capturado own-code em `ground_search_nodes` (`visited.len()` antes do drop) e propagado ao thread_local `SCAN_CANDIDATES` (irmão do `SCAN_PAGES_READ` do M67); distingue "grafo caro de navegar" (candidates alto) de "I/O pesado / spill" (pages alto). `theodb.scan_stats` agora retorna 4-tupla (`pages_read, candidates_seen, latency_us, results`); catálogo heap `theodb._index_scan_stats` ganha `sum_candidates`; `theodb.index_scan_stats` expõe `avg_candidates` (pilar (c) do wiring-triad = catálogo consultável, crash-safe M35 — não histograma Prometheus, adiado por YAGNI). REVOKE FROM PUBLIC. **Doc de operação** `docs/ops/vector-scan-diagnostics.md`: playbook recall-baixo/latência-alta + tabela sinal→causa→ação. **pg_tests GREEN** (`explain_scan_shows_index_and_candidates`, `scan_stats_records_real_pages_read` estendido p/ 4-tupla + `sum_candidates>0`). Observabilidade → validado por teste determinístico, **sem benchmark de performance** (nenhum claim "Nx"). ADR-0027.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.57.0] - 2026-07-09
### Added
- **M67 — auto-tune de índices vetoriais (`theodb.recommend_ef` + coletor de stats)** (`theodb_rs/src/am/autotune.rs` (NEW), `am/mod.rs`, `am/hnsw_page.rs`, `api.rs`, `benchmarks/run_m67_autotune.py` (NEW)): **recomendador determinístico** `theodb.recommend_ef(index, vec_col, samples, recall_target, k)` — bisecção monotônica sobre recall(ef) (monotônico, Malkov & Yashunin) contra GT exato amostrado (seqscan), retorna o menor ef que atinge o alvo (ctid como id estável; MAX_EF se inatingível). **Coletor** `theodb.scan_stats(tbl,col,query,ef,k)` — mede o **pages_read REAL** (thread_local que o traverse HNSW bumpa — 1 add in-memory, sem page write) + latência, persiste no catálogo heap `theodb._index_scan_stats` (FORA das páginas do índice — crash-safe, M35); `theodb.index_scan_stats(rel)` lê os agregados. REVOKE FROM PUBLIC. **5 pg_test GREEN** (stack real) + 12 pytest (MAE/RQUT/convergência). **Benchmark (10k sintético) — CONVERGED com nuance honesta:** o recomendador converge na média (recall 0.986 ≥ alvos), MAS (1) corpus fácil demais (baseline ef=64 dá recall 1.0; todos os alvos → ef=10 — não estressa a curva ef; SIFT1M mostraria o scaling), (2) RQUT 12% de cauda (mean-optimal, não tail-safe — v2). **NÃO auto-tune online** (deferido por evidência ADR-0026 — oscilação; SOTA é early-termination acadêmico DARTH/Ada-ef). **amcostestimate:** fórmula M48 (f(ef)) retida + auditabilidade via scan_stats; calibração-in-planning DEFERIDA por risco EC-3 (SPI no planning abortaria TODO o planejamento). `docs/benchmarks/m67-autotune.{md,json}`, ADR-0026.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.56.0]