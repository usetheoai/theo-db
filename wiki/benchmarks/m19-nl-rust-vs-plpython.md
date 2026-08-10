---
type: Measurement
title: m19 — latência do NL→SQL: Rust contra plpython3u
description: Isola a cola de validação mantendo a chamada ao modelo constante entre os braços — o desenho que torna a comparação significativa numa função dominada por I/O.
resource: git:f7c7b93:docs/benchmarks/m19-nl-rust-vs-plpython.md
tags: [benchmark, reescrita, rust, nl2sql, nao-regressao, m19]
milestone: M19
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m19
    resource: git:f7c7b93:docs/benchmarks/m19-nl-rust-vs-plpython.md
    title: M19 benchmark — ai.nl_to_sql Rust vs plpython3u
---

**Veredito: sem regressão** — razão de 0,883 contra uma barra de 1,20.

# O desenho que torna a comparação significativa

Os dois braços chamam **a mesma implementação de chat em Rust** contra **o mesmo stub determinístico**.
Isso **mantém o round-trip ao modelo constante**, de modo que o delta medido isola **a cola de
validação** — a varredura de tokens e o `EXPLAIN` de um lado, contra a expressão regular e o `EXPLAIN` do
outro.

Sem essa construção, os dois braços seriam dominados pelo I/O e a medição não diria nada sobre a
reescrita. **É a diferença entre medir a mudança e medir o ambiente.**

# Resultado

| Implementação | média | ± desvio | p95 |
|---|---|---|---|
| Rust | 0,663 ms | 0,072 | 0,889 ms |
| plpython3u | 0,752 ms | 0,090 | 1,013 ms |

Cinco runs de 20 chamadas cada, com aquecimento excluído. **Razão 0,883.**

E, como nos irmãos, o enquadramento é: a latência ponta a ponta da função é dominada pela chamada ao
modelo, então **a expectativa honesta era paridade, e o gate é não-regressão, não speedup**.

# Relacionados

A feature resultante — com sua defesa em quatro camadas contra prompt injection — é
[consultas em linguagem natural](/features/12-linguagem-natural.md). Os irmãos desta série são
[m17](/benchmarks/m17-embed-rust-vs-plpython.md) e [m18](/benchmarks/m18-ai-rust-vs-plpython.md).
