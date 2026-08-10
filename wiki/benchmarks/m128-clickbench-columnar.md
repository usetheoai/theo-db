---
type: Measurement
title: m128 — ClickBench sobre o colunar próprio: 43 queries byte-idênticas
description: Acrescenta ao benchmark oficial o oráculo de correção que ele não tem; e reporta que a otimização de pushdown bateu num bug real, medindo o caminho sem ela.
resource: git:f7c7b93:docs/benchmarks/m128-clickbench-columnar.md
tags: [benchmark, clickbench, columnar, oraculo, bug, m128]
milestone: M128
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m128
    resource: git:f7c7b93:docs/benchmarks/m128-clickbench-columnar.md
    title: M128 — Official benchmark COLUMNAR pillar
    last_modified: 2026-07-20
---

Segunda aplicação do padrão adotar-e-envolver, agora no pilar colunar.

**Veredito:** as **43 queries** rodam sobre a tabela colunar e são provadas **byte-idênticas** a uma
cópia em heap — **o oráculo de correção que o próprio ClickBench não tem**.

Essa é a lacuna que o [ADR 0050](/decisions/0050-official-benchmark-adopt-and-wrap.md) nomeou: o
`check` do benchmark oficial é literalmente um `SELECT 1`, de modo que **uma engine rápida e errada
poderia liderar sem ser detectada**. Aqui a correção é verificada query a query.

# O que não deu certo, e está dito

A otimização de pushdown vetorizado **bateu num bug real de planner** na tabela larga de 105 colunas do
dataset real — **registrado como issue**.

**A medição sólida e completa é sobre o caminho de storage**, com o executor nativo, e o pushdown fica
como follow-up rastreado.

**Reportar o resultado do caminho que funciona, nomear o bug do que não funciona, e não misturar os
dois** é o que mantém o artefato utilizável. A alternativa — atrasar tudo até o bug estar resolvido, ou
publicar números do caminho quebrado — seria pior de qualquer lado.

O bug foi diagnosticado e corrigido em [m131](/benchmarks/m131-columnar-agg-accelerated.md), onde a
causa-raiz reportada originalmente também se mostrou **errada**.

# Ressalva

Não é o hardware canônico, então **os tempos não são comparáveis ao leaderboard**.
