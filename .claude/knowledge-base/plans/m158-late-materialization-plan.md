---
slug: m158-late-materialization
milestone_id: M158
created_at: 2026-07-25
goal: Adicionar um CustomScan de late materialization (theodb_columnar_topk) que funde Limit(k)→Sort([key])→columnar-project, decodificando só {filtro∪chave} para todas as linhas e materializando a projeção completa só para o top-k, com speedup MEDIDO por flamegraph e byte-identidade (LIMIT-preserving A/B) — ou honest-negative se o ganho não materializar.
---

# Plano — M158: Late materialization (theodb_columnar_topk) no regime ORDER BY key LIMIT k

## Goal

Fundir `Limit(k) → Sort([single key]) → CustomScan(theodb_columnar_project)` sobre uma tabela colunar num novo
CustomScan `theodb_columnar_topk` que decodifica só {colunas do filtro ∪ chave de ordenação} para TODAS as N linhas,
faz top-k por `BinaryHeap<(Key, Locator)>`, e materializa a projeção COMPLETA (`form_row`) só para as k linhas
sobreviventes — **medido:** flamegraph mostra `form_row`/`palloc` caindo para ~k/N do baseline (1287ms/100k), e o
resultado é byte-idêntico ao heap (LIMIT-preserving symmetric-EXCEPT = 0). Honest-negative aceito se o ganho não
materializar (anti-sunk-cost, como o M155).

## Context

O M148 mediu ~80% do scan colunar = materializar cada linha como heap-tuple (`form_row`/`palloc`). O M149 projeta as
colunas do targetlist, mas no regime `SELECT <cols> … ORDER BY key LIMIT k` TODAS as N linhas materializam e só k
sobrevivem (baseline medido: 1287ms para `SELECT * … ORDER BY eventtime LIMIT 10`, 105 cols × 100k). O blueprint
`columnar-late-materialization` (discover, VIÁVEL-COM-RESTRIÇÕES) + o coverage-gate (`docs/benchmarks/m158-coverage-gate.md`,
PASSA) + a verificação de empate (top-10 determinístico, fwd=rev=0) confirmam: viável e provável. Critério SOTA
(Abadi 2007): LIMIT k≪N + compressão → late materialization. Cita `rules/architecture.md` (CustomScan) +
`theodb-evolution` (five-question gate + invariantes MVCC) + `parsimony-ladder`.

## Prior Art & Related Work

- Blueprint `columnar-late-materialization-blueprint.md` (discover, este ciclo) — o desenho + Abadi 2007 + custo estimado.
- M148 (flamegraph + harness `profile_columnar_scan.sh`), M149 (`theodb_columnar_project` — o CustomScan a estender), M115 (Agg-swap post-planning — o padrão de swap), M155 (o tie-caveat + o lever apontado).

## Baseline Context

### Files that will be touched

| Arquivo | LoC | Papel | Mudança |
|---|---|---|---|
| `theodb_rs/src/am/columnar_project.rs` (ou onde vive o `theodb_columnar_project`) | ? | O CustomScan de projeção M149 | + o path topk (fase 1 chave, fase 2 refetch) OU um novo módulo `columnar_topk.rs` |
| `theodb_rs/src/am/columnar.rs` | ~1700 | `decode_stripe`/`form_row`/`want_mask` + o row-locator (stripe,chunk_group,row) | expor decode-só-da-chave + re-decode por locator |
| `theodb_rs/src/am/guc.rs` (ou onde) | ? | GUCs | + `enable_columnar_late_mat` (default OFF) |

### Current callers / dependents

- O `theodb_columnar_project` (M149) roteia `SELECT <cols>` sobre colunar. O plano `Limit→Sort→project` é o alvo do swap.
- `decode_stripe`(columnar.rs:715) / `form_row`(:671) / `want_mask`(M149) — reusados para as duas fases.
- O locator interno `(stripe_idx, chunk_group=i/CHUNK_GROUP_ROWS, row)` é estável sob o set de stripes fixado no begin (MVCC, como o agg path).

### Domain glossary

- **Late materialization:** decodificar só {filtro∪chave} p/ todas as linhas; materializar a projeção completa só p/ o top-k.
- **Locator:** `(stripe_idx, chunk_group, row_in_cg)` — endereça uma linha para re-materialização na fase 2.
- **Trigger gate:** `enable_columnar_late_mat=on` E o shape (1 chave de sort, LIMIT k presente, projeção ≥2 cols ou SELECT *) E k/N ≲ 0.1 — senão cai no path M149 (early).

### Architecture boundaries affected

Novo CustomScan (`theodb_columnar_topk`) na fronteira do planner (post-planning swap, como M115). Sem novo formato de página, sem novo write. Reusa decode_stripe/form_row.

## ADRs

### ADR-1 — Post-planning swap de `Limit(k)→Sort([key])→columnar-project` (padrão M115), não um novo path do planner

- **Decisão:** detectar no `planner_hook` (pós-standard_planner) o padrão `Limit → Sort(1 key) → CustomScan(project sobre colunar)` e trocar pelo `theodb_columnar_topk`; senão, deixar o plano nativo.
- **Rationale:** reusa a mecânica de swap provada do M115/M156/M157 (Regra 9/KISS); o post-planning tem o LIMIT/Sort/projeção resolvidos. Alternativa rejeitada: um Custom Path em set_rel_pathlist (mais invasivo, compete com o custo do planner).

### ADR-2 — Row-locator interno (stripe, chunk_group, row), estável sob MVCC pelo set de stripes fixado no begin

- **Decisão:** a fase 1 emite `(key, (stripe_idx, chunk_group, row))`; a fase 2 re-decodifica os chunk-groups tocados e `form_row` só das k linhas.
- **Rationale:** o CustomScan é dono do pipeline (não expõe TID); o set de stripes visível é fixado no begin (mesma snapshot), então o locator é estável e MVCC-correto (a linha re-materializada = a que o eager veria). Alternativa rejeitada: expor ctid/TID no scan (mudança grande no TableAM, YAGNI).

### ADR-3 — Trigger-gated + honest-negative measured (o critério de parada)

- **Decisão:** `enable_columnar_late_mat` default OFF; só ativa no shape gated. Se o flamegraph pós-fix NÃO mostrar `form_row` caindo para ~k/N OU a query não ficar mais rápida num regime medido → honest-negative (documentar, não shipar ligado).
- **Rationale:** measurement-first (DoD), anti-sunk-cost (M155). O ganho é estimado (~4-8×) mas DEVE virar número medido ou UNBENCHMARKED. Guard de amplificação de I/O fora-de-cache (blueprint rec 5).

## Dependency Graph

```
Fase 1 (swap Limit→Sort→project → topk + GUC) ─→ Fase 2 (decode-só-chave + BinaryHeap top-k + locators) ─→ Fase 3 (re-fetch + form_row do top-k, emit em ordem) ─→ Fase 4 (flamegraph antes/depois + LIMIT-preserving A/B + MVCC)
```

## Phase 1 — Swap detection + GUC

### T1.1 — Detectar `Limit→Sort([key])→columnar-project` e trocar por `theodb_columnar_topk`

#### Why this step
O gate: só o shape provadamente-seguro (1 chave, LIMIT, colunar) entra. Raciocínio: ADR-1.

#### Concurrency tests
(none — single-threaded) roda no planner.

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/` — `planner_hook` walk: match `T_Limit → T_Sort (numCols==1) → CustomScan(project colunar)`; extrair k (limitCount), a chave (sortColIdx[0] + direção), a projeção. Guard: `enable_columnar_late_mat`, 1 key, k const, colunar. Swap para o novo `theodb_columnar_topk` CustomScan (custom_private carrega key attno + dir + k + projeção + filtro).

#### TDD
- **RED:** `SET enable_columnar_late_mat=on; EXPLAIN SELECT * FROM col ORDER BY key LIMIT 10` mostra `theodb_columnar_topk` (não Limit→Sort→project). Falha antes.
- **GREEN:** o swap.
- **REFACTOR:** declina (path M149) quando OFF, k não-const, >1 key, não-colunar, sem LIMIT.

#### Acceptance criteria
- [ ] Com `enable_columnar_late_mat=on`, o plano `SELECT <cols> … ORDER BY key LIMIT k` sobre colunar mostra `theodb_columnar_topk` (verificado por: EXPLAIN).
- [ ] OFF (default) OU shape não-suportado → path M149 (early) inalterado (verificado por: EXPLAIN + as 32 anteriores diverged=0).

#### DoD
- Com GUC on o EXPLAIN mostra `theodb_columnar_topk`; com GUC off o `run_m128 --agg` mantém `result_ab.diverged==0` nas 32 (verificado por: EXPLAIN + o JSON diverged==0).

## Phase 2 — Fase 1 do late-mat: decode-só-chave + top-k

### T2.1 — decode da chave + filtro + `BinaryHeap<(Key, Locator)>`

#### Why this step
O coração do late-mat: não materializar as N linhas; só a chave + locator. Raciocínio: blueprint Corner 4 + ADR-2.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/columnar.rs` + o novo path — decodificar só {colunas do filtro ∪ chave}; aplicar o filtro (reusar `build_filter`/o predicate do M149); manter `BinaryHeap<(KeyOrd, Locator)>` de tamanho k (min-heap ou max-heap conforme a direção); locator=(stripe,chunk_group,row).

#### TDD
- **RED:** o top-k por chave (fase 1) = o top-k do heap (LIMIT-preserving symmetric-EXCEPT nas CHAVES = 0). Falha antes.
- **GREEN:** decode-chave + heap.
- **REFACTOR:** direção ASC/DESC + nulls-first/last casando o PG Sort; k > N (heap com < k); filtro removendo tudo (heap vazio).

#### Acceptance criteria
- [ ] O conjunto de k chaves = o do heap (verificado por: symmetric-EXCEPT das chaves = 0, LIMIT mantido).
- [ ] ASC/DESC/nulls casam o PG (verificado por: A/B com cada direção).

#### DoD
- O conjunto das k chaves = heap via symmetric-EXCEPT com LIMIT mantido = 0 (verificado por: m158_ec_harness `fwd==0 AND rev==0`).

## Phase 3 — Fase 2 do late-mat: re-fetch + materialização do top-k

### T3.1 — re-decode dos chunk-groups tocados + `form_row` do top-k, emit em ordem

#### Why this step
Materializar a projeção completa SÓ das k linhas (o ganho). Raciocínio: ADR-2.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `theodb_rs/src/am/columnar.rs` — dado o top-k de locators, re-decodificar os chunk-groups tocados e `form_row` (projeção completa) só das k linhas; emitir em ordem de sort (o heap já dá a ordem).

#### TDD
- **RED:** `SELECT * … ORDER BY key LIMIT 10` via topk = heap (LIMIT-preserving symmetric-EXCEPT = 0, linhas COMPLETAS). Falha antes.
- **GREEN:** re-fetch + form_row + emit.
- **REFACTOR:** MVCC (a linha re-materializada = a que o eager veria sob a snapshot); k linhas no mesmo chunk-group (re-decode uma vez); k linhas em chunk-groups distintos (re-decode cada um).

#### Acceptance criteria
- [ ] `SELECT * … ORDER BY key LIMIT k` byte-idêntico ao heap (verificado por: symmetric-EXCEPT COMPLETO = 0, LIMIT mantido).
- [ ] MVCC: a linha re-materializada é a visível sob a snapshot (verificado por: EC com update/delete concorrente-ish serial).
- [ ] `cargo build`+clippy limpos.

#### DoD
- `SELECT * … ORDER BY key LIMIT k` via topk = heap (symmetric-EXCEPT COMPLETO com LIMIT = 0) e `cargo build --release`+clippy saem 0 (verificado por: m158_ec_harness + exit codes).

## Phase 4 — Flamegraph + A/B + veredito

### T4.1 — flamegraph antes/depois + LIMIT-preserving A/B + veredito medido

#### Why this step
DoD measurement-first: o ganho DEVE ser medido (não estimado); honest-negative se não materializar.

#### Concurrency tests
(none — single-threaded) `max_parallel_workers_per_gather=0`.

#### Failure scenarios
(none — no external I/O touched)

#### Files to edit
- `docs/benchmarks/m158-late-materialization.md` (NEW) + `docs/benchmarks/m158-artifacts/`; `benchmarks/m158_ec_harness.sql` (NEW, LIMIT-preserving A/B); `CHANGELOG.md`.

#### TDD
- **RED:** flamegraph pós-fix mostra `form_row`/`palloc` caindo para ~k/N; a query fica ≥2× mais rápida (medido). Antes: 1287ms.
- **GREEN:** rodar no droplet (flamegraph via `profile_columnar_scan.sh` + timing).
- **REFACTOR:** medir fora-de-cache (guard de I/O); veredito honesto (número medido ou UNBENCHMARKED; honest-negative se não ganha).

#### Acceptance criteria
- [ ] Flamegraph antes/depois: `form_row`/`palloc` cai para ~k/N (verificado por: os folded/selftime).
- [ ] Speedup MEDIDO na query alvo (verificado por: EXPLAIN ANALYZE antes/depois) OU honest-negative documentado.
- [ ] LIMIT-preserving symmetric-EXCEPT = 0 (byte-identidade) para as queries sem empate profundo (verificado por: m158_ec_harness).
- [ ] CHANGELOG cita M158. Zero droplets efêmeros ao fim.

#### DoD
- `docs/benchmarks/m158-late-materialization.md` existe com o speedup medido por EXPLAIN ANALYZE antes/depois (ou honest-negative com número) + o flamegraph (verificado por: o doc + os artefatos citados).

## Coverage Matrix

| Requisito (DoD do ROADMAP M158) | Task |
|---|---|
| Ganho MEDIDO por flamegraph (form_row cai) | T4.1 |
| A/B byte-idêntico (LIMIT-preserving) | T2.1, T3.1, T4.1 |
| MVCC preservada (linha re-materializada = eager) | T3.1 |
| Honest-negative aceito se não ganha | T4.1, ADR-3 |
| CHANGELOG | T4.1 |

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| Empate profundo na fronteira do LIMIT → o top-k diverge do PG (ambos válidos) | ALTA | A verificação mediu tie-depth baixa (fwd=rev=0); documentar o caveat; a chave (eventtime) tem alta cardinalidade | implementer |
| Re-fetch de sobreviventes espalhados → amplificação de I/O fora-de-cache | ALTA | Medir fora-de-cache; gatilho só p/ cacheado/clusterizado se perder (blueprint rec 5) | implementer |
| Overhead do CustomScan/swap come o ganho | MÉDIA | Medir; honest-negative se net-negativo (ADR-3) | owner |
| `run_m128` remove o LIMIT → não exercita o late-mat | MÉDIA | Oráculo bespoke `m158_ec_harness` que MANTÉM o LIMIT (symmetric-EXCEPT) | implementer |

## Unresolved Questions

- (none — every decision is resolved at plan time). A forma exata do re-decode por locator (re-abrir o stripe vs cachear os chunk-groups tocados) será decidida no implement; o flamegraph de T4.1 é o gate do ganho.

## Global DoD

- [ ] Testes verdes (RED→GREEN no droplet); clippy limpo.
- [ ] Flamegraph antes/depois provando `form_row` cair + speedup medido (ou honest-negative honesto).
- [ ] LIMIT-preserving A/B byte-idêntico.
- [ ] `/code-quality` ∉ {FAIL_HARD, INVALID}.
- [ ] CHANGELOG.
- [ ] Droplet da maratona destruído ao fim de M159.

## Final Phase — Integration Validation

Build no droplet + flamegraph antes/depois + LIMIT-preserving A/B (symmetric-EXCEPT=0) + MVCC + regressão zero (GUC OFF
mantém as 32 diverged=0). A cadeia só está completa quando o ganho é MEDIDO (não estimado) E o resultado é byte-idêntico
E a regressão é zero — OU o veredito é honest-negative documentado com números. Falha → volta ao implement.
