---
type: Measurement
title: M169 medido — ClickBench 100M vai de 28/43 para 30/43, e a leitura honesta é 28 pelo streaming + 2 pelo recuo eager
description: Medido 2026-07-31 na mesma box, mesmo corpus (99.997.497 linhas), mesmo teto de 300 s, com o so_md5 como única variável. Consertadas q20/q32/q33/q34. A regressão q08/q09 (EMFILE no spill) foi corrigida pelo ADR-0059, mas elas completam pelo RECUO ao eager, com consumo O(N). Byte-identidade provada 4/4 (q20/q32/q33/q34) contra o gêmeo heap, 0 divergentes.
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

## Estado depois do fix da regressão (atualizado 2026-07-31)

As duas regressões foram corrigidas ([ADR-0059](../../../../docs/adr/0059-m169-fail-open-cobre-falha-de-spill.md))
e remedidas com o binário `debde5f3`: **q08 `ok` 28,5 s, q09 `ok` 36,6 s, q32 `ok` 295,6 s**.

Duas ressalvas que a soma esconde e que fazem parte do número:

- **q08/q09 completam pelo RECUO ao caminho eager**, com o consumo O(N) que o milestone remove. Provado por duas
  linhas de `theodb_agg_stream_fallback` no log do servidor, uma por consulta, cada uma com
  `Os { code: 24, "Too many open files" }`. A leitura honesta é **28 pelo streaming + 2 pelo recuo**, não
  "30/43" liso — e o harness hoje **não** distingue os dois, porque `agg_routed` vem do `EXPLAIN`, que é fato
  de planejamento e é idêntico nos dois braços.
- **A q32 passa com 1,5% de margem** (295,6 s contra teto de 300 s). Frágil, não folgado.

## Byte-identidade — parcialmente provada

O campo `ab_identical` da corrida é `None` nas 30: aquele harness mede **conclusão**, não correção. A prova veio
depois, por `benchmarks/m169_ab_verify.py` contra o gêmeo `hits_heap`, com ordem total e o pushdown agregado
confirmado no plano antes de comparar:

**4/4, 0 divergentes, 0 não-verificadas** (`docs/benchmarks/m169-ab-verify.md`, binário `debde5f3`):

| q | byte-idêntica | linhas (colunar/heap) | colunar | heap |
|---|---|---|---|---|
| q20 | **sim** | 1 / 1 | 59,5 s | 165,7 s |
| q32 | **sim** | 10 / 10 | 279,9 s | 886,2 s |
| q33 | **sim** | 10 / 10 | 110,2 s | 474,3 s |
| q34 | **sim** | 10 / 10 | 111,2 s | 469,4 s |

Efeito colateral com número, registrado porque alguém vai perguntar por que a verificação demorou: o lado
colunar é **2,8× a 4,2× mais rápido** que o gêmeo heap nas mesmas consultas. Não é claim de performance — é o
custo do oráculo, medido com teto de 4h e sem controle de deriva.

As 11 `timeout` são todas `agg_routed = false` — não entram no caminho agregado, então este milestone não as
toca. A q17 reproduziu 301,6 s contra 301,6 s do baseline, e a q18 (que completa) deu 149,5 s contra 147,1 s —
1,6 % de diferença, evidência de que as duas corridas são comparáveis.

## Relacionados

- [measurement/teto-offsets-i32](teto-offsets-i32.md) — o defeito-alvo, 3 instâncias
- [measurement/pico-do-groupby-e-linear-na-cardinalidade](pico-do-groupby-e-linear-na-cardinalidade.md) — a reta, e a previsão dela que caiu aqui
- [measurement/limite-de-escala-100m-nao-conclusao](limite-de-escala-100m-nao-conclusao.md) — o baseline contra o qual isto é subtraído
- [failure-mode/contagem-agregada-mistura-classes-de-falha](../failure-modes/contagem-agregada-mistura-classes-de-falha.md) — por que o delta é publicado por CLASSE
- [failure-mode/extrapolar-reta-para-regime-de-outro-mecanismo](../failure-modes/extrapolar-reta-para-regime-de-outro-mecanismo.md)
