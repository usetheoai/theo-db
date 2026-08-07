---
type: Measurement
title: m55 — o muro do VACUUM: RAM de pico, lock exclusivo e WAL
description: Caracteriza uma parada total de ~86 s a 100k e projeta linearmente para a escala-alvo, marcando a projeção como não medida e de baixa confiança.
resource: git:f7c7b93:docs/benchmarks/m55-vacuum-wall.md
tags: [benchmark, vacuum, memoria, lock, projecao, bloqueador, m55]
milestone: M55
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m55
    resource: git:f7c7b93:docs/benchmarks/m55-vacuum-wall.md
    title: M55 — VACUUM-fold wall baseline
---

**Caracterização, não comparação competitiva** — e a evidência que ancora a decisão de
[manutenção a escala](/decisions/0017-m55-index-maintenance-at-scale.md).

# O muro, medido

A 100k × 768d, o fold do índice inteiro:

- segura o lock **EXCLUSIVO por ~86 s** (91 s de tempo de parede) — **parada total de queries vetoriais
  por cerca de um minuto e meio**;
- pico de residência de **~1,44 GB**;
- **~340 MB de WAL**.

# A projeção, marcada como projeção

Para a escala-alvo de 1M × 768d, o modelo linear projeta **~14 GB de RAM, ~14 min de parada e ~3,4 GB de
WAL**.

**O documento marca isso como projeção O(N), NÃO medida**, e declara a confiança como **baixa** — porque
é extrapolação de **um único ponto**. As escalas maiores **não couberam na máquina**, e isso é dito em
vez de omitido.

**Uma projeção rotulada como projeção, com a confiança declarada, é usável; a mesma projeção apresentada
como medição corromperia toda decisão que a consumisse.**

# Três notas honestas que o artefato carrega

- O fold O(N) foi acionado pelo limiar default, e uma tentativa de desligá-lo falhou porque o parâmetro
  exige valor mínimo — **a warning é benigna, e o fold reconstruiu o índice inteiro de qualquer forma**,
  o que é justamente o que os 86 s e os 340 MB provam.
- A medição de residência **privada** falhou, então o número reportado é o **pico total como proxy** —
  com a justificativa de por que o proxy é aceitável.
- As escalas maiores foram **puladas por gate de RAM**, com o cálculo da necessidade contra a memória
  disponível registrado — evitando um OOM em vez de descobri-lo.

# Consequência

O muro **é real e escala linearmente**, o que confirma a decisão de adotar o desenho híbrido — tombstone
in-place no caminho comum e fold apenas para compaction — e torna a fase 1 daquele desenho
**pré-requisito de qualquer alegação de produção**.
