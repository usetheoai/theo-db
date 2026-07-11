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
- M82 (pg_scann fase 7 — veredito final): head-to-head MEDIDO do índice v4 IVF-AQ+AH como Access Method, dentro do
  Postgres, a SIFT1M completo (GT oficial válido a 1M) vs a baseline f32-IVF own-code na mesma tabela (rigor A/B
  same-data M46). Artefatos `docs/benchmarks/m82-pgscann-headtohead.{md,json}` + veredito `docs/adr/0037-m82-am-ivf-aq-measured-verdict.md`. **Achado honesto:** o índice v4 é funcionalmente correto (recall byte-idêntico ao f32-IVF exato — AH pruning lossless), mas **não entrega ganho de QPS** no AM (78.5 QPS @ recall 0.985, classe f32-IVF, ~24× abaixo do ScaNN) — os 5-7× in-memory do M75 são mascarados pelo custo I/O+probe do AM. Confirma e estende o veredito M73 (ADR-0035). Fecha o track pg_scann (M75→M82) e o Roadmap v6.

### Changed
- M82: treino do codebook AVQ no `ambuild` passa a amostrar deterministicamente (stride) até 50k vetores antes de
  encodar TODOS — torna o `CREATE INDEX` do índice v4 tratável a 1M+ (o treino ingênuo era super-linear, o blocker
  do M75). Recall inalterado (medido byte-idêntico ao f32-IVF exato a 1M).

### Deprecated

### Removed

### Fixed

### Security

## [0.70.0] - 2026-07-11

### Added
- **pg_scann M81 — lifecycle transacional do índice IVF-AQ v4:** o `scan_ivf_aq` (`am/scan.rs`) agora **folda a região pending** (rows INSERTed pós-build, f32, scored exatamente) — antes eram silenciosamente perdidas; `main_index_pages`/`read_pending` ficaram v4-aware (`am/page.rs`). O VACUUM é **safe no-op** no índice v4 (`vacuum_rebuild` gate em `am/build.rs` — o rebuild f32 rejeitaria/corromperia; correção holds via fold do pending + MVCC re-check; compactação v4 = REINDEX, follow-up documentado). `amcostestimate` v4-aware (`am/cost.rs`). Provado: `ivf_aq_v4_folds_post_build_inserts` (INSERT pós-build aparece no scan) + **236 pg_tests GREEN, zero regressão**. Fecha ROADMAP M81.

## [0.69.0] - 2026-07-11

### Added
- **pg_scann M77+M78+M79+M80 — IVF-AQ+batched-AH no AM `theodb_ivfflat` (a capacidade que o M75 provou, agora em produção):** `CREATE INDEX ... USING theodb_ivfflat WITH (pq_subspaces=M)` persiste um layout **v4** (`am/page.rs::write_ivf_aq`) com os códigos AVQ 4-bit em blocks32 transpostos por inverted list (+ f32 para rerank + codebook), e o scan (`am/scan.rs::scan_ivf_aq`) faz probe → **`ah_score_block` batched (FastScan pshufb)** → rerank f32 exato — o scan 2-estágios provado no M75 (~5-7× QPS vs f32 a recall casado), lendo de página O(probes). Isolado do path v3 f32 (byte-idêntico, intocado). Provado: `ambuild_ivf_pq_subspaces_v4_scans_high_recall` (recall@10 ≥ 0.8 vs seqscan exato) + **235 pg_tests GREEN, zero regressão**. Fecha ROADMAP M77-M80. Honesto: benchmark recall×QPS a SIFT1M = M82 (exige otimizar o AVQ train super-linear); lifecycle aminsert/VACUUM do índice v4 = M81.

## [0.68.0] - 2026-07-11

### Added
- M76 (pg_scann Fase 1, AM scaffold) fechado por **Rule 9**: o AM `theodb_ivfflat` existente (registro IndexAmRoutine, ambuild, busca exata IVF, metapage+page+WAL GenericXLog, opclass, set-equal-vs-seqscan tests ~134 GREEN) **já é o scaffold** — o pg_scann ESTENDE o IVF AM (modo AQ+batched-AH), não cria AM novo. **Re-escopo honesto de M77-M82** (memória `pgscann-am-mostly-exists`): o delta real colapsa para (M77) layout block32 dos códigos AQ nas IVF-list-pages + (M79) o `scan_ivf_structured` usar o `ah_score_block` batched (o scan que o M75 provou ~5-7×); o resto (AVQ, aminsert, vacuum, cost, rerank-pool) já existe. Fecha ROADMAP M76.

## [0.67.0] - 2026-07-11

### Added
- M75 (pg_scann Fase 0, spike measurement-first): índice IVF-AQ+AH in-memory own-code (`theodb_rs/src/ann/ivf_aqah.rs`) — compõe (Rule 9) a partição IVF + o AVQ (`am/aq.rs`) + o kernel batched AH-LUT já existente (`vec/ah.rs`, layout transposed block32) num scan 2-estágios probe→AH→rerank. Pipeline provado correto (3 pg_tests GREEN). **Veredito D3 = GO (medido, SIFT real):** IVF-AQ+AH entrega **~5-7× o QPS do full-precision a recall casado** (captura ~5-7× dos ~25× do gap ScaNN M33) — 1º lever own-code que move o gap; reabre o eixo de QPS. Caveat honesto: medido a n=5000 (AVQ train naive super-linear bloqueia 1M in-session → otimização é M77). `docs/benchmarks/m75-ivf-aqah-spike.{md,json}`. Gate ABERTO: M76-M82 arrancam.
- DISCOVER cycle + ROADMAP v6 para o **pg_scann** (índice IVF-AQ+AH nativo — ScaNN own-code): blueprint web-grounded SHIPPABLE_WITH_CAVEATS (`.claude/knowledge-base/discoveries/blueprints/pg-scann-am-blueprint.md`, R0: AVQ paper + AlloyDB + arXiv:2603.23710 SIGMOD 2026) + 8 milestones M75-M82 (Fase 0 spike-gate D3 + 7 fases: AM scaffold → layout contíguo → AVQ → AH-scan → rerank → lifecycle → planner). Tese não-refutada (M59): AQ+AH sobre carrier IVF batch-scan; measurement-first (M75 é o gate, honest-negative é saída válida).

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.66.0] - 2026-07-10

### Added
- Veredito do lever condicional de quantização (M74, ADR-0036): RaBitQ é o lever viável não-refutado (core vendorizado, ADR-0032; spike D3 1M medido) — mas o ganho é **memória/billion-scale** (5.3MB @ 98.4%), NÃO superioridade de QPS. Decisão honesta (anti-sunk-cost/D3): não implementar o AM completo agora; full IVF-RaBitQ = follow-up gated por demanda billion-scale. Fecha ROADMAP M74 → **ROADMAP v5 (pilar vetorial P0) COMPLETO**.

## [0.65.0] - 2026-07-10

### Added
- Veredito MEDIDO do North Star vetorial (M73, ADR-0035 + `docs/benchmarks/m73-headtohead-verdict.{md,json}`): paridade own-code de recall classe-pgvector ALCANÇADA + throughput multi-cliente competitivo-a-superior (M72) + superioridade de QPS vs ScaNN/AlloyDB MEDIDA como não-alcançável por extensão PG permissiva (gap ~25-44× @ 0.99 é de paradigma). Estado medido final propagado ao CLAUDE.md North Star. Fecha ROADMAP M73.

## [0.64.0] - 2026-07-10

### Added
- Benchmark M72: QPS multi-cliente a 1M×128d (8 clientes concorrentes, ≥3 runs) — theodb_hnsw competitivo-a-superior vs pgvector a recall casado no regime clusterizado (+11% QPS @ ~0.91, build 3× mais rápido), com caveat honesto de corpus gaussian-mixture vs SIFT1M literal (`docs/benchmarks/m72-qps-multiclient.md`, `benchmarks/run_m72_multiclient.py`). Fecha ROADMAP M72.

## [0.63.0] - 2026-07-10

### Added
- **Veredito medido do pilar vetorial P0 + proposta de reposicionamento do North Star** (`docs/benchmarks/vector-pillar-verdict-2026-07.md` (NEW), `docs/benchmarks/rabitq-spike/rabitq_ivf_mstg_1m768d.log` (NEW), `docs/adr/0033-north-star-reposition-proposal.md` (NEW, PROPOSED)): fechamento da investigação de superioridade vetorial. Gap 2 (QPS) atacado com o SOTA permissivo (RaBitQ vendorizado, ADR-0032) e **medido a 1M×768d** (spike D3): MSTG-RaBitQ-mem = 8.2ms @ 98.4% recall (competitivo com full-precision ~10-15ms, **NÃO os 25× do ScaNN**); variante disk = 98.4% @ **5.3 MB residentes** (o ganho real do RaBitQ é MEMÓRIA, não QPS). Conclusão honesta (Regra 3/5): **superioridade de QPS vetorial sobre AlloyDB/ScaNN NÃO é alcançável como extensão Postgres permissiva** (o 25× do ScaNN é do AH-LUT anisotrópico + não pagar o imposto PG). Alvos honestos: paridade classe-pgvector (Gap 1, fix do select_from) + RaBitQ como feature de **memória/billion-scale** + AI-native/HTAP. Proposta ADR-0033 (requer assinatura do owner) reposiciona o North Star. Prior-art R0: rabitq-rs/RaBitQ-Library/LanceDB/Qdrant (permissivos, estudo+vendor); VectorChord/srvdb (AGPL, só estudo de design).
- **Vendorizado o CORE do `rabitq-rs` (Apache-2.0) para o futuro índice IVF-RaBitQ** (`theodb_rs/src/rabitq/vendor/` (NEW): `quantizer.rs`, `rotation.rs`, `fastscan.rs`, `fastscan_kernel.rs`, `simd.rs`, `math.rs` + `LICENSE` + `VENDORED.md`; `docs/adr/0032-vendor-rabitq-rs-core.md` (NEW)): ataque ao Gap 2 do pilar vetorial (superioridade de QPS vs ScaNN/AlloyDB). RaBitQ (arXiv:2405.12497, quantização 1-bit training-free com bound de erro provado; canônica `VectorDB-NTU/RaBitQ-Library` Apache-2.0, adotada por Milvus/Faiss/Elasticsearch) é o lever **não-refutado** (M57 SBQ + M59 anisotrópico falharam no carrier HNSW; o carrier certo é IVF, que já temos em `ann/ivf.rs`). Vendorizado o core do algoritmo (commit upstream `10b9a4e`), NÃO a camada de storage (substituída pela nossa IVF page-native + WAL). Regra 9 (não reinventar) + D1 (Apache→Apache, LICENSE+atribuição preservados). Arquivos inertes até o wiring (implement); gate D3 (spike local de recall/velocidade) antes do AM completo. ADR-0032.

### Changed
- **HNSW build: `extendCandidates` (default ON) fecha a degradação de recall por escala — f32 0.974→0.990, SBQ 0.986→0.994 a 500k×768d** (`theodb_rs/src/ann/hnsw.rs`, `ann/hnsw_parallel.rs`, `docs/adr/0034-hnsw-extend-candidates-navigability.md` (NEW), `docs/benchmarks/gap1-extend-candidates.md` (NEW)): o Gap 1 (navegabilidade) foi localizado por **método white-box** (analisador de estrutura do grafo, local — conectividade perfeita mas 100% das misses são ROTEAMENTO, hop-distance cresce com a escala) e a causa é paper-grounded: faltava o `extendCandidates` do HNSW (Malkov-Yashunin — recomendado p/ dados clusterizados, nosso regime de 256 clusters). Fix: estende o pool de candidatos com os vizinhos-dos-vizinhos antes do `select_from`, nos dois caminhos de build. **Medido a 500k×768d:** recall f32 0.974→**0.990** (curva inteira +~5pt; agora alcança ≥0.99, antes platôava em 0.974), SBQ 0.986→**0.994** = paridade de valor de recall com pgvector (0.994). 63/63 pg_tests GREEN. **Honesto (Regra 3):** NÃO é paridade de FRONTIER — pgvector ainda tem recall maior no mesmo ef (iso-recall ~1.8× mais lento); o fix sobe o teto, não iguala a eficiência recall-por-ef (follow-up: `select_from`/`SelectNeighbors` exato). Build ~2-3× mais lento (trade-off recall>build-speed) — opt-out via `THEODB_HNSW_EXTEND_CANDIDATES=0`. ADR-0034.

### Deprecated

### Removed

### Fixed

### Security

## [0.62.0] - 2026-07-10

### Added
- **P0 bloqueador-raiz — 2 achados decisivos que reformulam o gap de recall** (`docs/benchmarks/p0-vector-superiority-root-blocker.md`, `docs/benchmarks/m60-raw/m60_efc_{sweep_100k,seq_vs_parallel_500k}768d.json`, knob `THEODB_HNSW_EF_CONSTRUCTION` em `theodb_rs/src/am/build.rs`): experimento efc×modo-de-build em droplet — (1) o "gap" é **degradação por ESCALA**, não defeito fixo: theodb recall@10 = **0.998 a 100k×768d** (excelente, ≈/> pgvector) e só cai a 0.974 a 500k; (2) a hipótese do **overwrite paralelo é REFUTADA** (7º lever): sequential 0.974 ≈ parallel 0.972 a 500k — o build sequencial (sem overwrite) tem o MESMO plateau. A degradação é inerente ao algoritmo de build a escala, nos dois modos. Notícia de produto: para ≤100k vetores o vetor do theodb está em paridade/superioridade com pgvector. Knob `THEODB_HNSW_EF_CONSTRUCTION` (benchmark-only, default 64 — comportamento inalterado; espelha `THEODB_HNSW_PARALLEL_THRESHOLD`).
- **M71 (discover) — blueprint de latência iso-recall do scan** (`.claude/knowledge-base/discoveries/blueprints/m71-scan-latency-blueprint.md`): diagnóstico dual-source (theodb↔pgvector) + SOTA (PANORAMA arXiv:2510.00566, Faiss FastScan, KScaNN arXiv:2511.03298) do gap de latência a iso-recall (theodb precisa ~5× o `ef` do pgvector p/ o mesmo recall). Levers ranqueados: (1) qualidade de grafo (multi-entry build já +29% QPS medido), (2) kernel de distância com early-out por limiar (onde theodb pode SUPERAR pgvector), (3) SIMD multi-accumulator + hoist da norma da query no cosseno. Rigor iso-recall (não QPS-sweep). Implement+benchmark exigem droplet.

### Changed
- **M71 CONCLUÍDO — melhoria de latência do AM medida (multi-entry build), DoD reenquadrada (ADR-0031)** (`theodb_rs/src/ann/hnsw.rs`, `ann/hnsw_parallel.rs`, `docs/adr/0031-m71-latency-improvement-not-superiority.md` (NEW), `docs/benchmarks/m71-scan-latency.md` (NEW), `ROADMAP.md § M71` [x]): o build do HNSW próprio carrega o conjunto completo `W` como entry-set entre camadas (Malkov-Yashunin Alg.1 `ep←W` / pgvector) em vez de colapsar a um único nó → grafo melhor-conectado → **+29% QPS a 500k×768d, recall-neutral (0.972 vs 0.974), 63/63 pg_tests GREEN**. DoD reenquadrada (measurement-first como o M60): superioridade iso-recall gateada na navegabilidade do grafo (theodb precisa ~2× o `ef` do pgvector a 100k, ~5× a 500k — mesma raiz do M60) → M71 entrega a **melhoria medida** e documenta o gap iso-recall (pgvector 2.13ms vs theodb 3.16ms a recall 0.996/100k). Cortes de custo/candidato (kernel bounded, norm-hoist) = follow-up. Sem claim de superioridade. ADR-0031.

### Deprecated

### Removed

### Fixed

### Security

## [0.61.0] - 2026-07-10

### Added
- **M60 — medição decisiva do recall do HNSW próprio vs pgvector a 500k×768d** (`docs/benchmarks/m60-hnsw-recall.md`, `docs/benchmarks/m60-raw/`, `benchmarks/run_m60_recall.py` (NEW), `benchmarks/run_m60_pgvector_control.py` (NEW), blueprint `m60-hnsw-recall-quality`): head-to-head no MESMO corpus gaussian-mixture (droplet c-8, pg17) — pgvector best recall@10 = **0.988**, theodb_hnsw f32 = 0.974, theodb SBQ (over_fetch=32) = **0.986**. Dois achados (Regra 3): (1) **o gate 0.99 é artefato do dado** — o próprio pgvector só chega a 0.988 (256 clusters apertados em 768d → teto de recall@10 < 0.99 para índices HNSW); a DoD do M60 deve virar **paridade-pgvector**, não 0.99 absoluto; (2) existe um gap real **~1.4pt** (f32 vs pgvector), com o SBQ já em quase-paridade. Duas hipóteses de fix do discover (descida de build por beam ef=1; multi-entry `ep←W`) foram **implementadas e REFUTADAS por medição** a 500k×768d (no-op no recall) — revertidas; 5 levers refutados no total. Fechamento do M60 via reenquadramento de DoD → ver a entrada em `Changed` (ADR-0030). O grafo multi-entry rendeu +29% de QPS a recall igual (achado registrado para o M71).
- Roadmap v5 "Superioridade vetorial P0 (MEDIDA)" definido (`ROADMAP-v5.md` + seção `# Roadmap v5` em `ROADMAP.md`): fecha o pilar P0 do North Star (`docs/adr/0002`) que segue parcial — superioridade vetorial comprovada por benchmark. Milestones: **M60** (fundação — recall HNSW ≥0.99 a escala, já aberto), **M71** (latência-superior do AM, scan hot-path v2), **M72** (QPS a 1M+ multi-cliente), **M73** (head-to-head MEDIDO vs ScaNN/AlloyDB — o veredito de superioridade), **M74** (CONDICIONAL — quantização SOTA só com lever não-refutado por M57/M59). Measurement-first + honesto (Regra 3/5): cada milestone tem gate executável e ACEITA honest-negative como conclusão; o v5 NÃO promete vencer o ScaNN (~25× gap de QPS medido no M33; M57 SBQ + M59 anisotrópica+AH já honest-negative) — promete o veredito medido de onde o TheoDB está vs o SOTA.

### Changed
- **M60 CONCLUÍDO — DoD de recall reenquadrada para PARIDADE-pgvector (ADR-0030), fechado pelo caminho SBQ** (`docs/adr/0030-m60-recall-parity-not-absolute-099.md` (NEW), `ROADMAP.md § M60` [x]): a medição head-to-head a 500k×768d provou que o gate `recall@10 ≥0.99` é **artefato do dado** — o próprio pgvector só chega a **0.988** (256 clusters apertados em 768d ⇒ teto de recall@10 < 0.99 para índices HNSW). A DoD passa a **paridade-pgvector** (measurement-first, North Star ADR-0002). **Paridade atingida pelo SBQ: 0.986 ≈ 0.988** (GT exato). Gap do f32 puro (0.974, ~1.4pt) = **follow-up autorizado** (opção B) — resistiu a 5 levers refutados por medição. Sem claim de superioridade (paridade de recall; latência/QPS = M71). ADR-0030.

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