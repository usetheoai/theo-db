---
type: Measurement
title: m45 — Pareto rigoroso de recall × QPS contra o pgvector em SIFT1M
description: Veredito de paridade com média e desvio sobre níveis de recall compartilhados; entrega metade do requisito de claim público e declara a outra metade em aberto.
resource: git:f7c7b93:docs/benchmarks/m45-pareto-sift1m.md
tags: [benchmark, pareto, sift1m, paridade, reprodutibilidade, m45]
dataset: SIFT1M
milestone: M45
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m45
    resource: git:f7c7b93:docs/benchmarks/m45-pareto-sift1m.md
    title: M45 — rigorous mean±std recall×QPS Pareto
---

**Veredito: PARIDADE**, sob um gate de **efeito maior que variância** aplicado sobre os níveis de recall
compartilhados.

**Config:** 1M vetores, 500 queries, 3 runs com média e desvio, grade de `ef` varrida, dataset real. Os
parâmetros de build são **casados** entre os dois lados, com workers de manutenção desligados em ambos.

# A fronteira própria

| `ef_search` | recall@10 | QPS (média ± desvio) |
|---|---|---|
| 40 | 0,9278 | 294,0 ± 15,5 |
| 64 | 0,9646 | 178,1 ± 9,8 |
| 100 | 0,9832 | 139,9 ± 2,8 |
| 200 | 0,9932 | **43,5 ± 19,1** |
| 400 | 0,9968 | 44,8 ± 2,8 |

Note o **desvio de 19,1 no ponto de `ef=200`** — quase metade da média. **Publicar o desvio ao lado da
média é o que permite ao leitor ver que aquele ponto específico é instável**, em vez de tratá-lo como os
demais.

O build próprio é notavelmente mais rápido que o da referência (271 s contra 467 s), fruto da
[linhagem de otimização de build](/benchmarks/m44-parallel-build.md).

# O que este artefato entrega, e o que ainda falta

O documento é explícito: ele entrega **metade** do requisito para um claim comparativo público — **o
artefato reproduzível**. **A outra metade — reprodução independente por terceiro — permanece EM
ABERTO.**

Nomear a metade faltante em vez de tratar o artefato como suficiente é o que impede um resultado válido
de virar claim inválido. É também o gap que o
[ADR 0050](/decisions/0050-official-benchmark-adopt-and-wrap.md) endereça, ao adotar drivers oficiais e
entrada em leaderboard público **por comparabilidade externa**.
