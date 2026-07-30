---
type: Technique
title: Acervo local primeiro, web depois, memória do modelo por último
description: 25 PDFs e 33 repos versionados no acervo; citar arquivo:linha do disco é mais barato, offline e já passou pelo gate de licença.
resource: rules/discover-phd-rigor.md
tags: [pesquisa, rigor, R0]
timestamp: 2026-07-30T00:00:00Z
---

# Acervo local primeiro, web depois, memória do modelo por último

> **MOVIDO de `invariants/` para `techniques/` em 2026-07-30 após review.** É um **método de pesquisa exigido**
> (R0/R0.1), não uma propriedade de plataforma — e o gatilho de leitura de `invariants/` ("mexer em storage, FFI,
> recovery…") nunca dispara quando o agente vai **pesquisar**, então o conceito estava no tipo que o tornava
> inalcançável no momento de uso. O `Failure Mode` que o originou é
> [diagnostico-aceito-sem-reproduzir](../failure-modes/diagnostico-aceito-sem-reproduzir.md) — opinar de memória
> do modelo é a forma extrema dele.

## O invariante (R0.1 / R0 de `discover-phd-rigor.md`)

1. **Acervo local** — 25 PDFs em `references/papers/` e 33 repos de peers em `references/`, catalogados em
   `references-catalog.md`. Citar `arquivo:linha` ou o PDF.
2. **Web** (WebSearch/WebFetch) — obrigatória para o que o acervo não cobre: SOTA posterior ao clone, blogs,
   releases. **R0 é regra máxima**: deep research sem varredura web verificável é *deep-research theatre*.
3. **Conhecimento interno do modelo** — último recurso, e **declarado como tal**.

## Atalhos por assunto

| Tema | Abrir antes de opinar |
|---|---|
| `unsafe` / FFI / pgrx | `references/pgrx/` |
| colunar / vetorização | `papers/morsel-parallelism-leis-2014.pdf`, `papers/monetdb-x100-boncz-2005.pdf`, `references/datafusion/` |
| vetorial | `papers/hnsw-*.pdf`, `papers/scann-*.pdf`, `references/hnswlib/` |
| lexical / BM25 | `papers/iir-manning-2008-BOOK.pdf`, `references/tantivy/` |
| **qualquer afirmação de performance** | `papers/rigorous-perf-eval-georges-2007.pdf` |

## Invioláveis

- O acervo é **read-only** (hook `boundary-check.sh`); achados vão para `knowledge-base/discoveries/blueprints/`.
- **Citação que não resolve no disco não entra** — é hard cap dos golden rules de discover.

## Relacionados

- [invariant/licenca-agpl-e-study-only](../invariants/licenca-agpl-e-study-only.md)
- [failure-mode/diagnostico-aceito-sem-reproduzir](../failure-modes/diagnostico-aceito-sem-reproduzir.md)
- [technique/desenho-ababab](desenho-ababab.md)
