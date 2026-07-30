---
type: Honest Negative
title: DoD de ≤1,2× vs pgvector FALSIFICADO — page-native é 7-23× mais lento
description: O caminho page-native não alcança o alvo; o own-path fica em ~1,95× a recall 1.0. Registrado como ADR-0033.
resource: docs/benchmarks/m118-resume-discarded.md
tags: [vetorial, storage, veredito]
timestamp: 2026-07-30T00:00:00Z
---

# DoD de ≤1,2× vs pgvector **falsificado** — page-native é 7-23× mais lento

## O veredito (M118)

O DoD pedia ficar em **≤1,2×** do pgvector. Medido: o caminho **page-native** é **7-23× mais lento**; o own-path
fica em **~1,95×** a recall 1.0. DoD falsificado, registrado em docs/benchmarks/m118-resume-discarded.md.

> **CORRIGIDO 2026-07-30 após review.** O conceito atribuía o registro do veredito ao **ADR-0033** —
> **nenhum ADR menciona o M118**. O ADR-0033 é a proposta de reposicionamento do North Star, e o artefato do M118
> o cita como *"consistente com"*, jamais como o registro. O veredito vive em
> `docs/benchmarks/m118-resume-discarded.md` (`:17` DoD FALSIFIED, `:20` 7-23× slower, `:33` ~1,95×). A
> **substância** confere byte a byte; o defeito era de proveniência — e **herdado do arquivo de memória**.

## O achado de método embutido

O bug de recall que apareceu no caminho foi encontrado **por evidência**, não por inspeção — o que reforça que
recall é propriedade a medir, nunca a inferir do desenho.

## Correlato — E2 / SymQG in-PG

Mesmo padrão: o AM estava **correto**, e ainda assim o `hnsw` era **2,6-3,9× mais rápido** em warm. O "page tax"
é real e não desaparece com corretude de implementação. Gate não atingido; próximo lever identificado
(FastScan 1-bit SIMD) — e depois medido em **1,07–1,22×** por ablação mesmo-índice (contra os 2,8× que a comparação cross-box
sugeria) — a faixa, não o topo dela, que é o arredondamento-para-o-favorável que este bundle condena.

## Relacionados

- [technique/ablacao-mesmo-indice](../techniques/ablacao-mesmo-indice.md)
