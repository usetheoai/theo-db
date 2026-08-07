---
type: Measurement
title: m158 — gate de cobertura antes de implementar
description: Mede se a otimização proposta tem alvo real no benchmark ANTES de construí-la — o gate que o spike anterior ensinou a exigir.
resource: git:f7c7b93:docs/benchmarks/m158-coverage-gate.md
tags: [benchmark, gate, pre-implementacao, cobertura, m158]
milestone: M158
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m158cg
    resource: git:f7c7b93:docs/benchmarks/m158-coverage-gate.md
    title: M158 — coverage gate + baseline
    last_modified: 2026-07-25
---

**O gate obrigatório, executado ANTES de qualquer implementação:** medir se a otimização proposta tem
**cobertura real** no benchmark.

**Veredito: passa — construa.**

# Por que este gate existe

O documento se identifica como **eco do spike anterior** — o
[m155](/benchmarks/m155-topn-spike.md), que descobriu que a otimização proposta **substituiria algo que
já existia**.

**A lição foi institucionalizada:** antes de construir, meça se há alvo. E "há alvo" tem duas partes —
**existe query que se beneficiaria** e **ela é significativa no conjunto**.

Um gate de cobertura responde a primeira de forma barata e objetiva: **contando** as queries com a forma
que a otimização atinge.

# O resultado

Uma classe específica de query é identificada como **alvo primário**, o que dá à implementação seguinte
um caso concreto para otimizar — e um caso concreto para verificar.

**Um gate que passa é tão valioso quanto um que barra**, porque ele torna o investimento seguinte
justificado por evidência em vez de por intuição.

# Continuação

A implementação e seu veredito estão em [m158 late-mat](/benchmarks/m158-late-mat-verdict.md), que herda
inclusive a neutralização da ressalva de empates que o m155 deixara.
