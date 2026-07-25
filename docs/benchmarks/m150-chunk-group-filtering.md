# M150 — Chunk-group filtering no scan colunar: skip por zone-map, medido

**Data:** 2026-07-25
**Milestone:** M150 (chunk-group filtering — pular chunk-groups por min/max no scan geral, sem descomprimir)
**Box de medição:** droplet DigitalOcean `c-8` (8 vCPU / 16 GB), efêmero, destruído ao fim.
**Build:** `theodb_rs` release, PostgreSQL 18.4 do pgrx. Dataset: 1M linhas, `a int` monotone (clusterizado), `b int`, `c text`.
**Artefatos:** `docs/benchmarks/m150-artifacts/m150-1m-ab.txt`.

## TL;DR

> **Uma query seletiva sobre 1M linhas pula 99% dos chunk-groups (99/100) e acelera ~52–90× (geomean ~68×),
> com resultado byte-idêntico ao heap.** O skip é um filtro de admissão puro: o `ExecScan` re-checa o WHERE
> completo, então o resultado é idêntico com skip on/off (A/B diverged=0). **DoD (skip ≥ 80%, ganho ≥ 5×,
> A/B byte-idêntico) excedido decisivamente.**

## Correção — A/B byte-idêntico + os 6 testes (50k linhas)

Harness de DO-blocks (RAISE EXCEPTION on failure), reproduzível em `benchmarks`/o artefato:

| Teste | Resultado |
|---|---|
| T3.1 skip + A/B | `skip=4/5 A/B=1` (4 de 5 chunk-groups pulados p/ `a=25000`, resultado 1 linha == heap) |
| T3.1 never_loses_row | 6 predicados (Eq presente, range, `>` cauda, `<` cabeça, extremos) — A/B idêntico |
| T4.1 GUC gate | `off=0 on=4` — ablação no mesmo binário: skip off = 0 poda (pré-M150), on = 4 |
| T2.2 best-effort | `OR-noskip=0 AND-skip=4` — OR não empurra nada (correto), AND empurra o conjunto simples |
| T2.1 subxact-abort ABA | full scan = 50000 após subxact-abort que instalou `a=25000` — **não herda predicado stale** |
| 1M A/B | `AB_OK diverged=0` (Eq + range) |

O `diverged=0` é o gate de correção (Rule 5): o skip só pula um chunk-group quando o min/max **prova** que
nenhuma linha casa (`chunk_can_match` fail-safe: `has_minmax=false` / `MinMaxKind::None` / NaN → "must scan").
Pular um chunk podável-mas-com-match seria bug; sobre-admitir é só perda de perf (o `ExecScan` re-filtra).

## Ganho — 1M linhas, skip 99% (OFF vs ON, mesmo binário)

`EXPLAIN (ANALYZE) SELECT a FROM b_col WHERE a = 500000` (100 chunk-groups de 10k linhas):

| Métrica | OFF (skip off) | ON (skip on) | Delta |
|---|---|---|---|
| `Rows Removed by Filter` | 999999 | 9999 | 100× menos linhas materializadas |
| `Buffers: shared hit` | 397 | 7 | **57× menos páginas lidas** |
| `Execution Time` | 561 ms | 5.6 ms | **~100×** |

`\timing` (mediana de 5 runs, `OFFSET 1000000` força a materialização completa do scan):

| Query | OFF (mediana) | ON (mediana) | Ganho |
|---|---|---|---|
| Q1 `SELECT a WHERE a = 500000` (pontual) | 540.5 ms | 6.0 ms | **~90×** |
| Q2 `SELECT a, b WHERE a BETWEEN 400000 AND 400050` (range) | 629.9 ms | 12.2 ms | **~52×** |

**geomean do ganho = √(90 × 52) ≈ 68×** — muito acima do DoD (≥ 5×). skip% = **99.0%** (99/100), acima do DoD (≥ 80%).

O ganho vem exatamente do previsto no blueprint: pular os 99 chunk-groups que não podem casar `a=500000` evita
o `read_chunked`+zstd de suas colunas E a materialização row-by-row (`form_row`) de ~990k linhas. O único
chunk-group que sobrevive (o que contém 500000) é decodificado; o `ExecScan` filtra suas 10k linhas para a 1
que casa. Buffers 397→7 confirma que as páginas dos chunks pulados nunca são tocadas.

## Limitação honesta — quando o skip NÃO ajuda

- **`count(*) WHERE a = X` não pula** (`count_skipped=0` medido). Para um `count(*)` sem colunas no target, o
  filtro `a=X` é aplicado por um nó acima do projection scan, então o scan não vê qual empurrável. O skip
  beneficia queries que **materializam linhas** (`SELECT col… WHERE col op const`), não o `count(*)` puro. (O
  `count` seletivo já tem o seu caminho no `columnar_agg` M114/M105 min-max fast-path — ortogonal a este M150.)
- **Requer clustering pela coluna do predicado.** Se `a` não for clusterizado, os min/max por-chunk sobrepõem e
  nada é pulado. O ganho medido vale para o regime clusterizado (o caso de time-series / append-ordenado — o
  alvo do Citus chunk-group filtering). Sem clustering, o skip é um no-op correto (fallback decode-tudo).
- **Regime serial.** Como o M149, o CustomScan não é `parallel_safe` (o side-channel `thread_local` não cruza
  workers). Sob parallel scan a projeção/skip não engajam — resultado correto (fallback), sem o ganho.

## Metodologia / reprodução

```bash
# no box de medição, com theodb_rs instalado (cargo pgrx install --release):
psql -c "CREATE TABLE b_col (a int, b int, c text) USING theodb_columnar"
psql -c "INSERT INTO b_col SELECT g,(g%100),'row-'||g FROM generate_series(1,1000000) g"
# skip% + A/B:  m150_bench2.sql  (DO-block: AB_OK diverged=0 | SKIP_PCT=99.0)
# ganho OFF vs ON: m150_bench3.sql (\timing 5 runs, SET theodb.enable_chunk_skip off/on)
# métricas SQL: SELECT theodb_columnar_chunks_skipped(), theodb_columnar_chunks_scanned();
```

Nota de honestidade: o ganho é a **ablação OFF-vs-ON no MESMO binário** (só o GUC `theodb.enable_chunk_skip`
muda) — sem confound de box/build, o oráculo controlado do M150. skip% é determinístico (o zone-map min/max é
o mesmo). O `OFFSET 1000000` no `\timing` força o scan a materializar tudo antes de descartar, medindo o custo
real do scan (não o early-return do LIMIT).
