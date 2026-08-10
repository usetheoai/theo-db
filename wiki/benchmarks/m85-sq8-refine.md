---
type: Measurement
title: m85 — tier de rerank comprimido: ganho de memória, parcial honesto em QPS
description: Índice 3,5× menor com recall neutro, mas o veredito separa o eixo provado (tamanho) do eixo apenas parcial (QPS em cache quente).
resource: git:f7c7b93:docs/benchmarks/m85-sq8-refine.md
tags: [benchmark, sq8, rerank, memoria, veredito-parcial, m85]
dataset: SIFT1M
milestone: M85
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m85
    resource: git:f7c7b93:docs/benchmarks/m85-sq8-refine.md
    title: M85 — pg_scann v6 SQ8-refine
    last_modified: 2026-07-11
---

**Veredito: GO — ganho de memória (3,5× menor); parcial honesto em QPS de cache quente.**

**Um veredito com duas metades explicitamente diferentes** é mais útil que um veredito único: ele diz
quais decisões pode sustentar e quais não.

# O que muda

O rerank passa a operar sobre **códigos comprimidos** — 128 bytes por vetor em vez de 512 —, motivado
pelo achado do [m84](/benchmarks/m84-recall-confirmation.md) de que as leituras aleatórias em precisão
plena erodiam o ganho na fronteira de alto recall.

A quantização é **assimétrica**: a query permanece em precisão plena, e só os candidatos são
comprimidos — o que preserva a precisão do lado que não custa memória.

# Os dois eixos

**Tamanho: provado.** 3,5× menor, com neutralidade de recall dentro de um epsilon declarado.

**QPS em cache quente: parcial.** O ganho de tamanho **não se converte** em throughput quando tudo já
está em memória — o que é mecanicamente esperado: **menos bytes só importam quando os bytes precisam vir
do disco**.

**É exatamente por isso que a tese seguinte precisava de um regime out-of-RAM** para ser testada — e é aí
que ela [não pôde ser provada](/benchmarks/m88-billion-scale-verdict.md), porque o build estourava a
memória antes.

# Rigor

Mesmo dataset, mesmos parâmetros, dois índices sobre os **mesmos dados** — a disciplina de same-data
herdada de [m46](/benchmarks/m46-highrecall-qps.md).
