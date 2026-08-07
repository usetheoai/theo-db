---
type: Measurement
title: m58 — SIMD para cosseno e produto interno: 3,15× por candidato
description: Fecha o gap de que só a distância euclidiana era vetorizada, no eixo que os embeddings reais de fato usam.
resource: git:f7c7b93:docs/benchmarks/m58-simd-cosine.md
tags: [benchmark, simd, cosseno, micro-benchmark, m58]
milestone: M58
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m58
    resource: git:f7c7b93:docs/benchmarks/m58-simd-cosine.md
    title: M58 — SIMD para cosine/inner-product
---

**Caracterização** do ganho por candidato ao vetorizar o hot-path de cosseno e produto interno — **o eixo
que os embeddings reais usam**.

O gap era estrutural: até aqui **apenas a distância euclidiana tinha caminho vetorizado**, e cosseno e
produto interno rodavam escalar. Como o cosseno é a métrica padrão de embeddings de texto, o caminho
otimizado não era o caminho usado.

# Micro-benchmark por candidato

| Kernel | 200k iterações | custo por candidato |
|---|---|---|
| escalar | 2,3927 s | ~12,0 µs |
| **vetorizado** | **0,7597 s** | **~3,8 µs** |
| **ganho** | **3,15×** | |

# O detalhe de rigor

O despacho é **forçado por branch** durante o teste, de modo que os dois kernels são medidos **na mesma
máquina, sobre o mesmo vetor** — isolando o kernel em vez de comparar execuções que poderiam ter
escolhido caminhos diferentes.

E a reprodução **grava o ratio num arquivo e no log do servidor**, o que torna o número verificável em
vez de apenas relatado.

# Enquadramento

É **caracterização, não comparação competitiva** — mede o ganho de um componente, não uma posição contra
outro sistema. O efeito desse kernel no throughput ponta a ponta depende de o scoring ser ou não o
gargalo, e [m31b](/benchmarks/m31b-simd-distance.md) já mostrara que frequentemente **não é**.
