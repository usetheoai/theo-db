---
type: Measurement
title: m140.3 — engine BM25 de produção: cache contra reload, e o ganho que escala
description: Mata o recarregamento por query do spike, com ganho que cresce com o tamanho do índice, e reproduz a qualidade anterior byte a byte na forma final.
resource: git:f7c7b93:docs/benchmarks/m140-3-bm25-engine.md
tags: [benchmark, bm25, cache, mvcc, escala, m140]
milestone: M140.3
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m1403
    resource: git:f7c7b93:docs/benchmarks/m140-3-bm25-engine.md
    title: M140.3 — engine BM25 de produção own-code
    last_modified: 2026-07-22
---

**Manchete:** a engine de produção, com cache MVCC-correto, **mata o recarregamento por query** do
spike — **e o ganho ESCALA com o tamanho do índice**.

# Por que "escala" é a palavra que importa

Um ganho de cache que fosse constante seria uma otimização; um que **cresce com o tamanho do índice**
indica que o custo eliminado era **proporcional ao índice** — ou seja, o spike recarregava a estrutura
inteira a cada consulta.

**A forma da curva identifica o mecanismo**, o mesmo raciocínio que a contagem de páginas usou em
[m35](/benchmarks/m35-hnsw-structured-scan.md).

# A reprodução byte a byte

O nDCG medido **dentro do banco** reproduz **byte a byte** o valor da medição anterior, feita fora.

Isso confirma que **a forma final não degradou a qualidade** — o risco real quando um protótipo vira
produção, com cache, transações e integração no meio. Sem essa checagem, uma queda de qualidade
introduzida pela engenharia passaria despercebida.

# O que o cache precisa garantir

Ser **MVCC-correto**: um leitor com snapshot antigo não pode ver uma construção de outra sessão. Isso é
provado no milestone seguinte, [m140.4](/benchmarks/m140-4-robustness-consumer.md), junto com crash e
VACUUM — e a correção fina do mecanismo de leitura está no
[ADR 0055](/decisions/0055-m140-4-lexical-robustness-consumer.md).

A decisão de que esta superfície supersede a dependência externa é o
[ADR 0054](/decisions/0054-m140-3-bm25-supersede-textsearch.md).
