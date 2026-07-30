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

## Relacionados

- [technique/proveniencia-em-todo-artefato](../techniques/proveniencia-em-todo-artefato.md)
