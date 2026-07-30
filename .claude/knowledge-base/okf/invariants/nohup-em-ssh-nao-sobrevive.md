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

## O erro SIMÉTRICO, e ele é pior: o comando de FOREGROUND sobrevive ao cliente (M169, 2026-07-30)

O canal `ssh` não controla o tempo de vida do processo remoto **em nenhuma das duas direções**. O caso acima é o
processo que morre quando você achou que sobreviveria. O simétrico:

```bash
timeout 900 ssh host 'psql -c "select count(*) from hits;"'    # o timeout mata o CLIENTE
```

O `timeout` (ou um `Ctrl-C`, ou a queda da rede) mata o **ssh local**. O comando remoto **continua rodando** — e
o backend que ele abriu continua consumindo CPU e I/O. Medido no M169: um `count(*)` órfão rodou **1489 s** depois
que meu lado já tinha recebido "exit code 0", competindo com a medição seguinte na mesma box:

| pid | idade | origem |
|---|---|---|
| 71494 | **1489 s** | órfão — cliente morto no timeout |
| 76437 | 366 s | a medição que eu achava estar sozinha |

Isto é o mesmo defeito que já custou um braço de A/B antes (um `psql` morto no timeout deixou um backend de
1862 s contaminando o braço seguinte). O sintoma é **carga que não se explica** e um número deslocado — ver
[medir-com-carga-concorrente](../failure-modes/contaminacao-por-concorrencia.md).

## A checagem que fecha os dois lados

Antes de qualquer medição numa box remota, **enumere o que está rodando lá** — não confie no seu histórico local:

```sql
SELECT pid, state, round(extract(epoch from now()-query_start)) AS idade_s, left(query,40)
FROM pg_stat_activity WHERE backend_type='client backend' AND pid <> pg_backend_pid();
```

E para matar um órfão: `pg_terminate_backend(pid)`. **Medido:** ele devolve `t` imediatamente mas o backend pode
levar até ~15 s para sair quando está numa espera de I/O (`DataFileRead`) — a latência é do ponto de
`CHECK_FOR_INTERRUPTS`, não um defeito de cancelabilidade. Confirme a saída por consulta, não pelo retorno.

## Relacionados

- [failure-mode/falso-verde-de-script](../failure-modes/falso-verde-de-script.md)
- [technique/separar-transporte-de-conteudo](../techniques/separar-transporte-de-conteudo.md)
- [failure-mode/medir-com-carga-concorrente](../failure-modes/contaminacao-por-concorrencia.md)
