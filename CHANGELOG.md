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
- Roadmap amended: added M94 per-scan membership scoping — swap-discipline around each vector-child pull (thread-local registry keyed by node + save/restore active slot) so multiple filtered vector scans in one plan (UNION/self-join/Append) each see their OWN membership; replaces the M93 fail-loud guard with real support (M94)

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.80.1] - 2026-07-13

### Fixed
- Integrity: ship the v5 selectivity-adaptive probing loop + the Pareto-frontier benchmark harness that the M92/M93 263-test suite and the SIFT `INLINE-dominates-POST` measurement actually ran against — they were left uncommitted when v0.80.0 was cut, so the released source now matches the benchmark artifact (`docs/benchmarks/m92-arbitrary-where.{md,json}`). No behavior change at the benchmarked 1%/5% selectivity; the v5 adaptive only materially affects ultra-selective (<0.1%) recall (M92)

## [0.80.0] - 2026-07-13

### Added
- **M92/M93 (arbitrary-WHERE filtered vector search via a Custom Scan Provider, veredito `GO` — experimental, OFF by default behind `theodb.enable_vecfilter`): push an arbitrary scalar `WHERE` INTO the IVF-AQ vector scan.** A hand-rolled 2-child Custom Scan node intercepts `WHERE <scalar> ORDER BY e <-> q LIMIT k`, runs the planner's native bitmap sub-plan over the scalar column (Rule 9 — reuses BitmapAnd/Or), materializes a lossy-safe TID membership, and the vector scan's Stage-1 skips non-members inline (+ M91 adaptive probing); the vector child's own qpqual Filter is the MVCC recheck of the lossy/pending over-admits. **MEASURED (DO 8-vCPU Xeon Gold 6548N, SIFT1M, real neighbors): INLINE dominates the native post-filter on BOTH recall AND QPS — 1% sel recall 0.953 @ 266 QPS vs POST 0.673 @ 21 QPS (+0.28 recall, ~12× QPS); 5% sel 0.915 @ 126 vs 0.593 @ 92 (+0.32, ~1.4×)** (`docs/benchmarks/m92-arbitrary-where.{md,json}`). Correctness proven byte-identical to exact seqscan on a non-label column (pending + lossy rechecked); the inline skip engages on both the v5 plain-vector and v7 label layouts. **263 tests GREEN, GUC-off path byte-identical.** Concurrent filtered vector scans in one plan (UNION/self-join) **fail loud** (per-backend membership; per-scan scoping is a follow-up) — never silently wrong (Rule 8). Sign-off council-rust-pgrx + council-index-storage + council-benchmark (1 BLOCKER + 3 HIGH found in review and fixed). NOT a QPS-superiority claim vs ScaNN/AlloyDB (teto M73/M82) — the AlloyDB "inline filtering" tier ③ mechanism in a permissive OSS Postgres extension. (M92, M93)
- Roadmap amended: added M92 arbitrary-WHERE Custom Scan Provider + M93 Custom Scan node integration (`/roadmap-feature`) (M92, M93)

## [0.79.0] - 2026-07-13

### Added
- Selectivity-adaptive probing on the v7 INLINE filtered scan (M91): a selective label filter automatically probes more IVF lists until the matching-candidate pool fills, recovering filtered recall@10 from 0.741 to ~1.0 at 0.01% selectivity on SIFT1M while leaving loose/unfiltered scans byte-identical. Self-tuning on the measured match count — no new GUC, no on-disk format change (no REINDEX). Opt-in `THEODB_SCAN_PROFILE=1` now reports `probes_effective` vs `probes_default` (M91)

## [0.78.0] - 2026-07-12

### Added
- **M90 (inline label filter, veredito `GO`): filtro de label empurrado PARA DENTRO da travessia do IVF-AQ** (Approach A — scan-key/label-in-index, o mecanismo do pgvectorscale, código próprio). Um índice `theodb_ivfflat (e, lbl)` com coluna `smallint[]` faz o planner empurrar `lbl && '{…}'` como Index Cond; o novo layout **v7** co-localiza o label nas code-pages e a Stage-1 PULA candidatos sem-overlap antes do rerank (`xs_recheck` garante correção). **MEDIDO (DO c-8, 500k, ~1% seletividade): recall@10 1.00 (inline v7) vs 0.52 (M87 post-filter v5) — delta +0.48 + ~19× QPS** (`docs/benchmarks/m90-inline-filter.{md,json}`, ADR `0040`). 253 pg_tests GREEN (250 + 3 v7: inline/vacuum/pending), zero regressão; vetor-only e v5/v6 sem-label byte-idênticos (v7 opt-in na 2ª coluna). Honesto: só a coluna de label + `&&`, format v7 + REINDEX p/ usar labels; NÃO é claim de QPS-superior vs ScaNN/AlloyDB (teto M73/M82); o arbitrary-WHERE inline (Custom Scan) é o M91. Sign-off council-index-storage + rust-pgrx + benchmark (2 blockers de correção achados no review e corrigidos: VACUUM no-op v7, xs_recheck no pending). (M90)
- Roadmap amended: added M91 adaptive filter strategy (pre/inline/post pela cardinalidade do bitmap — a peça adaptive AM-local; gated M90) (`/roadmap-feature adaptive-filter-strategy`) (M91)
- Roadmap amended: added M90 inline filter pushdown (bitmap-in-traversal via Custom Scan — fecha o inline filtering vs AlloyDB; gated M87/M89) (`/roadmap-feature inline-filter-pushdown`) (M90)

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.77.0] - 2026-07-12

### Added
- **M89 (build escalável — ambuild streaming, veredito `DOD_MET`): o build do índice vetorial agora tem memória limitada por-lista.** Fecha o teto de memória descoberto no M88 (ADR-0038): o `ambuild` do `theodb_ivfflat` picava ~4× o dataset base em RAM → OOM a 30M. Duas mudanças byte-idênticas ao formato on-disk (sem REINDEX): (1) `build_owned` **move** o corpus p/ o índice (sem clonar); (2) os writers v5/v6 leem os vetores por referência e **escrevem cada lista incrementalmente**, liberando o blob f32 por-lista (elimina o clone `list_entries()` + os buffers `enc_vec`/`items`). **MEDIDO (DO m-8vcpu-64gb, 30M×128 = 15.4 GB base):** o build de 30M agora **completa** num box de 64 GB com pico **1.28× (v5) / 1.50× (v6)** base — o build antigo OOMou a **4.21×/64.7 GB** (reproduz o M88). 250 pg_tests GREEN, zero regressão. Honesto: NÃO é `O(maintenance_work_mem)` (o pico ainda tem a cópia 1× `idx.vectors`) → 100M+ ainda não cabe em RAM commodity; o streaming via `tuplesort` dos vetores é o follow-up. `docs/benchmarks/m89-ambuild-streaming.{md,json}`, ADR `0039`. Sign-off council-index-storage + council-rust-pgrx + council-benchmark. (M89)
- Roadmap amended: added M89 ambuild streaming (flush incremental via `tuplesort` nativo — derruba o teto de memória de build ~4×→~1× base descoberto no M88; gated M88) (`/roadmap-feature ambuild-streaming`) (M89)

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.76.0] - 2026-07-12

### Added
- **M88 (Roadmap v7 — veredito terminal da track storage-separation, `SIZE_CONFIRMED / OUT_OF_RAM_QPS_INCONCLUSIVE`).** A medição terminal da separação de armazenamento SQ8 (v6) vs f32 (v5) no regime out-of-RAM. **Medido a 16M** (DO m-8vcpu-64gb, sign-off council-benchmark): índice v6/SQ8 **3.52× menor** que v5/f32 (confirma o 3.5× do M85 a 16× a escala); **+21% cold-QPS a probes=32** (direcional, limite inferior). **Honesto:** o DoD ≥100M **NÃO foi atingido** — o ambuild pica ~4× o base em RAM (2 OOM-kills medidos a 30M: 47 GB, 64 GB anon-rss num box de 62 GB usáveis), 16M foi o maior build viável; a recall (0.291) é degenerada por dados sintéticos tie-saturados (SIFT1M real deu 0.98 no mesmo código, M84). Crossover QPS out-of-RAM fica direcional-não-provado; superioridade sobre ScaNN/AlloyDB **não é reivindicada** (teto de paradigma M73/M82 permanece). `docs/benchmarks/m88-billion-scale-verdict.{md,json}`, ADR `0038` (estende `0037`). Follow-up recomendado: ambuild streaming (derruba o teto ~4×-base) + dados bilhão-scale reais. (M88)
- **M88 Phase 1 — build IVF escalável.** kmeans-train sampling (subsample determinístico por stride, capado em `KMEANS_TRAIN_SAMPLE=1.1M`) + parallel full-N assignment (`assign_all_parallel`, `std::thread::scope`) — ataca o O(N·k·d) que era o gargalo real a 100M+ (custo de kmeans fixo ~1M-scale). **Byte-idêntico a ≤1M** (todos os testes + benchmarks 1M inalterados); **249 pg_tests GREEN**. Melhoria de produto (build escalável), não só p/ o M88. (M88)

### Changed

### Deprecated

### Removed

### Fixed
- **M87 — teste de regressão do filtered ANN commitado.** O `filtered_ann_v5_iterative_preserves_recall` (parte dos 248 pg_tests GREEN reportados no M87, validado no run do M87) ficou uncommitted no release v0.75.0; agora está no tree. (M87)

### Security

## [0.75.0] - 2026-07-12

### Added
- **M87 (Roadmap v7 — filtered ANN + planner, veredito GO): iterative scan para TODO IVF (v3/v4/v5/v6).** O iterative do M52 era HNSW-only, então um `WHERE` seletivo COLAPSAVA o recall no IVF (os candidatos dos primeiros probes eram filtrados, o AM retornava false). Agora os scans IVF retornam `Vec` + recebem `probes`/`rerank_pool` como param, e o re-search iterativo cresce **probes** (alcança listas não-probed) E o **rerank pool** até emitir `max_scan_tuples` tids distintos (recall preservado); dedup-by-tid via o `emitted` HashSet do `amgettuple`. `amcostestimate` já era v5/v6-aware. **Medido a SIFT1M:** filtered recall@10 **0.894 @ 10% sel, 0.942 @ 30%** (sem o fix colapsaria); EXPLAIN confirma `Index Scan` para a query filtrada ordenada. `docs/benchmarks/m87-filtered-ann.{md,json}`. **248 pg_tests GREEN (247 + 1 M87), zero regressão.** Classe pgvector-relaxed_order; NÃO é o inline/adaptive filtering do AlloyDB (gap de paradigma). Fecha o escopo M85-M87.

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.74.0] - 2026-07-12

### Added
- **M86 (Roadmap v7 — SOAR spill, veredito HONEST-NEGATIVE no QPS SIFT1M): atribuição SOAR** (Sun et al. NeurIPS 2023, arXiv:2404.00774) atrás de `WITH (soar_lambda=N)` — cada vetor é spilled p/ uma 2ª lista escolhida pela loss de resíduo ortogonal-amplificado, então uma query com MENOS probes ainda o encontra. `ivf.rs::with_soar_spill` (~40 LoC), reloption `soar_lambda`; dedup-by-tid reusa o `emitted` HashSet do `amgettuple` (sem mudança de scan). **Medido a SIFT1M (A/B vs no-SOAR):** o lever centroid-probe é REAL (recall +0.12 a probes=4, +0.06 a probes=8), mas **NÃO dá ganho de QPS** (0.66-0.80× em todo ponto) — o bind do SIFT1M é o read da Fase 2 (M85), não o nº de probes; e a impl mínima dobrou o índice (f32 duplicado no layout v5 per-list). `docs/benchmarks/m86-soar-spill.{md,json}`. **247 pg_tests GREEN (246 + 1 SOAR), zero regressão.** Opt-in (default 0=off); veredito honest-negative no SIFT1M (o ganho projeta-se a bilhão-scale/M88). NÃO vence o ScaNN-biblioteca (M73/ADR-0035).

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.73.0] - 2026-07-11

### Added
- **M85 (Roadmap v7 — SQ8 refine tier, veredito GO memory-win): índice IVF-AQ v6 SQ8-REFINE** atrás de `WITH (separate_storage=1, refine=1)` — o rerank da Fase 2 lê códigos SQ8 (`dim` B/vec, 128B) em vez de f32 (512B). Novo quantizador `sq8.rs` (~90 LoC, sem lib — FAISS QT_8bit per-dim min/max, asymmetric decode-then-metric); layout v6 (`write_ivf_aq_split_sq8`/`read_ivf_aq_meta_split_sq8`/`read_sq8_at`/`ivf_is_v6`, reloption `refine`, cost/vacuum/pending v6-aware). **Medido a SIFT1M (A/B vs v5 f32): índice 3.5× MENOR (153 MB vs 528 MB) a ε≤2% de recall** (`docs/benchmarks/m85-sq8-refine.{md,json}`). **246 pg_tests GREEN (238 + 6 sq8 + 2 v6), zero regressão.** Honesto: o QPS-a-recall-casado é flat-to-marginal em warm-cache 1M (o decode SQ8 + a perda de recall compensam o ganho de I/O — caveat da pesquisa); o ganho de QPS/I/O compõe a bilhão-scale (M88, onde o índice 3.5× menor cabe em RAM e o f32 não). Perfil AlloyDB-SQ8-default; opt-in (v5 f32 exato continua default).

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [0.72.0] - 2026-07-11

### Added
- **M84 (Roadmap v7 — confirmação high-recall, veredito GO): o layout v5 storage-separated MANTÉM a vantagem a alta-recall.** Medido a SIFT1M (A/B same-data): frente de Pareto v5 vs v4 — recall 0.98 → **8.7×**, recall 0.998 → **5.0×**, recall 0.9985 → **8.1×**; todo ponto high-recall vence ≥3× (`docs/benchmarks/m84-recall-confirmation.{md,json}`). Tradeoff honesto: pool maior → mais random-reads f32 na Fase 2 → vantagem estreita no frontier extremo (motiva o M85 SQ8). recall v5==v4 lossless.

- **M83 (Roadmap v7 fase 0 — spike D3 GATE, veredito GO): índice IVF-AQ v5 STORAGE-SEPARATED** atrás de `WITH (separate_storage=1)` — os códigos AQ e os vetores f32 vivem em cadeias de páginas DISTINTAS, então o scan lê só os códigos compactos na Fase 1 (poda AH) e faz random-read do f32 só dos sobreviventes do rerank na Fase 2 (a alavanca que o ADR-0037/M82 nomeou). Novo `write_ivf_aq_split`/`read_ivf_aq_meta_split`/`read_vec_at` (`am/page.rs`), `scan_ivf_aq_split` (`am/scan.rs`), reloption `separate_storage` (`am/options.rs`); `main_index_pages`/VACUUM-gate/`amcostestimate` v5-aware. **Medido a SIFT1M (A/B same-data vs v4 interleaved): 2.7×–11.8× mais QPS a recall CASADO (6.2× @ probes=32), 3–14× menos buffer-accesses** (`docs/benchmarks/m83-split-storage-spike.{md,json}`). **238 pg_tests GREEN (236 + 2 v5), zero regressão; recall v5==v4 byte-idêntico (lossless).** Veredito GATE = **GO** para M84 (layout v5 produção). Caveats honestos: recall-teto ~0.80 deste run (rerank pool fixo em 64, investigação M84); ganho warm-cache é lower bound (bilhão-scale compõe, M88). NÃO vence o ScaNN-biblioteca (imposto de paradigma permanece, M73/ADR-0035).
- Deep research web-grounded (R0) do caminho **storage-separated ScaNN-fidelity** (a alavanca não-testada do ADR-0037): `docs/research/scann-storage-separation-2026-07.md`. Convergência de 4 SOTA (FAISS FastScan, AlloyDB ScaNN, VectorChord, pgvectorscale) — todos separam fisicamente códigos↔vetores brutos. Reformulação honesta do alvo (arXiv:2603.23710 SIGMOD 2026: 84.4% do tempo do ScaNN-in-PG é overhead de sistema; teto AlloyDB = ~4× sobre pgvector HNSW): meta ACHIEVABLE = classe AlloyDB-in-Postgres (~4–6× recuperável), jamais vencer o ScaNN-biblioteca. Roadmap v7 (M83 spike D3 gate → M84 layout v5 → M85 SQ8 refine → M86 SOAR → M87 filtered+planner → M88 bilhão-scale) adicionado ao `ROADMAP.md`.

### Changed

### Deprecated

### Removed

### Fixed
- **M84 — rerank pool do scan AQ era um no-op latente:** `over_fetch().max(64)` ficava SEMPRE em 64 (over_fetch≤64, o `.max(64)` sempre vencia), então `theodb_hnsw.over_fetch` nunca alargava o pool de rerank AQ — a causa da recall-teto ~0.80 do M83. Corrigido para `64 * over_fetch()` (`am/scan.rs`, ambos os scans AQ v4/v5); default (over_fetch=1) inalterado em 64; over_fetch=8/32 → pool 512/2048 → recall sobe a 0.98/0.998. 238 pg_tests GREEN, zero regressão.

### Security

## [0.71.0] - 2026-07-11

### Added
- M82 (pg_scann fase 7 — veredito final): head-to-head MEDIDO do índice v4 IVF-AQ+AH como Access Method, dentro do
  Postgres, a SIFT1M completo (GT oficial válido a 1M) vs a baseline f32-IVF own-code na mesma tabela (rigor A/B
  same-data M46). Artefatos `docs/benchmarks/m82-pgscann-headtohead.{md,json}` + veredito `docs/adr/0037-m82-am-ivf-aq-measured-verdict.md`. **Achado honesto:** o índice v4 é funcionalmente correto (recall byte-idêntico ao f32-IVF exato — AH pruning lossless), mas **não entrega ganho de QPS** no AM (78.5 QPS @ recall 0.985, classe f32-IVF, ~24× abaixo do ScaNN) — os 5-7× in-memory do M75 são mascarados pelo custo I/O+probe do AM. Confirma e estende o veredito M73 (ADR-0035). Fecha o track pg_scann (M75→M82) e o Roadmap v6.

### Changed
- M82: treino do codebook AVQ no `ambuild` passa a amostrar deterministicamente (stride) até 50k vetores antes de
  encodar TODOS — torna o `CREATE INDEX` do índice v4 tratável a 1M+ (o treino ingênuo era super-linear, o blocker
  do M75). Recall inalterado (medido byte-idêntico ao f32-IVF exato a 1M).

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