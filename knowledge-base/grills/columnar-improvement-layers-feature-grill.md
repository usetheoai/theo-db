---
slug: columnar-improvement-layers
generated_by: roadmap-feature
date: 2026-07-26
status: completed
provenance: deep-dive blueprint (not interactive grill — content is evidence-backed from the post-M159 deep-dive)
---

# Feature grill — 3 columnar-improvement layers (M160/M161/M162)

Não-interativo: os 4 itens do grill (o quê/por-quê-agora, dependências, DoD, riscos) foram derivados do deep-dive
pós-M159 (`knowledge-base/discoveries/blueprints/columnar-improvement-deepdive-blueprint.md`, 3 council agents +
flamegraph empírico + `admit_trace` ground-truth + landscape de concorrentes computado dos JSONs oficiais do ClickBench).
95% de confiança satisfeita pela evidência medida, não por interview.

## Q1 — O quê + por quê agora
As 3 layers ranqueadas do deep-dive: A=ponte de decode (maior ROI, classe coberta 7.54×→2-3×), B=cobertura das 11
não-cobertas (303×→rumo à classe coberta, ~+3-5 realista), C=100M larger-than-RAM (não-medido). Agora: o M159 mediu o
gap (19.4× geral) e localizou EXATAMENTE onde melhorar; as 3 layers são a resposta measurement-first.

## Q2 — Dependências
Todas dependem de M159 `[ ]` (o baseline medido). M161/M162 recomendam M160 antes (não bloqueante).

## Q3 — DoD
Ver os blocos M160/M161/M162 em ROADMAP.md — cada um measurement-first (flamegraph/A-B/re-run harness), com gate de
corretude byte-idêntico e honest-negative aceito.

## Q4 — Riscos NOVOS
A: endianness/nullable no zero-copy. B: gauntlet de corretude por-classe (overflow/colação/epoch) + tentação de "+11".
C: construir encoding sem medir 100M primeiro; formato persistente = subsistema de upgrade M137.

## SOTA delta
Nenhum clone novo — as referências (datafusion, arrow-rs, parquet-format, duckdb, pg_clickhouse, cstore/monetdb/morsel
papers) JÁ estão no acervo; embutidas nos "Prior art / referências" de cada milestone.
