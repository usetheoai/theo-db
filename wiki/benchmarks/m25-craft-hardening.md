---
type: Measurement
title: m25 — hardening de código: evidência antes e depois
description: Refactor preservador de comportamento com complexidade ciclomática medida por ferramenta, não estimada — e o arquivo que ficou acima do budget virou emenda de ADR em vez de exceção silenciosa.
resource: git:f7c7b93:docs/benchmarks/m25-craft-hardening.md
tags: [benchmark, refactor, complexidade, lizard, code-quality, m25]
milestone: M25
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m25
    resource: git:f7c7b93:docs/benchmarks/m25-craft-hardening.md
    title: M25 — Craft Hardening evidence
    last_modified: 2026-07-01
---

**Natureza:** refactor **preservador de comportamento** — sem mudança funcional e sem dependência nova.
A prova é a suíte inalterada passando em paridade **mais** a complexidade medida caindo abaixo dos
limiares.

# Complexidade medida, não estimada

A medição usa uma ferramenta de complexidade ciclomática, com limiar de consenso ≤ 10:

| Função | antes | depois | como |
|---|---|---|---|
| geração de NL→SQL | **19** | **8** | decomposta em validação, checagem de allowlist e wrapper |
| validação sintática (extraída) | — | 7 | guarda pura de statement único e tokens banidos |
| checagem de allowlist (extraída) | — | 5 | pura |
| fusão RRF | **12** | **9** | extraída a resolução do vetor de query |
| resolução do vetor (extraída) | — | 4 | vetor explícito vence, senão embeda |

**Usar ferramenta em vez de julgamento** é o que torna esta evidência auditável: o número antes e o
número depois vêm do mesmo instrumento sobre duas árvores de código identificadas por commit.

# O que este milestone entregou de mais interessante

Ele **não** conseguiu cumprir literalmente o próprio critério. O arquivo de superfície SQL ficou em 640
linhas, acima do budget de 500 — e isso está registrado no documento, não maquiado.

A resposta foi uma **emenda formal de ADR**
([ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md)), argumentando que o budget é
heurístico, que o arquivo é ~87% SQL declarativo com complexidade ciclomática próxima de zero, e que
fatiá-lo seria **complexidade acidental auto-imposta**.

**O defeito que a revisão apontou não foi o desenho — foi a divergência não registrada.** É a diferença
entre descumprir um critério e descumpri-lo em silêncio.

# Relacionado

A decisão de arquitetura resultante é o [ADR 0009](/decisions/0009-theodb-rs-api-surface-single-module.md).
