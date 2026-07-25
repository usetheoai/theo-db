---
slug: m153-groupby-text
milestone_id: M153
created_at: 2026-07-25
goal: Rotear GROUP BY por chave de texto no path AGG_SORTED ao CustomScan colunar quando a saída é re-ordenada por um Sort acima e a collation é determinística, subindo a cobertura ClickBench acima de 18 com A/B byte-idêntico ao heap.
---

# Plano — M153: Rotear GROUP BY texto (AGG_SORTED) ao CustomScan colunar

## Goal

Relaxar o declínio `swap_sorted_text_group_collation` (`columnar_agg.rs:943-946`) para ACEITAR `GROUP BY <coluna texto>`
no path `AGG_SORTED` quando **(1)** a collation de toda chave de grupo texto é DETERMINÍSTICA e **(2)** o nó pai do
`Agg` é um `Sort` pleno (que re-ordena a saída) — **medido:** `columnar_customscan_count` sobe acima de 18 (número real,
q16/q17/q33 do ClickBench), com `result_ab.diverged == 0` byte-idêntico ao heap.

## Context

O M152 (routing-map) mediu `swap_sorted_text_group_collation` como o first-blocker de q16,17,33,38 (4 queries; ~3
marginais depois de descontar q38 que tem text-`<>` no WHERE). O DoD do ROADMAP M153 e o M152 confirmam: o group-key
texto JÁ é aceito (`arrow_supported_group_type` inclui 25/1042/1043) e o path AGG_HASHED-texto já roteia; o único
declínio remanescente é o AGG_SORTED-texto, que o executor não consegue reproduzir na ordem de collation do PG (ele
emite ASC byte-wise). Insight: em `GROUP BY texto ORDER BY count(*) DESC LIMIT` o planner escolhe GroupAgg(AGG_SORTED) e
coloca um `Sort` (por count) ACIMA → a ordem de grupo é descartada pelo re-sort → a ordem byte-wise do executor é
irrelevante. Sob collation determinística a EQUIVALÊNCIA de grupo é byte-wise (contagens corretas).

## Prior Art & Related Work

- Routing-map `docs/benchmarks/m152-routing-map.md` (q16/17/33/38 first-blocker medido); blueprint § Classe 2.
- M114/M115 (o Agg-swap + o executor grouped que já processa chave texto), M151/M154 (o padrão guard-declina-ao-nativo
  por collation determinística), M131 #135 (`deparse_safe_tlist` — o Sort-sobre-agregado já tratado no swap).
- DataFusion `physical-plan/src/aggregates/group_values/multi_group_by/bytes.rs` (hash byte-keyed Utf8).

## Baseline Context

### Files that will be touched

| Arquivo | LoC | Papel | Mudança |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | ~1650 | `try_swap_agg` (:890) declina AGG_SORTED-texto (:943); `swap_walk` (:1018) recursa sem passar o pai | passar `parent` por `swap_walk`→`try_swap_agg`; relaxar o declínio texto com os 2 guards |

### Current callers / dependents

- `try_swap_agg` (columnar_agg.rs:890) — chamado SÓ por `swap_walk` (:1024). Threading do `parent` é local (2 call-sites recursivos + os `swap_walk_list`).
- `swap_walk` (:1018) — chamado pela raiz (o hook de plano) + recursivamente (lefttree/righttree) + `swap_walk_list` (Append/MergeAppend). Todos passam o novo `parent`.
- O executor grouped (`run_columnar_grouped_aggs`, df_executor) — já processa chave texto (M114/M115); NÃO muda.

### Domain glossary

- **AGG_SORTED (GroupAgg):** o planner assume input ordenado pela chave e emite grupos NA ordem da chave. O nosso executor emite ASC byte-wise. Numérico: reproduz ASC-nulls-last (já aceito). Texto: byte-order ≠ collation-order → hoje declina.
- **Collation determinística:** `get_collation_isdeterministic(collid)` — equality = bytes idênticos. Garante que o hash byte-wise do DataFusion agrupa exatamente como o PG (contagens corretas). Não-determinística: 2 strings byte-diferentes collation-iguais → grupos separados no DataFusion, juntos no PG → divergência.
- **Sort pleno acima (re-sort):** um nó `T_Sort` pai re-ordena TODA a saída por suas chaves → a ordem que emitimos é irrelevante. Exclui `T_IncrementalSort` (depende de pré-ordenação do input).

### Architecture boundaries affected

Nenhuma nova. Só relaxa a admissão do swap (planner-time) + threading de um ponteiro de pai. Sem novo formato de página, sem novo write.

## ADRs

### ADR-1 — Aceitar AGG_SORTED-texto SÓ com re-sort acima + collation determinística
- **Decisão:** no ramo `strat == AGG_SORTED` com chave texto, ao invés de declinar incondicionalmente, aceitar se `(1)` toda chave texto tem `get_collation_isdeterministic(collid) == true` E `(2)` `parent` é `T_Sort` (não `T_IncrementalSort`, não outro nó). Senão, declinar.
- **Rationale:** `(1)` garante contagens corretas (equality byte = collation sob determinismo); `(2)` garante ordem correta (o Sort pleno acima re-ordena a saída, tornando a ordem byte-wise do executor irrelevante). Ambos fail-closed. `GROUP BY texto ORDER BY texto` (ordem de grupo consumida direta, sem re-sort) e collation não-determinística DECLINAM. Espelha o guard do M151/M154.
- **Alternativa rejeitada:** emitir grupos em ordem de collation no executor (sort collation-aware no DataFusion) — complexo, superfície grande, sem ganho medido além de q16/17/33 (que têm re-sort). Rejeitada: aceitar texto sem checar o pai — divergiria em `GROUP BY texto ORDER BY texto`.

### ADR-2 — Passar `parent` por `swap_walk` vs buscar o pai por outra via
- **Decisão:** adicionar um parâmetro `parent: *mut pg_sys::Plan` a `swap_walk`/`swap_walk_list` (raiz passa `null`); `try_swap_agg` recebe o pai.
- **Rationale:** `swap_walk` já percorre com o nó atual em mãos — ele É o pai dos filhos que visita. Threading é local e KISS; nenhuma estrutura nova.
- **Alternativa rejeitada:** um segundo walk pré-computando pais num mapa — mais estado, mais complexidade, sem ganho.

## Dependency Graph

```
Fase 1 (threading parent + guard AGG_SORTED-texto, columnar_agg) ─→ Fase 2 (build + A/B + cobertura medida no droplet)
```

## Phase 1 — Relaxar o declínio AGG_SORTED-texto com os 2 guards

### T1.1 — parent-threading + guard (determinismo + re-sort)

#### Why this step
É o único ponto de declínio das GROUP-BY-texto sorted (M152). O executor já processa a chave texto; falta só relaxar a
admissão com prova de correção (contagem via determinismo, ordem via re-sort acima). Raciocínio: ADR-1/ADR-2.

#### Concurrency tests
(none — single-threaded) `try_swap_agg`/`swap_walk` rodam no planner (single-thread).

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — assinatura `swap_walk(slot, rtable, parent)` + `swap_walk_list(list, rtable, parent)`; a raiz chama com `parent = null`; recursões passam `plan` como pai; `try_swap_agg(plan, rtable, parent)`; no ramo `:943` texto: aceitar se `get_collation_isdeterministic` em toda chave texto E `parent` é `T_Sort` (não `T_IncrementalSort`/null), senão `admit_trace("swap_sorted_text_group_collation")` + decline.

#### TDD
- **RED:** harness A/B — `SELECT phrase, COUNT(*) FROM t_col GROUP BY phrase ORDER BY COUNT(*) DESC LIMIT 5` (collation default determinística) roteia (`theodb_columnar_agg` no EXPLAIN) e A/B == heap (conjunto+ordem). Falha antes (declina).
- **GREEN:** threading + os 2 guards.
- **REFACTOR:** provar por A/B os declínios: `GROUP BY phrase ORDER BY phrase` (sem re-sort → parent não-Sort → declina), collation ICU não-determinística (declina), numérico AGG_SORTED inalterado (regressão zero).

#### Acceptance criteria
- [ ] `GROUP BY texto ORDER BY count DESC LIMIT` (collation determinística) mostra `Custom Scan (theodb_columnar_agg)` no EXPLAIN (verificado por: grep no plano).
- [ ] A/B byte-idêntico (conjunto E ordem, incl. o corte do LIMIT) vs heap em toda query GROUP-BY-texto-sorted roteada (verificado por: A/B no pg_test + run_m128).
- [ ] `GROUP BY texto ORDER BY texto` (ordem de grupo direta, sem Sort pleno acima) DECLINA ao nativo (verificado por: EXPLAIN sem theodb_columnar_agg + A/B correto).
- [ ] Coluna texto com collation NÃO-determinística (ICU case-insensitive) DECLINA (verificado por: EXPLAIN + A/B — harness de regressão).
- [ ] AGG_SORTED numérico e AGG_HASHED inalterados — regressão zero (verificado por: run_m128 diverged=0 + as 18 anteriores continuam roteando).

#### DoD
- Testes passam (RED→GREEN no droplet); q16/q17/q33 roteiam.

## Phase 2 — A/B + cobertura medida

### T2.1 — Gate A/B ClickBench + cobertura + ablação + CHANGELOG

#### Why this step
DoD measurement-first: a cobertura sobe acima de 18, diverged=0, e o ganho de latência é medido por ablação OFF-vs-ON no mesmo binário.

#### Concurrency tests
(none — single-threaded) `max_parallel_workers_per_gather=0`.

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `docs/benchmarks/m153-groupby-text.md` (NEW) + `docs/benchmarks/m153-artifacts/`.
- `benchmarks/m153_ec_harness.sql` (NEW) — os guards A/B (determinística roteia, ORDER-BY-key declina, ICU declina, numérico inalterado).
- `CHANGELOG.md` `[Unreleased]`.

#### TDD
- **RED:** `run_m128 --agg` mostra `columnar_customscan_count > 18` + `diverged == 0`. Antes, 18.
- **GREEN:** rodar no droplet (head p/ cobertura + systematic p/ correção alta-cardinalidade, como M154).
- **REFACTOR:** ablação OFF-vs-ON no mesmo binário (oráculo M149/M150) para o ganho de latência das q16/17/33.

#### Acceptance criteria
- [ ] `columnar_customscan_count > 18` das 43 (verificado por: o JSON do run).
- [ ] `result_ab.diverged == 0` em toda query roteada, incluindo alta cardinalidade com work_mem adequado (verificado por: o oráculo A/B nos dois regimes).
- [ ] Ganho medido por ablação OFF-vs-ON (verificado por: os dois JSON, mesmo binário).
- [ ] CHANGELOG `[Unreleased]` (verificado por: `grep -c M153 CHANGELOG.md` >= 1).
- [ ] Droplet destruído (verificado por: 0 efêmeros).

#### DoD
- `docs/benchmarks/m153-groupby-text.md` com a cobertura medida (>18) + diverged=0 + ablação.

## Coverage Matrix

| Requisito (DoD do ROADMAP M153) | Task |
|---|---|
| Cobertura columnar_customscan sobe (14→14+K; aqui 18→18+K) | T2.1 |
| A/B byte-idêntico vs heap em toda GROUP-BY-texto roteada | T1.1, T2.1 |
| Guard de collation não-determinística DECLINA (harness de regressão) | T1.1 |
| Ganho medido por ablação OFF-vs-ON no mesmo binário | T2.1 |
| CHANGELOG | T2.1 |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Collation não-determinística agrupa strings byte-diferentes no PG mas não no DataFusion → contagem divergente | ALTA | Guard determinístico obrigatório (ADR-1(1)); harness de regressão ICU provando o decline | implementer |
| Ordem de grupo consumida direta (`ORDER BY texto`, sem re-sort) → ordem byte-wise ≠ collation → divergência | ALTA | Guard parent-é-Sort-pleno (ADR-1(2)); A/B em `GROUP BY texto ORDER BY texto` provando o decline | reviewer |
| Threading do `parent` quebra um caminho de `swap_walk` (Append/SubqueryScan) → pai errado → aceita indevidamente | MÉDIA | Raiz e todos os recursivos passam o pai correto; fail-closed (só `T_Sort` aceita); review rust-pgrx | implementer |
| IncrementalSort acima depende de pré-ordenação → nosso emit byte-wise quebra | MÉDIA | Excluir `T_IncrementalSort` explicitamente (só `T_Sort` pleno) | implementer |

## Unresolved Questions

- (none — every decision is resolved at plan time). A forma exata do plano das q16/17/33 (`Limit→Sort→Agg`) será CONFIRMADA por EXPLAIN no droplet antes do GREEN (measurement-first); se houver nó intermediário entre Sort e Agg, o guard fail-closed declina (correto) e ajusto o alvo — sem risco de incorreção.

## Global DoD

- [ ] O harness A/B `m153_ec_harness.sql` roteia a query determinística e declina as 3 negativas (verificado por: EXPLAIN mostra `theodb_columnar_agg` na determinística e `Aggregate` nas outras + valores A/B iguais).
- [ ] `run_m128 --agg` reporta `columnar_customscan_count > 18` E `result_ab.diverged == 0` nos dois regimes head+systematic (verificado por: os 2 JSON em `docs/benchmarks/m153-artifacts/`).
- [ ] A ablação OFF-vs-ON no mesmo binário quantifica o ganho das q16/17/33 (verificado por: os 2 JSON, mesmo `.so`).
- [ ] `/code-quality` emite verdict ∉ {FAIL_HARD, INVALID} (verificado por: o campo `verdict` do relatório de code-quality).
- [ ] `CHANGELOG.md [Unreleased]` cita M153 (verificado por: `grep -c M153 CHANGELOG.md` >= 1).
- [ ] Zero droplets efêmeros restam (verificado por: `doctl compute droplet list` sem o droplet do M153).

## Final Phase — Integration Validation

Build no droplet + EXPLAIN confirmando a forma do plano das q16/17/33 + A/B das 43 (diverged=0, count>18) + prova dos
declínios (ORDER-BY-key, ICU não-determinística) + ablação. A cadeia só está completa quando o A/B é byte-idêntico E a
cobertura sobe E os guards declinam o que deve. Falha → volta ao implement.
