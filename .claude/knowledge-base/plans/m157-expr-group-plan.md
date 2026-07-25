---
slug: m157-expr-group
milestone_id: M157
created_at: 2026-07-25
goal: Rotear GROUP BY date_trunc(timestamp, unit) como chave de grupo ao CustomScan colunar (3º canal custom_private + group-expr DataFusion), subindo a cobertura ClickBench acima de 31 com A/B byte-idêntico e declinando fail-closed timestamptz/HAVING/CASE/EXTRACT.
---

# Plano — M157: Rotear GROUP BY por expressão (date_trunc) ao CustomScan colunar

## Goal

Admitir `GROUP BY date_trunc('unit', ts_col)` (`ts_col` = `timestamp` sem tz; `unit ∈ {sec,min,hour,day,month,quarter,year}`)
como chave de grupo no CustomScan colunar vetorizado, serializando-a num 3º canal de `custom_private` e reconstruindo-a
como `date_trunc(lit(unit), col(base))` no `.aggregate` do DataFusion — **medido:** `columnar_customscan_count` sobe
acima de 31 (a query q42 do ClickBench), com `result_ab.diverged == 0` byte-idêntico ao heap.

## Context

O blueprint `columnar-expr-group-having` (discover, SHIPPABLE_WITH_CAVEATS) mediu, por fonte primária, que das 7 queries
expr-group/HAVING **só a q42 (`date_trunc('minute', EventTime)`) é genuinamente alcançável**: as 2 queries HAVING
(q27/q28) morrem em blockers INDEPENDENTES (`AVG(length(URL))` agg-sobre-expr, `REGEXP_REPLACE`) → implementar HAVING
sozinho roteia **zero** queries (lição M155). CASE/EXTRACT declinam. Hoje `classify_target_node`
(`columnar_agg.rs:490`) só admite chave `T_Var`. O M156 provou o 2º canal `custom_private`; o M157 REUSA o padrão (3º
canal). Risco crítico provado pelo blueprint: `date_trunc` sobre `timestamptz` diverge do PG sob `TimeZone≠UTC`
(a mesma classe do cross-type temporal que o review M151 pegou).

## Prior Art & Related Work

- Blueprint `columnar-expr-group-having-blueprint.md` (discover, este ciclo) — o desenho + fontes primárias (PG `timestamp.c` timezone; DataFusion `date_trunc.rs`).
- M156 (2º canal custom_private + guards), M153 (group-key + collation), M151 (guard cross-type temporal — a classe de divergência).

## Baseline Context

### Files that will be touched

| Arquivo | LoC | Papel | Mudança |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | ~1720 | `classify_target_node`(:490, só `T_Var` group-key), `Admitted`(:415), `admit`(:734), encode/decode do custom_private | + `GroupExprSpec`/`GroupFunc`; ramo `T_FuncExpr date_trunc` na admissão; 3º canal de serialização; layout kind=2 |
| `theodb_rs/src/am/df_executor.rs` | ~790 | `run_columnar_grouped_aggs`(:460) — group_exprs só `col(name)` | + group-expr `date_trunc(lit(unit), col(base))` + materialização de volta (out_typoid 1114, unit Arrow ns→µs) |

### Current callers / dependents

- `classify_target_node` — chamado por `build_admission`(:514) no walk do target; hoje `T_Var`→group, `T_Aggref`→agg, resto→None.
- `Admitted.group_cols` (Vec<(attno,typoid)>) + `layout` (kind 0=group,1=agg) → `encode_private`/`begin_custom_scan`/`run_columnar_grouped_aggs`. O 3º canal (group-exprs) é paralelo, como o 2º canal texto do M156.
- `run_columnar_grouped_aggs` (df_executor.rs:460) monta `group_exprs = group_cols.map(col)`; ganha os group-exprs date_trunc.

### Domain glossary

- **GroupExprSpec:** `{ base_attno: i32, func: GroupFunc, unit: String, out_typoid: u32 }`, `enum GroupFunc { DateTrunc }`. Uma chave de grupo que é `date_trunc('unit', col[base_attno])`, saída `timestamp` (1114).
- **3º canal (custom_private):** hoje `custom_private = List[IntList, text_preds_List]` (M156). Vira `List[IntList, text_preds_List, group_exprs_List]`; cada group-expr = `[Integer(base_attno), Integer(func), String(unit), Integer(out_typoid)]` (nós Value serializáveis — padrão M156).
- **Guards (per ADR-2):** admite só `func=date_trunc` + base tipo **1114** (timestamp) + `unit` na whitelist; `timestamptz` (1184), `week`, granularidade fora, CASE/EXTRACT/const/aritmética → DECLINAM.

### Architecture boundaries affected

Nenhuma nova. Estende a serialização `custom_private` (mesma fronteira M156) + o `run_columnar_grouped_aggs` (M100/M114). Sem novo formato de página, sem novo write.

## ADRs

### ADR-1 — Chave-expr date_trunc via 3º canal de nós (reuso do padrão M156, não reinventar)

- **Decisão:** `custom_private` ganha um 3º elemento `group_exprs_List`; cada entry = `[Integer(base_attno), Integer(func), String(unit), Integer(out_typoid)]`.
- **Rationale:** o M156 já provou o canal de nós `Value` (Integer/String) copyObject-safe. Reusar (Regra 9/KISS) — zero mecanismo novo. Alternativa rejeitada: um novo formato de página / GUC (YAGNI); serializar a expr inteira via copyObject do FuncExpr (mais frágil que os 4 campos escalares).

### ADR-2 — Guard de timezone: admite `timestamp` (1114); declina `timestamptz` (1184) incondicionalmente

- **Decisão:** só `date_trunc` sobre base tipo 1114 empurra; 1184 declina sempre.
- **Rationale:** fonte primária (blueprint Corner 3): PG `timestamp_trunc` trunca campo-a-campo timezone-independente (casa o DataFusion `parsed_tz=None`); `timestamptz_trunc` usa `session_timezone` → diverge do DataFusion (UTC) sob `TimeZone≠UTC`. Declinar 1184 é a mesma decisão do cross-type temporal do M151 (fail-closed → nativo). Alternativa rejeitada: empurrar 1184 assumindo UTC — divergência silenciosa sob TZ≠UTC (o A/B de ClickBench, TZ=UTC, não pegaria).

### ADR-3 — HAVING/CASE/EXTRACT declinam (honest-negative measured; date_trunc é o único lever)

- **Decisão:** M157 NÃO implementa HAVING/CASE/EXTRACT.
- **Rationale:** o blueprint mediu que HAVING roteia 0 queries (q27/q28 têm blockers independentes) → implementar seria esforço sem valor (esforço≠complexidade, anti-sunk-cost, lição M155). CASE/EXTRACT declinam por unificação de tipo / numeric-output. Documentado como honest-negative.

## Dependency Graph

```
Fase 1 (GroupExprSpec + admissão date_trunc, columnar_agg) ─→ Fase 2 (3º canal encode/decode, columnar_agg) ─→ Fase 3 (group-expr date_trunc no DataFusion + materialização, df_executor) ─→ Fase 4 (build droplet + A/B + cobertura)
```

## Phase 1 — GroupExprSpec + admissão da chave date_trunc

### T1.1 — `classify_target_node` ramo `T_FuncExpr date_trunc` + guards

#### Why this step
É o gate de correção: só `date_trunc(timestamp, unit-whitelist)` entra; timestamptz/granularidade-fora/CASE/EXTRACT declinam. Raciocínio: ADR-2 + blueprint Corner 4.

#### Concurrency tests
(none — single-threaded) roda no planner.

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — `enum GroupFunc { DateTrunc }`, `struct GroupExprSpec`; em `classify_target_node`, ramo `T_FuncExpr`: nome `date_trunc` (via `get_func_name`/proname, sem OID hardcoded), arg0 `Const` text ∈ {sec,min,hour,day,month,quarter,year}, arg1 `Var` base-rel tipo 1114. `Admitted` ganha `group_exprs: Vec<GroupExprSpec>`; layout kind=2. `admit` aceita group_exprs não-vazio como chave.

#### TDD
- **RED:** A/B — `GROUP BY date_trunc('day', ts) , count(*)` roteia (`theodb_columnar` no EXPLAIN) e A/B == heap. Falha antes (declina — group-key não-Var).
- **GREEN:** admissão + guards + GroupExprSpec.
- **REFACTOR:** provar declínios: `date_trunc('week',...)`, `date_trunc(..., ts_tz timestamptz)`, `EXTRACT`, `CASE`, `GROUP BY (col+1)` → todos declinam ao nativo, A/B correto.

#### Acceptance criteria
- [ ] `GROUP BY date_trunc('day'|'month'|'minute', ts::timestamp)` roteia (verificado por: EXPLAIN mostra o CustomScan colunar).
- [ ] `timestamptz`, `week`, granularidade fora, EXTRACT, CASE, `(col+1)` DECLINAM (verificado por: EXPLAIN não contém 'Custom Scan' E `result_ab_identical==true` no EC harness).

#### DoD
- A/B `GROUP BY date_trunc('day', ts)` == heap no droplet (EC harness/run_m128) e EXPLAIN mostra o CustomScan colunar (verificado por: exit 0 do harness + grep 'Custom Scan').

## Phase 2 — 3º canal de serialização (group-exprs)

### T2.1 — encode/decode do canal de group-exprs

#### Why this step
A chave-expr precisa sobreviver ao ciclo de plano pelo canal de nós (padrão M156). Raciocínio: ADR-1.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — `encode_private` embrulha `List[IntList, text_preds_List, group_exprs_List]`; cada group-expr = `[Integer(base_attno), Integer(func), String(unit), Integer(out_typoid)]`. `begin_custom_scan` desembrulha o 3º elemento (ausente → 0 group-exprs, backward-compat) e reconstrói `Vec<GroupExprSpec>`.

#### TDD
- **RED:** `GROUP BY date_trunc('day', ts)` executa sem `bad private` e A/B == heap. Falha antes.
- **GREEN:** encode/decode do 3º canal.
- **REFACTOR:** todas as granularidades da whitelist round-trip; backward-compat (as 31 anteriores + o text-WHERE do M156 continuam roteando).

#### Acceptance criteria
- [ ] Round-trip do group-expr correto (verificado por: a query date_trunc executa sem `bad private` E `result_ab.diverged==0`).
- [ ] Backward-compat: as 31 do baseline + o text-WHERE M156 continuam diverged=0 (verificado por: run_m128).

#### DoD
- As 31 do baseline + o text-WHERE M156 mantêm `result_ab.diverged==0` no `run_m128 --agg`, e a query date_trunc executa sem `bad private` (verificado por: os 2 JSON diverged==0).

## Phase 3 — Group-expr date_trunc no DataFusion + materialização

### T3.1 — `run_columnar_grouped_aggs` group-expr + materialização de volta

#### Why this step
O executor precisa avaliar `date_trunc` como chave de grupo e materializar a saída como `timestamp` PG. Raciocínio: blueprint Corner 2.

#### Concurrency tests
(none — single-threaded) DataFusion single-thread neste caminho.

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/df_executor.rs` — `run_columnar_grouped_aggs`: estender `group_exprs` com `date_trunc(lit(ScalarValue::Utf8(unit)), col(base_name))`; a coluna base entra na projeção; materializar a saída via `arrow_value_to_datum` com `out_typoid=1114`, atento à UNIT Arrow (ns→µs — MUST-verify no A/B).

#### TDD
- **RED:** o A/B de `GROUP BY date_trunc('day', ts)` (T1.1) passa a byte-idêntico. Falha antes (group-expr inexistente no exec).
- **GREEN:** o group-expr + materialização.
- **REFACTOR:** minute/day/month/year byte-idênticos ao PG; `cargo build`+clippy limpos; a unit Arrow (ns→µs) confirmada byte-idêntica (o risco ALTA).

#### Acceptance criteria
- [ ] `date_trunc` minute/day/month/year byte-idêntico vs heap (verificado por: `result_ab.diverged==0` no `run_m128 --agg`).
- [ ] Materialização Arrow→Datum (ns→µs) correta (verificado por: `result_ab.diverged==0`; timestamps do date_trunc iguais ao heap, não off-by-1000 µs).
- [ ] `cargo build --release` sai 0 E `cargo clippy --features pg18` sem warning no código novo (verificado por: exit codes no droplet).

#### DoD
- `result_ab.diverged==0` para date_trunc no `run_m128 --agg` (head+systematic) e `cargo build --release`+clippy saem 0 (verificado por: os JSON + os exit codes).

## Phase 4 — A/B + cobertura medida

### T4.1 — Gate A/B ClickBench + cobertura + CHANGELOG

#### Why this step
DoD measurement-first: cobertura sobe acima de 31 (q42), diverged=0 nos dois regimes; honest-negatives documentados.

#### Concurrency tests
(none — single-threaded) `max_parallel_workers_per_gather=0`.

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `docs/benchmarks/m157-expr-group.md` (NEW) + `docs/benchmarks/m157-artifacts/`; `benchmarks/m157_ec_harness.sql` (NEW); `CHANGELOG.md`.

#### TDD
- **RED:** `run_m128 --agg` mostra `columnar_customscan_count > 31` (q42) + `diverged == 0`. Antes, 31.
- **GREEN:** rodar no droplet (head + systematic).
- **REFACTOR:** documentar honestamente: q42 roteia; HAVING/CASE/EXTRACT/timestamptz permanecem honest-negative (com o motivo medido).

#### Acceptance criteria
- [ ] `columnar_customscan_count > 31` (verificado por: o JSON do run).
- [ ] `result_ab.diverged == 0` nos dois regimes (verificado por: os 2 JSON).
- [ ] EC harness prova o decline de timestamptz (sob `SET TimeZone='America/Sao_Paulo'`) + granularidade-fora/CASE/EXTRACT.
- [ ] CHANGELOG `[Unreleased]` cita M157.
- [ ] Zero droplets efêmeros ao fim da maratona.

#### DoD
- `docs/benchmarks/m157-expr-group.md` existe com `columnar_customscan_count>31` + `diverged==0` (head+systematic) + os honest-negatives (verificado por: os 2 JSON citados no doc).

## Coverage Matrix

| Requisito (DoD do ROADMAP M157) | Task |
|---|---|
| Cobertura columnar_customscan sobe (>31) | T4.1 |
| A/B byte-idêntico vs heap (date_trunc group-key) | T1.1, T3.1, T4.1 |
| Guards fail-closed (timestamptz/granularidade/CASE/EXTRACT declinam) | T1.1, ADR-2 |
| Serialização do group-expr (3º canal) | T2.1, ADR-1 |
| Semântica date_trunc timezone casa PG ou declina | ADR-2, T1.1 |
| CHANGELOG | T4.1 |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Materialização Arrow date_trunc ns→µs ≠ o µs do PG timestamp → off-by-1000 | ALTA | A/B byte-idêntico (T3.1) prova a unit; converter explicitamente se necessário | implementer |
| `timestamptz` empurrado por engano → divergência sob TZ≠UTC | ALTA | Guard tipo 1114-only fail-closed (ADR-2); EC sob `SET TimeZone` provando o decline | reviewer |
| Cobertura marginal = +1 (só q42) — esforço alto p/ ganho pequeno | MÉDIA | Aceito: é uma CAPABILITY (time-bucketing colunar), não só a query; HAVING/CASE honest-negative documentado (ADR-3) | owner |
| Granularidade `week` do PG (ISO) ≠ Arrow → divergência | MÉDIA | `week` fora da whitelist → declina (ADR-2) | implementer |

## Unresolved Questions

- (none — every decision is resolved at plan time). A unit exata do Arrow `date_trunc` (ns vs µs) na materialização de volta será confirmada por leitura no implement; o A/B de T3.1 é o gate.

## Global DoD

- [ ] Testes verdes (RED→GREEN no droplet); clippy limpo.
- [ ] `run_m128 --agg` diverged=0 + count > 31 (head + systematic).
- [ ] Benchmark em `docs/benchmarks/m157-*` com honest-negatives.
- [ ] `/code-quality` ∉ {FAIL_HARD, INVALID}.
- [ ] CHANGELOG.
- [ ] Droplet da maratona reusado (destruído só ao fim de M159).

## Final Phase — Integration Validation

Build no droplet + A/B das 43 (diverged=0, count>31) + prova dos declínios (timestamptz sob TZ≠UTC, granularidade-fora,
CASE/EXTRACT) + round-trip do group-expr. A cadeia só está completa quando o A/B é byte-idêntico E a cobertura sobe E os
guards declinam o que devem. Falha → volta ao implement.
