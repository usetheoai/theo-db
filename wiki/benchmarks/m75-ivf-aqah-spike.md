---
type: Measurement
title: m75 — spike IVF-AQ+AH: a hipótese não-refutada, medida in-memory
description: Testa se o carrier de batch-scan entrega o ganho que o de grafo não deu, compondo peças que já existiam; o caveat in-memory que ele declara viria a ser load-bearing.
resource: git:f7c7b93:docs/benchmarks/m75-ivf-aqah-spike.md
tags: [benchmark, spike, ivf, asymmetric-hashing, caveat, m75]
milestone: M75
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m75
    resource: git:f7c7b93:docs/benchmarks/m75-ivf-aqah-spike.md
    title: M75 — Spike IVF-AQ+AH
    last_modified: 2026-07-10
---

# A hipótese sob teste

A única **não-refutada** que restava do [ADR 0019](/decisions/0019-m59-ah-needs-code-vector-separation.md):
que a quantização anisotrópica sobre um **carrier IVF de batch-scan contíguo** daria o ganho de QPS que o
carrier de grafo não deu.

# O que torna o spike barato

Ele **compõe peças que já existiam** — a partição IVF, o codebook anisotrópico e o **kernel de LUT em
lote**, este último **descoberto já pronto e testado**.

Testar uma hipótese cara montando o que já se tem, em vez de construir do zero, é o que permite ao gate
ser executado **antes** do investimento.

# Rigor de dados

Dataset **real**, com **ground truth exato** computado por força bruta sobre o corpus carregado — o que,
como o documento nota, **é válido em qualquer escala**, diferente de um ground truth herdado que só vale
no tamanho original.

E a compressão declarada: 16 bytes por código contra 512 de precisão plena, isto é 8×.

# O caveat que viria a ser load-bearing

O spike declara explicitamente que mede **in-memory, single-thread, sem o imposto de página e WAL**.

**Esse caveat era load-bearing**: o ganho de 5–7× medido aqui **não sobreviveu** ao caminho real de
access method, conforme o [m82](/benchmarks/m82-pgscann-headtohead.md) e o
[ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md) — porque ler os códigos **paginava também
os vetores**, e o scan era limitado por I/O, não por compute.

**A lição foi internalizada:** o spike seguinte da linhagem passou a exigir medição **dentro** do banco,
com o próprio documento dizendo que um segundo spike in-memory seria **teatro de medição**.
