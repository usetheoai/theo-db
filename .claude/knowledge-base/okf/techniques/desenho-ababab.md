---
type: Technique
title: Intercalar os braços em vez de medi-los em blocos
description: Comparar A e B em janelas separadas herda todo confundidor temporal; intercalar par a par o neutraliza — e a razão pareada mostra se o pareamento está funcionando.
resource: references/papers/rigorous-perf-eval-georges-2007.pdf
tags: [benchmark, estatistica, rigor]
timestamp: 2026-07-30T00:00:00Z
---

# Intercalar os braços em vez de medi-los em blocos

## A fonte

Georges, Buytaert & Eeckhout, **OOPSLA'07**, § 2.1.2:

> "Other considerations concerning the experimental design include […] **back-to-back measurements ('aaabbb')
> versus interleaved measurements ('ababab')**."

O paper nomeia isto como **eixo de desenho experimental**, não detalhe de execução.

## O caso que ensinou — M168, nos dois níveis

| Nível | Estrutura | Protegido? |
|---|---|---|
| **dentro** de uma coleta | `ababab` — o harness alterna eager/stream par a par | **sim** |
| **entre** coletas | `aaa…bbb` — A às 14:46, F às 22:13 | **não** — é o anti-padrão |

## O diagnóstico que só a razão pareada dá

Se o confundidor vazasse para o efeito, a razão marcharia com o tempo junto com os absolutos. Medido:

| | rho de Spearman vs ordem da coleta |
|---|---|
| `stream` **absoluto** | **+1,00** (monotonia perfeita) |
| **efeito** (razão pareada) | **+0,71** — crítico a n=6 é 0,886 |

Absolutos derivam de forma perfeita; o efeito **não** deriva. É exatamente o que se espera de um `ababab` correto
sob confundidor temporal — **o pareamento está funcionando**. Se o confundidor vazasse, o rho da razão também
seria ~1,00.

## O controle decisivo

Converter `aaabbb` em `ababab` no nível de coleta: **reconstruir o binário antigo e rodá-lo intercalado com o
novo, numa única janela.** Resultado no M168: o **mesmo** binário deu −0,6% numa coleta e **+2,3%** na
intercalada — 2,9 pontos de deriva sem mudança de código. Pergunta fechada por experimento.

## Relacionados

- [measurement/deriva-de-box-m168](../measurements/deriva-de-box-m168.md)
- [failure-mode/contaminacao-por-concorrencia](../failure-modes/contaminacao-por-concorrencia.md)
