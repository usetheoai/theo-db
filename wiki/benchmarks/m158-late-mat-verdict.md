---
type: Measurement
title: m158 — materialização tardia no top-k: veredito
description: O gate de correção é uma comparação simétrica que preserva o limite, sobre uma chave única escolhida de propósito para eliminar empates da equação.
resource: git:f7c7b93:docs/benchmarks/m158-late-mat-verdict.md
tags: [benchmark, columnar, top-k, materializacao-tardia, oraculo, m158]
milestone: M158
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m158lm
    resource: git:f7c7b93:docs/benchmarks/m158-late-mat-verdict.md
    title: M158 — late-materialization top-k
    last_modified: 2026-07-25
---

Materialização tardia numa query larga com ordenação e limite — o regime que o
[profile](/benchmarks/m148-flamegraph-scan.md) apontara como pesado em reconstrução de tuplas.

# O gate de correção

**"Byte-idêntico ou não embarca."**

O oráculo é uma comparação **simétrica que preserva o limite** entre o plano nativo e o acelerado — não
basta que os conjuntos sejam iguais; a **quantidade e o recorte** precisam ser.

# A escolha que elimina a ambiguidade

A chave de ordenação é **única de propósito**, e o documento diz por quê: **sem empates no limite do
top-k, a comparação é determinística**.

Isso **neutraliza explicitamente a ressalva de empates** que o [m155](/benchmarks/m155-topn-spike.md)
levantara. Com empates, dois planos corretos podem devolver conjuntos diferentes na fronteira — e a
comparação byte a byte acusaria uma divergência que não é defeito.

**Desenhar o experimento para remover a ambiguidade é melhor que tratá-la depois com tolerância** — uma
tolerância esconderia divergências reais junto com as falsas.

# O contexto

A implementação foi autorizada pelo [gate de cobertura](/benchmarks/m158-coverage-gate.md), que mediu
antes que houvesse alvo real — o processo que o m155 ensinou a exigir.
