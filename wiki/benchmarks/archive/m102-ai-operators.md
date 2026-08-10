---
type: Measurement
title: m102 — operadores de IA como nós otimizáveis: dois artefatos, dois propósitos
description: Um braço determinístico sem HTTP mede o mecanismo de forma exatamente reproduzível; o braço com modelo real mede a latência que só um modelo real tem.
resource: git:f7c7b93:docs/benchmarks/archive/m102-ai-operators.md
tags: [benchmark, ai-surface, batching, pushdown, determinismo, arquivo, m102]
milestone: M102
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m102
    resource: git:f7c7b93:docs/benchmarks/archive/m102-ai-operators.md
    title: M102 — AI operators as optimizable plan nodes
    last_modified: 2026-07-16
---

# A separação em dois artefatos

**Braço determinístico, sem HTTP:** mede o ganho de round-trip e a redução de linhas pelo push-down —
**o mecanismo** —, sem modelo vivo, de modo que é **exatamente reproduzível** e roda em CI.

**Braço com modelo real:** mede a latência de parede do caminho em lote, com 3 runs.

**Essa separação resolve um conflito real de medição.** O mecanismo precisa ser verificável de forma
determinística; a latência só existe com um modelo de verdade. Misturar os dois num artefato só produziria
ou um teste flaky, ou uma medição de mecanismo que não prova nada.

É a mesma disciplina que os benchmarks de reescrita usaram ao adotar stubs determinísticos como oráculo e
tratar o provedor real como benchmark, não como asserção.

# O que cada um mostra

Determinístico: **1 round-trip contra N**, e a IA avaliada em **≤ K sobreviventes** em vez de todos os N.

Real: latência **≈12× menor** em lote.

# O teto honesto

É ganho de **composabilidade e round-trip com acurácia estatística**, **ortogonal ao recall vetorial** —
nunca enquadrado como "mais rápido no vetor".

E a ressalva fina: **as respostas não são asseridas byte-idênticas num modelo real**, porque as N
perguntas compartilham uma mensagem e há *context bleed*. O modelo determinístico é o gate de correção; o
real é o benchmark.

A decisão completa, incluindo a postura de segurança contra prompt injection, é o
[ADR 0043](/decisions/0043-m102-ai-operators-batched-pushdown.md).
