# Blueprint: Late materialization de colunas de saída no scan colunar (M158)

**Slug:** columnar-late-materialization
**Owner:** paulohenriquevn
**Date:** 2026-07-25
**Cycle:** discover-execute (measurement-first; aceita honest-negative como o M155)

## Objective

Responder **se e como** fazer *late materialization* no regime `SELECT <cols> … ORDER BY key LIMIT k`
do scan colunar do TheoDB — decodificar só a chave de ordenação (+ um row-locator) para todas as N
linhas, fazer top-k, e materializar as colunas restantes **só para as k linhas** — preservando MVCC e
byte-identidade, e emitir um **VEREDITO de viabilidade com números** ancorado no baseline medido do M148
(re-fetch vs materialização evitada).

**VEREDITO (antecipado no topo, detalhado em ADR D3): VIÁVEL-COM-RESTRIÇÕES.** A técnica é sólida e o
custo é favorável **quando k ≪ N e as linhas são largas** (a parte cara — `form_row`/`heap_form_tuple`/
`palloc`, ~80% do scan — passa a ser paga só para k linhas; o re-fetch só re-paga o *decode*, que é ~7%).
É **honest-negative** no regime complementar (k≈N, projeção só-da-chave, sobreviventes espalhados fora de
cache) **e** carrega o mesmo risco de **cobertura marginal** do M155 (a maioria das Top-N do ClickBench é
`GROUP BY … ORDER BY count LIMIT`, que já roteia pelo agg CustomScan que **não** materializa linha-a-linha).

## Context

O M148 (`docs/benchmarks/m148-flamegraph-scan.md:37-48`) mediu, com flamegraph sobre `theodb_columnar`
(ClickBench `hits`, 105 colunas), que o scan colunar é **100% CPU-bound** (I/O = 0.00%, cabe em
`shared_buffers=4GB`) e que a **materialização de cada linha como heap-tuple domina ~80%** do scan puro:

| % (cassert-descontado) | Alavanca | Fonte |
|---|---|---|
| 57.4% | alocação por-linha (`palloc`/`memcpy`/`free` de cada heap-tuple) | `m148-flamegraph-scan.md:43` |
| 22.5% | `form_row` + `heap_form_tuple` | `m148-flamegraph-scan.md:44` |
| 11.2% | executor Volcano (`ExecProcNode`) | `m148-flamegraph-scan.md:45` |
| 7.2% | decode / zstd (`ZSTD_decompressSequences`) | `m148-flamegraph-scan.md:46` |
| 1.8% | `heap_deform_tuple` | `m148-flamegraph-scan.md:47` |

No path atual (`theodb_rs/src/am/columnar.rs`), `decode_stripe:715` decodifica cada chunk-group e chama
`form_row:671` para **cada** linha (`columnar.rs:789-791`), que faz `heap_form_tuple` e guarda os bytes;
`columnar_scan_getnextslot:1202` depois faz `heap_deform_tuple` + `ExecStoreVirtualTuple`. O M149 (`want_mask`)
já materializa **só as colunas projetadas** (targetlist∪qual), mas no regime `ORDER BY key LIMIT k` o Sort do
PG puxa **todas** as N linhas do scan → **todas** materializam e só k sobrevivem. O M155
(`memory: m155-topn-honest-negative`) mediu que o Sort do PG já é `top-N heapsort` (~1.6-2.6ms, **não** é o
gargalo) e apontou **explicitamente** este lever: *"decodificar só a chave p/ todas as linhas, materializar as
demais só p/ o top-k (late materialization à C-Store/MonetDB)"*. Este blueprint executa esse spike.

Regras consultadas: `.claude/rules/discover-phd-rigor.md` (R0 web + SOTA-anchoring), `.claude/rules/architecture.md`
(fronteiras do CustomScan). CLAUDE.md: measurement-first, anti-sunk-cost, Esforço≠Complexidade.

## Coverage Corner 1 — Integration Tests

**Q5 — Como PROVAR byte-identidade + MVCC e flamegraph-medir o antes/depois.**

O método de prova é o mesmo tripé já usado em M149/M150/M153/M156, reusável sem código novo de harness:

1. **A/B byte-idêntico (oráculo de correção).** `benchmarks/run_m128_clickbench.py` roda cada query
   `enable_columnar_late_mat = on` vs `off` e exige **`diverged = 0`** (conjunto de linhas idêntico). O
   arquivo já documenta o oráculo: *"ORDER BY are deterministic; aggregates are single-row; sorting makes the
   compare order-independent"* (`run_m128_clickbench.py:143`). **Caveat herdado do M155
   (`run_m128_clickbench.py:173`):** com muitos empates na chave, o corte do `LIMIT` é não-determinístico na
   fronteira → o oráculo prova **igualdade de CONJUNTO**, não de ordem na borda. Para late-mat isso é
   suficiente: o top-k re-materializado tem de conter **as mesmas k linhas** que o eager escolheria sob a mesma
   regra de desempate (a fronteira ambígua já é ambígua no eager).
2. **Flamegraph antes/depois (prova do ganho).** `benchmarks/profile_columnar_scan.sh` (o harness M148:
   `perf record --call-graph dwarf -F 111`, `stackcollapse-perf.pl | flamegraph.pl`, gate **EC-1 ≥ 500
   amostras** — `profile_columnar_scan.sh:11`) re-rodado na query alvo deve mostrar os frames
   `form_row`/`heap_form_tuple`/`palloc` (81.7% hoje) **caírem** para ~(k/N) do original, com um pequeno pico
   novo de decode do re-fetch. `benchmarks/m148_selftime.py` extrai a tabela de self-time dos folded, sem rede.
3. **MVCC.** O CustomScan resolve o conjunto de stripes **uma vez** no `begin` sob o snapshot do scan
   (`columnar.rs:137` `read_visible_stripes` roda sob a snapshot ativa via SPI; `columnar.rs:483-487` — set
   MVCC-fixo pela vida do scan). O row-locator (índice global de linha na ordem `ORDER BY first_row_number,
   stripe_id`, `columnar.rs:142`) é **estável dentro do scan**: mesma snapshot, mesmo conjunto de stripes,
   mesma ordem de decode. Teste: `SELECT … ORDER BY key LIMIT k` numa transação que insere/deleta linhas
   concorrentes — a linha re-materializada tem de ser a MESMA que o eager veria sob a mesma snapshot (o EC de
   MVCC já usado em M154/M156).

## Coverage Corner 2 — Dependencies

**Q4 — DataFusion TopK reusável, ou o PG top-N heapsort basta?**

O DataFusion tem um TopK maduro em
`.claude/knowledge-base/references/datafusion/datafusion/physical-plan/src/topk/mod.rs`:
`pub struct TopK { … heap: TopKHeap }` (`mod.rs:113,129`), construído com `k: usize` (`mod.rs:336`,
`TopKHeap::new(k)` `mod.rs:368`) e alimentado por `insert_batch(&mut self, batch: RecordBatch)`
(`mod.rs:379`) — um heap O(N log k) sobre `RecordBatch` Arrow. É o **mesmo algoritmo** que o M155 provou que
o PG já usa (`Sort Method: top-N heapsort`).

**Decisão (D1/D3): NÃO reusar o TopK do DataFusion no path in-PG; NÃO depender do heapsort do PG.** Razões:
- O TopK do DataFusion opera sobre `RecordBatch` Arrow — para usá-lo precisaríamos **materializar** a chave em
  Arrow, o que reintroduz um custo por-linha; e ele guarda uma referência ao **batch inteiro** para
  reconstruir a linha vencedora (`mod.rs:434` `register_batch`) — ou seja, o DataFusion TopK **já faz early-ish
  materialization do batch**, não o late-mat por-locator que queremos.
- O heapsort do PG (M155) roda **depois** do `scan_getnextslot`, isto é, **depois** de a materialização já ter
  acontecido — não dá para injetar o row-locator ali (o slot é `ExecStoreVirtualTuple`, sem `tts_tid`,
  `columnar.rs:~1226`).
- **Rung-5 da parsimony-ladder:** um `BinaryHeap<(Key, Locator)>` de Rust (std) sobre a chave já-decodificada
  em forma colunar (via `decode_columns`, `columnar.rs:824`) é O(N log k), sem dependência nova, sem
  materializar heap-tuple. É a peça mais simples que resolve (Rule 9/KISS). O `decode_columns` **já existe** e
  já devolve `Vec<Option<Vec<u8>>>` por coluna **sem** formar linha (é o path Arrow do M100) — é exatamente a
  primitiva de "decodificar só a chave" que Q2 pede.

## Coverage Corner 3 — Tools

**Q6 — Armadilhas vetorizadas do MonetDB/X100 que dariam falso-ganho.**

`monetdb-x100-boncz-2005.pdf` documenta o custo da **materialização completa** por *positional joins* e a
armadilha de banda/cache:

- *"queries … will materialize an entire result column for each function … MIL materializes much more data than
  strictly necessary, causing its high bandwidth consumption"* (`monetdb-x100-boncz-2005.pdf:659-666`). Ou seja:
  materializar cedo/demais gera tráfego de memória desnecessário — o que late-mat evita ao só reconstruir k linhas.
- *"MonetDB materializes the relevant result columns of the select() using six positional joins. These joins are
  not required in a Volcano-like pipelined execution model"* (`:668-673`). **A armadilha:** o re-fetch por
  row-locator É um *positional join* (posição → valor). Se ele virar **acesso aleatório por-linha em storage
  comprimido**, perde-se a localidade sequencial que dá a eficiência de CPU do X100.
- *"DBMS performance is strongly impaired by memory access cost (cache misses) … can significantly improve if
  cache-conscious data structures are used"* (`:309-314`); o X100 processa *"(1000 values) vertical chunks of
  cache-resident data"* (`:761`).

**O que medir para não dar falso-ganho** (herdado direto do M148/MonetDB): (a) o re-fetch deve decodificar no
grão de **chunk-group (10 000 linhas — `columnar_codec.rs:24` `CHUNK_GROUP_ROWS`, `columnar.rs:1404`)**, nunca
por-linha (o zstd é all-or-nothing por chunk); (b) o flamegraph do re-fetch não pode reintroduzir os frames
`palloc`/`heap_form_tuple` para mais que k linhas; (c) I/O = 0 tem de continuar valendo — **fora de cache**, k
sobreviventes espalhados em k chunk-groups distintos disparam k × leituras de chunk-group (amplificação de I/O),
o falso-ganho clássico de late-mat (ver Abadi, Corner 4).

## Coverage Corner 4 — Techniques

**Q1 — O critério late-vs-early (C-Store + Abadi/web R0).**

`cstore-stonebraker-2005.pdf` é o read-optimized column store (RS/WS + *projections* + *storage keys*/positions
para reconstrução de tupla — `:70,153,317`), mas **não** dá o critério late-vs-early explícito. A fonte canônica
é o follow-up **Abadi, Myers, DeWitt, Madden, "Materialization Strategies in a Column-Oriented DBMS", ICDE 2007**
(R0 web, resolve: `https://www.cs.umd.edu/~abadi/papers/abadiicde2007.pdf`), que define exatamente o critério:

> *"late materialization … does not form tuples until after some part of the plan … the problem with this late
> materialization approach is that it requires **re-scanning the base columns to form tuples**"* (Abadi 2007, §1).

E a **heurística de conclusão** (verbatim, Abadi 2007 § Conclusion):

> *"if output data is **aggregated**, or if the query has **low selectivity (highly selective predicates)**, or if
> input data is **compressed** using a light-weight compression technique, a **late materialization** strategy
> should be used. Otherwise, for high selectivity, non-aggregated, non-compressed data, **early materialization**
> should be used."*

**Mapeamento ao nosso regime `ORDER BY key LIMIT k`:** um `LIMIT k` com k≪N é *aggregation-like* (reduz N→k, como
o "output aggregated" da heurística) **e** é *highly selective* (poucas linhas sobrevivem). Nossos dados são
**zstd-comprimidos** (light-weight-ish). Os TRÊS gatilhos da heurística de Abadi apontam para **late
materialization** neste regime. Diferença honesta vs o caso clássico de Abadi: lá o predicado seletivo produz a
position-list **antes** de tocar as colunas de saída; aqui a "seleção" (top-k) exige ler a **chave inteira** de
todas as N linhas (inevitável) — é um híbrido "scan-full-da-chave + re-access-seletivo das colunas de saída".

**Q2 — O DESENHO no nosso path + o row-locator disponível.**

Infra existente reusável: já há CustomScan no projeto (`theodb_rs/src/am/customscan.rs`,
`columnar_agg.rs` [2096 linhas], `df_executor.rs`, `set_rel_pathlist`/`create_upper_paths`) e a primitiva de
leitura colunar `decode_columns(rel, projection, predicates, skip)` (`columnar.rs:824`) que decodifica **só as
colunas pedidas** em `Vec<Option<Vec<u8>>>` **sem formar linha**.

Desenho concreto do M158 — um **CustomScan que substitui `Scan + Sort + Limit`** quando o path é
`SELECT <out_cols> FROM t_columnar ORDER BY key LIMIT k` (sem GROUP BY):

```
FASE 1 (chave-only, vetorizada — sobre TODAS as N linhas):
  decode_columns(rel, Some(&[key_col]), preds, skip)   // columnar.rs:824 — só a chave, sem heap_form_tuple
  para cada linha global i (na ordem first_row_number,stripe_id — columnar.rs:142):
     BinaryHeap<(Key, Locator)> de tamanho ≤ k          // Rust std, O(N log k) — NÃO DataFusion TopK
     Locator = índice global i  → (stripe_idx, chunk_group = i/10000, r = i%10000)
FASE 2 (re-fetch — só as k linhas vencedoras):
  agrupa os k locators por (stripe, chunk_group)        // grão de 10k linhas — nunca por-linha
  para cada chunk-group tocado: decode_stripe/decode_columns das OUT_COLS desse cg
  form_row(out_cols) SÓ para as k linhas alvo           // columnar.rs:671 — heap_form_tuple só k vezes
  emite as k linhas na ordem do heap
```

**Row-locator:** hoje **não** há TID exposto — `columnar_scan_getnextslot` faz `ExecStoreVirtualTuple` sem
setar `tts_tid` (`columnar.rs:1202-1229`); o `row_number` reservado na escrita (`columnar.rs:170,451`) é usado
só para ordem/contabilidade, nunca materializado no slot. **Isso não é bloqueador:** o CustomScan **é dono** de
todo o pipeline scan→topk→refetch dentro de UM nó do executor, então o locator é **interno** (`(stripe_idx,
chunk_group, r)`), não precisa ser um ctid que o executor carrega. Estabilidade sob MVCC: garantida **dentro do
scan** pelo set de stripes MVCC-fixo no `begin` (`columnar.rs:483-487`) + ordem determinística por
`first_row_number` (`columnar.rs:142`).

**Q3 — O CUSTO do re-fetch vs o ganho (o núcleo — números).**

Âncora (M148, marcada como estimativa escalada das proporções medidas): query alvo com **N ≈ 13 005** linhas,
materialização eager ≈ **104 ms** (a fatia ~80% do scan; ~**8 µs/linha**); scan total ≈ 130 ms; fatia decode
7.2% ≈ **~9 ms**. (São ESTIMATIVAS a partir do baseline real M148 — o número absoluto exige re-medir com o
harness Q5; as proporções 80%/7% são medidas.)

Estimativa late-mat, **k = 10**, N = 13 005:

| Fase | Custo | Derivação |
|---|---|---|
| F1 decode(1 chave) | ~1-2 ms | 7.2% ÷ Ncols do scan; 1 coluna via `decode_columns` |
| F1 TopK O(N log k) | <1 ms | 13005·log₂10 ≈ 43k compares int (BinaryHeap std) |
| F2 re-decode chunk-groups tocados | ~1-9 ms | N=13005 = 2 chunk-groups; pior caso decodifica ambos ≈ 1 passe de decode (~9 ms) |
| F2 `form_row`(k) | ~0.08 ms | 10 × 8 µs |
| **Total late-mat** | **~3-12 ms** | vs **~104 ms** eager |

**Ganho estimado: ~9-30× na materialização; ~4-8× na query inteira** para k≪N. A razão de o re-fetch NÃO comer
o ganho: a parte CARA (`form_row`/`palloc`, ~80%) é paga só para **k** linhas; o re-fetch só re-paga o **decode**
(~7%, barato). Mesmo pior-caso (sobreviventes espalhados), o re-fetch é limitado por **um passe extra de decode**,
não por re-materialização.

**Break-even / quando PERDE:** late-mat deixa de ganhar quando `(N−k)/N × 80%  ≤  (chunk_groups_tocados/total) ×
7% + custo_TopK`. Grosso modo ganha enquanto **k/N ≲ 0.9**; sweet-spot que paga a complexidade do CustomScan é
**k/N ≲ 0.1**. **Regime de PERDA (honest-negative):** (a) **k ≈ N** (LIMIT grande) → nada a diferir + overhead
de re-fetch → perda; (b) **projeção só-da-chave** (`SELECT key … ORDER BY key`) → nada a diferir → puro overhead;
(c) **fora de cache + sobreviventes espalhados** → k leituras de chunk-group de 10k linhas cada → amplificação de
I/O (o falso-ganho de Abadi §1 "re-processing disk blocks"); (d) **cobertura marginal** — a maioria das Top-N do
ClickBench é `GROUP BY x ORDER BY count LIMIT`, que já roteia pelo **agg CustomScan** (M100/M114) que **não**
materializa linha-a-linha → poucas queries puras `SELECT cols … ORDER BY key LIMIT k` sobram (o eco do M155,
`cobertura marginal = 0`).

## Cross-cutting Comparison — early vs late materialization

| Dimensão | Early (hoje, M149) | Late (M158 proposto) |
|---|---|---|
| Materialização evitada (`form_row`/`palloc`, ~80% do scan) | paga para **N** linhas | paga só para **k** linhas → economiza (N−k)/N × ~80% |
| Custo de decode | 1 passe (chave+out juntos) | chave (1 passe, 1 col) + out re-decode dos chunk-groups tocados (~1 passe pior-caso) |
| Custo do re-fetch | zero (não há) | **positional-join à MonetDB** (`monetdb…:668`); grão de 10k linhas; barato se cacheado, amplifica I/O fora de cache |
| MVCC / byte-identidade | trivial (path atual) | preservado: set de stripes MVCC-fixo (`columnar.rs:483`) + locator interno estável; A/B `diverged=0` (conjunto) |
| Overhead de nó executor | Scan+Sort+Limit (3 nós) | 1 CustomScan (pode ser **net-positivo** — menos nós; o Sort ~1.6ms some) |
| Quando vence (Abadi 2007) | high-selectivity, não-agregado, não-comprimido | **low-selectivity (k≪N), aggregation-like (LIMIT), comprimido (zstd)** ✅ nosso regime |
| Quando PERDE | — | k≈N; projeção só-da-chave; fora de cache espalhado; cobertura marginal (Top-N já é GROUP BY) |

## ADRs

### D1 — Desenho do CustomScan (scan+topk+refetch num nó) com `BinaryHeap` std, não o TopK do DataFusion

**Decisão:** implementar um CustomScan que **substitui `Scan+Sort+Limit`** só no path `SELECT <out_cols> FROM
t_columnar ORDER BY key LIMIT k` (sem GROUP BY), com heap de top-k em **`BinaryHeap<(Key,Locator)>` de Rust std**
sobre a chave decodificada via `decode_columns` (`columnar.rs:824`), reusando a infra de CustomScan já existente
(`customscan.rs`, `columnar_agg.rs`). **Alternativas rejeitadas:** (i) **TopK do DataFusion** (`topk/mod.rs:113`)
— opera sobre `RecordBatch` e guarda o batch inteiro para reconstrução (`mod.rs:434`), ou seja já faz
early-ish-materialization, contra o objetivo; exige materializar a chave em Arrow. (ii) **heapsort do PG** (M155)
— roda **depois** do `scan_getnextslot`, quando a materialização já ocorreu e o slot é virtual sem `tts_tid`
(`columnar.rs:1226`) — impossível injetar o locator. Cita `architecture.md` (fronteira: o CustomScan é o
composition-root do pipeline; o `decode_columns` é o seam de leitura colunar já documentado, `columnar.rs:819`).
Five-question gate (theodb-evolution): reusa `decode_columns`/CustomScan (não reinventa); MVCC preservado;
A/B byte-idêntico obrigatório.

### D2 — Row-locator INTERNO (stripe_idx, chunk_group, r), não um ctid exposto

**Decisão:** o locator é interno ao CustomScan — índice global de linha na ordem `ORDER BY first_row_number,
stripe_id` (`columnar.rs:142`) decomposto em `(stripe_idx, chunk_group = i/10000, r = i%10000)`
(`CHUNK_GROUP_ROWS=10000`, `columnar_codec.rs:24`). Estável sob MVCC **dentro do scan** pelo set de stripes
fixado no `begin` sob a snapshot (`columnar.rs:483-487`). **Alternativa rejeitada:** materializar `tts_tid`/
`row_number` no slot (`columnar.rs:170`) e usar as callbacks TID do TableAM — hoje **não suportadas**
(`columnar.rs:1638-1649` `columnar_unsupported!` para tidrange/tuple_tid_valid); implementá-las seria escopo
grande e desnecessário (o CustomScan é dono do pipeline, não precisa que o executor carregue o TID). YAGNI.

### D3 — Critério de viabilidade / honest-negative COM NÚMEROS (o ponto do milestone)

**VEREDITO: VIÁVEL-COM-RESTRIÇÕES.** Gatilhar o CustomScan late-mat **somente** quando **k/N ≲ 0.1** E
**≥ 2 colunas de saída além da chave** (ou ≥1 coluna varlena larga). Nesse regime, estimativa ancorada no M148:
**~3-12 ms vs ~104 ms** eager (**~9-30× na materialização; ~4-8× na query**), porque a materialização cara
(~80%) passa a ser paga só para k linhas e o re-fetch só re-paga o decode (~7%). **Honest-negative / NÃO
gatilhar** quando: k≈N (LIMIT grande), projeção só-da-chave, ou fora de cache com sobreviventes espalhados
(amplificação de I/O no grão de 10k linhas — Abadi 2007 §1 "re-processing disk blocks"). **Risco de cobertura
(o eco do M155, a ser medido ANTES de construir):** contar quantas queries do ClickBench são `SELECT cols …
ORDER BY key LIMIT k` **sem** GROUP BY — se ≈0 (a maioria é GROUP BY, já no agg CustomScan M100/M114 que não
materializa linha-a-linha), o veredito degrada para **honest-negative prático** como o M155 (técnica correta,
ganho de cobertura nulo → anti-sunk-cost, não construir). **Todos os números acima são ESTIMATIVAS** escaladas
das proporções medidas do M148 (80% materialização / 7% decode são medidos; os ms absolutos exigem re-medir com
o harness de Q5 num box). Cita `discover-phd-rigor.md` R3 (perf é claim com metodologia+número OU `UNBENCHMARKED`)
+ CLAUDE.md (measurement-first, anti-sunk-cost).

## Recommendations

**Para o `/to-plan` do M158 (SE a medição de cobertura passar):**

1. **Gate de cobertura PRIMEIRO (measurement-first, barato — evita o custo do M155).** Antes de qualquer código,
   rodar `EXPLAIN` sobre as queries do ClickBench (`benchmarks/clickbench/`) e **contar** quantas são
   `SELECT <cols> … ORDER BY key LIMIT k` **sem GROUP BY** e com k/N ≲ 0.1 e ≥2 out-cols. Se a contagem for ~0,
   **PARAR** e registrar honest-negative (o eco do M155). Se ≥1, seguir.
2. **CustomScan late-mat** (D1): match do path `Scan+Sort+Limit` em `set_rel_pathlist`/`create_upper_paths`,
   reusando `customscan.rs`/`columnar_agg.rs`; FASE 1 (`decode_columns` só-da-chave + `BinaryHeap<(Key,Locator)>`)
   → FASE 2 (re-decode dos chunk-groups tocados + `form_row` só para as k linhas). Locator interno (D2).
3. **Gate de gatilho** (D3): só ativar quando `k/N ≲ 0.1` E `n_out_cols ≥ 2` (ou varlena larga); caso contrário
   cair no path M149 atual (early). `enable_columnar_late_mat` default OFF até o A/B.
4. **Prova** (Q5): A/B `run_m128_clickbench.py` com **`diverged=0`** (igualdade de conjunto; caveat de empate do
   M155) + flamegraph `profile_columnar_scan.sh` mostrando `form_row`/`palloc` caírem para ~k/N + EC de MVCC
   (linha re-materializada = a que o eager veria sob a snapshot). Re-medir os ms absolutos (as estimativas D3
   viram números medidos ou `UNBENCHMARKED`).
5. **Guard de amplificação de I/O:** medir também **fora de cache** (dataset > `shared_buffers`) — se o
   re-fetch de sobreviventes espalhados disparar k leituras de chunk-group e a query ficar mais lenta que o
   eager, restringir o gatilho a datasets cacheados ou a chaves clusterizadas.

**SE o gate de cobertura falhar:** recomendação **honest-negative** — não construir; registrar que a técnica é
sólida (Abadi 2007 confirma o critério; o custo é favorável para k≪N) mas a **cobertura de queries do ClickBench
é marginal** porque as Top-N já roteiam pelo agg CustomScan que não materializa linha-a-linha (M155). Anti-sunk-cost.

## References

**Técnica: critério late-vs-early materialization (Q1)**
- `.claude/knowledge-base/references/papers/cstore-stonebraker-2005.pdf` (RS/WS, projections, storage keys/positions para reconstrução — `:70,153,317`). *primária, disco.*
- **[WEB R0]** Abadi, Myers, DeWitt, Madden, "Materialization Strategies in a Column-Oriented DBMS", ICDE 2007 — `https://www.cs.umd.edu/~abadi/papers/abadiicde2007.pdf` (heurística de conclusão verbatim: aggregated / low-selectivity / compressed → late; re-scanning cost §1). *primária, web, resolve (HTTP 200).*

**Técnica: desenho no nosso path + custo (Q2, Q3)**
- `theodb_rs/src/am/columnar.rs` — `form_row:671`, `decode_stripe:715`, loop de materialização `:789-791`, `decode_columns:824` (seam de leitura colunar), `read_visible_stripes:137-142` (ordem MVCC-estável), `ColumnarScanState:483-495` (set MVCC-fixo), `columnar_scan_getnextslot:1202-1229` (`ExecStoreVirtualTuple`, sem `tts_tid`), `columnar_unsupported!` TID `:1638-1649`, 10k chunk-groups `:1404`. *primária, disco.*
- `theodb_rs/src/am/columnar_codec.rs:24` — `CHUNK_GROUP_ROWS = 10_000` (grão do re-fetch). *primária, disco.*
- `docs/benchmarks/m148-flamegraph-scan.md:37-48` — baseline medido (81.7% materialização / 7.2% decode / I/O=0). *primária, disco.*
- **[WEB R0]** ClickHouse, "ORDER BY" / query optimization docs — `https://clickhouse.com/docs/en/sql-reference/statements/select/order-by` (o SOTA OSS que ships lazy/late reading em `ORDER BY … LIMIT`). *secundária, web, resolve (HTTP 200).*

**Dependência: TopK / heap (Q4)**
- `.claude/knowledge-base/references/datafusion/datafusion/physical-plan/src/topk/mod.rs:113,336,368,379,434` — `struct TopK`, `k: usize`, `TopKHeap::new`, `insert_batch`, `register_batch` (guarda o batch → early-ish). *primária, disco.*
- `memory: m155-topn-honest-negative` — PG já usa `top-N heapsort` (~1.6-2.6ms, não é gargalo); aponta late-mat como o lever. *primária, projeto.*

**Ferramenta / armadilhas vetorizadas (Q6)**
- `.claude/knowledge-base/references/papers/monetdb-x100-boncz-2005.pdf:659-673,309-314,761` — custo de banda da materialização completa por positional-joins; cache/TLB; chunks vetoriais cache-resident. *primária, disco.*
- **[WEB R0]** Abadi 2007 (acima) §1 — re-scanning/re-processing disk blocks é o custo do late (amplificação de I/O fora de cache). *primária, web.*

**Método de prova (Q5)**
- `benchmarks/profile_columnar_scan.sh:11` (EC-1 ≥500 amostras), `benchmarks/m148_selftime.py` (self-time dos folded), `benchmarks/run_m128_clickbench.py:143,173` (oráculo A/B `diverged=0` + caveat de empate). *primária, disco.*

**Regras**
- `.claude/rules/discover-phd-rigor.md` (R0 web, R1 SOTA-anchoring, R3 benchmark-or-UNBENCHMARKED), `.claude/rules/architecture.md` (fronteiras/composition-root do CustomScan). *disco.*
