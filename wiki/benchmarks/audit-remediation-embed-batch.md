---
type: Measurement
title: audit-remediation — colapso N→1 no embed em lote
description: Diferente dos benchmarks de reescrita, este mede um ganho estrutural real; e o documento distingue explicitamente as duas naturezas.
resource: git:f7c7b93:docs/benchmarks/audit-remediation-embed-batch.md
tags: [benchmark, embed, n+1, ganho-estrutural, auditoria]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: audrem
    resource: git:f7c7b93:docs/benchmarks/audit-remediation-embed-batch.md
    title: Audit-remediation — embed_batch N→1 latency
    last_modified: 2026-06-29
---

Evidência de que a função em lote **colapsa N round-trips HTTP síncronos em UM** — remediação do achado
crítico de N+1 de uma auditoria de system design.

# A distinção que o documento faz questão de marcar

> Diferente do benchmark por linha — que é uma checagem de **não-regressão** limitada por I/O —, este
> mede **um colapso N→1 real**.

**São naturezas diferentes de resultado.** Os benchmarks de reescrita ([m17](/benchmarks/m17-embed-rust-vs-plpython.md),
[m18](/benchmarks/m18-ai-rust-vs-plpython.md), [m19](/benchmarks/m19-nl-rust-vs-plpython.md)) provam que
**nada piorou**; este prova que **algo melhorou estruturalmente**.

Confundir os dois seria vender uma checagem como ganho — e o repositório recusa isso consistentemente.

# O ganho e sua forma

O speedup é **dominado pelos round-trips economizados e CRESCE com N** — porque a diferença é `N` chamadas
contra `1`, não uma constante.

E a ressalva é honesta na direção **conservadora**: os números vêm de um stub local determinístico, sem
variância de rede; **contra um endpoint remoto com latência real, o ganho absoluto é MAIOR**.

**Medir com um stub subestima o ganho** — o oposto de escolher a condição favorável.

# Contexto

É o follow-up que o [ADR 0007](/decisions/0007-synchronous-per-row-model-http.md) registrou como entregue
ao aceitar a semântica síncrona por linha, e a superfície resultante está em
[acelerar consultas](/features/08-acelerar-consultas.md).
