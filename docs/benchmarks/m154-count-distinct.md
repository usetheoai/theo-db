# M154 — COUNT(DISTINCT) roteado ao CustomScan colunar (medido)

**Data:** 2026-07-25 · **Milestone:** M154 · **Box:** droplet DO `c-8`, efêmero, destruído.
**Método:** DataFusion `count_distinct` EXATO (`DistinctCountAccumulator`, `distinct:true` sobre o `count_udaf` — nunca
approx/HLL). `run_m128_clickbench.py --agg --n 100000 --sample head` (cobertura é estrutural = independente do sample) +
harness A/B focado dos guards. **Artefatos:** `docs/benchmarks/m154-artifacts/{m154_agg.json, m154_ec_guards.txt}`.

## TL;DR

> **Cobertura ClickBench: 14 → 18 (+4), `result_ab.diverged = 0` (byte-idêntico columnar==heap em 43/43).**
> As 4 novas são TODAS COUNT(DISTINCT): **q4/q5 puras** (`COUNT(DISTINCT UserID)`, `COUNT(DISTINCT SearchPhrase)`)
> + **q8/q9 agrupadas por int** (`RegionID, COUNT(DISTINCT UserID) GROUP BY RegionID`). Superou a previsão do
> routing-map M152 (~2): o `count_distinct` AGRUPADO do DataFusion é correto sobre coluna-int (o group-key int não
> tem o problema de collation/deparse que trava o GROUP BY texto). geomean hot 0.171s.

## Cobertura marginal (medida, vs M152/M151 = 14)

| Query | SQL | Antes (M152) | M154 | A/B |
|---|---|---|---|---|
| q4 | `COUNT(DISTINCT UserID)` | declina `agg_distinct_filter_order` | **roteia** | byte-idêntico |
| q5 | `COUNT(DISTINCT SearchPhrase)` (texto) | declina `agg_distinct_filter_order` | **roteia** | byte-idêntico |
| q8 | `RegionID, COUNT(DISTINCT UserID) GROUP BY RegionID` | declina distinct | **roteia** (agrupado) | byte-idêntico |
| q9 | `RegionID, SUM, COUNT(*), AVG, COUNT(DISTINCT UserID) GROUP BY RegionID ORDER BY c DESC LIMIT` | declina distinct | **roteia** (agrupado) | byte-idêntico |

As outras 14 (q0,1,2,3,6,7,15,19,23,24,25,26,32,41) permanecem roteadas como no M151/M152. Total 18/43.

## Prova de correção dos guards (A/B focado — `m154_ec_guards.txt`)

| Caso | Comportamento medido | A/B |
|---|---|---|
| `COUNT(DISTINCT int)` | roteia (`Custom Scan theodb_columnar_agg`) | 45 = 45 ✓ |
| `COUNT(DISTINCT text)` collation default (determinística) | roteia | 10 = 10 ✓ |
| `COUNT(DISTINCT all-NULL)` (EC-2) | 0 (exclui NULL) | 0 = 0 ✓ |
| `COUNT(DISTINCT)` tabela vazia (EC-2b) | 0 | 0 = 0 ✓ |
| `COUNT(DISTINCT col+1)` (EC-3) | **declina** ao nativo (Aggregate→Sort→SeqScan) | 45 = 45 ✓ |
| `SUM(DISTINCT col)` (EC-3b) | **declina** ao nativo (ADR-M154-2) | 1110 = 1110 ✓ |
| `COUNT(DISTINCT col) GROUP BY expr` (EC-4) | declina (group-key expressão) | 6 linhas idênticas ✓ |
| **`COUNT(DISTINCT text) COLLATE ci` não-determinística (EC-1)** | **declina** (ADR-M154-3) | **2 = 2** ✓ |

**EC-1 é o guard crítico:** sob a collation ICU `ci` (`deterministic=false`), a igualdade byte-wise do
`count_distinct` do DataFusion daria **4** (`abc`,`ABC`,`xyz`,`XYZ` distintos em bytes) enquanto o PostgreSQL dá **2**
(igualdade case-insensitive). O guard `!get_collation_isdeterministic(varcollid)` declina ao nativo → ambos dão 2.
Sem o guard, seria uma divergência silenciosa de resultado.

## O que continua honest-negative (fora do escopo M154)

`COUNT(DISTINCT expr)`, `COUNT(DISTINCT a,b)` (multi-arg), `SUM/AVG/MIN/MAX(DISTINCT ...)`, e COUNT(DISTINCT) sob
collation não-determinística: TODOS declinam ao plano nativo (correto). Alta cardinalidade não foi um regime medido
aqui (o gate é correção byte-idêntica; performance de count_distinct de altíssima cardinalidade fica como
follow-up de medição, não bloqueia a correção).

## Metodologia / reprodução

```bash
# behavior: SET theodb.enable_columnar_agg = on;  (default OFF = storage path)
python3 benchmarks/run_m128_clickbench.py --agg --n 100000 --sample head   # → customscan_count=18, diverged=0
psql -f ec_harness.sql   # guards A/B (int/text/all-null/empty/expr-decline/sum-decline/grouped/ci-collation)
```

O `count_distinct` é EXATO (`DistinctCountAccumulator`) — jamais approx/HLL (ADR-M154-1); é o que torna o A/B
byte-idêntico legítimo.
