---
type: Invariant
title: nohup ... & dentro de ssh não sobrevive ao fechamento do canal
description: Você acha que lançou o processo e não lançou. Exige script + setsid, com verificação de PID depois.
tags: [ssh, operacao, shell, falso-verde]
timestamp: 2026-07-30T00:00:00Z
---

# `nohup ... &` dentro de `ssh` não sobrevive ao fechamento do canal

## O invariante

```bash
ssh host 'nohup ./longo.sh > /dev/null 2>&1 &'    # <- pode NÃO sobreviver
```

Quando o `ssh` fecha o canal, o processo em background pode morrer junto — dependendo de como o shell remoto
trata o `SIGHUP` e de o `nohup` ter ou não conseguido se desprender a tempo. O sintoma é o pior possível:

> **você acha que lançou e não lançou.**

Nenhum erro, nenhum aviso — o comando retorna 0, e a corrida simplesmente nunca começou. Neste projeto isso
aconteceu **duas vezes** antes de virar lição.

## A forma que sobrevive

```bash
ssh host 'setsid ./longo.sh > /dev/null 2>&1 < /dev/null & echo "pid=$!"'
# e DEPOIS verificar que o PID existe:
ssh host 'ps -p <pid> > /dev/null && echo vivo || echo MORTO'
```

Três elementos, e nenhum é opcional: **script** (não comando inline), **`setsid`** (nova sessão, desligada do
terminal), e **verificação de PID depois** — porque o `echo "pid=$!"` prova que o shell criou o processo, não que
ele continua vivo um segundo depois.

## A regra transferível

Todo lançamento remoto de tarefa longa precisa de uma **confirmação separada de que a tarefa está viva** — pelo
PID, pelo log crescendo, ou por um marcador de início no arquivo. O código de saída do `ssh` fala do `ssh`, não
da tarefa.

## Relacionados

- [failure-mode/falso-verde-de-script](../failure-modes/falso-verde-de-script.md)
- [technique/separar-transporte-de-conteudo](../techniques/separar-transporte-de-conteudo.md)
