---
type: Failure Mode
title: Aplicar o teste errado e publicar a significância dele
description: Empate contado como derrota, família de multiplicidade errada, clustering ignorado, e a magnitude tirada da coleta mais lisonjeira.
resource: references/papers/rigorous-perf-eval-georges-2007.pdf
tags: [estatistica, benchmark, rigor]
timestamp: 2026-07-30T00:00:00Z
---

# Aplicar o teste errado e publicar a significância dele

## Assinatura

Um `p` pequeno que não sobrevive a nenhuma das três correções óbvias. Uma magnitude que muda conforme a coleta
escolhida.

## Caso pago — M168, um `p=0,014` destruído por três caminhos independentes

| Defeito | Efeito |
|---|---|
| **Empate contado como derrota** — q25 coleta B, 128,7 vs 128,7 | o teste do sinal exclui empates; incluí-los enviesa contra |
| **Família de multiplicidade errada** | com Bonferroni × 4 o `p` vira **0,066** |
| **Clustering ignorado** | 2 de 5 coletas apontavam para o lado oposto; a unidade de replicação era a coleta, não o par |
| **Magnitude da coleta mais lisonjeira** | pares 5-6 por coleta: 12,0 · 14,2 · 12,4 · 16,8 · **17,7** · 13,6 — publiquei 17,7. Corrigido para **13,6** (pool) |

Correlato: no M123/M130/M131, **CV baixo não é significância pareada** — um coeficiente de variação apertado em
cada braço não diz nada sobre a diferença entre os braços.

## Como evitar

- Teste do sinal: **exclua empates** explicitamente do `n`.
- Declare a família de multiplicidade **antes** de olhar os `p`.
- Se as coletas são a unidade de replicação, o teste é sobre coletas — não sobre pares dentro delas.
- Publique a magnitude do **pool**, ou a **menor**; nunca a que favorece.
- Leia `references/papers/rigorous-perf-eval-georges-2007.pdf` antes de medir (regra R3 do projeto).

## Relacionados

- [technique/desenho-ababab](../techniques/desenho-ababab.md)
- [measurement/deriva-de-box-m168](../measurements/deriva-de-box-m168.md)
