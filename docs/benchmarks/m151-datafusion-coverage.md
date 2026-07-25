# M151 — Cobertura do CustomScan vetorizado DataFusion: `<>` + cross-type, medido

**Data:** 2026-07-25
**Milestone:** M151 (ampliar a cobertura do CustomScan vetorizado — rotear mais agregados do ClickBench pelo caminho Arrow/DataFusion)
**Box de medição:** droplet DigitalOcean `c-8` (8 vCPU / 16 GB), efêmero, destruído ao fim. **NÃO** é o box canônico AWS `c6a.4xlarge` — os timings/QPS não são comparáveis a leaderboard (ADR M128-2).
**Build:** `theodb_rs` release, PostgreSQL 18.4 do pgrx. Dataset: ClickBench `hits` real, subamostrado a 100.000 linhas (systematic, CC-BY-NC-SA, CI-only).
**Artefatos:** `docs/benchmarks/m151-artifacts/m151-clickbench-agg.json` (as 43 queries, A/B por-query).

## TL;DR

> **A cobertura do CustomScan vetorizado sobe para 14 das 43 queries do ClickBench, com A/B byte-idêntico ao
> heap (`diverged=0`, 43/43 pass).** A contribuição do M151 é **+3 agregados** (`q1`, `q7`, `q41`) que filtram por
> `<>`/`=`/`<` em coluna numérica com literal de outro tipo (cross-type). O measurement-first foi decisivo: revelou
> que o `<>` exact-type roteava zero queries reais e que o lever verdadeiro é a coerção cross-type.

## Atribuição honesta (o que cada milestone contribuiu)

| Estado | Cobertura | Queries roteadas (0-idx) | Contribuição |
|---|---|---|---|
| Baseline pré-M149 (M131) | **6** | 0, 2, 3, 6, 15, 32 | agregados escalares/GROUP-BY sem WHERE |
| Após M149 (projection, v0.141.0) | **11** | +19, 23, 24, 25, 26 | projeções `SELECT col … WHERE` (o CustomScan de projeção) |
| **Após M151 (cross-type, este)** | **14** | **+1, 7, 41** | **agregados com `<>`/`=`/`<` cross-type** |

- **q1:** `SELECT COUNT(*) FROM hits WHERE AdvEngineID <> 0` — `AdvEngineID` é `int2`, o `0` é `int4` (cross-type `<>`).
- **q7:** `SELECT AdvEngineID, COUNT(*) … WHERE AdvEngineID <> 0 GROUP BY AdvEngineID` — cross-type `<>` + GROUP BY.
- **q41:** `SELECT WindowClientWidth, WindowClientHeight, COUNT(*) … GROUP BY …` — agregado com predicado numérico cross-type.

Honestidade (Regra 3): o salto 6→11 é do **M149** (já released); o M151 entrega o salto **11→14**. Reportar o 14
como "ganho do M151" seria desonesto — o número real e atribuído está acima.

## Correção — A/B das 43 queries

| Métrica | Valor |
|---|---|
| `result_ab.diverged` | **0** |
| `result_ab.pass` | **43/43** |
| `columnar_customscan_count` | **14** (era 6 no baseline pré-M149) |

`diverged=0` é o gate de correção (Rule 5): TODA query roteada ao DataFusion retorna o resultado byte-idêntico ao
heap. Isso inclui os predicados **cross-type coercidos** — a coerção `int4→int2` (range-checked) nunca muda o
resultado, e um const fora do range da coluna (`s <> 40000` num `int2`) **declina** e cai no plano nativo (também
byte-idêntico). Provado adicionalmente no harness focado: `Custom Scan (theodb_columnar_agg)` para `a<>0` (int4),
`s<>0` (cross-type) e `s=0` (cross-type) + A/B idêntico em count/GROUP-BY/multi-predicado/out-of-range.

## Mecanismo (as duas descobertas que o measurement-first corrigiu)

1. **`<>` NÃO é uma estratégia btree** (btree define só 1-5: `<,<=,=,>=,>`). É detectado como o **negador do `=`**
   (`get_negator` → o `=` da família btree, strategy 3). O council rust-pgrx provou via `pg_operator.c` que isso é
   sound (o PG bloqueia duplo-negador → `get_negator(op)==` implica que `op` é o `<>` canônico).
2. **O ClickBench `<>`/`=` é cross-type** (coluna `int2`, literal `int4`). O extractor exigia `consttype==vartype`
   (D5) → declinava `AdvEngineID(int2) <> 0(int4)`. A primeira medição (6→11) era **inteiramente do M149**; o `<>`
   exact-type roteava **zero** queries reais. O lever verdadeiro é a **coerção cross-type**: `encode_const_coerced`
   lê o const no seu tipo e casta ao domínio min/max da coluna com **range-check** (out-of-range → declina, seguro).
   Isso desbloqueia `<>` E `=`/`<`/`>` cross-type — o padrão dominante do ClickBench.

## Limitação honesta — `<>` em texto (follow-up)

`<>` em coluna **texto** (`SearchPhrase <> ''`, a maioria das queries `<>` do ClickBench) **não roteia** por este
milestone. Razão técnica (ADR-4 do plano): o const de texto não cabe no `ZonePredicate` (`const_bits: u64`) nem na
serialização `custom_private` (`encode_private` usa `lappend_int`). Serializá-lo pelo caminho agg **released**
(M114/M115) é uma fatia própria com risco ao caminho estável — honest-negative, não rushed. É o próximo slice
bem-especificado. (As queries texto-`<>` q13/q30/q31 continuam no plano nativo, correto, sem o ganho vetorizado.)

## Metodologia / reprodução

```bash
# no box de medição, theodb_rs instalado (cargo pgrx install --release), enable_columnar_agg = on:
PGPORT=<p> PGUSER=pgtest python3 benchmarks/run_m128_clickbench.py --agg --n 100000 --sample systematic \
  --out m151_agg.json
#   -> columnar_customscan_count = 14 ; result_ab.diverged = 0 ; pass 43/43
# harness focado (cross-type routing + A/B): m151_validate.sql (3 EXPLAIN + DO-block A/B), M151_VALIDATE_OK.
```

Nota de honestidade: box self-hosted (não o `c6a.4xlarge` canônico) → o `columnar_customscan_count` (a cobertura)
é o resultado; os timings absolutos não são leaderboard-comparáveis. A cobertura é determinística (independe do
box/tamanho), então o 6→14 é reprodutível; o `diverged=0` é o gate de correção.
