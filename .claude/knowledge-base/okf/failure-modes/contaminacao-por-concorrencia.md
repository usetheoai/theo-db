---
type: Failure Mode
title: Medir com carga concorrente e atribuir o resultado ao código
description: Qualquer processo competindo pela box durante uma medição — inclusive um do próprio operador — desloca o número, e o deslocamento vira propriedade alegada do código.
tags: [medicao, contencao, benchmark]
timestamp: 2026-07-30T00:00:00Z
---

# Medir com carga concorrente e atribuir o resultado ao código

## Assinatura

Números que não reproduzem entre coletas do **mesmo binário**. Absolutos que derivam monotonicamente com o
horário. Um braço de A/B que degrada mais que o outro sem razão mecânica.

## Casos pagos

| Caso | O confundidor | Efeito medido |
|---|---|---|
| M168 | CI + k3s na mesma box (8 vCPU, load 21) | o **mesmo binário** deu −0,6% numa coleta e **+2,3%** noutra — 2,9 pontos de deriva sem mudança de código |
| M169 | **eu**, rodando `count(*)` de verificação por **921 s** dentro do baseline | q4–q11 mediram contenção, não código |
| M169 | `timeout 1800 psql` mata o *cliente*, não a consulta | o backend do braço OFF ficou órfão **1862 s** e rodou junto com o braço ON |

## Custo

O M168 gastou doze rodadas de revisão em grande parte por causa disto. O M169 perdeu a primeira passada de 43
consultas.

## Como evitar

1. **Box dedicada** — e ela só é dedicada se o operador também sair de cima dela. A regra que adotei:
   *enquanto uma medição estiver em voo, nada meu toca o cluster.*
2. **Gate de ocioso antes de medir** — `select count(*) from pg_stat_activity where state='active'` deve dar 0,
   senão **aborta**. Barato e determinístico.
3. **`statement_timeout` no servidor**, nunca `timeout` no cliente — o backend se cancela e não deixa órfão.
4. Para comparar coletas separadas no tempo, use [technique/desenho-ababab](../techniques/desenho-ababab.md).

## Relacionados

- [technique/desenho-ababab](../techniques/desenho-ababab.md)
- [invariant/nao-usar-a-box-do-ci](../invariants/nao-usar-a-box-do-ci.md)
