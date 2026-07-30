---
type: Measurement
title: Gap medido vs ClickHouse no ClickBench: 19,4× geral, 7,54× na classe coberta, 303× na não-coberta
description: Mesma box. O landscape publicado situa o resultado, e o deep-dive identificou a ponte de decode como gargalo da classe coberta.
resource: docs/benchmarks/m159-clickhouse-gap-verdict.md
tags: [benchmark, clickbench, colunar]
timestamp: 2026-07-30T00:00:00Z
---

# Gap medido vs ClickHouse no ClickBench

## Proveniência e ressalva da própria fonte

`docs/benchmarks/m159-clickhouse-gap-verdict.md:26-28,41-45`. **A fonte marca a comparação com o landscape como
`[NO-BASELINE-COMPARABLE]`**: o nosso é 1M/8-vCPU e o publicado deles é 100M/c6a.4xlarge. Os 19,4×/7,54×/303× são
mesma-box e comparáveis entre si; o landscape situa, não mede o mesmo. (Âncora e ressalva acrescentadas
2026-07-30 após review.)

## Os números (mesma box)

| Recorte | Gap |
|---|---|
| geral | **19,4×** |
| consultas **cobertas** pelo pushdown | **7,54×** (≈ pg_mooncake) |
| consultas **não cobertas** | **303×** |

Landscape publicado para situar: DuckDB 1,8× · pg_mooncake 6,2× · Citus 167× · PostgreSQL puro 2178×.

## As leituras que os números sustentam

- **1,8× do DuckDB é o teto prático de uma extensão PG.** Não é ambição; é o que a arquitetura permite.
- O gargalo da classe **coberta** é a **ponte de decode** — `.to_vec` por célula + re-cópia em `build_arrow` —
  confirmado por flamegraph, e **não** o compute.
- A cobertura das consultas não-cobertas é **compound-limited**: os bloqueios são compostos, então destravar um
  eixo rende ~+3-5 consultas, não +11 (medido pelo trace de admissão do M152).

## Correlato — M148

O flamegraph mostrou que ~**80%** do custo do scan colunar era **materialização linha-a-linha**, não decode.
Isso inverteu a hipótese de trabalho e reordenou três milestones (M149 projeção → M151 cobertura → M150
chunk-group filtering).

## Relacionados

- [technique/instrumentar-em-vez-de-adivinhar](../techniques/instrumentar-em-vez-de-adivinhar.md)
