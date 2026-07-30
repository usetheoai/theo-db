---
type: Failure Mode
title: O teste passa — e passaria também sem o fix
description: Um teste que não falha no código ANTIGO não prova nada; validação real pegou três defeitos assim nos meus próprios testes.
tags: [teste, tdd, falso-verde]
timestamp: 2026-07-30T00:00:00Z
---

# O teste passa — e passaria também sem o fix

## Os três casos, todos meus, todos no mesmo milestone (M144)

Validação **real** no droplet — não só compilar — pegou três defeitos nos testes que eu havia escrito:

| Teste | O defeito | Por que passava |
|---|---|---|
| T1.3 `#[pg_test(error="vectorizer delete failed")]` | a string nunca casaria | pgrx faz `longjmp` do erro SQL cru; a mensagem esperada não era a emitida |
| T2.4 caso EDGE construindo CSR denso a `u32::MAX` | OOM de ~34 GB | o CSR é `vec![0u64; nn+1]` indexado por id cru — o teste matava a máquina antes de testar |
| T2.2 sanitização de Unicode | **passava no código ANTIGO também** | o desync corrompe mas redige o segredo, então a asserção frouxa não distinguia antes de depois |

O terceiro é o mais instrutivo: um teste que passa na versão **sem** o fix não é teste de regressão, é decoração.
Corrigido para asserção de saída limpa exata — RED→GREEN de verdade.

## A assinatura

O teste nunca esteve vermelho. Se o RED não foi **observado**, não há prova de que o teste discrimina.

## Como evitar

- **RED antes de GREEN, observado** — não presumido. `cycle-implement` exige isso, e a razão é exatamente esta.
- Quando o RED não é possível (teste retroativo), rode-o contra o commit anterior. Se passar, ele não testa o fix.
- Desconfie de asserção que casa **substring genérica** ou que só verifica "não lançou".

## Relacionados

- [technique/controle-positivo](../techniques/controle-positivo.md)
- [failure-mode/medicao-vacuosa-aceita](medicao-vacuosa-aceita.md)
