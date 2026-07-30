---
type: Technique
title: Meça o incremento barato ISOLADO antes de pagar o caro — o lever planejado costuma não ser o lever
description: O plano do M89 escolheu FFI do tuplesort do PG; medir o incremento 1 sozinho mostrou que as cópias dominantes eram outras, e o incremento 2 sem FFI bateu o DoD.
resource: docs/adr/0039-m89-ambuild-streaming-verdict.md
tags: [parsimonia, medicao, planejamento, metodo]
timestamp: 2026-07-30T00:00:00Z
---

# Meça o incremento barato **isolado** antes de pagar o caro — o lever planejado costuma não ser o lever

## O caso (M89, ADR-0039)

O `ambuild` do `theodb_ivfflat` picava **~4× o dataset base** em RAM — dois OOM-kills a 30M num box de 62 GB
usáveis, capando o M88 a 16M. O plano (com `plan-confidence` SHIPPABLE_WITH_CAVEATS) e o grill escolheram a
**FFI do `tuplesort` do PostgreSQL**.

A implementação **não usou FFI**, e a razão foi medida:

| | Medido |
|---|---|
| Incremento 1 (clone-elimination) **isolado** | ainda OOMa a **4,21×** |
| As cópias dominantes de verdade | clone `list_entries()` (**16 GB**) + buffering `enc_vec`/`items` dos writers (**~32 GB**) |
| Incremento 2 (streaming page-writes), sem FFI | **1,28× / 1,50×** — 30M constrói no box de 64 GB |

O clone do build — o alvo que o plano assumia — **não era o gargalo**. Medir o incremento barato sozinho revelou
isso antes de qualquer linha de FFI ser escrita.

## A técnica

1. Quando o plano tem um passo **caro e arriscado** (FFI, fork, reescrita) precedido por um **barato**, implemente
   e **meça o barato isolado** primeiro.
2. Se o barato não move a agulha, isso é **informação sobre onde o custo mora** — não um fracasso do incremento.
3. Se o barato move a agulha até o DoD, o caro é **YAGNI medido** (degrau 1 da parsimony ladder), e o desvio do
   plano é **parsimony-positivo**: solução mais simples, DoD atingido, risco menor.
4. Registre o desvio no ADR com a medição que o justifica. Um desvio sem número é workaround; com número é
   engenharia.

> Isto é o inverso do sunk-cost: o plano ter escolhido a FFI não é razão para pagá-la.

## Relacionados

- [technique/a-forma-da-curva-diagnostica-a-causa](a-forma-da-curva-diagnostica-a-causa.md)
- [invariant/build-pica-4x-o-dataset-base](../invariants/build-pica-4x-o-dataset-base.md) — o teto que este trabalho fechou
- [failure-mode/diagnostico-aceito-sem-reproduzir](../failure-modes/diagnostico-aceito-sem-reproduzir.md)
