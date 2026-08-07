---
type: Measurement
title: m21 — paridade de recall dos índices ANN próprios
description: HNSW e IVFFlat próprios atingem recall igual ou melhor que a referência em todos os knobs varridos; a latência não é comparável ainda, e o documento diz por quê.
resource: git:f7c7b93:docs/benchmarks/m21-ann-index-parity.md
tags: [benchmark, paridade, hnsw, ivfflat, recall, m21]
milestone: M21
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m21
    resource: git:f7c7b93:docs/benchmarks/m21-ann-index-parity.md
    title: M21 — Own ANN index recall@k parity
---

**Veredito: paridade alcançada.** Recall@10 contra ground truth exato por força bruta, com tolerância
declarada, 3 runs com média e desvio.

# Resultado

| Algoritmo | Knob | Referência | Próprio | Paridade |
|---|---|---|---|---|
| HNSW | ef=10 | 0,8622 ± 0,0055 | 0,8617 ± 0,0000 | ✅ |
| HNSW | ef=40 | 0,9917 | 0,9917 | ✅ |
| HNSW | ef=100 | 1,0000 | 1,0000 | ✅ |
| IVFFlat | probes=8 | 0,6561 ± 0,0170 | 0,6750 | ✅ |
| IVFFlat | probes=16 | 0,8672 ± 0,0057 | 0,8983 | ✅ |
| IVFFlat | probes=32 | 0,9978 ± 0,0031 | 1,0000 | ✅ |

Passa em todos os knobs, com o IVFFlat próprio ficando **acima** da referência nos pontos intermediários.

Note o desvio **zero** do lado próprio: o build é determinístico por semente fixa, enquanto a referência
tem variância entre runs.

# O que este benchmark deliberadamente NÃO compara

**Latência.** A forma chamável por SQL medida aqui **reconstrói o grafo em memória a cada chamada** — era
o escopo measurement-first da fatia. A referência consulta um índice **persistido em disco**.

**Os perfis de latência não são comparáveis até que o índice próprio seja persistido**, e o documento diz
isso em vez de publicar uma razão enganosa. A persistência veio depois, no
[ADR 0010](/decisions/0010-m26-index-am-scope.md), e a comparação de latência honesta é o
[m26](/benchmarks/m26-index-am.md) e o [m34](/benchmarks/m34-ivfflat-reloption.md).

**Comparar apenas o eixo que a medição sustenta** é a disciplina que este artefato exemplifica.
