---
type: Failure Mode
title: Duas sessões trabalhando no mesmo working tree
description: Dois agentes no mesmo checkout trocam de branch por baixo um do outro; o segundo encontra a árvore sem seus arquivos e pode concluir que perdeu trabalho.
tags: [git, coordenacao, operacao]
timestamp: 2026-07-30T00:00:00Z
---

# Duas sessões trabalhando no mesmo working tree

## Assinatura

Arquivos que você acabou de escrever somem da árvore. `HEAD` aponta para commits que você não fez. Um hook de
Stop bloqueia você por causa de um commit alheio.

## Caso pago — 2026-07-30

Outra sessão trocou o checkout de `develop` para `workspace-clean` e passou a commitar (#217, #219, #220, #171).
Meus cinco commits do M169 continuaram **íntegros em `develop`**, mas sumiram da árvore. O `stop-validation.sh`
me bloqueou exigindo CHANGELOG por causa de `theodb_rs/src/am/columnar_agg.rs` — alterado pelo commit **da outra
sessão**.

O `CLAUDE.md` do umbrella já nomeia isto: *"duplica o mesmo repositório em dois checkouts que divergem em
silêncio"*.

## Como evitar / recuperar

1. **Não entre em pânico e não force.** `git reflog` reconstrói a história de refs; commits não referenciados
   continuam no object store.
2. Antes de qualquer `switch`, meça: `git status --porcelain` (nada não-commitado?) e
   `git branch -a --contains <sha>` (o trabalho do outro está alcançável por algum ref?).
3. `git switch`, nunca `checkout` (`rules/git-safety.md` § 2).
4. **Nunca satisfaça um gate assinando mudança alheia** — escrever CHANGELOG pelo commit de outro é assumir
   autoria do que você não leu.
5. A solução estrutural é `git worktree` ou checkouts separados; a mitigação é commitar com frequência para
   reduzir a janela.

## Relacionados

- [invariant/git-switch-nao-checkout](../invariants/git-switch-nao-checkout.md)
