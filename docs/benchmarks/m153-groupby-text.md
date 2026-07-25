# M153 — GROUP BY texto (AGG_SORTED) roteado ao CustomScan colunar (medido)

**Data:** 2026-07-25 · **Milestone:** M153 · **Box:** droplet DO `c-8`, efêmero, destruído.
**Método:** relaxar o declínio `swap_sorted_text_group_collation` com 2 guards — determinismo de collation (contagem) no
admit + re-sort acima (ordem) no swap. `run_m128_clickbench.py --agg` (head 100k + systematic 300k) + harness A/B focado.
**Artefatos:** `docs/benchmarks/m153-artifacts/{m153_agg_head.json, m153_agg_sys.json, m153_ec_guards.txt}`.

## TL;DR

> **Cobertura ClickBench: 18 → 21 (+3), `result_ab.diverged = 0` (byte-idêntico columnar==heap em 43/43), confirmado
> em DOIS regimes:** head 100k E systematic 300k (`UserID`≈290k distintos + `work_mem=256MB`) — ambos 21/43, diverged=0.
> As 3 novas são GROUP-BY-texto sorted: **q16** (`GROUP BY UserID, SearchPhrase`), **q33** (`GROUP BY URL`), **q38**
> (`GROUP BY URL WHERE CounterID=…`). Ablação ON-vs-OFF no mesmo binário (q33): **49,1ms vs 1566,2ms ≈ 32×**.

## Cobertura marginal (medida, vs M154 = 18)

| Query | SQL (resumo) | Antes | M153 | A/B |
|---|---|---|---|---|
| q16 | `UserID, SearchPhrase, COUNT(*) GROUP BY UserID, SearchPhrase ORDER BY c DESC LIMIT` | declina `swap_sorted_text_group_collation` | **roteia** | byte-idêntico |
| q33 | `URL, COUNT(*) GROUP BY URL ORDER BY c DESC LIMIT` | declina idem | **roteia** | byte-idêntico |
| q38 | `URL, COUNT(*) GROUP BY URL WHERE CounterID=… ORDER BY c DESC LIMIT` | declina idem | **roteia** | byte-idêntico |

**q17 permanece não-roteada** (honest) — e é a demonstração mais limpa do **guard de ordem**: q17 é
`SELECT UserID, SearchPhrase, COUNT(*) FROM hits GROUP BY UserID, SearchPhrase LIMIT 10` (chave texto SearchPhrase,
**sem ORDER BY / sem Sort pleno acima**; sem WHERE). Sem re-sort acima, o `LIMIT 10` observaria a ordem de emissão —
byte-wise ≠ collation → o guard (2) declina (o caso EC-3). Para rotear q17 o executor teria de emitir em ordem de
collation (fora do escopo). O M153 destravou q38 (GROUP BY URL + `WHERE CounterID=` int-pushável + `ORDER BY count`).
Total **21/43 = 16 agregação (theodb_columnar_agg) + 5 projeção (M149)**; o **+3 é 100% agregação**.

**Nota sobre o oráculo:** o A/B do `run_m128` é order-blind (remove LIMIT + canonicaliza) — logo `diverged=0` prova a
**equivalência do CONJUNTO** de grupos. Isso é o oráculo COMPLETO para as roteadas justamente porque elas têm um Sort
pleno acima (guard 2): o Sort do PG re-ordena o mesmo conjunto → não há ordem para divergir na saída. O **guard de
ordem** em si é provado pelo **EC harness** (EC-1: linhas LIMIT'd idênticas colunar==heap; EC-3: decline sem re-sort).

## O fix (2 guards, correção decomposta)

O executor colunar emite grupos em ordem ASC **byte-wise** — que ≠ ordem de collation do PG. Para texto o AGG_SORTED
(GroupAgg) não consegue reproduzir a ordem prometida. Decompondo a correção:

1. **Contagem (equivalência de grupo) — guard no admit (`classify_target_node`):** o hash byte-keyed do DataFusion só
   agrupa como o PG sob collation DETERMINÍSTICA (deterministic ⟺ igualdade é byte-wise). Coluna texto com collation
   não-determinística declina — aplicado no admit, cobrindo **ambos** os paths (AGG_HASHED **e** AGG_SORTED), fechando
   um bug latente do AGG_HASHED-texto não-determinístico.
2. **Ordem — guard no swap (`try_swap_agg`, com o `parent` passado pelo `swap_walk`):** o AGG_SORTED-texto é seguro só
   quando um `Sort` PLENO acima re-ordena toda a saída (então a ordem byte-wise do executor é irrelevante). Aceita se
   `parent` é `T_Sort` (não `T_IncrementalSort`); senão declina. `GROUP BY texto ORDER BY texto` (ordem consumida
   direta, sem re-sort) declina.

## Prova dos guards (A/B focado — `m153_ec_guards.txt`)

| Caso | Comportamento medido | A/B |
|---|---|---|
| `GROUP BY texto ORDER BY count DESC LIMIT` (re-sort acima, collation default determinística) | **roteia** (`theodb_columnar_agg`) | conjunto grouped divergent = **0** |
| chave de grupo texto NULL (EC-2) | agrupa NULL corretamente | 286 = 286 |
| `GROUP BY texto ORDER BY texto` (ordem direta, sem re-sort — EC-3) | **declina** ao nativo | divergent = **0** |
| `GROUP BY texto COLLATE ci2` NÃO-determinística (EC-4) | **declina** no admit | 2 = 2 grupos (byte-wise daria 4) |
| `GROUP BY int` (AGG_SORTED numérico — EC-5, regressão) | **roteia inalterado** | divergent = **0** |

**EC-4 é o guard de contagem:** sob a collation ICU `ci2` (`deterministic=false`), `abc`==`ABC` no PG (2 grupos); o
hash byte-wise agruparia como 4. O guard de determinismo declina → ambos os lados dão 2. Sem ele, contagens divergiriam.
**EC-3 é o guard de ordem:** sem `Sort` pleno acima, a ordem byte-wise ≠ collation → declina (correto).

## Ablação (mesmo binário, mesmos dados colunares)

| Query | agg OFF (executor nativo sobre storage colunar) | agg ON (CustomScan vetorizado) | speedup |
|---|---|---|---|
| q33 `GROUP BY URL ORDER BY count DESC LIMIT 10` (18.180 grupos, 100k linhas) | 1566,2 ms | 49,1 ms | **≈ 32×** |

EXPLAIN ANALYZE (TIMING OFF), oráculo M149/M150 (OFF-vs-ON no mesmo `.so`). Não é claim de leaderboard.

## bpchar excluído (review MEDIUM — fecha um bug latente também no M154)

O `bpchareq` do PG ignora espaços à direita na igualdade (semântica de TIPO, `varchar.c:756-773`), ortogonal ao
determinismo de collation: `'ab'` e `'ab '` são IGUAIS no PG (1 grupo) mas byte-diferentes no storage → o hash
byte-wise do DataFusion contaria 2. O guard de determinismo NÃO cobre isso. **Fix:** `bpchar` (OID 1042) foi removido
de `arrow_supported_group_type` (`df_executor.rs:141`) → `GROUP BY bpchar` E `COUNT(DISTINCT bpchar)` (o mesmo gate,
então fecha também o path do M154) declinam ao nativo. Provado (EC-6, `m153_ec_guards.txt`): `GROUP BY bpchar` e
`COUNT(DISTINCT bpchar)` mostram plano nativo + A/B 2=2 (byte-wise daria 4/3). `char(n)`-com-tamanho seria seguro
(padded), mas é indistinguível de `bpchar` puro por OID → exclusão conservadora (declina ao nativo, correto; ClickBench
não usa `char(n)`, cobertura permanece 21).

## Metodologia / reprodução

```bash
# SET theodb.enable_columnar_agg = on;  (default OFF)
python3 benchmarks/run_m128_clickbench.py --agg --n 100000 --sample head        # → customscan=21, diverged=0
python3 benchmarks/run_m128_clickbench.py --agg --n 300000 --sample systematic  # → customscan=21, diverged=0 (work_mem 256MB)
psql -f benchmarks/m153_ec_harness.sql   # os 5 guards A/B (SET enable_hashagg=off força GroupAgg/AGG_SORTED)
```
