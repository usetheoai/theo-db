---
type: Measurement
title: m43 — build do HNSW ~2,2–2,9× mais rápido por SIMD, com paridade de recall
description: O build usava distância escalar enquanto o scan já usava SIMD; alinhar os dois derrubou o build de 24 para 8,4 minutos a 1M — e o recall é paridade, não byte-idêntico, por razão explicada.
resource: git:f7c7b93:docs/benchmarks/m43-hnsw-build.md
tags: [benchmark, build, hnsw, simd, paridade, m43]
milestone: M43
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m43
    resource: git:f7c7b93:docs/benchmarks/m43-hnsw-build.md
    title: M43 — theodb_hnsw build-time optimization
    last_modified: 2026-07-03
---

**Veredito: ganho.** O build do grafo fica **~2,2× mais rápido** num A/B rigoroso de 3 amostras com
bandas de desvio separadas, e cai de 24 para **8,4 minutos a 1M** — ~2,9× contra o baseline.

# A inconsistência que a otimização revelou

O build calculava distância pelo caminho **escalar**, executando bilhões de operações, **enquanto o scan
já usava o kernel SIMD**. A correção roteia a distância do build para o mesmo kernel.

Isso não só acelera: **alinha a métrica do build com a do scan**. Antes, o grafo era **construído com
distância escalar e buscado com SIMD** — uma inconsistência silenciosa entre as duas fases.

# A ressalva de paridade, com a razão

**O recall é PARIDADE, não byte-idêntico.** A razão é dita: a operação fundida do SIMD **arredonda
diferente**, então o grafo pode divergir em algumas seleções quase empatadas. Nas escalas testadas, o
recall mediu idêntico.

E há um escopo cuidadoso: a função de distância usada por **operadores, rerank e busca exata permanece
inalterada** — só o build e a busca **aproximados** usam a variante SIMD. **A paridade numérica com a
referência, que outro artefato provou, não é comprometida.**

# A linhagem

Este é o segundo passo de uma sequência de build: 24 minutos com escalar, 8,4 com SIMD, e **4,3 com
paralelismo** ([m44](/benchmarks/m44-parallel-build.md)).
