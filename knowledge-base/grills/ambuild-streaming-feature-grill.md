---
slug: ambuild-streaming
generated_by: roadmap-feature
date: 2026-07-12
status: completed
new_milestone_id: M89
---

# Feature grill — ambuild-streaming (M89)

## Feature & why now (Q1)

Reescrever o `ambuild` do `theodb_ivfflat` para **flush incremental de páginas** em vez de
bufferizar o `AnnIndex` inteiro + cópias em RAM. **Motivação MEDIDA (M88, ADR-0038):** o ambuild
atual pica ~4× o dataset base em anon-rss → **2 OOM-kills medidos a 30M** num box de 62 GB usáveis
(47 GB e 64 GB anon-rss); 16M foi o maior build viável. Este teto impediu o DoD ≥100M do M88
(crossover de QPS out-of-RAM ficou direcional-não-provado). É a alavanca de maior impacto nomeada
pelo M88 — nova linhagem pós-v7 (o roadmap v7 fechou ROADMAP_COMPLETED 18/18 no v0.76.0).

## Decisions (grill answers)

- **Abordagem técnica:** `tuplesort`/spool nativo do Postgres (o mesmo mecanismo do ambuild do btree
  e do build HNSW do pgvector) — ordena/agrupa por lista em disco com `maintenance_work_mem` bounded,
  escreve páginas por lista incrementalmente. **Regra 9 (não reinventar).** Pico alvo ~1× base.
- **Escopo (Q2):** SÓ o streaming build. A medição terminal bilhão-scale (≥100M — o que o M88 não
  alcançou) vira **M90 gated por M89**. Mantém "one milestone, one DoD, one release" + gate
  measurement-first.
- **DoD de memória (Q3):** build de 30M (base 15.4 GB) completa num box de 64 GB com **pico anon-rss
  ≤ ~1.5× base (~23 GB), medido** — o cenário que OOMou 2× no M88 — **+ zero regressão nos 249
  pg_tests + byte-idêntico a ≤1M.**
- **Prioridade (Q4):** registrar M89 no ROADMAP agora; construir a seguir via `/auto-plan M89` (skill
  não encadeia downstream).

## Dependencies

- **M88** `[x]` (o estado atual do AM `theodb_ivfflat`, layout v5/v6 storage-separated + build Phase 1
  escalável do commit `fba16d0` — kmeans-train sampling + parallel assignment).

## Definition of Done (verifiable)

1. `ambuild` do `theodb_ivfflat` usa `tuplesort`/spool nativo do PG; **pico anon-rss do build ≤ ~1.5×
   o dataset base**, MEDIDO num build de 30M que completa num box de 64 GB (o cenário que OOMou no M88).
2. **Zero regressão:** 249 pg_tests GREEN; recall **byte-idêntico** ao build atual a ≤1M (mesmo A/B
   same-data do M46).
3. Artefato de evidência de memória (`docs/benchmarks/m89-*.{md,json}`) — pico anon-rss vs N (16M, 30M)
   com o build antigo vs novo, provando que o pico deixou de escalar ~4×.
4. `maintenance_work_mem` respeitado (o spill honra o budget de memória configurado).
5. Sem novas deps externas fora do que já está declarado (tuplesort é do próprio Postgres, via `pg_sys`).

## Top 2 NEW risks

1. **API do tuplesort via pgrx/pg_sys** — o `tuplesortstate` é C interno do Postgres; expor via `pg_sys`
   pode exigir bindings não-triviais (FFI, lifetimes, `extern "C-unwind"`). Mitigação: espelhar o padrão
   do build HNSW do pgvector (referência permissiva) + council-rust-pgrx no review. Owner: implementador.
2. **Regressão de tempo de build** — o spill em disco pode ser mais lento que o build in-RAM atual a
   escalas pequenas (≤1M) onde tudo cabe em RAM. Mitigação: fast-path in-RAM quando `N·base ≤
   maintenance_work_mem` (mantém o caminho atual); medir o cruzamento. Owner: implementador.

## Notes

- SOTA delta: **nenhum peer novo** — o `tuplesort` é do core do Postgres e o build HNSW do pgvector
  (permissivo, já referência) cobre o padrão. Regra 9.
- out-of-scope cross-check: o ROADMAP não tem seção "Explicitly out of scope"; nada a checar.
