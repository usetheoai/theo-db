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
| M169 | `cargo-udeps` não roda sem `~/.pgrx/config.toml` | emitiu `auditor_unavailable_cargo-udeps` — honesto, e o cap existe para isso |
| M169 | `tree_sitter_languages` incompatível com `tree_sitter 0.26` | emitiu `auditor_unavailable_tree-sitter-rust` com a mensagem *"the audit did not run; this is NOT evidence that no symbol is fabricated"* |
| M168 | verdict citava um `so_md5` que nenhum artefato carregava | corrigido com `m168_collect_all.sh`, que grava proveniência em **todo** log |

O acerto aqui é do design: `code-quality-golden-rule.md` trata `auditor_unavailable_*` como **soft cap**, não
como PASS. A falha é humana — tratar o soft cap como ruído a dispensar.

## Como evitar

- Nunca dispense um `auditor_unavailable` por ADR quando o auditor **pode** rodar noutro lugar. Rode lá.
  (No M169, mover o `/code-quality` para a box com pgrx tirou os dois caps e deu `PASS_WITH_CAVEATS`.)
- Todo artefato de medição carrega `so_md5`, `postmaster`, `nproc`, `free`, `loadavg` — sem proveniência não é
  evidência.

## Relacionados

- [technique/proveniencia-em-todo-artefato](../techniques/proveniencia-em-todo-artefato.md)
