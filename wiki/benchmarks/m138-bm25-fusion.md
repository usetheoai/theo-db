---
type: Measurement
title: m138 — trocar a perna lexical da fusão por BM25: honest-negative em dois corpora
description: Não autoriza a troca; e no corpus que favorece o lexical a troca mede significativamente PIOR — o resultado contraintuitivo que manteve o default.
resource: git:f7c7b93:docs/benchmarks/m138-bm25-fusion.md
tags: [benchmark, bm25, fusao, significancia, honest-negative, m138]
milestone: M138
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m138
    resource: git:f7c7b93:docs/benchmarks/m138-bm25-fusion.md
    title: M138 — fusão híbrida com BM25 vs ts_rank_cd
    last_modified: 2026-07-21
---

**Manchete: a medição em DOIS corpora NÃO autoriza trocar o default lexical para BM25 — e no corpus
lexical-pesado a troca mede como significativamente PIOR.**

# Os números

Num corpus, a fusão com BM25 (0,7418) **não vence com significância** a fusão com o ranker nativo
(0,7337): **p = 0,51**, com 54 vitórias, 49 derrotas e 197 empates.

Noutro, que favorece o lexical, a troca mede **pior com significância**.

# Por que este resultado é contraintuitivo — e por que ele decide

O BM25 é **melhor sozinho**: o [m53](/benchmarks/m53-hybrid-beir.md) mediu 0,6881 contra 0,0703 do nativo
em ranking lexical isolado.

**E ainda assim, dentro da fusão, trocá-lo não ganha — e pode perder.**

A explicação plausível é que a [RRF](/technologies/rrf.md) usa **posições, não scores**: ela consome o
*ranking* de cada perna, e uma perna que ordena melhor internamente não necessariamente contribui
posições mais úteis à fusão, sobretudo quando a outra perna já cobre bem os mesmos documentos.

**A conclusão prática: otimizar um componente isolado não implica melhorar o sistema.** É por isso que
o default embarcado permaneceu o nativo, apesar de existir um
[motor BM25 próprio](/features/18-motor-lexical-bm25.md) medido como melhor no eixo isolado.

# O rigor

**Dois corpora, com regimes opostos**, mais **teste de significância pareado** com contagem explícita de
vitórias, derrotas e empates — a metodologia que a linhagem passou a exigir depois de
[m123](/benchmarks/m123-hybrid-significance.md).

E o harness usa uma réplica da fusão **byte-idêntica à de dentro do banco**, para que o que é medido seja
o que o produto faz.
