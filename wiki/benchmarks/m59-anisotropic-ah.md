---
type: Measurement
title: m59 — quantização anisotrópica + Asymmetric Hashing: negativo com achado mecânico
description: A implementação está correta e validada, e mede paridade — mas desta vez a causa-raiz é entendida com precisão, e ela aponta para o carrier, não para o quantizador.
resource: git:f7c7b93:docs/benchmarks/m59-anisotropic-ah.md
tags: [benchmark, quantizacao, anisotropica, carrier, honest-negative, m59]
milestone: M59
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m59
    resource: git:f7c7b93:docs/benchmarks/m59-anisotropic-ah.md
    title: M59 — Quantização anisotrópica + AH
---

**Veredito: HONEST-NEGATIVE com achado mecânico preciso** — e a diferença entre este e os negativos
anteriores está justamente no "preciso".

O índice foi **implementado e validado por correção**, com toda a suíte verde e compatibilidade
retroativa intacta. Mas o ganho de QPS que fecharia o gap **não se materializa no carrier de grafo**: a
500k × 768d, o caminho quantizado fica em **paridade** com precisão plena — 1,01 a 1,03× —, tanto in-RAM
quanto sob pressão.

# O que muda em relação aos negativos anteriores

Os anteriores diziam "não ganhou". Este diz **por que não ganhou, com aritmética que bate com a
medição**: o gargalo é **working set e leitura de página**, não scoring — e a causa era o **layout**
co-localizando códigos com vetores, de modo que ler 4 bytes de código paginava 3 KB de vetor.

A conta completa está no [ADR 0019](/decisions/0019-m59-ah-needs-code-vector-separation.md), incluindo o
desfecho: **corrigir o layout era necessário mas não suficiente** — porque sob pressão o rerank lê
vetores frios que recolocam o custo.

**A conclusão: o lever anisotrópico está correto e é a base; o ganho exige um carrier de batch-scan
contíguo, não a caminhada por ponteiros do grafo.**

# Método herdado

O harness **reusa** o do milestone anterior — geração de dados, queries, ground truth, medição e o
padrão de constranger a RAM entre build e medição. Reusar o instrumento é o que torna dois negativos
consecutivos **comparáveis entre si**, em vez de dois experimentos distintos que por acaso deram
negativo.

Máquina dedicada e limpa, com o mesmo guard de carga.

# Consequência

Esta medição, somada à do [m57](/benchmarks/m57-sbq-superiority.md), é o que levou a vendorizar um
quantizador com carrier IVF ([ADR 0032](/decisions/0032-vendor-rabitq-rs-core.md)) — e essa aposta
também terminou medindo **memória, não QPS**
([ADR 0036](/decisions/0036-m74-rabitq-conditional-lever-verdict.md)).
