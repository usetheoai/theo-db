---
type: Measurement
title: m17 — latência do embed: Rust contra plpython3u
description: Gate de não-regressão para a reescrita, com o enquadramento explícito de que a função é limitada por I/O e a linguagem não governa a latência.
resource: git:f7c7b93:docs/benchmarks/m17-embed-rust-vs-plpython.md
tags: [benchmark, reescrita, rust, nao-regressao, io-bound, m17]
milestone: M17
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m17
    resource: git:f7c7b93:docs/benchmarks/m17-embed-rust-vs-plpython.md
    title: M17 — theodb.embed latency Rust vs plpython3u
    last_modified: 2026-06-29
---

Evidência measurement-first de que reescrever a função de embedding para Rust **não regride latência** —
o gate da reescrita incremental com paridade decidida no
[ADR 0006](/decisions/0006-own-code-postgres-based-rust-go.md).

# O enquadramento honesto, dito antes dos números

**A função é limitada por I/O.** Cada chamada faz um round-trip HTTP síncrono, que **domina o tempo de
parede**. A reescrita é sobre **possuir o código em Rust**, provada em **paridade funcional** — **não** é
ganho de velocidade.

**Os números documentam ausência de regressão; eles não são claim de performance.** A latência por
chamada é governada pelo endpoint, não pela linguagem da função.

Essa é a diferença entre um benchmark que prova o que precisa e um que se deixa ler como o que não prova.

# Método — o que torna a comparação limpa

**Mesmo container, mesmo PostgreSQL, mesmo endpoint, mesmo modelo** para as duas implementações — **a
ÚNICA variável é a linguagem da função**. Isso isola a diferença e remove variância de imagem e de rede.

As duas versões ficam instaladas **no mesmo container**, com a antiga recriada a partir do corpo
efetivamente aposentado — não de uma reconstrução aproximada.

O endpoint é um **stub local determinístico** com um modelo real, e a carga é de 200 chamadas seriais por
run, 5 runs, com aquecimento descartado, reportando média e desvio.

# Relacionados

A mesma disciplina foi aplicada às outras reescritas: [m18](/benchmarks/m18-ai-rust-vs-plpython.md) para a
superfície de chat e [m19](/benchmarks/m19-nl-rust-vs-plpython.md) para NL→SQL. A guia operacional da
função está em [embeddings a partir do SQL](/guides/sql-embeddings.md).
