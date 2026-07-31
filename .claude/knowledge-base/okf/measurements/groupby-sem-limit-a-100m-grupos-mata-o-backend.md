---
type: Measurement
title: `GROUP BY` de ~10⁸ grupos SEM `LIMIT` consome 19,5 GB de anon-rss e o kernel MATA o backend — o custo é o RESULT SET, não o estado do agregado
description: Medido 2026-07-31 na box de 31 GB. A MESMA consulta (q32 do ClickBench) completa em 295,6 s com `LIMIT 10` e é OOM-killed sem ele. O discriminador é quantas linhas voltam ao cliente, não quantos grupos o agregado constrói — os dois casos constroem os mesmos ~10⁸.
resource: benchmarks/m169_ab_verify.py
tags: [groupby, memoria, oom, clickbench, resultset, 100m, oraculo]
timestamp: 2026-07-31T00:00:00Z
---

# `GROUP BY` de 10⁸ grupos: com `LIMIT` completa, sem `LIMIT` mata o servidor

## Os números

Consulta q32 do ClickBench, `hits` colunar com 99.997.497 linhas, box de 16 vCPU / 31 GB, `work_mem = 256MB`:

| forma | desfecho |
|---|---|
| `… GROUP BY WatchID, ClientIP ORDER BY c DESC **LIMIT 10**` | **ok**, 295,6 s |
| a mesma, **sem** `LIMIT` | **backend morto por signal 9** |

```
[Fri Jul 31 19:49:31 2026] Out of memory: Killed process 175145 (postgres)
    total-vm:28213236kB  anon-rss:19493436kB  shmem-rss:3816960kB
19:49:39 postgres: client backend (PID 175145) was terminated by signal 9: Killed
19:49:39 postgres: terminating any other active server processes ... reinitializing
```

O cluster inteiro reinicializou e fez crash recovery. `hits` e `hits_heap` sobreviveram porque a segunda já
havia sido convertida para `LOGGED` — fosse `UNLOGGED`, teria sido truncada
([invariant](../invariants/unlogged-truncado-por-recovery.md)) e o oráculo passaria a comparar contra vazio.

## O que discrimina os dois casos

**Não é o estado do agregado.** As duas formas constroem os mesmos ~10⁸ grupos. O que muda é **quantas linhas
voltam ao cliente**: com `LIMIT 10` o servidor devolve 10; sem ele, materializa e envia ~10⁸.

Isso é uma correção fina, mas importante, sobre
[a reta de pico do GROUP BY](pico-do-groupby-e-linear-na-cardinalidade.md): aquela mede o pico da **pool do
DataFusion**, que o streaming limita e faz derramar. O que matou aqui está **fora** dessa pool — é a
materialização do result set no lado PostgreSQL. Um knob que capa a pool não capa isto.

## Como isto apareceu (e o que ensina sobre oráculos)

Não foi uma medição planejada: foi o **oráculo de byte-identidade** que se matou sozinho. Ele removia o
`LIMIT N` final antes de comparar — regra herdada do oráculo do M128, cujo motivo é legítimo (empates fazem o
corte escolher 10 linhas arbitrárias-mas-válidas, e comparar isso seria falso-negativo de ordem de varredura).

A regra é correta para consultas pequenas e **inviável** nesta classe. A saída certa não é abandonar o
desempate: é tornar a ordem **TOTAL** — acrescentar as colunas de saída como critérios posicionais
(`ORDER BY c DESC, 1, 2, 3, 4, 5 LIMIT 10`) — de modo que os dois lados devolvam deterministicamente as MESMAS
10 linhas, e a consulta continue sendo a forma que o ClickBench define, que é a que se quer provar.

Custo colateral do defeito: a queda envenenou a conexão e as duas consultas seguintes viraram
`cursor already closed`, aparecendo no relatório como `roteou=NÃO` — artefato do envenenamento lido como
medição. Um oráculo deve abrir conexão **por consulta**, senão perde dado bom por causa de dado ruim.

## Relacionados

- [measurement/pico-do-groupby-e-linear-na-cardinalidade](pico-do-groupby-e-linear-na-cardinalidade.md) — o pico DA POOL; este conceito mede o que está fora dela
- [measurement/delta-medido-m169-28-para-30](delta-medido-m169-28-para-30.md)
- [invariant/unlogged-truncado-por-recovery](../invariants/unlogged-truncado-por-recovery.md) — por que as tabelas sobreviveram
- [invariant/maintenance-work-mem-nao-capa-rss-de-rust](../invariants/maintenance-work-mem-nao-capa-rss-de-rust.md) — a mesma forma: um knob que não capa o que se supõe
- [failure-mode/medicao-vacuosa-aceita](../failure-modes/medicao-vacuosa-aceita.md) — o oráculo que compara o caminho errado
