---
type: Measurement
title: M169 medido — ClickBench 100M vai de 28/43 para 30/43; as 4 falhas roteadas caem e 2 consultas REGRIDEM
description: Medido 2026-07-31 na mesma box, mesmo corpus (99.997.497 linhas), mesmo teto de 300 s, com o so_md5 como única variável. Consertadas q20/q32/q33/q34; regredidas q08/q09 por exaustão de descritores no spill. A byte-identidade NÃO foi executada.
resource: benchmarks/m169_delta.py
tags: [clickbench, 100m, streaming, regressao, spill, delta, m169]
timestamp: 2026-07-31T00:00:00Z
---

# M169 — o delta medido, com a regressão na frente

## Os números

Duas corridas, mesma box (16 vCPU / 31 GB), mesmo `data_directory`, mesmo corpus de **99.997.497** linhas,
`statement_timeout = 300 s`, `work_mem = 256MB`. A **única** linha que muda entre elas é o `so_md5`
(`a6ab6507…` → `5ba1e09e…`) — é essa a variável independente.

| | antes (T1.2) | depois (T4.1) |
|---|---|---|
| completam | 28/43 | **30/43** |
| `error:XX000` | 4 | 2 |
| `timeout` | 11 | 11 |

### Consertadas — 4, todas atribuíveis

Falhavam **com** `agg_routed = true`, então o caminho que este milestone toca é o que explica a mudança:

| q | antes | depois |
|---|---|---|
| q20 | `error:XX000` 52,1 s | **ok** 59,5 s |
| q32 | `timeout` 303,6 s | **ok** 290,5 s |
| q33 | `error:XX000` 57,3 s | **ok** 125,0 s |
| q34 | `error:XX000` 48,4 s | **ok** 125,7 s |

q20/q33/q34 são as três instâncias do [teto de offsets i32](teto-offsets-i32.md) — mesma coluna (`URL`), causa
única. A **q32 não estava prevista**: eu havia concluído por aritmética que ela não teria como completar. Ver
[failure-mode/extrapolar-reta-para-regime-de-outro-mecanismo](../failure-modes/extrapolar-reta-para-regime-de-outro-mecanismo.md).

### REGREDIDAS — 2, e esta é a linha que um "+2 consultas" esconde

| q | antes | depois | erro |
|---|---|---|---|
| q08 | `ok` 13,1 s | `error:XX000` 12,3 s | `Failed to create partition file` (hint: `ulimit -n`) |
| q09 | `ok` 17,4 s | `error:XX000` 11,9 s | idem |

As duas são `COUNT(DISTINCT UserID) … GROUP BY RegionID`. Soft limit de descritores do postmaster medido:
**1024** (hard: 1.048.576).

## O mecanismo — um só, com dois sinais

O streaming deu ao agregado a capacidade de **derramar para disco**, que o caminho eager (um único batch
gigante, sem ponto de parada) não tinha. Isso **salva** a q32, cujo estado deixa de precisar caber na pool, e
**quebra** q08/q09, cujo spill de `COUNT(DISTINCT)` esgota os descritores.

Que a mesma explicação preveja o ganho **e** o dano colateral é o que a sustenta; uma que só servisse ao caso
favorável seria suspeita.

## O que este número NÃO prova

**A byte-identidade não foi executada.** O campo `ab_identical` é `None` nas 30 consultas que completam, e o
resumo da corrida diz literalmente `A/B: n/a — nenhuma comparação executada`. Está provado que **saem de erro
para `ok`**; **não** está provado que o resultado é igual ao do `hits_heap`. O DoD do milestone exige as duas
coisas, e tratar "completou" como "correto" seria
[aceitar um verde sem execução](../failure-modes/cobertura-alegada-sem-execucao.md).

As 11 `timeout` são todas `agg_routed = false` — não entram no caminho agregado, então este milestone não as
toca. A q17 reproduziu 301,6 s contra 301,6 s do baseline, e a q18 (que completa) deu 149,5 s contra 147,1 s —
1,6 % de diferença, evidência de que as duas corridas são comparáveis.

## Relacionados

- [measurement/teto-offsets-i32](teto-offsets-i32.md) — o defeito-alvo, 3 instâncias
- [measurement/pico-do-groupby-e-linear-na-cardinalidade](pico-do-groupby-e-linear-na-cardinalidade.md) — a reta, e a previsão dela que caiu aqui
- [measurement/limite-de-escala-100m-nao-conclusao](limite-de-escala-100m-nao-conclusao.md) — o baseline contra o qual isto é subtraído
- [failure-mode/contagem-agregada-mistura-classes-de-falha](../failure-modes/contagem-agregada-mistura-classes-de-falha.md) — por que o delta é publicado por CLASSE
- [failure-mode/extrapolar-reta-para-regime-de-outro-mecanismo](../failure-modes/extrapolar-reta-para-regime-de-outro-mecanismo.md)
