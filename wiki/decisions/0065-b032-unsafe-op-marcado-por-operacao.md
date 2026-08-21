---
type: Decision
title: ADR-0065 — o lint `unsafe_op_in_unsafe_fn` vira erro no crate inteiro, e a marcação é por operação
description: 1.444 operações (não 2.872 — o número antigo contava duas vezes), anotadas por operação e não por corpo, com `deny` no crate como portão. O `cargo fix` do rustc envolve o corpo e é teatro.
tags: [unsafe, pgrx, ffi, portao, b-032, honest-negative]
item: B-032
generated: { by: claude-code/opus-5, at: 2026-08-21T00:00:00Z }
---

Peças relacionadas: [pgrx](../technologies/pgrx.md), a fronteira em que esse lint mora, e
[ADR-0064](0064-maintenance-work-mem-nao-e-contrato.md), a decisão irmã que também trocou um número
herdado por um medido.

# Contexto

O [[B-032]] registrou **2.872 operações inseguras sem bloco explícito**, concentradas na área que o
`CLAUDE.md` chama de mais cara: dentro de uma `unsafe fn` sem `unsafe {}` explícito, **o corpo inteiro
é implicitamente inseguro**, e some a capacidade de apontar quais linhas são as perigosas.

## O número era metade — e a razão é exata

Medido em 2026-08-21 sobre `cargo build --features pg18 --message-format=json`: **1.444**, não 2.872.
Cada arquivo bate em precisamente metade:

| arquivo | registrado | medido |
|---|---|---|
| `am/columnar_agg.rs` | 1236 | **618** |
| `am/page/mod.rs` | 354 | **177** |
| `am/columnar.rs` | 286 | **143** |
| `am/customscan.rs` | 140 | **70** |
| `am/build_stream.rs` | 140 | **70** |
| `am/page/ivf.rs` | 138 | **69** |

Todos exatamente 2×. A medição original saiu de `cargo pgrx test`, que compila o crate **duas vezes**
(alvo lib e alvo test) e emite cada aviso duas vezes. Não é estimativa: seis de seis batem.

# Decisão

**`#![deny(unsafe_op_in_unsafe_fn)]` no crate inteiro, com a marcação feita por OPERAÇÃO.**

## Por que `deny` no crate, e não um contador com baseline

O DoD do item oferecia três formas — de uma vez, por módulo, ou com prazo. Com o número **em zero**,
`deny` é a única que não pode ser burlada nem crescer em silêncio: o build para. Um baseline numérico
avisa só depois que a operação entrou, e precisa de alguém para atualizá-lo — que é exatamente como
2.872 avisos chegaram a ser impressos a cada build sem ninguém consumir.

## Por que por operação, e não por corpo — o `cargo fix` é teatro

O `cargo fix` do próprio rustc envolve o **corpo inteiro** da função:

```rust
pub(crate) unsafe fn encode(tid: pg_sys::ItemPointer) -> i64 { unsafe {
    let blkid = (*tid).ip_blkid;   // ...
}}
```

Isso satisfaz o lint e **preserva o status quo exatamente**: uma deref acrescentada amanhã dentro
daquele corpo continua sem avisar. Zero sinal ganho. Foi aplicado, medido e **revertido**. Nenhum
corpo foi envolvido na entrega.

## A medição que derrubou a hipótese de adiar

A hipótese razoável para adiar era: num arquivo onde 618 operações são inseguras, marcar cada uma
marca tudo e portanto não marca nada. **A densidade refuta.**

| arquivo | ops | linhas acesas | LOC | densidade |
|---|---|---|---|---|
| `am/page/mod.rs` | 177 | 289 | 1003 | **28,8%** |
| `am/options.rs` | 50 | 138 | 560 | 24,6% |
| `am/build_stream.rs` | 70 | 107 | 455 | 23,5% |
| `am/columnar_agg.rs` | 618 | 599 | 3248 | 18,4% |
| `am/columnar.rs` | 143 | 151 | 2736 | **5,5%** |
| `am/scan.rs` | 33 | 33 | 1405 | **2,3%** |

Pior caso 28,8% — a revisão ainda estreita **3,5×**; em `am/columnar.rs`, **18×**. O argumento de
visibilidade sobrevive nos 18 arquivos.

# O que a transformação NÃO é: puramente sintática

Envolver a operação parece anotação e quase sempre é, mas **cinco classes mudam significado ou não
compilam**, e todas foram encontradas pelo compilador, não por leitura:

| classe | o que quebra | tratamento |
|---|---|---|
| alvo de atribuição | `unsafe { *p } = v` não compila | envolve a instrução inteira |
| spans parcialmente sobrepostos | delimitador cruzado | só o primeiro entra |
| posição de instrução | `unsafe { X } != 0` é parseado como instrução + lixo | parênteses |
| **deref como *place*** | `unsafe { *p }.campo` **copia** a struct e projeta do temporário | estende por toda a cadeia postfix |
| **escopo de temporários** | `f(&unsafe { g()? })` libera o temporário antes da chamada | o `&` só entra se a operação for deref |

As duas últimas são as que importam. Para tipo não-`Copy` o compilador barra com `E0507`; **para tipo
`Copy` ele compila**, e `&mut unsafe { *p }.campo` passa a referenciar um temporário — escrita
perdida, em silêncio, num banco. É por isso que o envelope se estende pela cadeia postfix em vez de
fechar no deref.

O caso do escopo de temporários apareceu uma vez (`am/page/ivf.rs:58`) e o borrow checker gritou
(`E0716`). Um bloco abre escopo de temporários; `f(&g()?)` e `f(unsafe { &g()? })` não têm o mesmo
tempo de vida.

# Consequências

- **1.444 → 0.** 1.215 envolvidas mecanicamente pela ferramenta, 228 cobertas por envelope externo,
  1 à mão (`let ... else`, forma em que `}` antes de `else` é proibida).
- **488 testes passando, 0 falhando** — a prova de que é anotação, não semântica.
- `am/columnar_agg.rs` e `am/page/mod.rs`, que o DoD mandava tratar primeiro, foram tratados: **795
  de 1.444**, 55% do total.
- O lint não pode mais crescer: o build para.

# Alternativas descartadas

- **`cargo fix` (envolver o corpo)** — satisfaz o lint, destrói o sinal. É a forma que o ecossistema
  Rust aceita para migrar para a edition 2024, e é errada para o que este item quer.
- **`#[allow]` por módulo nos dois arquivos densos** — teria trocado a densidade por invisibilidade
  justamente onde há mais operações.
- **Baseline numérico em CI** — avisa depois do fato e precisa de curador. Com o número em zero,
  `deny` domina em todos os eixos.
- **Adiar os dois arquivos grandes** (permitido pelo DoD) — descartado porque a densidade mostrou
  que a visibilidade compensa lá também, e porque a ferramenta tornou o custo tratável.
