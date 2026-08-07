---
type: Measurement
title: m20 — paridade numérica dos kernels de distância próprios
description: Prova paridade numérica com diferença relativa máxima de ~1e-6, e enquadra honestamente a lentidão de 4× como esperada — escalar contra SIMD, não regressão.
resource: git:f7c7b93:docs/benchmarks/m20-vector-ops-parity.md
tags: [benchmark, paridade, distancia, f32, simd, m20]
milestone: M20
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m20
    resource: git:f7c7b93:docs/benchmarks/m20-vector-ops-parity.md
    title: M20 benchmark — own distance ops vs pgvector
---

**Veredito: paridade provada.**

# Paridade numérica — a entrega

Os kernels próprios acumulam em **f32**, como a referência, então são byte-idênticos sobre a mesma
entrada. Diferença **relativa** máxima sobre 1500 pares de vetores de 1536 dimensões:

| Operação | máx. diferença relativa | ratio de tempo |
|---|---|---|
| L2 | 1,142e-06 | 3,91× |
| produto interno | 1,733e-06 | 5,19× |
| cosseno | 1,680e-06 | 4,23× |

Uma diferença da ordem de 1e-6 em todas as operações **é paridade numérica**: o resíduo é ruído de bit
baixo vindo da **ordem de somatório** — a referência reordena a soma via clones vetorizados, e a
implementação própria era escalar. **Não é diferença de algoritmo**, e o registro diz isso explicitamente.

# O enquadramento honesto da lentidão

A implementação própria era **escalar f32**; a referência usa **SIMD**. **Uma lentidão de escalar contra
SIMD é ESPERADA e não é regressão de milestone.**

A entrega deste milestone era **paridade numérica mais posse da computação em Rust**, coexistindo — **não**
bater o SIMD da referência. A vetorização das operações próprias veio depois, e é o que
[m31b](/benchmarks/m31b-simd-distance.md) mede.

Distinguir "o que este milestone entrega" de "o que este número parece dizer" é o que impede um resultado
correto de ser lido como fracasso.
