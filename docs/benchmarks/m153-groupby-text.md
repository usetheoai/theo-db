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

**q17 permanece não-roteada** (honest): é `SearchPhrase, COUNT(*) WHERE SearchPhrase <> '' GROUP BY SearchPhrase` — tem
o bloqueio COMPOSTO text-`<>` no WHERE (unpushable), que o M153 não trata (é o candidato M156 do routing-map). O M153
destravou q38 (cujo único bloqueio era o GROUP-BY-texto; o WHERE `CounterID=` é int pushável). Total 21/43.

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

## Metodologia / reprodução

```bash
# SET theodb.enable_columnar_agg = on;  (default OFF)
python3 benchmarks/run_m128_clickbench.py --agg --n 100000 --sample head        # → customscan=21, diverged=0
python3 benchmarks/run_m128_clickbench.py --agg --n 300000 --sample systematic  # → customscan=21, diverged=0 (work_mem 256MB)
psql -f benchmarks/m153_ec_harness.sql   # os 5 guards A/B (SET enable_hashagg=off força GroupAgg/AGG_SORTED)
```
