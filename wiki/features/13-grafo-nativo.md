---
type: Feature
title: Grafo nativo — travessia CSR e GraphRAG
description: CSR persistido como bytea num catálogo, ganhando WAL, crash-safety e MVCC de graça do PostgreSQL; travessia medida 106–738× acima de CTE recursiva.
resource: git:f7c7b93:docs/features/13-grafo-nativo.md
tags: [feature, grafo, csr, graphrag, bfs, ppr]
feature_status: entregue
milestone: M108+M110+M111+M112
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: feat13
    resource: git:f7c7b93:docs/features/13-grafo-nativo.md
    title: Consultar um grafo nativo
---

**Status: entregue.** O motor é um **CSR persistido** — Compressed Sparse Row — serializado como `bytea`
num catálogo, o que lhe dá **WAL, crash-safety e MVCC de graça** pelo próprio PostgreSQL, sem página nem
resource manager próprio. É o mesmo truque de composição que o
[colunar](/decisions/0042-m99-own-code-columnar-tam.md) e o
[lexical](/decisions/0052-m140-1-lexical-storage-decision.md) usam.

Todas as funções compilam no binário **default**, sem feature flag.

# Por que existe

O gate de viabilidade ([ADR 0048](/decisions/0048-m107-native-graph-engine-go.md)) mediu a travessia
nativa **106–738× mais rápida** que CTE recursiva — e sobreviveu a um baseline mais justo, com
deduplicação, ainda com 106–232×. A medição veio **antes** de qualquer código de produção
([m107](/benchmarks/m107-graph-spike.md)).

Uma ressalva daquele spike moldou o desenho: **construir o CSR na hora domina o custo** a 1M de arestas,
o que colapsaria o ganho ponta a ponta. Por isso o CSR é **persistido** e refoldado, não construído por
query.

# Uso

```sql
CREATE EXTENSION IF NOT EXISTS theodb CASCADE;

CREATE TABLE friendship (src bigint, dst bigint);
INSERT INTO friendship VALUES (0,1),(1,2),(2,3),(3,4),(0,2),(0,3);
```

O engine consome **qualquer tabela com duas colunas `bigint`**, e as arestas são tratadas como
**não-direcionadas** na travessia — um detalhe que muda o resultado e precisa ser conhecido.

A superfície `theodb.graph_*` cobre construir e refoldar o CSR, expandir vizinhanças em até H hops —
tanto de origem única quanto multi-origem em lote — e rodar Personalized PageRank.

# GraphRAG ponta a ponta

O fluxo completo roda **sem sair do SQL**: extração de entidades e relações a partir de texto, montagem
do grafo, travessia limitada, e reranking de chunks pelo resultado. As funções de extração vivem na
superfície `ai.*`, o que faz este fluxo ser a materialização concreta do posicionamento AI-native.

Evidência do fluxo em [m111/m112](/benchmarks/archive/m111-m112-graphrag-retrieval.md), e o microbenchmark de
scan em [fu1](/benchmarks/fu1-samegraph-scan-microbench.md).

# Fronteira honesta

A **qualidade** do grafo — isto é, a qualidade da extração de entidades — é avaliação **separada**, que
o motor não resolve. O ADR 0048 diz isso explicitamente: o gate mediu a **primitiva de travessia**, não
a utilidade do grafo resultante para uma tarefa de recuperação.

# Relacionados

O substrato colunar que este motor reusa está em
[analítico colunar](/features/14-analitico-colunar.md), e a co-residência entre vetor e colunar que o
GraphRAG aproveita está no [ADR 0044](/decisions/0044-m103-vector-columnar-coresidence.md).
