---
type: Failure Mode
title: Um benchmark que re-implementa o código mede uma cópia — ele passa enquanto a produção regride
description: O bench do pgvectorscale re-implementa a estrutura de candidatos como cópia standalone, sem nenhum teste que assegure equivalência com a de produção.
resource: .claude/knowledge-base/discoveries/blueprints/fu1-samegraph-scan-microbench-blueprint.md
tags: [benchmark, metodologia, divergencia, teste]
timestamp: 2026-07-30T00:00:00Z
---

# Um benchmark que **re-implementa** o código mede uma cópia

## O anti-pattern, achado no prior art (FU-1)

Ao desenhar um micro-benchmark de scan, a varredura das duas referências Rust do campo encontrou formas opostas:

| Projeto | Como o bench toca o código |
|---|---|
| **pgvectorscale** | `benches/lsr.rs:38-64` **re-implementa** `ListSearchResult` como cópia standalone; a de produção vive em `access_method/graph/mod.rs`. **Nenhum teste assegura que as duas são equivalentes.** |
| **vectorchord** | crates puros e modulares — o bench exercita **o código real** |

O risco é nomeado no próprio blueprint como *known divergence hazard*: **o bench pode passar enquanto a busca de
produção regride**. As duas cópias divergem no primeiro refactor que toque só uma.

## Por que isto acontece, e por que é sedutor

O código de produção costuma estar preso a um runtime caro (aqui: uma extensão pgrx, que exige um servidor
PostgreSQL vivo). Copiar a estrutura para dentro do bench torna a medição fácil — e é exatamente essa facilidade
que a torna inútil como sinal de regressão.

O resultado é um número **honesto sobre a cópia** e **mudo sobre o produto**.

## A alternativa que preserva os dois

Abra uma **costura DIP** e faça o bench exercitar o caminho de produção através dela: uma trait/interface pequena
(`NeighborSource`, no caso) que a produção implementa contra páginas reais e o bench implementa contra um grafo
fixo em memória. O **loop de busca medido é o mesmo objeto** nos dois lados; só a fonte de dados muda.

Isso dá simultaneamente: código de produção sob medição, grafo **byte-idêntico** entre braços (elimina o
nondeterminismo de build) e medição intercalada no **mesmo processo** (elimina o ruído de box que já custou um
falso `+122%` de deriva no controle).

## Relacionados

- [technique/ablacao-mesmo-indice](../techniques/ablacao-mesmo-indice.md)
- [technique/braco-de-controle-inalterado](../techniques/braco-de-controle-inalterado.md) — a deriva de +122% que motivou o FU-1
- [failure-mode/assert-que-e-uma-identidade](assert-que-e-uma-identidade.md)
