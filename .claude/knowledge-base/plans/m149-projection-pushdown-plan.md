---
slug: m149-projection-pushdown
milestone_id: M149
created_at: 2026-07-24
goal: Reduzir o tempo de scan colunar em queries de projeção estreita via um CustomScan que materializa só as colunas referenciadas, com A/B byte-idêntico nas 43 queries do ClickBench
---

# Plano M149 — Projection pushdown no scan colunar (via CustomScan)

## Goal

Adicionar um CustomScan de projeção sobre `theodb_columnar` que materializa **apenas** as colunas
referenciadas por `targetlist ∪ qual`, medindo geomean ≥ 3× vs baseline pós-#190 em queries de projeção
estreita (`SELECT poucas_colunas FROM hits …`), com A/B byte-idêntico vs heap preservado nas 43 queries do
ClickBench.

## Context

Deriva do veredito medido do M148 (`docs/benchmarks/m148-flamegraph-scan.md`): o scan colunar é 100%
CPU-bound e ~80% do tempo é materializar cada linha como heap-tuple de 105 colunas (`form_row` + `palloc`);
o decode é só ~7%. O ganho do projection pushdown vem de reduzir a **materialização** (heap-tuple de N em vez
de 105 colunas), não do decode-skip. O discover (blueprint `m149-projection-pushdown-blueprint.md`) provou,
no source primário do Citus + PG18, que **um TableAM puro não vê as colunas projetadas** — projection exige um
CustomScan (padrão Citus `ColumnarAttrNeeded`). As peças difíceis já existem no repo.

## Baseline Context

| Peça | file:line | Papel no M149 | Reuso |
|---|---|---|---|
| `decode_stripe` | `theodb_rs/src/am/columnar.rs:684` | decodifica as 105 colunas de cada stripe (alvo) | estender c/ `wanted` |
| `form_row` | `columnar.rs:650` | `heap_form_tuple` de 105 datums (o frame ~22% do M148) | estender c/ `wanted` |
| `load_next_batch` | `columnar.rs:1026` | monta `cols` p/ todas via `(0..natts).map(coldesc)` | passar `wanted` |
| `decode_columns` | `columnar.rs:756` | **JÁ aceita `projection: Option<&[usize]>`** (M100/agg, caminho B) | consumir |
| `deform_rows_into_columns` | `columnar.rs:564` | **JÁ materializa só `wanted`** | consumir |
| `set_rel_pathlist_hook` + `PREV_HOOK` | `customscan.rs:218,264` | hook de path vivo (vector-filter), encadeável | registrar path de projeção |
| introspecção Var/targetlist/qual | `columnar_agg.rs` (~:205,:306) | análogo parcial do `ColumnarAttrNeeded` | reusar p/ `wanted` |
| baseline de latência | `docs/benchmarks/clickbench-1m-postfix-2026-07-24.md` | geomean pós-#190 (24,5s geral) | comparação |

git sha do baseline: v0.140.0 (`418c9ef`). Callers do `decode_stripe`/`form_row`: só `load_next_batch` (scan
plain) — o caminho B (df_executor) usa `decode_columns` direto, não é tocado.

## Prior Art & Related Work

- **Citus columnar** (source primário local `.claude/knowledge-base/references/citus/src/backend/columnar/`):
  `ColumnarAttrNeeded` (`columnar_customscan.c:1814`) = `pull_var_clause(targetlist ∪ qual)`; `columnar_getnextslot`
  (`columnar_tableam.c:322`) preenche só `attr_needed`. O padrão que o M149 transpõe.
- **PG18 contrato** (`references/postgres/src/include/access/tableam.h:334`): `scan_begin` sem bitmap de colunas → a razão do CustomScan.
- **Interno:** M100 (DataFusion CustomScan + `decode_columns` projetado), M105 (zone-map `directory_minmax`),
  M114/M115 (A/B in-PG byte-idêntico), o vector-filter CustomScan (`customscan.rs`).

## ADRs

### ADR-1 — Projeção via CustomScan, não via TableAM
- **Decisão:** implementar o projection pushdown como um `CustomScan` que vence o SeqScan sobre `theodb_columnar`, computando as colunas necessárias do `Plan`.
- **Rationale:** o contrato do TableAM PG18 (`tableam.h:334`, source primário) não expõe as colunas projetadas ao `scan_begin`; o slot é de largura total e a projeção acontece acima do nó de scan. É exatamente como o Citus resolve (Rule 9 — não reinventar; espelhar o prior art primário).
- **Alternativa rejeitada:** hackear `columnar_scan_begin` para inferir colunas — **impossível** pelo contrato (nenhum argumento carrega projeção). Rejeitada por inviabilidade arquitetural, não por preferência.

### ADR-2 — `wanted = pull_var_clause(targetlist ∪ qual)`, fail-safe para todas
- **Decisão:** derivar o conjunto de colunas da UNIÃO de targetlist e qual (+ colunas de predicados zone-map empurrados); se qualquer `Var` tiver `varattno ≤ 0` (system col / whole-row) OU a lista não for resolvível, fallback decode-tudo.
- **Rationale:** o risco sinalizado — `SELECT count(*) WHERE advengineid<>0` tem targetlist vazio; sem unir o qual, a coluna do filtro sumiria e o resultado mudaria. Citus (`ColumnarAttrNeeded`) faz exatamente a união. Fail-safe garante correção sobre performance.
- **Alternativa rejeitada:** só targetlist (mais simples) — quebra queries com filtro; rejeitada por correção.

### ADR-3 — Reusar `decode_columns` + `deform_rows_into_columns` (não duplicar o seam de leitura)
- **Decisão:** o novo caminho de projeção consome as primitivas de subconjunto já existentes; não cria um segundo decodificador.
- **Rationale:** DRY + o seam de leitura único (ADR do `columnar.rs:751`). As primitivas já são exercitadas pelo caminho B (M100), então reusá-las herda a correção medida.
- **Alternativa rejeitada:** um decode-por-projeção novo — duplicaria conhecimento; rejeitada (DRY).

### ADR-3.1 — Correção do ADR-3 na implementação: `want_mask` no caminho row-based (revisão pós-review)
- **Contexto:** o review (pilar architecture, MEDIUM) apontou que o M149 implementou `want_mask: &[bool]` em
  `decode_stripe`/`form_row` em vez de consumir literalmente o `decode_columns(projection: Option<&[usize]>)`
  do ADR-3, resultando em dois seams de projeção que compartilham o loop `read_chunked → zstd → decode_column`.
- **Decisão medida:** a divergência é **justificada e mantida**. O `decode_columns` (caminho B, M100) produz
  Arrow **column-major** para o executor vetorizado do DataFusion; o caminho de scan `getnextslot` (o que o
  M148 mediu como o gargalo) precisa de **heap-tuples row-major** via `form_row`. Reusar `decode_columns`
  diretamente forçaria uma conversão colunar→row que reintroduz a materialização que o M149 existe para
  cortar (o caminho (b) rejeitado no blueprint § Design). O `want_mask` é a projeção correta PARA o caminho
  row; ele decode-skip nas mesmas colunas + materializa só as pedidas, herdando o `deform`/`varlena_payload`
  compartilhado. A duplicação residual é o loop de decode (não a lógica de projeção), aceitável (KISS sobre
  uma abstração que uniria dois formatos de saída incompatíveis).
- **Débito honesto:** unificar o loop `read_chunked+zstd` das duas rotas num helper comum é um refactor de
  DRY sem mudança de comportamento — fora do escopo do M149 (não é correção); rastreado para follow-up.

## Nota de design (do template vecfilter `customscan.rs`)

O M149 é um **scan-replacing CustomScan** (estilo Citus `ColumnarScan`), NÃO um wrapper de children como
o vecfilter. O nó **É** o scan da tabela colunar (`scanrelid = rel.relid`):

- **Registro:** reusar `RegisterCustomScanMethods` + o `set_rel_pathlist_hook`/`PREV_HOOK` já vivos
  (`customscan.rs:263-265`); adicionar um `CustomPath` que vence o SeqScan quando a rel usa `theodb_columnar`
  e não é agg. Método tables próprias (`Methods<CustomPathMethods/CustomScanMethods/CustomExecMethods>` — o
  padrão `unsafe impl Sync` do vecfilter).
- **`wanted` via side-channel thread_local** (espelhar `SCAN_MEMBERSHIP`/`swap_active`/`ActiveGuard` — o
  RAII guard unwind-safe é obrigatório): `SCAN_PROJECTION: RefCell<Option<Rc<Vec<usize>>>>`. Derivado no
  begin de `pull_var_clause(plan.targetlist) ∪ pull_var_clause(plan.qual)`; `varattno==0`→None(todas),
  `varattno<0`→None(fallback).
- **begin:** `table_beginscan(rel, snapshot, 0, null)` (sem child); guardar o `TableScanDesc` no state
  (`#[repr(C)]` com `CustomScanState` primeiro). Respeitar `EXEC_FLAG_EXPLAIN_ONLY` (não abrir scan).
- **exec:** instalar `wanted` (ActiveGuard), `table_scan_getnextslot(scandesc, ForwardScanDirection, slot)`;
  o `columnar_scan_getnextslot` lê o thread_local e materializa só `wanted` (resto `isnull=true`).
- **end:** `table_endscan(scandesc)`. **rescan:** `table_rescan`. **xact/subxact cleanup:** limpar o
  thread_local (mesmo padrão do vecfilter — longjmp mid-pull).
- **columnar.rs:** `columnar_scan_getnextslot`/`load_next_batch`/`decode_stripe`/`form_row` leem o
  thread_local `SCAN_PROJECTION` (via um getter em customscan/columnar_project); `None`→todas (fallback,
  caminho plain intacto). Reusar `deform_rows_into_columns`(:564) p/ materializar o subconjunto.

**Riscos de lifecycle a cobrir (o vecfilter os documenta):** EXPLAIN_ONLY (não abrir scan), rescan
(nested-loop inner re-instala wanted), unwind-safety (ActiveGuard restaura o thread_local mesmo em
longjmp), múltiplos nós num plano (UNION — cada um instala o seu na janela de pull). O A/B nas 43 queries
é o gate de correção final.

## Phase 1 — CustomScan de projeção (skeleton + wanted)

### T1.1 — Registrar o CustomScan de projeção no path hook
- **Files to edit:** `theodb_rs/src/am/customscan.rs` (ou novo `columnar_project.rs` sibling), encadeando no `PREV_HOOK` existente.
- **Why this step:** o hook é o único ponto onde vemos o `RelOptInfo` para decidir vencer o SeqScan sobre `theodb_columnar` numa query não-agg. Reusa o mecanismo vivo (`customscan.rs:264`), não um novo hook.
- **TDD:** `test_projection_customscan_wins_on_columnar_seqscan` — `EXPLAIN SELECT url FROM hits_columnar` mostra o Custom Scan de projeção (não SeqScan). Given uma tabela `theodb_columnar` + query de projeção, When EXPLAIN, Then o plano contém o nó de projeção. RED: sem o registro, é SeqScan.
- **Acceptance:** o path só vence quando (a) a relação usa `theodb_columnar`, (b) não é roteada pelo columnar_agg, (c) não há agg. Fallback: SeqScan puro.

### T1.2 — Derivar `wanted = targetlist ∪ qual` com fail-safe
- **Files to edit:** o módulo do CustomScan (fn `columns_needed`).
- **Why this step:** ADR-2 — a união é o que preserva correção sob filtro. Reusa a introspecção de Var de `columnar_agg.rs`.
- **TDD:** `test_columns_needed_unions_targetlist_and_qual` — para `SELECT count(*) FROM hits WHERE advengineid<>0`, `wanted` contém `advengineid`; para `SELECT url FROM hits`, `wanted={url}`; para `SELECT * FROM hits`, `wanted`=todas; `varattno<=0` → None (fallback). Assert o bitmap exato.
- **Acceptance:** `varattno==0`→todas; `varattno<0`→None; caso normal→união 0-based.

## Phase 2 — Materialização projetada + fallback

### T2.1 — Empurrar `wanted` até `decode_stripe`/`form_row`
- **Files to edit:** `columnar.rs` (`decode_stripe`, `load_next_batch`, `form_row` recebem `wanted: Option<&[usize]>`; `None` = todas, preservando o caminho plain).
- **Why this step:** ataca o frame dominante do M148 — materializar heap-tuple de N colunas em vez de 105. Reusa `deform_rows_into_columns` (ADR-3).
- **TDD:** `test_decode_stripe_projected_materializes_subset` — decode com `wanted={col2}` produz slots onde só col2 está não-null e o valor é byte-idêntico ao decode-tudo naquela coluna. RED: hoje `decode_stripe` ignora `wanted`.
- **Concurrency tests:** (none — single-threaded scan path; o conjunto de stripes visíveis é resolvido no begin sob snapshot, intacto).

### T2.2 — Fallback decode-tudo quando `wanted` é None
- **Files to edit:** o exec do CustomScan + `load_next_batch`.
- **Why this step:** ADR-2 — correção sobre performance; o piso é o comportamento atual.
- **TDD:** `test_projection_fallback_decodes_all_on_whole_row` — `SELECT * FROM hits` / `SELECT ctid FROM hits` (system col) materializa todas as colunas, A/B byte-idêntico. RED: se o fallback quebrar, o A/B diverge.

## Phase 3 — Gate de correção + benchmark (evolution-gate)

### T3.1 — A/B byte-idêntico nas 43 queries do ClickBench
- **Files to edit:** reusar `benchmarks/run_m128_clickbench.py` (o oráculo A/B já existe; agora com `main()` fail-loud do M148).
- **Why this step:** Rule 5 — projeção não pode mudar resultado. O oráculo A/B (columnar vs heap) é o gate.
- **TDD/acceptance:** as 43 queries do ClickBench, A/B `diverged == 0` (o `run_m128_clickbench.py` sai != 0 se divergir). Rodado no droplet efêmero.

### T3.2 — Benchmark medido no droplet
- **Files to edit:** `docs/benchmarks/m149-projection-pushdown.md` (NEW).
- **Why this step:** DoD — geomean ≥ 3× em queries de projeção estreita, com o número REAL (não meta). Sem benchmark, sem claim (public-copy).
- **Acceptance:** doc com a metodologia + o número medido vs baseline pós-#190; honest-negative aceito (se o ganho for < 3×, registrar honestamente e re-priorizar M151).

## Coverage Matrix

| DoD (ROADMAP M149) | Task |
|---|---|
| scan obtém colunas referenciadas e passa a decode_stripe | T1.1, T1.2, T2.1 |
| A/B byte-idêntico nas 43 queries | T3.1 |
| geomean ≥ 3× em projeção estreita (número real) | T3.2 |
| fallback decode-tudo sem regressão | T2.2 |
| CHANGELOG [Unreleased] | Global DoD |

## Failure scenarios

- **Var não-resolvível / system col** → fallback decode-tudo (T2.2). Teste: `SELECT ctid, url FROM hits`.
- **Query com filtro re-checado no nó** (qpqual) sobre coluna não-projetada → `wanted` inclui qual (ADR-2). Teste: `WHERE advengineid<>0` sem projetar advengineid.
- **Path perde para outro CustomScan** (columnar_agg) → não registra; SeqScan/agg segue. Teste: query de agg mantém o caminho agg.

## Drawbacks & Risks

- **R1 (correção):** derivar `wanted` errado muda resultado. Severidade ALTA. Mitigação: ADR-2 união + A/B 43 queries (gate). Owner: implementação.
- **R2 (ganho < 3×):** se a materialização projetada não render 3× (ex.: overhead do CustomScan). Severidade MÉDIA. Mitigação: medir honestamente (T3.2); honest-negative re-prioriza M151 (vetorização é o teto real). Owner: benchmark.
- **R3 (interação com columnar_agg):** o novo path pode competir com o agg CustomScan. Severidade MÉDIA. Mitigação: registrar só quando não-agg (T1.1). Owner: implementação.

## Unresolved Questions

- O CustomScan de projeção deve também empurrar o `qual` como filtro (scan-level) ou deixar o executor re-checar? Para o M149, deixar o executor re-checar (KISS); o zone-map skip (M150) é o milestone do filtro. (resolvido: escopo M149 = só projeção de materialização.)

## Global DoD

- Todas as tasks com TDD RED→GREEN provado (build + A/B in-PG no droplet — `cargo pgrx test` não linka).
- A/B byte-idêntico nas 43 queries (T3.1).
- Benchmark medido publicado (T3.2).
- CHANGELOG `[Unreleased]` atualizado.
- Sem regressão no caminho B (df_executor) nem no plain SeqScan (fallback).
- `/code-quality` sem FAIL_HARD; `/review` READY_TO_MERGE.
