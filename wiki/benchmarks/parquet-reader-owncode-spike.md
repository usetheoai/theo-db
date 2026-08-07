---
type: Measurement
title: spike do leitor Parquet próprio: viável
description: Um spike deliberadamente falsificável que mediu paridade a 1/13 do tamanho, e cujo GO autorizou remover o último componente C++ do projeto.
resource: git:f7c7b93:docs/benchmarks/parquet-reader-owncode-spike.md
tags: [benchmark, spike, parquet, datafusion, falsificavel, own-code]
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: pqspike
    resource: git:f7c7b93:docs/benchmarks/parquet-reader-owncode-spike.md
    title: Spike — leitor Parquet own-code
    last_modified: 2026-07-22
---

**Veredito: VIÁVEL — GO.**

# A pergunta

O binário consegue ler Parquet externo **em código próprio**, usando bibliotecas Apache-2.0 **que já
estão dentro dele**, sem a dependência C++?

O documento se identifica como **spike falsificável** — ou seja, ele tinha um resultado possível que
mataria a proposta, e isso estava definido antes de rodar.

**Um spike sem condição de falha é uma demonstração.**

# O que foi medido

**Paridade byte a byte** contra o caminho existente, ao custo de **+9 MB** contra os **118 MB** do bundle
que sairia — cerca de **1/13 do tamanho**.

Note que as bibliotecas necessárias **já estavam no binário** por causa do pilar colunar; faltava
**ligar** o leitor. É o degrau da escada de parcimônia que diz "a dependência já instalada resolve?" — e
aqui resolvia.

# O que este GO autorizou

A **remoção total** da dependência ([ADR 0057](/decisions/0057-m143-pgduckdb-total-removal.md)) — tirando
**o último componente C++ do projeto** —, com o lakehouse passando ao build default e a imagem opcional
sendo aposentada.

E um efeito colateral que o spike não buscava: **removida a dependência, a restrição que forçava o
desenho de codegen desapareceu**, e a superfície ficou mais simples. Ver
[m143](/benchmarks/m143-pgduckdb-removal.md).
