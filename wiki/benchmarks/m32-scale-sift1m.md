---
type: Measurement
title: m32 — escala a 1M sobre SIFT1M real
description: A primeira corrida em escala real com dados genuinamente distintos, carregando um banner explícito que a isola da retratação de dados degenerados.
resource: git:f7c7b93:docs/benchmarks/m32-scale-sift1m.md
tags: [benchmark, escala, sift1m, dados-reais, m32]
dataset: SIFT1M
milestone: M32
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m32
    resource: git:f7c7b93:docs/benchmarks/m32-scale-sift1m.md
    title: TheoDB vector benchmark — m32-scale-sift1m
---

# O banner que este artefato carrega

O documento abre declarando que **esta é uma corrida sobre SIFT1M real**, com ground truth exato do
próprio dataset — **dados genuinamente distintos**. Ele **não** é afetado pela degenerescência que o
[ADR 0012](/decisions/0012-benchmark-data-degeneracy.md) documenta, e a retratação **não se aplica aqui**.

Anexar essa nota a um artefato **válido** é tão importante quanto retratar o inválido: sem ela, uma
retratação genérica contaminaria por associação toda medição da mesma época.

# Resultado a 1M

| Índice | Params | recall@10 | QPS | p50 | build |
|---|---|---|---|---|---|
| hnsw (referência) | ef=40 | 0,9260 | 132,8 | 6,96 ms | 472 s |
| hnsw (referência) | ef=100 | 0,9765 | 73,8 | 13,67 ms | 472 s |
| ivfflat (referência) | probes=10 | 0,8620 | 170,6 | 5,83 ms | 93 s |
| ivfflat (referência) | probes=1000 | 1,0000 | **3,1** | 311,8 ms | 93 s |
| theodb_ivfflat | fixo | 0,9845 | 28,7 | 36,1 ms | **86 s** |
| **theodb_hnsw** | fixo | 0,9595 | **277,9** | **3,50 ms** | 1440 s |

Dois pontos que os números mostram bem: sondar **todas** as listas leva o recall a 1,0 e o QPS a 3 — o
custo do recall perfeito é brutal; e o HNSW próprio entrega o melhor QPS da tabela, **ao custo de um
build 15× mais longo** que o do IVFFlat.

# Método declarado

Recall com limiar de distância, na semântica das suítes públicas, e ground truth exato recomputado a
partir dos vizinhos oficiais. **Média e desvio são dispersão por query dentro da amostra, não variância
entre runs**, e o QPS é melhor-de-N — distinção que evita confundir dispersão com reprodutibilidade.

# Relacionados

O scan estruturado que produziu o QPS do HNSW próprio é o [m35](/benchmarks/m35-hnsw-structured-scan.md);
o head-to-head contra o estado da arte é o [m33](/benchmarks/m33-scann-headtohead.md).
