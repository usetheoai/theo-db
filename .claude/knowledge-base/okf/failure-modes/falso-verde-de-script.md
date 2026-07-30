---
type: Failure Mode
title: O script relata sucesso que não houve
description: Captura de código de saída do comando errado, gate que casa string inexistente, ou `rc=0` impresso depois de um `echo` — o log mente e ninguém confere.
tags: [falso-verde, shell, gate]
timestamp: 2026-07-30T00:00:00Z
---

# O script relata sucesso que não houve

## Assinatura

O log termina com `rc=0` e o trabalho não aconteceu. Ou o gate "passa" porque procura algo que nunca é emitido.

## Casos pagos

| Caso | O defeito | Consequência |
|---|---|---|
| M169 | `echo "=== end rc=$? ==="` — `$?` era do **`echo` anterior**, não do python | o log declarou `rc=0` enquanto o harness tinha sido **`Killed`** por OOM |

> **Fronteira arrumada 2026-07-30 após review.** Este conceito listava dois casos do M168 que também estavam em
> `medicao-vacuosa-aceita` — os mesmos incidentes em duas casas, que é o § 4.3 do contrato ao contrário. Cada um
> voltou para a casa certa: o gate casando literal nunca emitido é a **definição** de
> [gate-desligado-em-silencio](gate-desligado-em-silencio.md); o oráculo não-diferencial é a classe de
> [medicao-vacuosa-aceita](medicao-vacuosa-aceita.md) / [controle-positivo](../techniques/controle-positivo.md).
> Aqui fica só o que é exclusivamente deste conceito: **o log mente sobre o desfecho**.

## Como evitar

- Capture `$?` **imediatamente** após o comando que importa, numa variável, antes de qualquer outro comando —
  inclusive `echo`.
- Todo gate que procura uma string precisa de um teste que **prove** que a string é emitida no caminho feliz
  (controle positivo) **e** que o gate reprova quando ela some (controle negativo).
- Prefira `set -o pipefail` e verificação explícita a confiar no último `$?` de um pipeline.

## Relacionados

- [technique/controle-positivo](../techniques/controle-positivo.md)
- [failure-mode/medicao-vacuosa-aceita](medicao-vacuosa-aceita.md)
