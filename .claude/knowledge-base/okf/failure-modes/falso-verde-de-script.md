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
| M168 | gate casando `ARM=stream`, string que o harness não emitia | gate teatral; corrigido emitindo `RAISE NOTICE 'ARM=%'` e construindo controle positivo in-tree |
| M168 | oráculo de cancelamento não-diferencial | passava com a proteção removida |

## Como evitar

- Capture `$?` **imediatamente** após o comando que importa, numa variável, antes de qualquer outro comando —
  inclusive `echo`.
- Todo gate que procura uma string precisa de um teste que **prove** que a string é emitida no caminho feliz
  (controle positivo) **e** que o gate reprova quando ela some (controle negativo).
- Prefira `set -o pipefail` e verificação explícita a confiar no último `$?` de um pipeline.

## Relacionados

- [technique/controle-positivo](../techniques/controle-positivo.md)
- [failure-mode/medicao-vacuosa-aceita](medicao-vacuosa-aceita.md)
