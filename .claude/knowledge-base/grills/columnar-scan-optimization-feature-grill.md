---
slug: columnar-scan-optimization
generated_by: roadmap-feature
date: 2026-07-24
status: completed
milestones_added: [M148, M149, M150, M151]
---

# Grill — otimização do scan colunar (M148–M151)

Requisitos derivados da análise SOTA desta sessão (2026-07-24), não de grill interativo — o usuário já
articulou o problema e a direção ("veja como referências SOTA resolvem"). Fonte: gate ClickBench 1M
pós-#190 (`docs/benchmarks/clickbench-1m-postfix-2026-07-24.md`) + análise em
`memory/columnar-scan-bottleneck-hypothesis.md`.

## Q1 — O que é e por que agora

O gate ClickBench 1M destravado (#190) revelou o número real do pilar colunar: **só 6/43 queries engajam o
pushdown vetorizado (geomean 0,476s); as outras 36 rodam a ~47s** (geomean geral 24,5s). O 1,90× do M131
vinha de uma amostra enviesada (`head -n`) — em dados representativos o colunar não é competitivo em escala.
**Por que agora:** o benchmark honesto tornou o gargalo medível e o pilar colunar é diferenciação declarada
do TheoDB (HTAP integrado). Sem isto, "colunar" é storage, não aceleração.

## Q2 — Gargalo (por código, a confirmar por flamegraph)

`columnar_scan_getnextslot` (`columnar.rs:1105`) → `load_next_batch` (:1026) → `decode_stripe(..., natts,
...)` (:1044) **decodifica as 105 colunas de cada stripe** (sem projection pushdown) e re-materializa cada
linha como heap-tuple, emitindo uma a uma pelo executor row-based (Volcano). O SOTA resolve com 3 técnicas —
2 já têm infra no crate.

## Q3 — SOTA e o que já temos

| Técnica | SOTA (referência) | Já temos? |
|---|---|---|
| Projection pushdown | Citus columnar "pula colunas não-lidas" | ❌ `decode_stripe` decodifica todas |
| Chunk-group filtering | Citus "pula chunks por min/max sem descomprimir" | ⚠️ `directory_minmax` (M105) só no CustomScan de agg |
| Execução vetorizada | DuckDB batches ~2048 → SIMD, cache 10-100× | ⚠️ DataFusion (M100) só cobre agregação simples |

## Q4 — Riscos NOVOS

- **R1 (projection pushdown):** o TableAM precisa saber quais colunas a query referencia; obter a lista via
  `scan_begin`/`rs_rd` sem quebrar queries que tocam todas. Mitigação: fallback para decode-tudo se a lista
  não estiver disponível.
- **R2 (vetorização ampla):** rotear mais queries pelo DataFusion pode divergir do resultado do executor
  nativo (correção). Mitigação: A/B byte-idêntico vs heap obrigatório em cada query roteada (o oráculo que
  já usamos).
- **R3 (medição):** priorizar a técnica errada sem flamegraph. Mitigação: M148 (spike) precede e gate os
  demais — measurement-first, padrão M75/M83.

## Decisões de escopo (do usuário, 2026-07-24)

- **Out-of-scope "columnar próprio" removido:** superado pelos M99–M143 (own-code shipado, pg_duckdb
  removido — ADR-0042/0057). Otimizar o scan é continuação, não reabertura.
- **Granularidade:** 4 milestones sequenciais (M148 spike → M149 projection → M150 chunk-filter → M151
  vetorização), cada um com DoD e benchmark próprios.
