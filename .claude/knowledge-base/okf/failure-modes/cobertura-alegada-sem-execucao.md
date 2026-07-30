---
type: Failure Mode
title: Alegar cobertura que a ferramenta não produziu
description: Um auditor indisponível, um detector que não rodou ou um teste que não existe viram 'nada encontrado' em vez de 'não verificado'.
tags: [cobertura, honestidade, code-quality]
timestamp: 2026-07-30T00:00:00Z
---

# Alegar cobertura que a ferramenta não produziu

## Assinatura

Ausência de achado sendo reportada como ausência de defeito.

## Casos

| Caso | O que faltava | O que o sistema fez de **certo** |
|---|---|---|
| M161/M163-M165 | `cargo-udeps` não roda sem `~/.pgrx/config.toml` | emitiu `auditor_unavailable_cargo-udeps` — honesto, e o cap existe para isso |
| M146 | `tree_sitter_languages` incompatível com a versão do `tree_sitter` instalada | emitiu `auditor_unavailable_tree-sitter-rust` com a mensagem *"the audit did not run; this is NOT evidence that no symbol is fabricated"* |
| M168 | verdict citava um `so_md5` que nenhum artefato carregava | corrigido com `m168_collect_all.sh`, que grava proveniência em **todo** log |

O acerto aqui é do design: `code-quality-golden-rule.md` trata `auditor_unavailable_*` como **soft cap**, não
como PASS. A falha é humana — tratar o soft cap como ruído a dispensar.

## Como evitar

- Nunca dispense um `auditor_unavailable` por ADR quando o auditor **pode** rodar noutro lugar. Rode lá.

> **CORRIGIDO 2026-07-30 (round 3).** As duas linhas acima estavam atribuídas ao **M169**, e havia a frase
> "no M169, mover o `/code-quality` para a box com pgrx tirou os dois caps e deu `PASS_WITH_CAVEATS`".
> **Não existe audit de code-quality do M169** (`ls knowledge-base/audits/ | grep m169` → nada) — o milestone
> está em voo. Os caps são reais e vivem nos audits de M161/M163-M165 e M146; o desfecho afirmado não tinha
> artefato. Afirmar resultado de gate para milestone não concluído é a própria classe deste conceito.
- Todo artefato de medição **deve** carregar `so_md5`, `postmaster`, `nproc`, `free`, `loadavg` — sem
  proveniência não é evidência. **Nenhum script do repo grava os cinco hoje** (o exemplar grava 2): é dívida
  declarada em [technique/proveniencia-em-todo-artefato](../techniques/proveniencia-em-todo-artefato.md), não
  regra cumprida.

## A saída não é dispensar o cap — é rodar onde a ferramenta existe (medido no M169, 2026-07-30)

O cap `auditor_unavailable_*` é **ambiental**, não sistêmico, e a diferença é mensurável. O MESMO plano, o MESMO
comando, duas máquinas:

| Onde | `~/.pgrx/config.toml` | verdict | caps |
|---|---|---|---|
| box de desenvolvimento | **ausente** (sem `bison`/`flex`/`sudo` para criar) | `NON_SHIPPABLE` (70) | `auditor_unavailable_cargo-udeps` + `symbol_fab_unverifiable_rust` |
| box dedicada de bench | **presente** | **`SHIPPABLE_WITH_CAVEATS` (89)** | só `symbol_fab_unverifiable_rust` |

A causa é concreta: `cargo-udeps` precisa **compilar** o crate, e num crate pgrx o build script do `pgrx-pg-sys`
exige o config. Sem ele o auditor não roda — e emitir o cap ali é **correto**.

> **Dispensar por ADR um gate que consegue rodar é workaround.** A dispensa do golden rule existe para caps
> irremediáveis, não para caps que só precisam da máquina certa.

Um ADR anterior deste mesmo milestone dispensava as duas caps alegando defeito do detector, e trazia **duas
saídas de comando que nunca foram produzidas**. As duas premissas caíram na verificação — o ADR foi reescrito
para "roda onde o auditor existe, sem dispensa". A lição que ele passou a carregar: *a regra de reproduzir antes
de afirmar vale sobretudo para as alegações que me **favorecem*** — essa convinha, e foi por isso que passou.

## Relacionados

- [technique/nenhuma-alegacao-sem-medicao](../techniques/nenhuma-alegacao-sem-medicao.md)
- [failure-mode/diagnostico-aceito-sem-reproduzir](diagnostico-aceito-sem-reproduzir.md)
- [invariant/nao-usar-a-box-do-ci](../invariants/nao-usar-a-box-do-ci.md) — a box certa não é a do CI
