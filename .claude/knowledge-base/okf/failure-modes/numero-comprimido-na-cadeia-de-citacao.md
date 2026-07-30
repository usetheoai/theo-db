---
type: Failure Mode
title: Um resumo funde dois números em um, e a cadeia de citação propaga o erro
description: Artefato mede duas grandezas; o ADR as comprime numa; o conceito cita o ADR e herda o erro — e o ADR cita o artefato que o contradiz.
resource: docs/benchmarks/gap1-extend-candidates.md
tags: [citacao, numero, adr, propagacao]
timestamp: 2026-07-30T00:00:00Z
---

# Um resumo funde dois números em um, e a cadeia de citação propaga o erro

## A cadeia, com o caso real

| Elo | O que diz | Certo? |
|---|---|---|
| **Artefato** `gap1-extend-candidates.md:39` | "a iso-recall 0.988, o theodb precisa de **~5× o `ef`** → **~1.8× mais lento**" | ✅ duas grandezas distintas |
| **ADR** `0035:21` | "a iso-recall alta o theodb ainda precisa de **~1,8× o `ef`**" | ❌ fundiu as duas |
| **Conceito OKF** | citava o ADR, fielmente | ❌ herdou |

O ADR **cita o artefato que o contradiz** — a fonte estava a um clique, e o resumo mesmo assim comprimiu.

## Por que é traiçoeiro

O conceito estava **fiel à sua fonte declarada**. Verificar "o `resource:` resolve? a linha diz isso?" — que é o
que o C6 e o `citation-verifier` fazem — **passa**. O defeito está um elo acima, e nenhum gate o alcança.

E o resultado comprimido é **plausível**: 1,8× existe no artefato, só que como outra grandeza. Um número
inventado é fácil de pegar; um número **real no lugar errado** não.

## Como evitar

Quando um conceito cita um **resumo** (ADR, verdict consolidado, README), abrir também o **artefato primário que
o resumo cita**. Se os dois discordam, o artefato vence — ele é a medição; o resumo é a leitura dela.

Sinal barato: um resumo que menciona **um** multiplicador onde o artefato tem **dois** (ganho e custo, `ef` e
latência, recall e QPS) quase sempre perdeu um.

## Relacionados

- [failure-mode/correcao-nao-propagada-pelo-grafo](correcao-nao-propagada-pelo-grafo.md) — a propagação lateral; esta é a vertical
- [failure-mode/crenca-intermediaria-congelada](crenca-intermediaria-congelada.md)
- [technique/nenhuma-alegacao-sem-medicao](../techniques/nenhuma-alegacao-sem-medicao.md)
