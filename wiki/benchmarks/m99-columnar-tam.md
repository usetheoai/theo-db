---
type: Measurement
title: m99 — o substrato colunar: ganho de tamanho, e scan em paridade ou mais lento por desenho
description: Declara antes dos números que este milestone entrega storage, não execução — então um scan mais lento é o esperado, e o ganho medido é compressão.
resource: git:f7c7b93:docs/benchmarks/m99-columnar-tam.md
tags: [benchmark, columnar, table-access-method, compressao, teto-honesto, m99]
milestone: M99
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m99
    resource: git:f7c7b93:docs/benchmarks/m99-columnar-tam.md
    title: M99 — theodb_columnar TAM benchmark
    last_modified: 2026-07-16
---

# O teto honesto, declarado antes dos números

Este milestone entrega o **substrato de storage** — formato colunar em disco, com compressão por coluna e
diretório de mínimo e máximo, e MVCC delegado a um catálogo heap.

Ele **ainda não tem** projeção pushdown, consumo do diretório para pular blocos, nem execução
vetorizada — **isso é o milestone seguinte**.

**Consequência declarada:** um scan comum **decodifica todas as colunas de todos os blocos** e reconstrói
tuplas completas, então seu tempo é **paridade ou pior que o heap, por desenho**.

**O ganho medido é tamanho em disco, por compressão. Isto não é claim de superioridade de
performance.**

Declarar isso **antes** dos números é o que impede a leitura errada — porque a leitura natural de "tabela
colunar mais lenta que heap" seria "o colunar falhou", quando na verdade **falta a metade que faz o
colunar valer**.

# Por que esse desenho é correto

Separar **storage** de **execução** é o que permite entregar e validar um de cada vez. E o storage aqui é
o que o [ADR 0042](/decisions/0042-m99-own-code-columnar-tam.md) chama de metade da costura: **o
substrato in-core que o pilar de planner único precisa** para empurrar scans para dentro.

O truque de correção que ele reusa — delegar visibilidade ao MVCC de uma linha de catálogo — é o que
evita reimplementar MVCC, **a coisa de maior risco que um access method de tabela poderia fazer**.

# A outra metade

A execução vetorizada, e o ganho que ela materializa, estão em
[m100](/benchmarks/m100-datafusion-executor.md).
