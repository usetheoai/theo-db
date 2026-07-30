---
type: Invariant
title: Spi::get_one marca a transação como mutável
description: Apesar do nome, get_one impede operações que exigem transação read-only; Spi::connect + c.select é o caminho que não marca.
tags: [pgrx, rust, spi, mvcc]
timestamp: 2026-07-30T00:00:00Z
---

# `Spi::get_one` marca a transação como mutável

## O invariante

Apesar do nome, `Spi::get_one` **não** é read-only para o PostgreSQL: ele marca a transação como mutável. Em
caminhos que precisam permanecer read-only (worker de background, hook sob snapshot, operação com `backend_xmin`
estável) isso quebra a operação de forma não-óbvia.

O caminho que preserva read-only:

```rust
Spi::connect(|c| {
    let rows = c.select("SELECT ...", None, None)?;
    // ...
})
```

## Onde custou — M140.4

Descoberto ao provar crash/VACUUM/MVCC no consumidor: um `get_one` numa verificação de leitura marcava a
transação e alterava a semântica do teste.

## Invariante irmão — `Spi::run` não engole erro SQL

Sob pgrx, um erro SQL dentro de `Spi::run` faz `longjmp` para o handler do PostgreSQL — ele **não** volta como
`Result` para o Rust. Código que assume "se falhar eu trato o `Err`" nunca executa o tratamento. (Medido no M144.)

## Relacionados

- [invariant/panic-atraves-da-fronteira-c](panic-atraves-da-fronteira-c.md)
