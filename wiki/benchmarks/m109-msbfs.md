---
type: Measurement
title: m109 — BFS multi-origem vetorizado: benchmark de cruzamento
description: Avança N buscas juntas por máscaras de bits, e trava a correção por um oráculo de hash de conjunto por lane, em todo N testado.
resource: git:f7c7b93:docs/benchmarks/m109-msbfs.md
tags: [benchmark, grafo, msbfs, vetorizacao, oraculo, m109]
milestone: M109
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m109
    resource: git:f7c7b93:docs/benchmarks/m109-msbfs.md
    title: M109 — Vectorized Multi-Source BFS
    last_modified: 2026-07-16
---

# O que é medido

BFS multi-origem em lote — **uma chamada, com N buscas avançadas juntas** por máscaras de bits de 64
posições — contra **N buscas sequenciais** de origem única.

É um **benchmark de cruzamento**: a pergunta não é se é mais rápido, e sim **a partir de qual N** o
formato em lote passa a valer. Abaixo do cruzamento, o overhead de montar as máscaras não se paga.

# O oráculo, que é o detalhe forte

A correção é **travada em todo N** por um **hash de conjunto por lane**: o conjunto alcançável de cada
lane é provado **byte-idêntico** ao da busca de origem única correspondente.

Verificar **por lane, e não só o agregado**, é o que pega o erro típico de vetorização: as máscaras
avançarem corretamente no total mas **misturarem origens entre si**. Um oráculo agregado não veria isso;
um por lane vê.

**Hash de conjunto** é a escolha certa porque a ordem de descoberta não importa — só o conjunto —, e
comparar conjuntos por hash é barato o bastante para rodar em todo ponto da varredura.

# Contexto

É a fase seguinte do pilar aberto pelo [ADR 0048](/decisions/0048-m107-native-graph-engine-go.md), sobre
a estrutura persistida em [m108](/benchmarks/archive/m108-persisted-csr.md).
