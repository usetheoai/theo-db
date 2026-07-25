---
slug: m152-routing-map
milestone_id: M152
created_at: 2026-07-25
goal: Instrumentar o admit() do CustomScan de agregação para emitir a razão de declínio por query e medir, sobre as 43 do ClickBench, a razão real pela qual cada uma das 29 não-vetorizadas declina — produzindo o mapa de roteamento que define o alvo de cobertura de M153-M155.
---

# Plano — M152: Spike measurement-first do roteamento das 29 queries não-vetorizadas

## Goal

Adicionar um trace de razão-de-declínio ao `admit()`/`classify_target_node` (atrás de `THEODB_ADMIT_TRACE=1`,
behavior-neutral quando off) e MEDIR sobre as 43 queries do ClickBench a razão REAL de cada declínio — produzindo
`docs/benchmarks/m152-routing-map.md` que mapeia cada uma das ~29 não-vetorizadas → classe → razão (`file:line`) e
computa a **cobertura marginal por fatia** (quantas queries CADA slice M153-M155 adicionaria), com número medido.

## Context

O gap colunar vs ClickBench é as 29/43 queries row-based (blueprint `columnar-gap-closing-strategy`). O blueprint
alertou que a razão de declínio de várias é AMBÍGUA estaticamente: `arrow_supported_group_type` (df_executor.rs:142)
JÁ inclui texto, então GROUP BY texto NÃO declina no group-key — declina por outro motivo (WHERE text `<>`, min(texto),
COUNT DISTINCT, shape do plano, GROUP BY expr). Ex.: q33 (`GROUP BY URL COUNT(*)` sem WHERE) não tem bloqueio óbvio
mas não roteia. Sem medir, o alvo de M153-M155 seria chute. Este spike mede a verdade (estilo M148 antes de M149-M151).

## Prior Art & Related Work

- Blueprint: `.claude/knowledge-base/discoveries/blueprints/columnar-gap-closing-strategy-blueprint.md`
- M148 (o precedente de spike-mede-antes-de-construir); M151 (`docs/benchmarks/m151-datafusion-coverage.md` + o JSON com o roteamento atual das 43).
- Instrumentação-como-medição já usada: `THEODB_SCAN_PROFILE` (M150, `columnar.rs`), `SKIP_STATS`.

## Baseline Context

### Files that will be touched

| Arquivo | LoC | Papel | Mudança |
|---|---|---|---|
| `theodb_rs/src/am/columnar_agg.rs` | ~1620 | `admit()`/`classify_target_node`/`build_admission` — os ~15 pontos de `return None` (declínio) | + macro/helper de trace por-ponto (emite `THEODB_ADMIT_TRACE`), behavior-neutral quando off |
| `benchmarks/run_m128_clickbench.py` | — | harness ClickBench (`columnar_customscan_count`) | reuso (nenhuma mudança) — o trace sai no log do backend |
| `docs/benchmarks/m152-routing-map.md` (NEW) | — | o deliverable do spike | mapa de roteamento + cobertura marginal por fatia |

### Current callers / dependents

- `admit()` é chamado pelo `set_upper_paths_hook`/M115 swap. Os `return None` já existem; o trace só ANOTA cada um antes de retornar — sem mudar o fluxo.
- O JSON do M151 (`m151-artifacts/m151-clickbench-agg.json`) tem o roteamento atual (14/43) — o trace explica o PORQUÊ das 29.

### Domain glossary

- **razão de declínio:** qual das ~15 condições `return None` do `admit()` a query bateu primeiro (a que impede o roteamento).
- **cobertura marginal por fatia:** nº de queries cujo(s) ÚNICO(s) bloqueio(s) são fechados por uma fatia (M153 GROUP BY texto / M154 COUNT DISTINCT / M155 Top-N). É o alvo REAL, não o total da classe.
- **trace behavior-neutral:** com `THEODB_ADMIT_TRACE` off, zero mudança de comportamento (o admit declina igual); só liga o log.

### Architecture boundaries affected

Nenhuma. É instrumentação de medição no planner (`columnar_agg.rs`), atrás de env flag, sem novo write, sem mudança de formato nem de roteamento (o admit declina exatamente as mesmas queries — o trace só reporta o porquê).

## ADRs

### ADR-1 — Instrumentação de trace vs análise estática pura
- **Decisão:** instrumentar os pontos de declínio do `admit()` (trace atrás de `THEODB_ADMIT_TRACE=1`) e medir a razão real.
- **Rationale:** a razão de declínio de várias queries é AMBÍGUA estaticamente (q33 não tem bloqueio óbvio mas não roteia — shape do plano). Measurement-first (Regra 5, CLAUDE.md): o código diz a verdade, meu parsing chuta. Espelha `THEODB_SCAN_PROFILE` do M150.
- **Alternativa rejeitada:** classificar só estaticamente — deixa a q33 e similares sem razão comprovada (viola "evidências 100%").

### ADR-2 — Trace behavior-neutral (env flag) vs mudar o admit
- **Decisão:** o trace NÃO muda o fluxo do admit (só anota antes do `return None` existente); off por default.
- **Rationale:** o M152 é spike de MEDIÇÃO — o roteamento (14/43) não pode mudar. O A/B do M151 (diverged=0) deve permanecer válido. Rung 5 parsimony (mínimo que mede).
- **Alternativa rejeitada:** já implementar um fix — seria M153+, não o spike; measurement-first exige medir antes.

## Dependency Graph

```
Fase 1 (instrumentar o trace) ─→ Fase 2 (build + rodar + coletar razões) ─→ Fase 3 (mapa + cobertura marginal + report)
```

## Phase 1 — Instrumentar as razões de declínio do admit()

### T1.1 — Trace por-ponto-de-declínio atrás de THEODB_ADMIT_TRACE

#### Why this step
Cada `return None` do `admit()`/`classify_target_node` é uma razão de declínio distinta. Anotar cada um (id +
`file:line` + a query) atrás de um env flag dá o ground-truth de PORQUÊ cada query declina — o que o parsing estático
não resolve (q33). Raciocínio: ADR-1, measurement-first.

#### Concurrency tests
(none — single-threaded) O admit roda no planner (single-thread); o trace é um `eprintln`/`pgrx::log` atrás de flag, sem estado compartilhado.

#### Files to edit
- `theodb_rs/src/am/columnar_agg.rs` — um helper `admit_trace(reason: &str)` (checa `THEODB_ADMIT_TRACE`, emite `pgrx::log!`) chamado imediatamente antes dos `return None` relevantes de `admit`/`classify_target_node`/`build_admission`, com um id estável por ponto (ex. `"decline: group_key_type"`, `"decline: aggdistinct"`, `"decline: min_max_unordered"`, `"decline: agg_expr"`, `"decline: unpushable_qual"`, `"decline: group_by_expr_const"`, `"decline: having_grouping_window_distinct"`, `"decline: agg_split_nonsimple"`).

#### TDD
- **RED:** `test_admit_trace_emits_reason` — com `THEODB_ADMIT_TRACE=1`, uma query que declina (ex. `COUNT(DISTINCT UserID)`) emite no log `decline: aggdistinct`; sem o flag, nada. Falha antes do helper existir.
- **GREEN:** adicionar o helper + as chamadas.
- **REFACTOR:** garantir zero mudança de roteamento (o admit ainda declina/aceita as mesmas queries — o trace só reporta).

#### Acceptance criteria
- [ ] Com `THEODB_ADMIT_TRACE=1`, cada query declinada emite `decline: <razão>` no log do backend (verificado por: grep no log após rodar as 43).
- [ ] Com o flag off, zero emissão e roteamento IDÊNTICO ao M151 (verificado por: `columnar_customscan_count` == 14, `diverged` == 0 — behavior-neutral).
- [ ] `cargo build` verde; `cargo clippy` limpo.

#### DoD
- Teste passa (RED→GREEN no droplet); build verde; behavior-neutral confirmado (cobertura 14 inalterada).

## Phase 2 — Medir as razões reais sobre as 43

### T2.1 — Rodar as 43 com o trace + coletar a razão primária por query

#### Why this step
Rodar `run_m128 --agg` com `THEODB_ADMIT_TRACE=1` e capturar, para cada uma das 29 não-roteadas, a PRIMEIRA razão
de declínio emitida — o ground-truth. Raciocínio: resolve a ambiguidade (q33 etc.) com evidência.

#### Failure scenarios
(none — no external I/O touched) É medição in-process; a única "falha" é uma query que erra (regexp timeout q28 pré-existente) → registrar como errored, não como declínio.

#### Concurrency tests
(none — single-threaded) `max_parallel_workers_per_gather=0`.

#### Files to edit
- `docs/benchmarks/m152-artifacts/` (NEW) — o log de trace + o JSON do run.

#### TDD
- **RED:** o run É o teste — sem o trace, não há razão por query (só routed/not do M151). Com o trace, cada uma das 29 tem uma razão.
- **GREEN:** rodar no droplet, capturar o log de trace + o JSON, extrair a razão primária por query.
- **REFACTOR:** validar consistência — TODA query não-roteada tem ≥1 razão; TODA roteada (14) tem ZERO trace de declínio (cross-check com o JSON).

#### Acceptance criteria
- [ ] Cada uma das ~29 não-roteadas tem a razão primária de declínio capturada do trace (verificado por: o log tem uma linha `decline:` por query não-roteada).
- [ ] As 14 roteadas não emitem trace de declínio (verificado por: cross-check log vs JSON — consistência).
- [ ] O droplet é destruído ao fim (0 efêmeros).

#### DoD
- Log de trace + JSON salvos; razão primária por query extraída; consistência com o JSON confirmada.

## Phase 3 — Mapa de roteamento + cobertura marginal por fatia + report

### T3.1 — Produzir docs/benchmarks/m152-routing-map.md

#### Why this step
Consolidar em um mapa acionável: cada query → classe → razão (`file:line`) → e a **cobertura marginal por fatia**
(quantas queries cada M153/M154/M155 realmente adiciona, considerando queries com MÚLTIPLOS bloqueios). Raciocínio:
é o deliverable que define o escopo REAL de M153-M155 — pode confirmar OU corrigir o ranking do blueprint.

#### Failure scenarios
(none — no external I/O touched)

#### Concurrency tests
(none — single-threaded)

#### Files to edit
- `docs/benchmarks/m152-routing-map.md` (NEW) — o mapa + a cobertura marginal + o veredito.
- `CHANGELOG.md` `[Unreleased]`.

#### TDD
- **RED:** sem o mapa, o alvo de M153-M155 é chute; o report É o teste (tem que responder "qual fatia adiciona quantas queries realmente").
- **GREEN:** escrever o mapa a partir do trace + a análise de bloqueios-múltiplos.
- **REFACTOR:** veredito honesto — se GROUP BY texto adiciona menos que COUNT DISTINCT (por bloqueios-múltiplos), re-priorizar M153↔M154 e anotar no ROADMAP.

#### Acceptance criteria
- [ ] `docs/benchmarks/m152-routing-map.md`: tabela das ~29 → classe → razão (`file:line`) → bloqueios (pode ser >1).
- [ ] Cobertura marginal por fatia: M153 (GROUP BY texto) = +X queries; M154 (COUNT DISTINCT) = +Y; M155 (Top-N) = +Z — número medido (queries cujo ÚNICO bloqueio remanescente é o da fatia).
- [ ] Veredito: a ordem de M153-M155 é confirmada OU corrigida com base no ganho marginal medido.
- [ ] CHANGELOG `[Unreleased]`.

#### DoD
- O mapa existe com a cobertura marginal medida por fatia; o veredito confirma/corrige o escopo de M153-M155.

## Coverage Matrix

| Requisito (DoD do ROADMAP M152) | Task |
|---|---|
| Cada query não-vetorizada → classe → roteia hoje → razão do declínio (`file:line`) | T1.1 (trace), T2.1 (medir), T3.1 (mapa) |
| Alvo de cobertura estimado por fatia (número medido) | T3.1 (cobertura marginal) |
| Veredito honesto: qual fatia tem o maior ganho não-coberto | T3.1 |
| Sem alteração de código de PRODUÇÃO (trace behavior-neutral atrás de flag) | T1.1 (ADR-2) |
| Artefato JSON do run | T2.1 |
| CHANGELOG | T3.1 |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| O trace muda o comportamento do admit (roteamento) | ALTA | Behavior-neutral: só anota antes do `return None` existente; cross-check `columnar_customscan_count`==14 com flag off | implementer |
| O mapa pode contradizer o blueprint (GROUP BY texto adiciona menos que o esperado) | MÉDIA | É o propósito do spike (measurement-first); re-priorizar M153-M155 honestamente e anotar no ROADMAP | implementer |
| Bloqueios-múltiplos subestimam/superestimam a cobertura marginal | MÉDIA | Capturar TODOS os bloqueios por query (não só o primeiro) via múltiplas passagens do trace, ou analisar o SQL + o trace juntos | reviewer |

## Unresolved Questions

- (none — every decision is resolved at plan time). O spike existe justamente para resolver as questões abertas (as razões de declínio); o método (trace instrumentado) está fechado nos ADRs.

## Global DoD

- [ ] Teste do trace verde (RED→GREEN no droplet); build/clippy limpos.
- [ ] Behavior-neutral: `columnar_customscan_count`==14, `diverged`==0 com flag off.
- [ ] `docs/benchmarks/m152-routing-map.md` com o mapa + cobertura marginal medida.
- [ ] `/code-quality` ∉ {FAIL_HARD, INVALID}.
- [ ] CHANGELOG `[Unreleased]`.
- [ ] Droplet efêmero destruído.

## Final Phase — Integration Validation

Build no droplet + rodar as 43 com/sem o trace: com o flag, cada não-roteada tem razão; sem o flag, roteamento
idêntico ao M151 (14, diverged=0). O mapa só está completo quando responde, com número medido, quantas queries cada
fatia M153-M155 adiciona — confirmando ou corrigindo o escopo. Falha em qualquer → volta ao implement.
