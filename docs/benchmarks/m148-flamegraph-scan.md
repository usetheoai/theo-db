# M148 — Flamegraph do scan colunar: veredito de priorização (measurement-first)

**Data:** 2026-07-24
**Milestone:** M148 (spike de medição — gate dos M149/M150/M151)
**Box de medição:** droplet DigitalOcean `c-8` (8 vCPU / 16 GB), efêmero, destruído ao fim.
**Build:** `theodb_rs` release **com** `-C debuginfo=2 -C force-frame-pointers=yes`; PostgreSQL 18.4 do
pgrx (`--enable-debug --enable-cassert` — ver § Confound).
**Dataset:** ClickBench `hits` REAL (105 colunas), 500 000 linhas (amostra `head` — ver § Por que head),
carregado em `theodb_columnar`.
**Método:** `perf record --call-graph dwarf -F 111 -p <backend_pid>` durante a query; folded via
`stackcollapse-perf.pl`; SVG via `flamegraph.pl`. Harness: `benchmarks/profile_columnar_scan.sh`.
**Artefatos:** `docs/benchmarks/m148-artifacts/{m148-slow,m148-scanpuro}-flamegraph.svg` + `-folded.txt`.

## TL;DR — o veredito CORRIGE a hipótese do grill

> **A materialização de cada linha como heap-tuple (`palloc` + `heap_form_tuple` por linha) domina o scan
> colunar — ~80% do tempo no scan puro. A descompressão das colunas (o gargalo que a hipótese do grill
> supunha) é só ~7%. O scan é 100% CPU-bound (0% I/O).**

A hipótese em `columnar-scan-optimization-feature-grill.md` (Q2) era: *"`decode_stripe` decodifica as 105
colunas de cada stripe → projection pushdown (M149) é a alavanca"*. **A medição refuta a parte do decode:**
o decode/zstd é trivial (~7%); o custo real é re-materializar cada linha como heap-tuple e emiti-la uma a
uma pelo executor Volcano. Sem este flamegraph, teríamos priorizado M149 esperando um ganho grande do
decode-skip e obtido ~7%.

## Amostras (EC-1 — piso de 500, honestidade da medição)

| Query | Amostras | Símbolos relevantes | Veredito |
|---|---|---|---|
| `slow` — `SELECT referer, count(*) FROM hits GROUP BY referer ORDER BY 2 DESC LIMIT 10` | **1440** | 129 | ✅ ≥ 500 |
| `scanpuro` — `SELECT count(*) FROM hits WHERE advengineid <> 0` | **1087** | 118 | ✅ ≥ 500 |

O gate EC-1 (MUST FIX do edge-case review) fez seu trabalho **duas vezes**: abortou em runs anteriores
(fixture de 5 colunas rápido demais: 174 amostras; query com coluna inexistente: 1 amostra). A medição
final só foi honrada porque passou o piso — o análogo do harness vácuo do #190.

## Self-time por alavanca — PRODUÇÃO (cassert descontado, ver § Confound)

**SCANPURO** (scan puro, sem GROUP BY/sort — o mais representativo do gargalo de scan):

| % | Alavanca | Mapeia para |
|---|---|---|
| **57.4%** | alocação por-linha (`palloc`/`malloc`/`memcpy`/`free` de cada heap-tuple) | **M151** (+ M149 reduz) |
| **22.5%** | materializa-row (`form_row` + `heap_form_tuple`) | **M151** (+ M149 reduz) |
| 11.2% | outro / executor-PG (Volcano `ExecProcNode` overhead) | M151 |
| **7.2%** | decode / zstd (`decode_column` + `ZSTD_decompressSequences`) | **M149** |
| 1.8% | deform (`heap_deform_tuple`) | M151 |

**SLOW** (com GROUP BY referer + ORDER BY — a agregação adiciona sort):

| % | Alavanca | Mapeia para |
|---|---|---|
| 41.6% | alocação por-linha | M151 (+ M149) |
| 27.1% | sort/collation (`__strcoll_l`) — **da agregação, não do scan** | fora de escopo (executor PG) |
| 14.3% | materializa-row (`form_row` + `heap_form_tuple`) | M151 (+ M149) |
| 11.4% | outro / executor-PG | M151 |
| 4.7% | decode / zstd | M149 |

O `__strcoll_l` (27%) só aparece no `slow` (comparação de strings do `GROUP BY referer`/`ORDER BY`) — é do
executor de agregação do PG, não do nosso scan, e some no scanpuro. Por isso o **scanpuro é a leitura
canônica do gargalo de scan**.

## Eixo CPU vs I/O

**Ambas as queries: I/O-frames = 0.00% → CPU-BOUND.** Nenhum frame de `pread`/`FileRead`/`mdread`/`io_uring`
apareceu (dataset cabe em `shared_buffers=4GB` + page cache). A otimização é sobre **ciclos de CPU gastos
materializando linhas**, não sobre leitura de disco. (O `perf stat` anexado foi removido do harness por ser
frágil — o eixo é derivado dos folded, robusto e reproduzível.)

## Priorização dos milestones (o entregável do M148)

A **ordem relativa** é robusta ao box e ao cassert (EC-4 — só o %absoluto varia por CPU):

1. **M151 (execução vetorizada) — a MAIOR alavanca.** ~80% do tempo do scan puro é
   alocação-por-linha + materialização heap-tuple. Só emitir **batches** (Arrow, via o DataFusion que já
   temos desde o M100) — em vez de 1 heap-tuple `palloc`'d por linha pelo executor Volcano — elimina esse
   custo fixo por-linha. É o teto real, e o mais complexo (rotear mais queries pelo DataFusion + A/B
   byte-idêntico obrigatório vs heap por query roteada — Rule 5).
2. **M149 (projection pushdown) — segunda alavanca, o passo mais BARATO.** O `decode_stripe` decodifica **e
   materializa** as 105 colunas mesmo quando a query usa 2. Pular as colunas não-referenciadas reduz o
   `form_row`/`memcpy` proporcionalmente (105→N colunas por heap-tuple) **e** o decode (~7%). Ataca os
   mesmos frames dominantes de M151 com complexidade bem menor — bom primeiro passo. **Correção honesta:** o
   ganho de M149 vem de reduzir a *materialização*, não do decode-skip que a hipótese supunha (decode é 7%).
3. **M150 (chunk-group filtering) — condicional, por último.** Nenhuma das 2 queries exercitou (sem `WHERE`
   seletivo em coluna com zone-map). Rende só em queries seletivas — várias das 36 queries lentas do
   ClickBench têm `WHERE` seletivo, então continua válido, mas não é o gargalo do caminho comum de scan.

**Sequência recomendada (ROI × complexidade): M149 → M151 → M150.** M149 é o quick-win que reduz a
materialização com baixa complexidade; M151 é o teto que a elimina; M150 é o complemento para workloads
seletivos.

## Confound: build com `--enable-cassert`

O PG do pgrx (`pg_config --configure` → `--enable-cassert`) roda memory-poisoning e validação de atributo a
cada `heap_form_tuple`. Frames **`randomize_mem` (6.8%)**, **`verify_compact_attribute` (12.9-15.3%)** e
`populate_compact_attribute_internal` são **inflados/exclusivos do cassert** — em produção (non-cassert)
`verify_compact_attribute` é quase free e `randomize_mem` não existe. Por isso a tabela de produção acima
**desconta** esses frames (23.6-25.7% do bruto). **A ordem M151 ≫ M149 ≫ decode é robusta ao desconto** — o
`form_row`/`palloc` domina em ambas as leituras (bruta e descontada). Re-medir com um PG non-cassert daria o
%absoluto de produção exato; para o M148 (priorizar entre 3 técnicas) a ordem relativa basta (EC-4).

## Por que amostra `head` (não sistemática)

Para **profiling** do custo de decode/materialização *por linha*, o viés de cardinalidade de uma amostra
`head` (que afeta a distribuição de valores num `GROUP BY`) é irrelevante — o que importa é o formato de 105
colunas reais e o volume. `head -n` é rápido (não varre os ~100 GB). **Este dataset NÃO deve ser usado para
claims de latência comparável** — só para o flamegraph. As latências honestas por query estão em
`docs/benchmarks/clickbench-1m-postfix-2026-07-24.md`.

## Reprodução

```bash
# no box de medição, com theodb_rs release+debuginfo instalado e perf liberado (paranoid=-1):
PGINST=/root/.pgrx/18.4/pgrx-install FLAME=/root/FlameGraph OUT_DIR=./m148-out \
  bash benchmarks/profile_columnar_scan.sh 500000
# -> m148-slow-flamegraph.svg, m148-scanpuro-flamegraph.svg, *-folded.txt
# análise de self-time por alavanca: ver o bloco Python em docs/benchmarks/m148-flamegraph-scan.md (histórico do commit)
```
