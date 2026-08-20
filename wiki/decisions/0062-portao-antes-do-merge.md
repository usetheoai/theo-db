---
type: Decision
title: ADR 0062 — O portão passa a olhar antes do merge, e paga por si trocando de gatilho
description: O gatilho de pull_request foi removido em 2026-08-12 porque o runner era único, serial e pago. Os runners migraram para GitHub-hosted num repositório público, onde são grátis, ilimitados e paralelos — a premissa caiu, e a medição mostra que o gate no PR custa MENOS que o gate pós-merge que ele substitui.
tags: [adr, ci, portao, pull-request, runner, custo-medido, b-052]
adr_id: "0062"
adr_status: Accepted
decision_date: 2026-08-20
generated: { by: claude-code/opus-5, at: 2026-08-20T22:00:00Z }
---

Este ADR **desfaz uma decisão explícita do owner**, e é por isso que ele existe em vez de um commit
silencioso. A decisão anterior estava certa quando foi tomada; o que mudou não foi a opinião, foi a
máquina.

Decisão irmã sobre o mesmo eixo — o que a esteira mede e sob qual autoridade:
[ADR-0061 — Todo pilar mensurável tem benchmark oficial público](0061-benchmark-oficial-por-pilar.md).

# Contexto

Em 2026-08-12 o gatilho de `pull_request` foi removido dos dez workflows, com a razão escrita em cada
arquivo:

> *"o runner é único e serial, e cada PR disparava a esteira inteira a cada push, com custo elevado"*

Era verdade e era um bom motivo. O `theodb-do` é uma máquina paga, única e serial: dez workflows
disparados pelo mesmo push enfileiram atrás uns dos outros. O custo medido disso foi registrado no
próprio `actionlint.yml` — o `lint-rust` levou **de 66 a 120 minutos** para rodar um
`cargo fmt --check`, quase tudo fila.

A consequência foi medida pelo [[B-052]] em 2026-08-13, e é severa: como `rules/git-safety.md § 1`
manda que **todo** trabalho nasça em `workspace`, e nenhum gate olhava `workspace`, o primeiro
momento em que um portão via a mudança era **depois** de ela estar integrada em `develop`. Entre a
última execução em `workspace` e a medição: **73 commits**, **13 tocando `theodb_rs/src/`**,
**+2.414/−7.420 linhas**, **0 execuções**.

# O que mudou, e é isso que reabre a decisão

Em 2026-08-20 os doze workflows migraram para `runs-on: ubuntu-latest`, e o repositório é **PUBLIC**.
Runner padrão do GitHub em repositório público é **grátis, ilimitado e paralelo**. As três
propriedades que motivaram a remoção — pago, único, serial — deixaram de valer, todas ao mesmo tempo.

Uma decisão cuja premissa inteira caiu não é uma decisão que se respeita por deferência. É uma
decisão que se remede.

# A medição, porque o DoD do B-052 exige custo comparado e não estimado

Janela de 9 dias, 2026-08-11 a 2026-08-20, lida de `gh run list` (150 corridas):

| origem | corridas PESADAS | minutos |
|---|---|---|
| `push` em `develop` | **30** | 1410 |
| `push` em `main` | 5 | 65 |

No mesmo período, **13 PRs `workspace → develop` foram mergeados**.

Ou seja: **30 corridas pesadas para 13 integrações** — porque cada push para `develop` dispara a
esteira, e um PR mergeado nem sempre corresponde a um único push.

# Decisão

Os oito workflows pesados passam a rodar em `pull_request: [develop, main]`, e o `push` deles fica
restrito a `main`.

**Não é acrescentar um gatilho — é TROCAR de gatilho.** Acrescentar dobraria as corridas, e a
restrição de capacidade continua sendo premissa do item mesmo depois de o custo ter caído: capacidade
grátis não é razão para desperdiçá-la.

Custo projetado a partir da medição: **~13 corridas pesadas** (uma por PR, com
`cancel-in-progress` colapsando pushes sucessivos ao mesmo PR) **+ 5 em `main`**, contra as **35** de
hoje. Menos corridas, e olhando o código enquanto ele ainda pode ser recusado.

## Por que `pull_request` testa a coisa certa

`pull_request` roda contra `refs/pull/N/merge` — a árvore **mergeada**, não a ponta do branch. É
exatamente o que o `push` para `develop` testava, um passo antes. Sem isso a troca seria um
downgrade disfarçado.

## O que fica em aberto, dito em vez de omitido

- **A árvore de merge pode envelhecer.** Se `develop` avança entre a corrida do PR e o merge, o que
  foi testado não é o que vai entrar. O GitHub recalcula o merge ref mas **não** re-roda sozinho. A
  proteção é a opção *"Require branches to be up to date before merging"* na branch protection —
  configuração de repositório, não de arquivo, e portanto **não entregue por este ADR**. Enquanto
  não estiver ligada, a janela existe; ela é menor que a de hoje, não zero.
- **`push` em `develop` deixou de rodar.** Um push direto para `develop` — proibido por
  `git-safety.md § 1` e bloqueado pelo hook local, mas possível em outra máquina sem o hook — passa
  sem portão. A alternativa era manter os dois gatilhos e dobrar o custo, o que o DoD proíbe.
- **Nada disto substitui rodar os gates localmente.** O procedimento do cabeçalho do `ci.yml`
  continua válido e foi usado no próprio PR que trouxe esta mudança — onde `cargo fmt --check`
  reprovou um teste recém-escrito. Sem o gate no PR, aquilo só apareceria depois de já estar em
  `develop`, que é precisamente o defeito que o [[B-052]] mediu.

# Consequências

- Um defeito passa a ser recusado antes de contaminar `develop`, em vez de ser descoberto pelo
  próximo a integrar.
- O número de corridas pesadas cai.
- A decisão de 2026-08-12 fica **superada, não apagada**: o texto dela permanece nos workflows acima
  do bloco novo, porque a razão pela qual ela existiu é o que explica por que este ADR precisou
  medir antes de desfazê-la.
