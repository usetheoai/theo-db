---
type: Measurement
title: m162 — o gap a 100M, com um falso-verde de cache pego e corrigido
description: A medição anterior a 1M reutilizava cache sem que isso fosse notado; aqui o dataset completo é re-materializado e o working set excede a RAM, entrando no regime real.
resource: git:f7c7b93:docs/benchmarks/m162-100m-gap-verdict.md
tags: [benchmark, escala, clickhouse, falso-verde, cache, m162]
milestone: M162
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: m162
    resource: git:f7c7b93:docs/benchmarks/m162-100m-gap-verdict.md
    title: M162 — gap a 100M
---

# O falso-verde que este milestone corrigiu

A medição anterior a 1M sofria de **reutilização de cache não percebida** — um falso-verde. Aqui o
dataset **completo** é re-materializado, com a contagem de linhas **verificada**, e não assumida a partir
de uma amostra.

**Verificar a contagem** é o tipo de checagem trivial que separa "rodamos a 100M" de "achamos que
rodamos".

# Por que 100M é o regime que importa

A esta escala **o working set excede a RAM da máquina**, de modo que queries frias **vão ao disco** — que
é o regime que a medição anterior marcara explicitamente como pendente.

**A escala não é vaidade: ela muda qual gargalo domina.** Um sistema que ganha em memória pode perder em
disco, e vice-versa — foi essa a lição de [m88](/benchmarks/m88-billion-scale-verdict.md), que **não
conseguiu** entrar nesse regime porque o build estourava antes.

# Os cuidados de execução

**Mediana de três execuções quentes**, timeout por statement, e **uma conexão nova por query, para que
um estouro de memória num backend não contamine o resto**.

Esse último detalhe é o que permite obter resultado parcial útil de um run que sofre falhas — em vez de
perder tudo por causa de uma query.

# Contexto

Estende o [gap medido a 1M](/benchmarks/m159-clickhouse-gap-verdict.md), que fora o primeiro baseline
real do adversário no repositório.
