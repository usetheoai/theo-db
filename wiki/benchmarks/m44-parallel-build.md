---
type: Measurement
title: m44 — build paralelo do HNSW: 2,82× a 50k, 1,95× a 1M
description: Paralelismo puro em Rust sobre o corpus já carregado, sem tocar o maquinário de workers do PostgreSQL — e o recall é paridade não determinística, por desenho declarado.
resource: git:f7c7b93:docs/benchmarks/m44-parallel-build.md
tags: [benchmark, build, paralelismo, hnsw, determinismo, m44]
milestone: M44
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m44
    resource: git:f7c7b93:docs/benchmarks/m44-parallel-build.md
    title: M44 — theodb_hnsw parallel build
    last_modified: 2026-07-03
---

**Veredito: ganho.** Build **2,82× mais rápido** num A/B rigoroso de 3 amostras costas com costas, com
bandas de desvio separadas, e **de 8,4 para 4,3 minutos a 1M**.

**A linhagem completa do build:** 24 minutos com distância escalar → 8,4 com
[SIMD](/benchmarks/m43-hnsw-build.md) → **4,3 com paralelismo**.

# Como

Concorrência com escopo de threads, tomando o corpus somente-leitura emprestado **sem contagem de
referência atômica**, e um lock por nó nas listas de vizinhos — leitores compartilham, escritores
excluem. **Livre de deadlock por construção: um lock de nó por vez.**

Um pânico em worker é **re-levantado na junção**, o que é fail-loud em vez de perda silenciosa.

Duas escolhas de escopo que valem registrar:

- **Despacho por tamanho:** abaixo de um limiar o build é **sequencial e determinístico**, o que mantém
  os corpora pequenos dos testes inalterados. O paralelismo entra só onde paga.
- **Sem maquinário de workers do PostgreSQL** — que é a abordagem da implementação de referência. O build
  do grafo é **Rust puro sobre o corpus já carregado**, então **as threads nunca tocam o Postgres**.

Essa última é a mesma invariante estrutural que o [ADR 0051](/decisions/0051-m139-tantivy-pg-page-directory-design.md)
descobriu ser obrigatória: **thread que toca o banco derruba o backend**.

# A ressalva de determinismo

**O recall é paridade, e NÃO determinístico:** a inserção paralela tem corrida, então **cada corrida
produz um grafo diferente, embora equivalente em recall**. Isso é decisão de desenho declarada, não
efeito colateral descoberto depois — e explica por que os testes pequenos ficam no caminho sequencial.
