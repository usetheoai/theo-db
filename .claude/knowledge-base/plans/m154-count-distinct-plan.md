---
slug: m154-count-distinct
milestone_id: M154
created_at: 2026-07-25
goal: Rotear COUNT(DISTINCT col) ao CustomScan DataFusion vetorizado via count_distinct exato, subindo a cobertura ClickBench acima de 14 (número medido) com A/B byte-idêntico ao heap.
---

# Plano — M154: Rotear COUNT(DISTINCT) exato ao CustomScan DataFusion

## Goal

Aceitar `COUNT(DISTINCT col)` no `admit()`/`classify_target_node` (removendo o declínio `aggdistinct` só para o
COUNT-DISTINCT-single-col) e mapeá-lo ao `count_distinct` EXATO do DataFusion — **medido:** cobertura
`columnar_customscan_count` sobe acima de 14 (número real no benchmark), com `result_ab.diverged == 0` byte-idêntico
ao heap; NUNCA usa approx/HLL.

## Context

O M152 (routing-map) mediu `agg_distinct_filter_order` como o 2º maior first-blocker (7 queries; q4/q5 puras
COUNT(DISTINCT), cobertura marginal ~2 + destrava compostos) e o reordenou como a fatia mais LIMPA (parity-clean) →
executar primeiro. O DataFusion já tem `count_distinct` exato (`functions_aggregate`); o gap é só o admit declinar
`aggdistinct` (columnar_agg.rs) + o AggSpec/serialização não ter o kind.

## Prior Art & Related Work

- Blueprint `columnar-gap-closing-strategy` § Classe 1; routing-map `docs/benchmarks/m152-routing-map.md` (q4/q5 medidas).
- M114/M115 (o AggSpec + Agg-swap), M151 (o padrão de gate A/B byte-idêntico).
- DataFusion `functions-aggregate/src/count.rs` (`DistinctCountAccumulator` exato) vs `approx_distinct`/HLL (o que NÃO usar).

## Baseline Context

### Files that will be touched

| Arquivo | LoC | Papel | Mudança |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | ~1640 | `classify_target_node` declina `aggdistinct` (~:400); encode/decode_private (kinds de agg) | aceitar `count(DISTINCT var)` → novo TargetSlot::Agg kind 8; serializar |
| `theodb_rs/src/am/df_executor.rs` | ~765 | `enum AggSpec` + `build_agg_exprs` (count/sum/avg) + `agg_datum` | + `AggSpec::CountDistinct(name)` → `count_distinct(col)`; datum int8 |

### Current callers / dependents

- `classify_target_node` (columnar_agg.rs) — chamado por build_admission. O check `aggfilter||aggorder||aggdistinct` (M152 trace `agg_distinct_filter_order`) declina TUDO com distinct; aceitar só COUNT-DISTINCT-single-col.
- `AggSpec` (df_executor.rs) — kinds serializados em encode_private (`kind` int) + decodificados em decode_private. Novo kind 8 = CountDistinct.
- `count_distinct` já importado? (funcs_aggregate) — verificar; senão importar (dep já no crate, Rule 9).

### Domain glossary

- **COUNT(DISTINCT col):** conta valores distintos não-NULL de `col`. PG e DataFusion `count_distinct` ambos exatos, excluem NULL → byte-idêntico. Output PG = int8 (bigint) = DataFusion Int64.
- **aggdistinct:** o campo `Aggref.aggdistinct` (não-nulo quando há DISTINCT). Aceitar só quando `name=="count"` + 1 arg Var; declinar em sum/avg/etc DISTINCT.
- **approx proibido:** `approx_distinct`/HLL dá ±~2% — jamais byte-idêntico; nunca rotear para ele.

### Architecture boundaries affected

Nenhuma nova. O COUNT(DISTINCT) entra pela mesma fronteira do admit→AggSpec→df_executor já estabelecida (M114). Sem novo write, sem mudança de formato de página.

## ADRs

### ADR-1 — count_distinct EXATO do DataFusion vs approx/HLL
- **Decisão:** `count_distinct(col(name))` (DistinctCountAccumulator exato).
- **Rationale:** PG COUNT(DISTINCT) é exato; o gate é A/B byte-idêntico. Approx/HLL (±2%) jamais casa. Rule 9 (reusar o exato do DataFusion). Cita blueprint § Classe 1.
- **Alternativa rejeitada:** approx_distinct/HLL — não byte-idêntico, viola o gate de correção.

### ADR-2 — Aceitar só COUNT(DISTINCT single-col Var) vs qualquer DISTINCT
- **Decisão:** aceitar `count(DISTINCT var)` de exatamente 1 arg que é `Var` de coluna base (arrow-suportado); manter declinado `sum/avg/min/max(DISTINCT ...)`, `aggfilter`, `aggorder`, `count(DISTINCT expr)` e `count(DISTINCT a,b)` (nº args ≠ 1 ou arg não-Var).
- **Rationale:** o count_distinct exato só é trivialmente correto para uma coluna base; DISTINCT em sum/avg tem semântica extra; expr/multi-arg re-materializa. KISS/YAGNI — escopo do que o M152 mediu (q4/q5 são count(DISTINCT col)).
- **Alternativa rejeitada:** aceitar todo DISTINCT — amplia a superfície A/B sem ganho medido.

### ADR-4 — Declinar COUNT(DISTINCT float4/float8) (review rust-pgrx HIGH)
- **Decisão:** no branch kind-8, declinar (`admit_trace("count_distinct_float_ieee_semantics")`) quando o tipo da coluna for `FLOAT4OID`/`FLOAT8OID`.
- **Rationale:** o `FloatDistinctCountAccumulator` do DataFusion dedup por total-order IEEE (`-0.0 ≠ +0.0`; NaN bit-patterns distintos contam separado); o `float8eq` do PG trata `0.0 == -0.0` e todo NaN como igual → `count(DISTINCT float)` diverge silenciosamente (ex.: `{0.0, -0.0}` → DataFusion 2, PG 1). O A/B do ClickBench não tem coluna com `-0.0` → não pega; o review prova o shape que o benchmark não exercita (lição M151). Restringir à classe provadamente-segura (int/text-determinístico) é o mesmo padrão do M151.
- **Alternativa rejeitada:** normalizar `-0.0→+0.0` + canonicalizar NaN antes do dedup — mais complexo, mais superfície, sem ganho de cobertura medido (nenhuma query ClickBench é `count(DISTINCT float)`).

### ADR-3 — Guard de collation determinística no COUNT(DISTINCT text) (edge review EC-1)
- **Decisão:** ao aceitar `count(DISTINCT var)` de coluna colacionável (texto), declinar (`admit_trace("count_distinct_nondeterministic_collation")`) quando a collation da `Var` NÃO for determinística (`!get_collation_isdeterministic(varcollid)`).
- **Rationale:** o `count_distinct` do DataFusion usa igualdade **byte-wise**; PG COUNT(DISTINCT text) usa a igualdade da collation. Para collation determinística (C/POSIX/default) coincidem (deterministic ⇒ igual = bytes idênticos); para NÃO-determinística (ICU `deterministic=false`) divergem silenciosamente → viola diverged=0. É o análogo, para igualdade, do guard de collation-order do M152. Fail-closed: declina, o nativo resolve correto.
- **Alternativa rejeitada:** aceitar todo texto — diverge sob ICU não-determinística (bug de correção). Rejeitada: declinar todo texto — perde q5 no caso comum (default determinística).

## Dependency Graph

```
Fase 1 (AggSpec::CountDistinct + build_agg_exprs + agg_datum, df_executor) ─→ Fase 2 (admit aceita count(DISTINCT var) + serialização, columnar_agg) ─→ Fase 3 (A/B + cobertura medida)
```

## Phase 1 — AggSpec::CountDistinct no df_executor

### T1.1 — CountDistinct kind + count_distinct expr + datum int8

#### Why this step
O executor precisa saber emitir `count_distinct(col)` e mapear o resultado (Int64) ao datum PG int8. Raciocínio:
ADR-1, é o kind que falta no AggSpec.

#### Concurrency tests
(none — single-threaded) O DataFusion roda single-thread neste caminho (o CustomScan não é parallel-aware).

#### Failure scenarios
(none — no external I/O touched) In-process sobre batch Arrow.

#### Files to edit
- `theodb_rs/src/am/df_executor.rs` — `enum AggSpec` + `CountDistinct(String)`; `build_agg_exprs` braço `count_distinct(col(name)).alias(...)`; `agg_datum`/output-arity (int8, arity 1); import `count_distinct` de functions_aggregate.

#### TDD
- **RED:** `test_count_distinct_expr_builds` — `AggSpec::CountDistinct("a")` produz um Expr `count(DISTINCT a)`; falha antes do braço existir (match não-exaustivo).
- **GREEN:** adicionar o braço + o import.
- **REFACTOR:** garantir output int8 (Int64→PG int8) via o mesmo caminho do CountStar.

#### Acceptance criteria
- [ ] `AggSpec::CountDistinct("a")` gera `count_distinct(col("a"))` (verificado por: `test_count_distinct_expr_builds` assere o Expr).
- [ ] O datum de saída é int8 (verificado por: A/B em T3.1 — o count bate o PG bigint).
- [ ] `cargo build` exit 0, `cargo clippy` 0 warning (verificado por: exit codes).

#### DoD
- Teste passa (RED→GREEN no droplet); build verde.

## Phase 2 — admit aceita COUNT(DISTINCT var) + serialização

### T2.1 — classify_target_node aceita count(DISTINCT single-col) + encode/decode_private kind 8

#### Why this step
O `classify_target_node` declina TODO `aggdistinct` (M152 trace). Aceitar só `count(DISTINCT var)` (1 coluna base),
mantendo declinado aggfilter/aggorder/outros-distinct, e serializar o novo kind. Raciocínio: ADR-2 — é o ponto que
o M152 mediu como blocker de 7 queries.

#### Concurrency tests
(none — single-threaded) admit roda no planner (single-thread).

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — `classify_target_node`: mudar o check para declinar só `aggfilter||aggorder` sempre + `aggdistinct` quando não for `count(DISTINCT single-var)` (nº args==1 E arg é `Var` de coluna base); guard ADR-3 (collation determinística no texto via `get_collation_isdeterministic(varcollid)`); aceitar → `ParsedAgg{kind:8, attno}`; `encode_private`/`decode_private` + kind 8; `AggSpec` mapping do kind 8 → CountDistinct(col_name).

#### TDD
- **RED:** `test_count_distinct_routes` (via harness A/B) — `SELECT COUNT(DISTINCT UserID) FROM t_col` roteia ao `theodb_columnar_agg` (EXPLAIN) e A/B == heap. Falha antes (declina).
- **GREEN:** aceitar + serializar + guard collation.
- **REFACTOR:** `count(DISTINCT expr)`, `count(DISTINCT a,b)` e `sum(DISTINCT ...)` continuam DECLINANDO (ADR-2); texto sob collation não-determinística declina (ADR-3) — todos provados por A/B (caem no nativo, correto).

#### Acceptance criteria
- [ ] `SELECT COUNT(DISTINCT UserID) FROM t_col` mostra `Custom Scan (theodb_columnar_agg)` no EXPLAIN (verificado por: grep no plano).
- [ ] A/B byte-idêntico: `COUNT(DISTINCT col)` == heap, incluindo NULLs excluídos e grupo vazio → 0 (verificado por: A/B valor no pg_test — cobre EC-2 todos-NULL→0).
- [ ] `count(DISTINCT col+1)`, `count(DISTINCT a,b)` (nº args≠1/não-Var) e `sum(DISTINCT col)` DECLINAM ao nativo (verificado por: EXPLAIN sem theodb_columnar_agg + A/B correto — cobre EC-3).
- [ ] `COUNT(DISTINCT text)` sob collation determinística (default) roteia e A/B==heap; sob collation ICU não-determinística DECLINA (verificado por: EXPLAIN + A/B — cobre EC-1/ADR-3).
- [ ] `COUNT(DISTINCT c) ... GROUP BY k` roteia com A/B==heap OU declina ao nativo (nunca roteia errado) (verificado por: EXPLAIN + A/B — cobre EC-4, escopo scalar-first).
- [ ] Round-trip encode/decode do kind 8 correto (verificado por: a query executa sem `bad agg kind`).

#### DoD
- Testes passam (RED→GREEN no droplet); q4/q5 roteiam; guards EC-1/3/4 provados.

## Phase 3 — A/B + cobertura medida

### T3.1 — Gate A/B ClickBench + cobertura + CHANGELOG

#### Why this step
O DoD é medido: a cobertura sobe acima de 14 (q4/q5 + compostos destravados), A/B diverged=0. Gate measurement-first.

#### Failure scenarios
(none — no external I/O touched) A única "falha" relevante: uma classe de tipo de coluna cujo count_distinct diverge → honest-negative (declina).

#### Concurrency tests
(none — single-threaded) `max_parallel_workers_per_gather=0`.

#### Files to edit
- `docs/benchmarks/m154-count-distinct.md` (NEW) + `docs/benchmarks/m154-artifacts/` — cobertura + diverged=0.
- `CHANGELOG.md` `[Unreleased]`.

#### TDD
- **RED:** `run_m128 --agg` deve mostrar `columnar_customscan_count > 14` (era 14) + `result_ab.diverged == 0`. Antes, 14.
- **GREEN:** rodar no droplet; capturar.
- **REFACTOR:** listar honestamente quais queries COUNT(DISTINCT) rotearam (q4/q5 + os compostos que só tinham distinct).

#### Acceptance criteria
- [ ] `columnar_customscan_count > 14` das 43 (verificado por: o JSON do run).
- [ ] `result_ab.diverged == 0` em toda query roteada (verificado por: o oráculo A/B).
- [ ] CHANGELOG `[Unreleased]` (verificado por: `grep -c M154 CHANGELOG.md` >= 1).
- [ ] Droplet destruído (verificado por: 0 efêmeros).

#### DoD
- `docs/benchmarks/m154-count-distinct.md` com a cobertura medida (>14) + diverged=0.

## Coverage Matrix

| Requisito (DoD do ROADMAP M154) | Task |
|---|---|
| Cobertura columnar_customscan sobe (número real) | T3.1 |
| A/B byte-idêntico vs heap | T2.1, T3.1 |
| NUNCA approx/HLL (guard) | T1.1 (ADR-1) |
| Texto sob collation não-determinística não diverge (EC-1) | T2.1 (ADR-3) |
| Custo de memória medido, honest-negative se não ganha | T3.1 |
| CHANGELOG | T3.1 |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Tentação de approx para performance → perde byte-identidade | ALTA | `count_distinct` EXATO só (ADR-1); teste que prova o valor exato vs nativo | implementer |
| DISTINCT em sum/avg ou expr acidentalmente aceito | MÉDIA | ADR-2: aceitar só `count(DISTINCT single-var)`; guard + A/B provando que sum(DISTINCT)/count(DISTINCT expr) declinam | reviewer |
| Alta cardinalidade (COUNT(DISTINCT UserID) ~1M) não ganha vs hash do PG | MÉDIA | Medir; honest-negative (declinar) se pior num regime medido | implementer |

## Unresolved Questions

- (none — every decision is resolved at plan time). count_distinct exato (ADR-1), só single-var count (ADR-2) — fechadas pelo M152 + o blueprint.

## Global DoD

- [ ] Testes verdes (RED→GREEN no droplet); clippy limpo.
- [ ] `run_m128 --agg` diverged=0 + count > 14.
- [ ] Benchmark em `docs/benchmarks/m154-*`.
- [ ] `/code-quality` ∉ {FAIL_HARD, INVALID}.
- [ ] CHANGELOG.
- [ ] Droplet destruído.

## Final Phase — Integration Validation

Build no droplet + A/B das 43 (diverged=0, count>14) + prova que q4/q5 roteiam e que sum(DISTINCT)/count(DISTINCT expr)
declinam. A cadeia só está completa quando o A/B é byte-idêntico E a cobertura sobe. Falha → volta ao implement.
