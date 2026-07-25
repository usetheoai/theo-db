# Blueprint — Fechar o gap colunar vs ClickBench: como outros bancos resolveram o imposto row-at-a-time

**Data:** 2026-07-25 · **Fonte:** deep research (2 agentes, R0 web + R0.1 acervo) ancorado nos números medidos M148-M151.
**Âncora medida:** 14/43 queries do ClickBench roteiam pelo caminho vetorizado (geomean 0,05s); 29/43 são
row-based (geomean 2,14s @100k, ~47s @1M). Gargalo (M148 flamegraph): **~80% do scan é materializar heap-tuple
por linha** (`palloc` 57% + `heap_form_tuple` 22%), decode só 7%, 100% CPU-bound.

## Achado unificador (com prova do PG source)

O gargalo dos 80% vive num ponto exato do nosso código: `columnar.rs::columnar_scan_getnextslot → form_row →
heap_form_tuple`. O teto de 14/43 é de **reconhecimento de shape/serialização**, NÃO de capacidade do DataFusion.

**O PG TableAM é estruturalmente linha-a-linha e NUNCA vai fazer batch/pushdown:**
- `scan_getnextslot` devolve UM slot (`references/postgres/.../access/tableam.h:354-359`), `Assert`-mandatório
  (`tableamapi.c:48`); zero variante batched no PG18 (grep `batch|vector` → nada).
- Upstream: o patch "Batching in executor" (Langote, out/2025) mede 5-10% de ganho, **regressão a 100M + segfault
  TPC-H Q22**, sem versão-alvo; década de propostas nunca mergeadas (Freund 2016, PostgresPro VectorTupleTableSlot).
- `TableAmRoutine` não tem callback de project/aggregate/qual/batch — só ScanKeys row-level. **Agg/filter pushdown
  no PG é CustomScan OU FDW, nunca TableAM** (`createplan.c:7303` `custom_scan_tlist`). Confirma a nota M149.

→ **A única porta compatível com as regras é mover a fronteira do batch para ACIMA do TableAM, via CustomScan.**
É o que já fazemos (M114/M115/M149/M150/M151). O gap é **largura de roteamento**, não engine.

## Coverage Corner 1 — Integration Tests / gate de correção

Toda classe nova roteada é gated por **A/B byte-idêntico (`diverged=0`) + não-vacuidade (`columnar_customscan_count`
sobe)** — o oráculo M151. Lição M151: o `diverged=0` sobre um workload fixo NÃO prova correção geral (o review pegou
o HIGH temporal/float cross-type que o A/B não exercitava). Cada shape novo precisa de harness de regressão do seu
risco semântico (ex.: collation não-determinística DEVE declinar ao nativo, como temporal/float declina).

## Coverage Corner 2 — Dependencies

**Nenhuma nova.** DataFusion/Arrow (Apache-2.0) JÁ estão no binário e cobrem nativamente as 5 classes não-roteadas
(`datafusion/physical-plan/src/aggregates/group_values/multi_group_by/bytes.rs` = group key Utf8;
`functions-aggregate/.../count→distinct` exato; `physical-plan/src/topk/` TopK; `functions/src/regex/`). Rule 9: o
engine não é o gap.

## Coverage Corner 3 — Tools

`run_m128_clickbench.py --agg` (cobertura `columnar_customscan_count` + `diverged`); ablação OFF-vs-ON no mesmo
binário (oráculo de ganho M149/M150); harness focado de regressão por-classe (collation, NULL, overflow).

## Coverage Corner 4 — Techniques (os 4 padrões + o veredito de paradigma)

### Padrões de escape do imposto row-at-a-time (ranqueados por ganho × viabilidade permissiva)

| Rank | Padrão | Ganho no nosso gap | Viabilidade D1 | Veredito |
|---|---|---|---|---|
| **1** | **Ampliar a cobertura de roteamento do CustomScan DataFusion** | Máximo — remove estruturalmente os 80% em cada subárvore nova | ✅ Apache-2.0, own-code, zero dep | **A APOSTA** |
| 2 | Expandir zone-map/chunk pruning (temos `am/zonemap.rs`) | Médio — corta linhas que chegam ao slot (só WHERE seletivo) | ✅ own-code | Complementar, barato |
| 3 | Generalizar pushdown de shapes via CustomScan | = subconjunto do #1 | ✅ | Dobra no #1 |
| 4 | Re-embed pg_duckdb (MIT) | Alto out-of-box | ✅ legal, ❌ own-code/C++/boundary não-ACID | **Rejeitado** (KISS/own-code; licença NUNCA foi o bloqueio) |
| 5 | Forkar o executor do PG (batched TableAM / VectorTupleTableSlot) | Eliminaria o imposto globalmente | ❌ regra no-fork (D3) + década de regressões OLTP upstream | **Rejeitado** |

Evidência: Citus columnar (AGPL, study-only) prova o **teto** do modelo CustomScan+TableAM sem engine vetorizado
(todo operador acima do scan roda no executor row do PG); nós já vamos além (M114/M115 computam o agg no nó e emitem
só grupos). DuckDB-in-PG (pg_duckdb MIT, ParadeDB AGPL, Hydra Apache) prova que o **seam** (CustomScan entrega
subárvore ao engine) é o ganho — e nós já temos esse seam via DataFusion. YugabyteDB (Apache core; study-only)
prova o **padrão de pushdown** (empurrar agg/filtro para não subir linhas) — mas é OLTP distribuído; só o padrão
transfere, e o análogo PG (`postgres_fdw` `GetForeignUpperPaths`) confirma que agg-pushdown é CustomScan/FDW.

### Veredito paradigma: vetorização (ampliar), NÃO compilação

- O gargalo medido (materialização, não cálculo) é *literalmente* o "tuple-at-a-time interpretation overhead" que o
  MonetDB/X100 nomeia (`papers/monetdb-x100-boncz-2005.pdf` Abstract) e que a vetorização cura.
- Kersten/Leis/Neumann/Boncz PVLDB 2018 ("Everything You Always Wanted to Know About Compiled and Vectorized
  Queries"): *"neither paradigm is clearly dominated by the other"*; compilação só vence em compute-bound
  cache-resident; vetorização iguala/vence em memory-bound/hash-heavy (OLAP real). SIMD-para-vetorização é
  *"dominated by memory access cost"*. (vldb.org/pvldb/vol11/p2209-kersten.pdf)
- DuckDB (SIGMOD2019): escolheu vetorização sobre JIT *"for portability reasons — JIT depends on massive compiler
  libraries (LLVM)"*. DataFusion (SIGMOD2024): vetorizado, batches 8192, sem codegen. Até Umbra (Neumann CIDR2020)
  recuou de LLVM eager (*"HyPer spends far more time on compilation than execution"* em queries baratas).
- → Escrever um compilador = reinventar a roda (Rule 9) por um ganho estreito e condicional. **A aposta é rotear
  mais classes pelo DataFusion vetorizado que já temos.** (Exceção honesta: JIT de EXPRESSÃO/kernel escopado, se um
  dia um workload for provadamente compute-bound — não é o caso hoje.)

### As 5 classes não-roteadas, ranqueadas por (ganho × baixo-risco-A/B × esforço)

| # | Classe | DataFusion nativo | Risco A/B | Estado hoje | Veredito |
|---|---|---|---|---|---|
| 1 | **COUNT(DISTINCT) exato** | Sim (HashSet exato) | BAIXO (NULL/grupo-vazio casam; FATAL só se usar approx/HLL) | declinado `columnar_agg.rs:399` | **Rotear** (nunca approx) |
| 2 | **GROUP BY texto** | Sim (byte-map Utf8) | BAIXO sob collation determinística; QUEBRA sob não-determinística (ICU case/accent) | quase pronto (`df_executor.rs:141-142` já aceita texto; declínio estreito só em AGG_SORTED `columnar_agg.rs:868-871`) | **Rotear** + guard `varcollid` determinística |
| 3 | ORDER BY … LIMIT k (TopK) | Sim (TopK O(K)) | BAIXO-MÉDIO (NULL defaults casam; tie-break instável + collation) | não roteado | Rotear depois (tie-breaker total) |
| 4 | Projeções que retornam LINHAS | Sim | BAIXO semântico, **PERF ALTO** (re-materializa Arrow→heap no boundary — os 80% voltam) | M149 já cobre projeção ESTREITA | **Pior ROI p/ linhas largas**; M149 já pega o caso bom |
| 5 | LIKE / regex | Sim (crate `regex` = RE2) | **ALTO** — RE2 ≠ POSIX ARE do PG (sem backref/look-around; leftmost-not-longest); ILIKE case-fold Unicode ≠ C-locale | não roteado (regex é a mais lenta, ~10s) | **NÃO rotear** regex; só LIKE-substring-literal-ASCII após prova A/B |

## ADRs

- **ADR-1: ampliar DataFusion routing, não trocar de paradigma nem re-embed engine.** Own-code permissivo + engine
  já embutido + gargalo = materialização (não cálculo). Alternativas rejeitadas: compilador (Rule 9, ganho estreito),
  pg_duckdb (own-code/C++/não-ACID), fork do executor (no-fork D3 + regressões upstream).
- **ADR-2: próxima fatia = GROUP BY texto (hash, collation-guardada); follow-up COUNT(DISTINCT) exato.** São
  agregados que retornam POUCOS grupos → escapam do imposto de re-materialização da Classe 4. Caminho quase pronto.
  Alternativa rejeitada: regex/LIKE (campo minado RE2≠POSIX), projeções largas (re-materialização).
- **ADR-3: gate obrigatório A/B byte-idêntico + regressão de collation não-determinística → declina ao nativo**
  (espelha o fix HIGH temporal/float do M151). Silent fallback é o modo de falha do pushdown; a não-vacuidade
  (`columnar_customscan_count`) o pega.

## Riscos / caveats de honestidade

1. **NÃO medido:** se o hash-GROUP-BY-texto JÁ roteia hoje (as queries ClickBench têm `ORDER BY count DESC LIMIT`
   → HashAgg, não AGG_SORTED → deveria rotear). A fatia COMEÇA por essa medição; se já roteia, o alvo real vira
   COUNT(DISTINCT). **Measurement-first antes de prometer o salto de cobertura.**
2. **Sem `×N`:** o medível é cobertura (14→14+K, determinístico) + `diverged=0`; o ganho de latência é ablação
   OFF-vs-ON no mesmo binário, não claim de leaderboard.
3. **Regime serial:** o CustomScan (`thread_local`) não é parallel-safe; o ganho vale no regime serial declarado.
   Torná-lo parallel-safe (DSM) é ampliação separada.
4. **Sem porta global:** só remove o imposto nas subárvores TOTALMENTE absorvidas; correlated subquery / função
   exótica / join cross-table não-roteado re-materializa. Não há caminho nas regras que remova o imposto globalmente
   (seria o fork do executor, rejeitado).

## Referências
- Local: `docs/benchmarks/m148-flamegraph-scan.md`, `m151-datafusion-coverage.md`; `theodb_rs/src/am/{columnar,columnar_agg,df_executor,zonemap}.rs`; `references/postgres/.../access/tableam.h:354-359`, `tableamapi.c:48`, `nodeSeqscan.c:49-114`, `createplan.c:7303`; `references/citus/.../columnar/*` (AGPL); `references/hydra/*` (AGPL); `references/datafusion/physical-plan/src/*` (Apache); `papers/monetdb-x100-boncz-2005.pdf`, `papers/morsel-parallelism-leis-2014.pdf`.
- Web: Kersten 2018 (vldb.org/pvldb/vol11/p2209-kersten.pdf), Neumann 2011 HyPer (vldb.org/pvldb/vol4/p539-neumann.pdf), Neumann 2020 Umbra (db.in.tum.de/~freitag/papers/p29-neumann-cidr20.pdf), DuckDB SIGMOD2019 (duckdb.org/pdf/SIGMOD2019-demo-duckdb.pdf), DataFusion SIGMOD2024 (dl.acm.org/doi/10.1145/3626246.3653368), YugabyteDB agg-pushdown (github.com/yugabyte/yugabyte-db/commit/e554bda + issue #1851), PG FDW GetForeignUpperPaths (postgresql.org/docs/current/fdw-callbacks.html), PG-hackers Batching-in-executor, RE2≠POSIX (clickhouse.com/blog/introducing-pg_re2-regex-in-postgres), PG collation determinística (postgresql.org/docs/current/collation.html).

Relacionado: [[m148-flamegraph-released]], [[m151-datafusion-coverage-released]]. Candidato a M152+ (GROUP BY texto).
