---
type: Measurement
title: A 100M o modo de falha deixa de ser lentidão e vira NÃO-CONCLUSÃO: 19/43 consultas completam
description: O ClickHouse serve as 43 em 0,008-10,1 s no mesmo box; o TheoDB completa 19, com 5 falhas duras. A taxa de conclusão é o veredito, não a razão.
resource: docs/benchmarks/m162-100m-gap-verdict.md
tags: [escala, clickbench, colunar, limite]
timestamp: 2026-07-30T00:00:00Z
---

# A 100M o modo de falha deixa de ser lentidão e vira **não-conclusão**

## O número que é o veredito

| | ClickHouse | TheoDB |
|---|---|---|
| consultas que **completam** | **43/43** (0,008 s – 10,1 s) | **19/43 (44%)** |
| falhas **duras** | 0 | **5** |

As 5 falhas duras, com o tempo do ClickHouse ao lado:

| q | classe | ClickHouse | TheoDB |
|---|---|---|---|
| q17 | native row-exec | 3,8 s | `statement_timeout` (>300 s) |
| **q20** | **pushdown** | 1,9 s | **`byte array offset overflow`** — o bug de offset i32 |
| q21 | native row-exec | 2,1 s | `statement_timeout` (>300 s) |
| q22 | native row-exec | 2,7 s | `statement_timeout` (>300 s) |
| q23 | native row-exec | 0,6 s | conexão caída (**backend OOM** no box de 15 GB) |

Além das 5, a corrida foi **OOM-killed em 24/43**, deixando 19 consultas nunca executadas.

Dataset **real** verificado por contagem: **99.997.497 linhas** (o falso-verde de reuso de cache do M159 foi pego
aqui).

## A leitura correta, e ela não é a razão

O artefato é explícito: *"The scale-limit failure, not the ratio, is the verdict."* O geomean de **24,3×** sobre
os 19 sobreviventes é **cross-population** — 43 consultas no baseline contra 19 aqui — e é **carregado por
outliers**: removendo q0 (`COUNT(*)`, 1495×) e q19 (`SELECT *`, 837×) ele cai a **~15,5×** (n=17).

> **Uma nota de escala não se manifesta como "mais lento" — manifesta-se como consultas que não terminam.**

## O que o artefato declara NÃO ter medido

- **`NOT ISOLATED`** — I/O vs decode-CPU vs materialização não foram separados; o contador decisivo não foi
  capturado, e nenhum substituto (iostat, CPU-util) tampouco.
- `shared_blks_read` é o **instrumento errado** aqui — ver [instrumento-cego-a-arquitetura](../failure-modes/instrumento-cego-a-arquitetura.md).
- A razão medida **superestima** o gap (duas assimetrias favorecem o ClickHouse) — *"an earlier draft stated this
  backwards; corrected here"*.
- Cold-vs-hot é **bimodal**, não "cold ≫ hot".

## Relacionados

- [measurement/teto-offsets-i32](teto-offsets-i32.md) — o q20 desta tabela
- [measurement/gap-vs-clickhouse-m159](gap-vs-clickhouse-m159.md) — o baseline de 1M que previu o alargamento
- [measurement/scanplan-e-on](scanplan-e-on.md)
