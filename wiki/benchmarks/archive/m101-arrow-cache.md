---
type: Measurement
title: m101 — cache Arrow com heap autoritativo: o subconjunto permissivo do colunar automático
description: Um cache MVCC-correto e provado por permutações de isolamento, mas com pragma MANUAL — o que o distingue explicitamente do motor auto-mantido da referência.
resource: git:f7c7b93:docs/benchmarks/archive/m101-arrow-cache.md
tags: [benchmark, cache, arrow, mvcc, htap, arquivo, m101]
milestone: M101
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m101
    resource: git:f7c7b93:docs/benchmarks/archive/m101-arrow-cache.md
    title: M101 — Heap-authoritative Arrow cache
    last_modified: 2026-07-16
---

# O que é medido

O ganho de uma agregação vetorizada sobre um lote [Arrow](/technologies/arrow.md) pré-construído em
memória — **sem varredura do heap** — contra a agregação nativa, para uma query analítica repetida e
pesada de leitura.

# As duas propriedades que definem o artefato

**O cache é MVCC-correto**, com invalidação na escrita mais um gate de compatibilidade de snapshot —
**provado por permutações de isolamento**, não por argumento. Uma escrita invalida o cache, e a leitura
seguinte paga a reconstrução.

Provar correção de cache **por permutações de concorrência** é o padrão certo: um cache que funciona nos
testes sequenciais e falha sob concorrência é pior que nenhum cache.

**E o pragma é MANUAL.** O documento diz isso literalmente e contrasta: **não é o motor auto-mantido da
referência de mercado — este é o subconjunto permissivo.**

# Por que a distinção é o ponto

O colunar in-memory automático é exatamente a capacidade que a barreira de licença torna inalcançável, e
que o [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md) registra como aposta
diferente.

**Entregar o subconjunto e nomeá-lo como subconjunto** é o que permite ter a capacidade sem
over-claiming — o operador sabe que precisa acionar, e sabe por quê.

# Relacionado

O substrato de storage é [m99](/benchmarks/m99-columnar-tam.md); a execução vetorizada,
[m100](/benchmarks/m100-datafusion-executor.md).
