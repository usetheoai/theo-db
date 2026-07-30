---
type: Invariant
title: pgrx 0.19 desenrola: um ERROR do PostgreSQL vira panic_any e as frames Rust desenrolam
description: O pg_guard no bloco extern C-unwind embrulha cada função bindgen; check_for_interrupts! desenrola limpo, ao contrário do que uma revisão alegou.
resource: references/pgrx/
tags: [pgrx, rust, ffi, unsafe]
timestamp: 2026-07-30T00:00:00Z
---

# pgrx 0.19 desenrola: um `ERROR` do PostgreSQL vira `panic_any` e as frames Rust desenrolam

## O invariante, verificado no código

`pg18.rs:35462` marca o único bloco `extern "C-unwind"` com `#[pgrx_macros::pg_guard]`; `rewriter.rs:184-193`
embrulha cada função em `pg_guard_ffi_boundary`. O comentário em `ffi.rs:85` diz que isso protege **toda** função
externa gerada por bindgen. Consequência: um `ERROR` do PostgreSQL vira `panic_any` e as frames Rust
**desenrolam** corretamente.

## Por que este invariante está registrado

Numa rodada de revisão do M168, um BLOCKER alegou que `check_for_interrupts!()` **não** desenrola limpo, e eu
aceitei. O revisor depois retratou; verifiquei as cinco citações e a alegação original estava errada.

A racionalização falsa, se tivesse ficado, condenaria os quatro `check_for_interrupts!()` legítimos nos laços de
`CREATE INDEX` (`build.rs:420,474,487,812`) e mais um no benchmark. Mantive o desenho novo, mas **reescrevi a
justificativa** — código certo com razão errada é dívida que explode na manutenção seguinte.

## O que continua exigindo cuidado

Desenrolar corretamente **não** torna `unsafe` seguro. Há **384 blocos `unsafe`** em `theodb_rs`, e a classe de
defeito mais cara já encontrada em review vive ali: alocação em `TopMemoryContext`, ponteiro que sobrevive ao
contexto, e MVCC do SPI.

## Relacionados

- [failure-mode/diagnostico-aceito-sem-reproduzir](../failure-modes/diagnostico-aceito-sem-reproduzir.md)
- [invariant/tableam-routine-em-topmemorycontext](tableam-routine-em-topmemorycontext.md)
