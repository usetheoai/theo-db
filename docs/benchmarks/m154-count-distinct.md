# M154 — COUNT(DISTINCT) roteado ao CustomScan colunar (medido)

**Data:** 2026-07-25 · **Milestone:** M154 · **Box:** droplet DO `c-8`, efêmero, destruído.
**Método:** DataFusion `count_distinct` EXATO (`DistinctCountAccumulator`, `distinct:true` sobre o `count_udaf` — nunca
approx/HLL). `run_m128_clickbench.py --agg --n 100000 --sample head` (cobertura é estrutural = independente do sample) +
harness A/B focado dos guards. **Artefatos:** `docs/benchmarks/m154-artifacts/{m154_agg.json, m154_ec_guards.txt}`.

## TL;DR

> **Cobertura ClickBench: 14 → 18 (+4), `result_ab.diverged = 0` (byte-idêntico columnar==heap em 43/43),
> confirmado em DOIS regimes:** head 100k (`m154_agg.json`) E systematic 300k com `UserID`≈290k distintos +
> `work_mem=256MB` (`m154_agg_systematic_wm256.json`) — ambos 18/43, diverged=0. A correção é provada em alta
> cardinalidade, não suposta.
> As 4 novas são TODAS COUNT(DISTINCT): **q4/q5 puras** (`COUNT(DISTINCT UserID)`, `COUNT(DISTINCT SearchPhrase)`)
> + **q8/q9 agrupadas por int** (`RegionID, COUNT(DISTINCT UserID) GROUP BY RegionID`). Superou a previsão do
> routing-map M152 (~2): o `count_distinct` AGRUPADO do DataFusion é correto sobre coluna-int (o group-key int não
> tem o problema de collation/deparse que trava o GROUP BY texto).

**Composição das 18** (mesma convenção união-projeção+agg da baseline M151): **13 são agregação**
(q0,1,2,3,4,5,6,7,8,9,15,32,41 — o CustomScan `theodb_columnar_agg`) + **5 são projeção/scan**
(q19,23,24,25,26 — o CustomScan de projeção M149). O M154 adiciona 4 à fatia de agregação (9→13).

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
| **`COUNT(DISTINCT float8)` com `{0.0,-0.0,1.5,1.5,NaN,NaN}` (EC-5, review HIGH)** | **declina** (ADR-M154-4) | **3 = 3** ✓ |

**Contrafactual medido (guard-off, `m154_ec_guards.txt`):** para o EC-1, `COUNT(DISTINCT s COLLATE "C")` (byte-wise =
semântica do `count_distinct` do DataFusion) = **4** vs `COUNT(DISTINCT s)` (collation `ci` do PG) = **2**. Sem o
guard, o roteamento daria 4 ≠ 2 do PG → divergência silenciosa. O EC-5 (float): sem o guard, o total-order IEEE do
`FloatDistinctCountAccumulator` daria 5 (`0.0`,`-0.0` separados + NaN-bits) ≠ 3 do `float8eq` do PG.

**EC-1 é o guard crítico:** sob a collation ICU `ci` (`deterministic=false`), a igualdade byte-wise do
`count_distinct` do DataFusion daria **4** (`abc`,`ABC`,`xyz`,`XYZ` distintos em bytes) enquanto o PostgreSQL dá **2**
(igualdade case-insensitive). O guard `!get_collation_isdeterministic(varcollid)` declina ao nativo → ambos dão 2.
Sem o guard, seria uma divergência silenciosa de resultado.

## Alta cardinalidade — medido (o contrato bounded-work_mem do M100, não um bug do M154)

O caminho de agregação colunar usa um `GreedyMemoryPool(work_mem)` do DataFusion — o contrato D3 do M100
(`df_executor.rs:375-382`): erros-não-panics, memória limitada por `work_mem`, para o CustomScan **jamais** dar OOM
no backend. **Isto governa TODO agg colunar** (um `GROUP BY` de milhões de grupos erra idêntico), não só o M154.

Medido no systematic (300k linhas, **`UserID` = 290.874 distintos ≈ único**):
- Com `work_mem` default (4MB): `COUNT(DISTINCT UserID)` (q4/q8/q9) → `ERROR: Resources exhausted … pool 4.0 MB`
  (limpo, não OOM/panic) — o HashSet de 290k distintos excede 4MB, **exatamente como qualquer agg colunar grande**.
- Com `work_mem=256MB` (tuning padrão PG para agregações grandes): q4 **roteia, byte-idêntico (290.874 = 290.874)
  e MAIS RÁPIDO que o nativo — 49,6ms vs 147,8ms** (EXPLAIN ANALYZE, `hits` colunar vs `hits_heap`). `SearchPhrase`
  (q5, baixa cardinalidade) roteia byte-idêntico mesmo com work_mem default.

**Veredito honesto:** o count_distinct herda o mesmo contrato bounded-work_mem de todo agg colunar. Não é uma falha
nova do M154 nem uma incorreção — é a mesma superfície de memória do M100, tunável pelo knob padrão `work_mem`. Sob
work_mem adequado é byte-idêntico até cardinalidade extrema e competitivo-a-superior em latência. O gate de
correção (byte-idêntico) vale em toda cardinalidade medida; o de memória é o contrato D3 existente, não regressão.

## O que continua honest-negative (fora do escopo M154)

`COUNT(DISTINCT expr)`, `COUNT(DISTINCT a,b)` (multi-arg), `SUM/AVG/MIN/MAX(DISTINCT ...)`, COUNT(DISTINCT) sob
collation não-determinística e **`COUNT(DISTINCT float4/float8)`** (ADR-M154-4): TODOS declinam ao plano nativo
(correto, provado por A/B).

## Metodologia / reprodução

```bash
# behavior: SET theodb.enable_columnar_agg = on;  (default OFF = storage path)
python3 benchmarks/run_m128_clickbench.py --agg --n 100000 --sample head   # → customscan_count=18, diverged=0
psql -f ec_harness.sql   # guards A/B (int/text/all-null/empty/expr-decline/sum-decline/grouped/ci-collation)
```

O `count_distinct` é EXATO (`DistinctCountAccumulator`) — jamais approx/HLL (ADR-M154-1); é o que torna o A/B
byte-idêntico legítimo.
