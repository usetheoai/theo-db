---
type: Invariant
title: O teto de escala costuma ser o BUILD, não a query — o ambuild picava ~4× o dataset base
description: Dimensionar a box pelo tamanho do índice é dimensionar pelo número errado: 30M OOMou a 64,7 GB num box de 62 GB usáveis enquanto o índice final tinha 15 GB.
resource: docs/adr/0038-m88-billion-scale-regime-verdict.md
tags: [escala, memoria, build, indice]
timestamp: 2026-07-30T00:00:00Z
---

# O teto de escala costuma ser o **build**, não a query

## O invariante medido (M88 → M89, ADR-0038 / ADR-0039)

O `ambuild` do `theodb_ivfflat` segurava simultaneamente: o `AnnIndex` inteiro (~1× base) **+** uma cópia coletada
**+** os buffers de página AQ/refine → **pico ~4,21× o dataset base** em anon-rss.

| Escala | Base | Pico do build | Resultado |
|---|---|---|---|
| 16M | 8,2 GB | ~34 GB | **maior que coube** num box de 64 GB |
| 30M | 15,4 GB | **64,7 GB** | **OOM-kill** (62 GB usáveis) |
| 30M (pós-M89) | 15,4 GB | 19,7 GB (**1,28×**) | completa |

O índice **final** v5 tinha 15 GB e o v6/SQ8 **4,46 GB**. Dimensionar a box pelo tamanho do índice teria dito
"cabe folgado" — e teria errado por 4×.

## As duas consequências que ninguém antecipa

1. **O DoD de escala pode ser barrado pelo build, não pelo alvo.** O M88 pedia ≥100M/1B e parou em 16M — *"um
   índice genuinamente out-of-RAM (índice > RAM) não foi construível"*. Registrado como dívida honesta, não como
   etapa pulada.
2. **Não dá para medir o regime out-of-RAM sem construir nele.** A hipótese inteira da track storage-separation
   (índice 3,52× menor → menos I/O → mais QPS) ficou `SIZE_CONFIRMED / OUT_OF_RAM_QPS_INCONCLUSIVE` porque o
   regime alvo não era alcançável.

## O irmão em plataforma diferente, mesma classe

`maintenance_work_mem` **não capa** o RSS quando o trabalho é Rust — ver
[maintenance-work-mem-nao-capa-rss-de-rust](maintenance-work-mem-nao-capa-rss-de-rust.md). Os dois juntos: o
orçamento de memória de um build não está nem no tamanho do artefato nem no knob do PG.

## Relacionados

- [technique/medir-o-incremento-isolado-antes-de-pagar-o-caro](../techniques/medir-o-incremento-isolado-antes-de-pagar-o-caro.md) — como o teto foi fechado
- [failure-mode/cold-medido-uma-vez-por-sweep](../failure-modes/cold-medido-uma-vez-por-sweep.md)
