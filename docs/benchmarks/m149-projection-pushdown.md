# M149 — Projection pushdown no scan colunar: correção + ganho medido

**Data:** 2026-07-24
**Milestone:** M149 (projection pushdown via scan-replacing CustomScan `theodb_columnar_project`)
**Box de medição:** droplet DigitalOcean `c-8` (8 vCPU / 16 GB), efêmero, destruído ao fim.
**Build:** `theodb_rs` release, PostgreSQL 18.4 do pgrx. Dataset: ClickBench `hits` real (105 colunas).
**Artefatos:** `docs/benchmarks/m149-artifacts/m149-clickbench-ab.json` (A/B das 43 queries),
`m149-micro-bench.log` (ganho por query).

## TL;DR

> **Projeção estreita ganha geomean 3.73× (acima do DoD de ≥3×), com resultado byte-idêntico ao heap.**
> A/B das 43 queries do ClickBench: **0 divergências** — a projeção nunca muda o resultado. 5 das 43
> queries engajam o CustomScan (as de projeção pura); as 38 agregações declinam corretamente para o path
> de agg, sem regressão.

## Correção — A/B das 43 queries do ClickBench (projection ON)

Fonte: `m149-clickbench-ab.json` (500k linhas, `run_m128_clickbench.py`, oráculo A/B columnar-vs-heap).

| Métrica | Valor |
|---|---|
| `result_ab.diverged` | **0** |
| `result_ab.pass` | 42/43 (1 errored = q28 regexp timeout, pré-existente, não do M149) |
| `verdict` | **byte-identical (columnar == heap)** |
| `columnar_customscan_count` | 5 (queries de projeção pura que usam o `theodb_columnar_project`) |

O `diverged=0` é o gate de correção (Rule 5): a projeção materializa só `targetlist ∪ qual`, então as
colunas não-projetadas nunca são lidas por nenhum nó superior — o resultado é idêntico ao decode-tudo.

## Ganho — micro-benchmark de projeção estreita (OFF vs ON)

`hits` real de 105 colunas, 300k linhas. Cada query medida com `EXPLAIN (ANALYZE)` 3× (min), com
`theodb.enable_projection` OFF (baseline decode-tudo) e ON (só as colunas referenciadas). A/B = md5 do
resultado columnar (ON) vs o heap twin.

| Query | colunas usadas / 105 | OFF (ms) | ON (ms) | Ganho | A/B |
|---|---|---|---|---|---|
| `SELECT url` | 1 | 5600 | 1341 | **4.18×** | ✅ OK |
| `SELECT title, url WHERE counterid<>0` | 2 (+1 filtro) | 5557 | 1783 | **3.12×** | ✅ OK |
| `SELECT searchphrase WHERE searchphrase<>''` | 1 | 5591 | 1224 | **4.57×** | ✅ OK |
| `SELECT watchid, url, title WHERE advengineid<>0` | 3 (+1 filtro) | 5596 | 1726 | **3.24×** | ✅ OK |

**geomean do ganho = 3.73×** — cumpre o DoD (≥3× em projeção estreita).

O ganho vem do que o M148 previu: reduzir a **materialização** (o `form_row` monta um heap-tuple de N
colunas em vez de 105) **e** o decode (as colunas não-projetadas pulam `read_chunked`+zstd). Como o M148
mediu a materialização row-by-row como ~80% do tempo do scan, cortar 102-104 das 105 colunas por linha
rende o ~3-4.5× medido. O ganho escala com quão estreita é a projeção (1 coluna → 4.18-4.57×; 2-3 colunas
→ 3.12-3.24×).

## Fallback e não-regressão

- `SELECT *` (whole-row) → `columns_needed` retorna `None` → decode-tudo (fallback ADR-2). A/B byte-idêntico.
- Queries de agregação (`GROUP BY`, `COUNT(DISTINCT)`) → o path hook declina (`hasAggs`) → SeqScan/agg path
  intacto. Nenhuma das 38 queries de agg do ClickBench mudou de resultado nem de plano.
- System column (`ctid`) / whole-row Var → fallback decode-tudo.

## Metodologia / reprodução

```bash
# no box de medição, com theodb_rs instalado:
# A/B das 43 queries (correção):
PGPORT=<p> python3 benchmarks/run_m128_clickbench.py --n 500000 --sample head --out m149_on.json
#   -> result_ab.diverged deve ser 0
# ganho de projeção estreita (OFF vs ON): benchmarks/... (o micro-harness da sessão, m149-micro-bench.log)
```

Nota de honestidade (amostra `head`): para medir o **custo de materialização por linha** (o eixo do M149),
o viés de cardinalidade da amostra `head` é irrelevante — o que importa é o formato de 105 colunas e o
volume. NÃO é claim de latência canônica (isso seria o box AWS `c6a.4xlarge`); é a comparação OFF-vs-ON
controlada (mesmo box, mesmo dado, só o flag muda), que é o oráculo do ganho.
