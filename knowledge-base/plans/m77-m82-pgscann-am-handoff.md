---
slug: m77-m82-pgscann-am
milestone_id: M77
created_at: 2026-07-11
goal: Portar o scan IVF-AQ+batched-AH (provado ~5-7× no M75) para o AM theodb_ivfflat, com set-equal-vs-seqscan GREEN e benchmark a escala.
---

# Handoff M77-M82 — pg_scann no AM `theodb_ivfflat` (design completo, de-riscado)

Design pronto para execução (sessão focada). **Contexto medido:** o M75 spike (`ann/ivf_aqah.rs`) provou o
IVF-AQ+batched-AH dar **~5-7× o QPS do full-precision a recall casado** (veredito D3 GO, `docs/benchmarks/m75-ivf-aqah-spike.md`).
**Achados Rule 9 (memórias `ah-batched-kernel-exists`, `pgscann-am-mostly-exists`):** o kernel batched (`vec/ah.rs::ah_score_block`),
o IVF (`ann/ivf.rs`), o AVQ (`am/aq.rs`), o AM completo (`am/`), a persistência AQ v3 (HNSW) e o reloption
`pq_subspaces` (compartilhado) JÁ EXISTEM. O delta é focado.

## O que já existe (NÃO reconstruir — Rule 9)

- Reloption `pq_subspaces`/`aq_threshold` compartilhado — getters `pq_subspaces_from_relation` (`options.rs:207`),
  `aq_threshold_from_relation` (`options.rs:247`). Parseável já para `theodb_ivfflat`.
- IVF persistência v3 f32: `page::write_ivf_structured` (`page.rs:683`), `read_ivf_meta` (v2/v3, `page.rs:720`),
  `read_ivf_list_bytes` (`page.rs:804`), `structured_page_items`, `encode_list` (`[tid i64][dim×f32]`/entry).
- IVF scan f32: `scan.rs::scan_ivf_structured` (`scan.rs:183`) — probe → `read_ivf_list_bytes` → `l2_dist_from_bytes`.
- Padrão HNSW-AQ a espelhar: `ambuild_hnsw` → `pack_aq(idx, 1, m, bits, thr)` (`build.rs:109+`), meta v3 com codebook,
  scan per-code `ah_score` (`hnsw_page.rs:1420`).
- Scan 2-estágios PROVADO (portar): `ann/ivf_aqah.rs::search` (probe → `ah_score_block` batched → prune → rerank f32).
- Primitivas: `AqQuantizer::{train,encode,to_meta_bytes,from_meta_bytes,m,dim}`, `build_lut16`, `ah_score_block`
  (layout transposed block32: `codes[p*32+v]`, `pairs=ceil(m/2)`).

## M77 — layout block32 dos códigos AQ nas IVF-list-pages (o delta real, isolado)

**Estratégia de baixo risco:** módulo NOVO `am/ivf_aq_page.rs` (v4 paralelo ao v3 f32 — NÃO tocar o v3, para não
quebrar os ~134 pg_tests). ambuild ramifica; scan ramifica na versão do meta.

1. **`am/ivf_aq_page.rs` (NEW):**
   - `write_ivf_aq(rel, dim, metric_tag, m, thr, quant: &AqQuantizer, centroids, lists: &[Vec<(i64,Vec<f32>)>])`:
     meta **v4** = `[IVF_STRUCT_MAGIC, ver=4, metric, dim, nlists, m, codebook_len, codebook_bytes, dir_npages, centroid_npages, gen_base]`
     + centroid pages + per-list pages `[n u32][ids i64×n][f32 vecs×n (rerank)][AQ codes block32 transposto]`.
     Reusa `extend_page_with_item` + `npages_for`. Codebook via `quant.to_meta_bytes()`.
   - `scan_ivf_aq(rel, query) -> Vec<(i64,f64)>`: read meta v4 → `AqQuantizer::from_meta_bytes` → `build_lut16(query)`
     → probe nprobe centroides → por lista: `ah_score_block` sobre os códigos block32 → coletar → `select_nth` top
     rerank_pool (GUC `over_fetch`) → rerank `l2_dist_from_bytes` sobre os f32 → top-k. (= a lógica de `ivf_aqah::search`.)
2. **`build.rs` ambuild:** `let m = pq_subspaces_from_relation(indexrel); if m>0 { train AqQuantizer + ivf_aq_page::write_ivf_aq } else { write_ivf_structured }` (o branch já é o padrão do `pick_hnsw_layout`).
3. **`scan.rs` scan_ivf_structured:** ler o meta magic/ver primeiro; `if ver==4 { ivf_aq_page::scan_ivf_aq } else { <atual f32> }`.
4. **TDD (o gate):** `ambuild_ivf_with_pq_subspaces_scans_set_equal` — build `WITH (lists=N, pq_subspaces=M)` sobre
   corpus pequeno dim divisível por M, scan via index, comparar com seqscan (oracle `build.rs:552`); com rerank_pool
   suficiente, recall alto (o AQ é aproximado → set-equal exige rerank cobrir os true-NN, ou asserir recall≥0.9).
   + negative: dim%m!=0 → typed Err. + `THEODB_SCAN_PROFILE` confirma AH move score+reads.

**Deferidos honestamente para M81 (lifecycle):** aminsert/VACUUM-fold no modo v4 — no M77, aminsert em índice v4
cai no pending f32 (rescan) OU rejeita com typed Err "AQ index é read-mostly, REINDEX para incluir novos" (KISS,
documentar). O gate M77 é o build+scan+set-equal.

## M78-M82 (majoritariamente já existe)

- **M78 (AVQ wiring):** o train+encode+to_meta_bytes já existem (usados no M77). Fecha com o M77.
- **M79 (batched-AH scan):** É o `scan_ivf_aq` do M77. Fecha com o M77 (M77+M79 são um slice).
- **M80 (rerank):** já no `scan_ivf_aq` (stage-2 f32). O `over_fetch` GUC é o rerank pool (`options.rs:29`).
- **M81 (lifecycle):** aminsert/VACUUM v4 (o deferido do M77). aminsert/ambulkdelete/amvacuumcleanup existem p/ v3.
- **M82 (planner + head-to-head):** `amcostestimate` existe. **Deliverable = o benchmark final** recall×QPS do
  `theodb_ivfflat WITH (pq_subspaces)` vs pgvector vs ScaNN (M33) a **SIFT1M** — exige **otimizar o `AqQuantizer::train`
  naive (super-linear, bloqueou o 1M no M75)**: paralelizar (rayon já é dep? não — usar threads) ou treinar num
  sample (ScaNN treina em 250k). ADR de veredito do North Star.

## Bloqueador conhecido (real, honesto)

O **`AqQuantizer::train` é super-linear** (M75: 23s@5k → impraticável@1M in-session). É pré-requisito do benchmark
a escala (M82) e do build 1M. **1º item de execução:** profile + otimizar o train (sample-based à la ScaNN, ou
paralelizar os m sub-kmeans). Sem isso, o benchmark 1M não roda.

## Ordem de ataque

1. Otimizar `AqQuantizer::train` (destrava tudo a escala) — com micro-bench criterion (same-graph, lição m46).
2. M77 = `ivf_aq_page.rs` (write_ivf_aq + scan_ivf_aq) + ambuild/scan branch + set-equal TDD. Release → M77+M78+M79+M80.
3. M81 lifecycle (aminsert/VACUUM v4).
4. M82 benchmark SIFT1M + ADR veredito.

Droplet: c-8 fra1, `cargo pgrx test pg17`. Padrão da sessão: `nohup` morre no ssh-disconnect → rodar com output em
ARQUIVO (`M75_OUT`-style) e `pkill postgres` + `rm postmaster.pid` entre runs.
