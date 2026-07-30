---
type: Invariant
title: git switch e restore — nunca checkout, revert, reset --hard ou force-push
description: Comandos ambíguos ou destrutivos são proibidos por regra do projeto; os substitutos preservam a capacidade de recuperar.
resource: rules/git-safety.md
tags: [git, seguranca, regra]
timestamp: 2026-07-30T00:00:00Z
---

# `git switch` e `restore` — nunca `checkout`, `revert`, `reset --hard` ou force-push

## O invariante (Regra Inquebrável 4)

| Proibido | Por quê | Em vez disso |
|---|---|---|
| `git checkout` | ambíguo (branch vs arquivo); descarta trabalho com facilidade | `git switch <branch>` / `git restore <path>` |
| `git revert` | esconde a reversão atrás de um commit automático | um commit explícito que reverte |
| `git reset --hard` | destrói trabalho não-commitado de forma irrecuperável | `git stash` ou `git reset --soft` |
| force-push em `main` / `develop` / `workspace` | reescreve história compartilhada | `--force-with-lease`, e nunca nessas branches |

## O corolário que salvou trabalho

Commits não referenciados **continuam no object store**. Quando outra sessão trocou o checkout e meus arquivos
sumiram da árvore, `git reflog` reconstruiu a história de refs e `git branch -a --contains <sha>` provou que nada
estava órfão — nem o meu trabalho, nem o dela.

Pânico seguido de `reset --hard` teria transformado um susto em perda real. A regra existe justamente porque o
momento em que se sente vontade de usar o comando destrutivo é o momento em que ele é mais caro.

## Protocolo antes de trocar de branch num checkout compartilhado

O procedimento de três passos vive em
[failure-mode/duas-sessoes-num-checkout](../failure-modes/duas-sessoes-num-checkout.md) § Como evitar / recuperar
— **não é repetido aqui** (§ 4.3 do contrato: atualizar o dono, nunca bifurcar). Este conceito guarda a
**propriedade de plataforma**; aquele guarda o **cenário e a recuperação**.

## Nota sobre falso-positivo do hook

O `validate-command.sh` bloqueia a menção literal ao comando de force-push dentro de um `Bash`. Isso é
intencional e correto — mas significa que **documentar** o comando exige escrever o arquivo por outra via
(Write tool). Vale registrar para que ninguém conclua que o hook está quebrado.

## Relacionados

- [failure-mode/duas-sessoes-num-checkout](../failure-modes/duas-sessoes-num-checkout.md)
