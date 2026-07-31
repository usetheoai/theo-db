---
type: Measurement
title: A 100M o modo de falha deixa de ser lentidão e vira NÃO-CONCLUSÃO — e a taxa depende do REGIME de memória
description: Medido duas vezes, 19/43 (box 15 GB, corpus maior que a RAM) e 28/43 (box 31 GB, corpus em page cache). Os dois números NÃO são comparáveis entre si; a classe vale nos dois.
resource: docs/benchmarks/m169-baseline-100m.md
tags: [escala, clickbench, colunar, limite, regime-de-memoria]
timestamp: 2026-07-30T00:00:00Z
---

# A 100M o modo de falha deixa de ser lentidão e vira **não-conclusão**

## Duas medições, dois regimes — e por que a diferença NÃO é o delta do produto

| | M162 (2026-07-29) | M169 (2026-07-30) |
|---|---|---|
| box | 8 vCPU / **15 GB** | 16 vCPU / **31 GB** |
| regime do corpus (16 GB colunar) | **maior que a RAM** | **cabe em page cache** (5 GB usados / 24 GB cache) |
| `statement_timeout` | 300 s | 300 s |
| corpus | 99.997.497 linhas | 99.997.497 linhas |
| **completam** | **19/43** | **28/43** |
| corrida chegou ao fim? | **não** — OOM-killed em 24/43 | **sim** — 43/43 vereditos |

**A diferença de 9 consultas NÃO mede melhoria de código: entre as duas corridas não houve mudança no
caminho colunar.** Ela mistura regime de memória e execução completa-vs-truncada, e nenhum dos dois pode
ser isolado *post hoc*. Atribuir a diferença ao código exigiria manter tudo o mais constante — a disciplina
de [ablação mesmo-índice](../techniques/ablacao-mesmo-indice.md), aqui aplicada ao regime de memória em vez
de ao índice.

O baseline honesto do M169 é o **28/43**, e o delta que o milestone pode reivindicar é T4.1 contra T1.2 na
**mesma box, mesmo dataset, mesmo teto**.

## O discriminador que impede a atribuição errada: `agg_routed`

Das **15 falhas** do M169, só **4 estão no caminho agregado colunar** — o que este milestone toca:

| q | veredito | `agg_routed` | leitura |
|---|---|---|---|
| **q20, q33, q34** | `error:XX000` | **true** | `byte array offset overflow` — o alvo, **3 instâncias**, não 1 |
| q32 | `timeout` | **true** | roteia e não termina — pico de ESTADO, não de offsets (**corrigida no T4.1**: 290,5 s) |
| q17, q19, q21…q28, q39 (11) | `timeout` | **false** | executor de linha do PostgreSQL; nenhuma mudança colunar as move |

Sem esse recorte, "28/43" é ambíguo: 11 das 15 falhas nem entram no caminho que o milestone endereça, e
contá-las junto infla o alvo. O sinal usado é o **agg-específico** (`plan_shows_agg_pushdown`), não o amplo
`Custom Scan (theodb_columnar`, que é quase sempre verdadeiro — ver
[contagem agregada mistura classes de falha](../failure-modes/contagem-agregada-mistura-classes-de-falha.md).

Duas consequências medidas que contrariam o palpite óbvio:

- **q19 passou a estourar o teto** aqui, e no M162 completava. Mais RAM não monotonamente melhora.
- **q23**, que no M162 caiu por **backend OOM**, aqui apenas estoura o teto com `agg_routed=false`. A RAM
  dobrada removeu o OOM; o caminho colunar não participa. Atribuir isso a melhoria de produto seria erro.

## O número original (M162), preservado

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

- [measurement/teto-offsets-i32](teto-offsets-i32.md) — o defeito-alvo; **3 instâncias** medidas no M169 (q20, q33, q34)
- [measurement/gap-vs-clickhouse-m159](gap-vs-clickhouse-m159.md) — o baseline de 1M que previu o alargamento
- [measurement/scanplan-e-on](scanplan-e-on.md)
