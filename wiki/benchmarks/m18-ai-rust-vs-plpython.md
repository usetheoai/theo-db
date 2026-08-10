---
type: Measurement
title: m18 — latência do chat: Rust contra plpython3u
description: Não-regressão na reescrita da superfície generativa; o delta favorável de meio milissegundo é explicitamente descartado como ruído de I/O.
resource: git:f7c7b93:docs/benchmarks/m18-ai-rust-vs-plpython.md
tags: [benchmark, reescrita, rust, nao-regressao, io-bound, m18]
milestone: M18
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m18
    resource: git:f7c7b93:docs/benchmarks/m18-ai-rust-vs-plpython.md
    title: M18 — ai._chat latency Rust vs plpython3u
    last_modified: 2026-06-30
---

Gate de não-regressão da reescrita da superfície generativa, com paridade funcional provada por uma
suíte de 36 testes servindo de oráculo.

# Resultado

| Implementação | média ± desvio |
|---|---|
| Rust | 1,447 ± 0,269 ms/chamada |
| plpython3u | 1,995 ± 0,059 ms/chamada |

Delta de **−0,547 ms por chamada** a favor do Rust.

# E por que esse delta favorável NÃO é reportado como ganho

**Está dentro do ruído de uma medição limitada por I/O** — os dois braços são dominados pelo mesmo
round-trip ao endpoint. O veredito é **sem regressão**, e não "13% mais rápido".

Este é o comportamento que separa disciplina de conveniência: **um número favorável que a metodologia não
sustenta é descartado com a mesma firmeza com que um número desfavorável seria**.

# Método

Mesmo container, mesmo endpoint, mesmo stub determinístico para os dois braços — **a única variável é a
linguagem**. Cem chamadas seriais por run, 5 runs, aquecimento descartado, com a versão antiga recriada
para a comparação.

# Relacionados

A mesma disciplina em [m17](/benchmarks/m17-embed-rust-vs-plpython.md) e
[m19](/benchmarks/m19-nl-rust-vs-plpython.md). A superfície resultante está em
[funções generativas em SQL](/guides/sql-ai-functions.md), e a decisão de reescrever para Rust é o
[ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md).
