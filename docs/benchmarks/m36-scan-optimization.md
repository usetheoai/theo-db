# M36 — Otimização do scan do índice: heap top-K lazy (Phase 1)

**Dataset:** sintético 200.000×128 (distintos, seed 42) · **Hardware:** i7-1355U · 12 cores · 15.3 GB · AVX2
(CPU móvel, single-thread — números absolutos subestimam um servidor) · **k=10**, best-of-5 × 2 passes, 200 queries.

## Contexto (measurement-first FALSIFICOU a premissa original)

O gate measurement-first do M36 (`THEODB_SCAN_PROFILE=1`) mostrou que a **distância full-precision é ~15%** do
custo de scan — **não o gargalo**. Os gargalos medidos são **`reads` (I/O de página) ~44–51%** e **`sort`
(ordenar TODOS os candidatos) ~35–41%**. O milestone foi re-escopado (ADR-1 do blueprint). **Phase 1 ataca o
`sort`.**

## A mudança (Phase 1)

Substituí o `results.sort_by` O(C·log C) de TODOS os candidatos (`am/scan.rs`) por um **heap min lazy**: heapify
O(C) no `amrescan` + pop O(log C) por `amgettuple`. O executor puxa ~k vezes para um `LIMIT k`, então o custo total
é **O(C + k·log C)** em vez de **O(C·log C)**. O top-K emitido é **byte-idêntico** ao sort → **recall inalterado**.

## Resultado medido (recall idêntico — provado por 61 testes de coexistência)

**Fase sort (profiler, probes=50, 50.332 candidatos):** de **~10.000–15.000µs (sort, M35)** para
**~760–1.130µs (heapify, M36)** — **~10–13× menos** naquela fase (o custo por-pop O(log C)×k migrou para o
`amgettuple`, limitado pelo LIMIT do executor).

**End-to-end (QPS a `ORDER BY <-> q LIMIT 10`):**

| probes | recall aprox | M35 (sort) | M36 (heap) | speedup |
|---|---|---|---|---|
| 10 | ~0.87 | 459.3 QPS | 635.0 QPS | **1.38×** |
| 50 | ~0.99 | 100.8 QPS | 208.3 QPS | **2.07×** |
| 100 | ~0.99+ | 58.8 QPS | 103.8 QPS | **1.77×** |

O heap ganha em **todos** os probes; a vitória **cresce com o número de candidatos** (mais candidatos → o sort
O(C·log C) dói mais → o heap ganha mais). **recall idêntico** em todos os pontos — os 61 testes de coexistência
(`test_index_am`, `test_hnsw_structured`, `test_reloption`, `test_ann_index`, `test_sbq_index`) retornam os MESMOS
kNN ids antes e depois.

## Veredito honesto

- **Fase sort fechada:** ~1.4–2.1× de speedup end-to-end a **recall idêntico**, crescendo com os probes.
- **Fecha o gap do ScaNN?** **Parcialmente.** Isto ataca a fase `sort` (~35–41%). A fase `reads` restante
  (~44–51%) é o **Phase 2 do M36** (códigos quantizados menores para cortar I/O + rerank f32). O gap total do
  ScaNN (~25×, M33) também exige poda de candidatos mais forte. **Honesto:** é um speedup de scan real de
  ~1.4–2.1× a recall preservado, **NÃO "25× do sort sozinho"**.

## Reprodução

```
PGPORT=<port> python3 benchmarks/run_m36_scan.py --n 200000 --probes 50 --k 10 --runs 5 --label <sort|heap>
# baseline (sort) no theo-db:m35; otimizado (heap) no theo-db:m36 — mesmos dados/índice/probes
```
