# Blueprint — M151: ampliar a cobertura do CustomScan vetorizado (DataFusion)

**Data:** 2026-07-25 · **Fonte:** discover (council-benchmark) + acervo local (DataFusion source, papers X100/Morsel) + medição real (docs/benchmarks/m131, m148). R0.1 cumprida (acervo local citado `arquivo:linha`); WebSearch indisponível neste contexto — o SOTA foi extraído do código-fonte do DataFusion no acervo (fonte mais autoritativa).

## 1. Estado atual — 6/43 roteadas, e ONDE decide (LOCKED)

**As 6 roteadas hoje** (medido, `docs/benchmarks/m131-clickbench-agg-on.json` → `routed_q=[0,2,3,6,15,32]`): agregados escalares sem WHERE (0,2,3), min/max de tipo ordenado (6), GROUP BY int sem WHERE (15,32).

**Arquitetura (crítica):** o executor vetorizado DataFusion é alcançável por **UM ponto só** — o swap de agregado. `columnar_agg.rs:501` intercepta **apenas** `UPPERREL_GROUP_AGG`; `admit()` (`columnar_agg.rs:428`) troca o nó `Agg` por um `CustomScan` que roda `run_columnar_aggs`/`run_columnar_grouped_aggs` (`df_executor.rs:309,392`). **Não existe caminho DataFusion que retorne LINHAS** — só os dois de agregação. O M149 (projection) e o M150 (chunk-skip) aceleram scans/filtros **row-based** (não usam DataFusion).

**Por que só 6:** o discriminador é o **WHERE**. `extract_all_predicates` (`columnar_agg.rs:216`) exige que TODA qual seja um `ZonePredicate`, e `extract_zone_predicate` (`:146`) só aceita `col op const` com op ∈ btree strategy 1-5 = `{<,<=,=,>=,>}` (`:197-204`). **Qualquer** qual fora disso → declina a query inteira.

## 2. Achado central — `<>` é o único bloqueio em 9 agregados limpos

Contagem exata sobre as 43 (do discover): `<>` (not-equal, btree strategy **6**) aparece em **19/43** queries e é o **único** bloqueio em **9 agregados limpos** (`{1,7,12,14,30,31,36,37,38}`). Adicionar `<>` ao filtro DataFusion leva a cobertura de **6 → 15** (número a VALIDAR no benchmark, não prometido). Nenhuma outra mudança única chega perto.

## 3. Prior art (acervo local — fonte primária)

- **DataFusion tricotomia Exact/Inexact/Unsupported** (`references/datafusion/datafusion/expr/src/table_source.rs:37-51`): `Exact`=provider filtra sozinho; `Inexact`=poda + executor re-aplica `Filter`; `Unsupported`=só o executor filtra. **É exatamente o padrão do M151** — o TheoDB já implementa `Inexact` sem nomear (zone-map poda, ADR D3, `df_executor.rs:253` "the skip is only an admission filter; the executor is the FINAL authority"). O que falta: permitir predicados **`Unsupported` na poda mas aplicados no `Filter`** (o `<>`).
- **DataFusion batch default = 8192** (`references/datafusion/datafusion/common/src/config.rs:733`) — o "vetor" que substitui tuple-a-tuple.
- **MonetDB/X100 (`papers/monetdb-x100-boncz-2005.pdf`):** "tuple-at-a-time execution causes high interpretation overhead"; column-at-a-time não sofre. Base teórica.
- **M148 flamegraph (`docs/benchmarks/m148-flamegraph-scan.md`):** ~80% do scan é materializar heap-tuple por-linha; o caminho DataFusion **nunca materializa heap-tuple** → ataca os 80%. É o gate estrutural do M151.

## 4. Risco de divergência semântica (o gate A/B byte-idêntico)

- **`<>` — BAIXO.** (a) `=`/`<>` em texto e int são **collation-independentes** no PG (bytewise; só `<`/`>`/ORDER BY usam collation) — por isso min/max de texto declina mas `<> ''` é seguro. (b) NULL: `col <> ''` em lógica ternária SQL exclui NULLs; o `not_eq` do DataFusion segue a mesma semântica → mesmas linhas. (c) tipo do const == tipo da coluna já é checado (`:168`). Gate A/B (`run_m128_clickbench --agg`, `diverged=0`) prova. Honest-negative disponível se algum tipo divergir.
- COUNT(DISTINCT) MÉDIO-ALTO (ganho incerto, memória) — **fora**. LIKE MÉDIO — opcional. regex ALTO (motor PG) — **fora**. plain-scan = risco de PERF (re-materializa, M148) — **fora**.

## 5. Escopo ENXUTO recomendado

**Núcleo (obrigatório) — desacoplar o filtro DataFusion do zone-map e adicionar `<>`:**
1. Separar em `Admitted` duas listas: os `ZonePredicate` de **poda** (subconjunto ordenável 1-5, inalterado — `chunk_can_match` NUNCA vê `<>`) e uma nova lista de **filtro-DataFusion** que inclui `<>` (strategy 6). `extract_all_predicates` deixa de declinar em `<>`: emite o termo na lista de filtro.
2. `build_filter_expr` (`df_executor.rs:256`) ganha o braço `Ne => c.not_eq(val)`. A coluna do `<>` entra na projeção decodificada do batch (mesmo mecanismo `proj.push(p.col)`, `df_executor.rs:241`).
3. **Sem size gate** neste caminho — o agregado colapsa N linhas → poucas, zero materialização heap, monotonicamente melhor. (O size gate do grill era para o caminho plain-scan, que fica **fora**.)

**Meta (a VALIDAR pelo benchmark):** 6 → 15, `run_m128_clickbench --agg --n 1000000` com `result_ab.diverged == 0`. Ganho de tempo por query roteada vem de eliminar a materialização row-by-row (M148 ~80%), mas o número é measurement-first (as queries decodificam texto largo — trabalho real; o harness mede, não se promete um "×").

**Opcional (só se medição rápida mostrar ganho claro):** LIKE-substring escalar (i=20, `COUNT(*) WHERE URL LIKE '%google%'`). Gated por A/B; divergiu → honest-negative.

**FORA (honestidade):** COUNT(DISTINCT), plain-scan vetorizado, grouping/agg por expressão, regex, HAVING — cada um é milestone próprio.

## ADRs

- **ADR-1: `<>` como filtro-DataFusion (Unsupported-para-poda) vs poda por min/max.** `<>` NÃO poda chunk (um chunk `[min,max]` quase sempre contém valores ≠ const) — entra só na lista de filtro que o DataFusion `Filter` aplica sobre o batch. `chunk_can_match` permanece intocado (só strategy 1-5). Alternativa rejeitada: tentar podar por `<>` (inútil — não exclui chunk).
- **ADR-2: desacoplar poda-list de filter-list vs uma lista única.** Duas listas em `Admitted` (poda ⊆ filtro). Mantém `chunk_can_match` correto (só ordenáveis) e permite o filtro DataFusion mais amplo. Alternativa rejeitada: uma lista só (misturaria `<>` na poda → ou poda errado ou complica o guard).
- **ADR-3: escopo = só `<>` (+ LIKE opcional) vs rotear todas as 36.** Measurement-first + A/B byte-idêntico inviabiliza rotear tudo num milestone. `<>` é o maior ganho×menor risco (6→15). Alternativa rejeitada: COUNT(DISTINCT)/plain-scan/regex (risco alto, honest-negative).

## Delta (existe vs escrever)

| Peça | Estado |
|---|---|
| Swap de agregado + `run_columnar_aggs/grouped` (df_executor) | **Existe (M100/M114)** |
| `build_filter_expr` monta eq/lt/gt sobre o batch Arrow | **Existe** — + braço `not_eq` |
| `extract_zone_predicate` (strategy 1-5) | **Existe** — best-effort na lista de filtro p/ strategy 6 |
| `Admitted` com lista única de preds | **Existe** — split em poda-list + filter-list |
| `ZoneOp::Ne` (só para o filtro, nunca chunk_can_match) | **Escrever** (enum + mapeamento strategy 6) |
| Proj do batch inclui a coluna do `<>` | **Existe** (proj.push) — garantir que `<>`-cols entram |
| A/B 43 queries diverged=0 + cobertura 6→15 medida | **Escrever** (benchmark) |

## Referências
- `theodb_rs/src/am/columnar_agg.rs` (`admit:428`, `extract_all_predicates:216`, `extract_zone_predicate:146`, strategy `:197-204`)
- `theodb_rs/src/am/df_executor.rs` (`build_filter_expr:256`, `run_columnar_aggs:309`, `run_columnar_grouped_aggs:392`, proj `:241`)
- `theodb_rs/src/am/zonemap.rs` (`enum ZoneOp:16`)
- `benchmarks/run_m128_clickbench.py` (gate A/B `--agg`, `diverged=0`, `columnar_customscan_count`)
- `.claude/knowledge-base/references/datafusion/datafusion/expr/src/table_source.rs:37-51` (Exact/Inexact/Unsupported)
- `.claude/knowledge-base/references/papers/monetdb-x100-boncz-2005.pdf` (vetorização)
- `docs/benchmarks/m131-clickbench-agg-on.json` (6 roteadas), `docs/benchmarks/m148-flamegraph-scan.md` (80% materialização)
- [[m148-flamegraph-released]], [[m149-projection-pushdown-released]]
