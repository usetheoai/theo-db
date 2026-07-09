---
slug: m67-autotune
milestone_id: M67
created_at: 2026-07-09
goal: Entregar um recomendador determinístico de ef_search por alvo de recall (own-code) + coletor de stats de scan + amcostestimate refinado, com convergência medida em benchmark.
---

# M67 — Índices vetoriais auto-tunados (ef_search/probes por workload)

## Goal

Entregar `theodb.recommend_ef(index, recall_target)` (recomendador determinístico own-code: bisecção monotônica no ef contra um GT amostrado, retorna o menor ef que atinge o alvo) + um coletor de stats de scan por índice + amcostestimate refinado, com a **convergência medida** (|recall(ef_recomendado) − alvo| ≤ tolerância) em `docs/benchmarks/m67-autotune.{md,json}`.

## Context

A discovery (`.claude/knowledge-base/discoveries/blueprints/m67-autotune-blueprint.md`, R0 web-citado) concluiu: (a) **quase nenhum sistema de produção auto-tuna ef online** — o SOTA é early-termination query-adaptativo acadêmico (DARTH/Ada-ef); (b) **rung-1 honesto = coletor + recomendador determinístico** (sugere ef, operador aplica com `SET`), NÃO auto-tune online (oscilação, colide com o SET do usuário); (c) **grande parte da instrumentação já existe** — `reads` counter (`hnsw_page.rs:1515`), `visited` HashSet (`scan_core.rs:109`), amcostestimate honesto f(ef) (`cost.rs:33`), sinal de convergência M52 (`scan.rs:335`); (d) recall(ef) é **monotônico** (Malkov & Yashunin) → bisecção é sã; (e) recall sem GT em prod → **exact-scan amostrado** é a base-de-verdade.

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Papel | Ação M67 |
|---|---|---|---|
| `theodb_rs/src/am/autotune.rs` | 0 (NEW) | recomendador determinístico + coletor | criar (~180 LoC) |
| `theodb_rs/src/lib.rs` | ~200 | module root | +`mod autotune;` (via am) |
| `theodb_rs/src/am/mod.rs` | ~160 | AM routine + amcostestimate | refino do cost (usar pages_read observado) |
| `theodb_rs/src/am/cost.rs` | ~80 | visit_ratio | +ler stat empírica quando presente |
| `theodb_rs/src/api.rs` | ~780 | superfície | +`theodb.recommend_ef` + `theodb.index_scan_stats` |
| `benchmarks/run_m67_autotune.py` | 0 (NEW) | benchmark convergência | criar |
| `benchmarks/tests/test_run_m67_autotune.py` | 0 (NEW) | aritmética (convergência) | criar |
| `docs/benchmarks/m67-autotune.{md,json}` | 0 (NEW) | relatório + dados | criar (droplet) |
| `docs/adr/0026-m67-autotune-recommender.md` | 0 (NEW) | ADR | criar |
| `CHANGELOG.md` | — | — | editar |

### Current callers / dependents

- Scan: `am/scan.rs` (`amgettuple:293`, iterative scan `scan.rs:315-339` — o 2× re-search do M52 é o sinal de convergência).
- Recall counters já existentes: `reads` (`hnsw_page.rs:1515`, logado sob `THEODB_SCAN_PROFILE`), `visited` (`scan_core.rs:109`).
- Cost: `am/mod.rs:123` (`amcostestimate`), `cost.rs:65` (`scan_visit_ratio`), `cost.rs:33` (`hnsw_visit_ratio`).
- GUCs: `am/guc.rs:25` (`EF_SEARCH`), `guc.rs:17` (`PROBES`), Userset per-query.
- Precedente de catálogo de stats: `theodb.vectorizer_worker_stats` (`vectorizer.rs:150`).

### Domain glossary

- **recall_target** — o alvo de recall (ex. 0.95) que o recomendador tenta atingir com o menor ef.
- **recommend_ef** — dado um índice + amostra de queries + alvo → o menor ef_search que atinge o alvo (bisecção monotônica).
- **exact-scan amostrado** — GT: top-k exato (ef altíssimo / seqscan) numa amostra de queries — a base-de-verdade de recall.
- **convergência** — |recall(ef_recomendado) − alvo| dentro da tolerância; iterações-até-convergir.
- **catálogo de scan-stats** — `theodb._index_scan_stats` (heap, key indexrelid, FORA das páginas do índice — crash-safety).

### Architecture boundaries affected

- `autotune.rs` é domínio do AM (usa o scan existente via SPI/SET, não reescreve o beam). As stats vivem num catálogo heap regular (NÃO nas páginas do índice — o scan continua read-only; contrato IndexAmRoutine intacto).

## Prior Art & Related Work

- Blueprint M67 (R0): `.claude/knowledge-base/discoveries/blueprints/m67-autotune-blueprint.md`.
- DARTH (arXiv:2505.19001), Ada-ef (arXiv:2512.06636), VDTuner (arXiv:2404.10413), HNSW monotonicidade (arXiv:1603.09320).
- amcostestimate honesto do projeto: `am/cost.rs` (entregue no M48), os ADRs do M48 em `docs/adr/`.

## ADRs

### ADR-0026-a — Recomendador determinístico (bisecção monotônica); NÃO auto-tune online

**Decisão:** `theodb.recommend_ef(index regclass, sample_query text[], recall_target float DEFAULT 0.95) RETURNS int` — para cada query da amostra computa o GT exato (ef altíssimo), depois faz doubling `[k,2k,4k,…]` + bisecção do bracket para achar o **menor ef** cujo recall médio na amostra ≥ alvo. Read-only, determinístico. O operador aplica com `SET theodb_hnsw.ef_search`. NÃO muta o GUC automaticamente.

**Rationale:** recall(ef) é monotônico (Malkov & Yashunin — a lista de ef+1 é superset) → a bisecção é sã (sem máximos locais). Auto-tune online que muta o ef vivo oscila, colide com o SET do usuário, e é difícil de tornar crash-safe/observável (nenhum vector-DB de produção faz). O DoD permite "auto-tune **ou** recomendação".

**Alternativas rejeitadas:**
- **(A) Auto-tune online (mutar ef_search por feedback)** — oscilação, afeta queries em voo. Rejeitada.
- **(B) Early-termination query-adaptativo (Ada-ef/DARTH)** — SOTA, mas probabilístico + (DARTH) modelo+treino offline. Deferido para v2 (bet medido antes de shipar).

### ADR-0026-b — Coletor de stats num catálogo heap (fora das páginas do índice) + amcostestimate refinado

**Decisão:** um catálogo `theodb._index_scan_stats (indexrelid oid PK, n_scans, sum_pages_read, sum_candidates, sum_latency_us, last_ef, last_updated)` (molde `vectorizer_worker_stats`); o coletor grava (amostrado) as stats que o scan já computa (`reads`/`visited`/latência). O amcostestimate refina: quando há stat empírica para o índice, usa o `pages_read` médio observado; senão fallback à fórmula f(ef) atual (M48).

**Rationale:** escrever stat nas páginas do índice via GenericXLog a cada scan violaria partial-read + imutabilidade M35 (write-amp no read path). O catálogo heap é crash-safe e mantém o scan read-only. O cost calibrado pela realidade fecha o gap M48/cost.

**Alternativas rejeitadas:**
- **(A) Stats nas páginas do índice** — viola crash-safety/partial-read. Rejeitada.
- **(B) Write SPI por-scan** — caro/transaccionalmente delicado no read path → amostragem. Rejeitada (amostragem é KISS).

## Dependency Graph

```
Phase 1 (recommend_ef + GT amostrado + pg_test)  ──→ Phase 2 (coletor de stats + catálogo + amcostestimate refino)  ──┐
                                                                                                                        ├─→ Phase 3 (benchmark convergência)
                                                                                             Phase 3 ──→ Phase 4 (ADR + integration)
```

## Phase 1 — `theodb.recommend_ef` (recomendador determinístico) + pg_test

### T1.1 — `autotune.rs::recommend_ef` (bisecção monotônica sobre GT amostrado)

#### Why this step
**Ação:** `theodb.recommend_ef(index regclass, sample_query text[], recall_target float DEFAULT 0.95, k int DEFAULT 10) RETURNS int`: para cada query da amostra, GT = top-k com ef=MAX (a base-de-verdade); depois testa ef ∈ doubling `[k,2k,…,ef_max]`, para no primeiro ef cujo recall médio ≥ alvo, bisecta o bracket `[ef_prev, ef]` para o menor ef que ainda atinge o alvo. Retorna esse ef.
**Raciocínio:** resolve a dor real (knob manual) de forma determinística e estável; recall(ef) monotônico → bisecção sã. O GT amostrado é a base honesta (sem GT-free não-confiável).

#### Files to edit
- `theodb_rs/src/am/autotune.rs` (NEW, ~120 LoC)
- `theodb_rs/src/api.rs` (+`_recommend_ef` + wrapper `theodb.recommend_ef`)
- `theodb_rs/src/am/mod.rs` (+`mod autotune;`)

#### Deep file dependency analysis
Usa o scan existente via SPI: `SET theodb_hnsw.ef_search = N; SELECT ... ORDER BY emb <=> q LIMIT k`. O GT é `ef_search = MAX_EF_SEARCH` (`guc.rs:23`). Recall = |ANN ∩ GT| / k por query, média na amostra. `pg.rs::err_input` para validação.

#### TDD
- RED: `recommend_ef_monotone_returns_min_ef` (num índice pequeno, alvo 0.9 → ef que atinge; alvo 1.0 → ef maior); `recommend_ef_rejects_bad_target` (target ∉ (0,1] → 22023); `recommend_ef_empty_sample_rejected`. Falham antes.
- GREEN: a bisecção.
- REFACTOR: extrair `recall_on_sample(ef)` puro.

#### Concurrency tests
(none — single-threaded) — o recomendador roda queries sequenciais numa sessão; sem estado compartilhado.

#### Failure scenarios
- **Índice vazio / amostra sem vizinhos** — GT vazio → recall indefinido → retorna o ef default com WARN (não crash).

#### Acceptance criteria
- `cargo pgrx test pg17 recommend_ef` → GREEN.
- `theodb.recommend_ef` retorna o **menor** ef que atinge o alvo (monotonicidade); target inválido → 22023.

#### DoD
- [ ] pg_test GREEN; bisecção monotônica; validação de target.

## Phase 2 — Coletor de stats + catálogo + amcostestimate refinado

### T2.1 — Catálogo `theodb._index_scan_stats` + `theodb.index_scan_stats()` + amcostestimate refino

#### Why this step
**Ação:** (a) catálogo `theodb._index_scan_stats` (molde `vectorizer_worker_stats`); (b) `theodb.record_scan_stat(index, pages_read, candidates, latency_us, ef)` (bump SPI) + `theodb.index_scan_stats(index) RETURNS TABLE(...)` (leitura); (c) o recomendador (Phase 1) grava a stat do melhor ef; (d) `cost.rs::scan_visit_ratio` refina: se há stat empírica (avg pages_read) para o índice, usa-a como base; senão fallback à fórmula f(ef) atual.
**Raciocínio:** o coletor + o cost calibrado fecham o gap M48/cost (custo estimado ≈ real). Catálogo heap = crash-safe (ADR-0026-b).

#### Files to edit
- `theodb_rs/src/am/autotune.rs` (record/read stats)
- `theodb_rs/src/api.rs` (+wrappers)
- `theodb_rs/src/am/cost.rs` (refino do ratio com stat empírica)

#### Deep file dependency analysis
`cost.rs:65` (`scan_visit_ratio`) ganha o branch "se há stat, usa avg pages_read / total tuples". Molde de bump: `vectorizer.rs:498`.

#### TDD
- RED: `record_and_read_scan_stat` (grava → lê a média); `cost_uses_empirical_pages_when_present` (com stat → ratio calibrado; sem stat → fórmula). Falham antes.
- GREEN: o catálogo + o refino.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded) — o bump é amostrado; concorrência de scans é coberta pelo catálogo heap (UPSERT atômico).

#### Acceptance criteria
- `cargo pgrx test pg17 scan_stat` → GREEN.
- amcostestimate usa a stat empírica quando presente; fallback à fórmula quando ausente (nunca error).

#### DoD
- [ ] pg_test GREEN; catálogo + cost refino; fallback honesto.

## Phase 3 — Benchmark de convergência (o gate)

### T3.1 — `run_m67_autotune.py` + rodar no droplet

#### Why this step
**Ação:** `benchmarks/run_m67_autotune.py` — num corpus (SIFT subset ou BEIR): para alvos R* ∈ {0.90, 0.95, 0.99}, chamar `theodb.recommend_ef`, medir o recall REAL do ef recomendado (contra GT exato), reportar **|recall_medido − alvo| (MAE)**, **RQUT** (% queries abaixo do alvo — tail safety), **iterações-até-convergir**, e o ef recomendado vs o joelho de diminishing-returns. Comparar vs baseline (ef fixo default).
**Raciocínio:** o DoD pede "medida de convergência". MAE + RQUT (não só a média — a cauda importa) + iterações provam o auto-tune. Se não converge, honest-negative.

#### Files to edit
- `benchmarks/run_m67_autotune.py` (NEW, ~200 LoC)
- `benchmarks/tests/test_run_m67_autotune.py` (NEW, ~100 LoC — aritmética MAE/RQUT/convergência)
- `docs/benchmarks/m67-autotune.{md,json}` (NEW, droplet)

#### Deep file dependency analysis
Reusa o harness recall (SIFT/BEIR) + `theodb.recommend_ef` (Phase 1). Aritmética (mae, rqut, converged) pura e unit-testável.

#### TDD
- RED: `test_mae_target_error`, `test_rqut_tail_fraction`, `test_converged_within_band`, `test_no_convergence_honest_negative`. Falham antes.
- GREEN: a aritmética.
- REFACTOR: reusar helpers M65/M66.

#### Concurrency tests
(none — single-threaded).

#### Failure scenarios
- **Índice não indexado / GT falha no setup** → SystemExit tipado (não mede lixo).

#### Acceptance criteria
- `docs/benchmarks/m67-autotune.json` com `per_target` (R*, ef_recomendado, recall_medido, MAE, RQUT, iterações).
- Convergência: |recall_medido − alvo| ≤ tolerância para os alvos; ou honest-negative com números.
- council-benchmark: HONESTO.

#### DoD
- [ ] `.json`+`.md` coletados; convergência medida (MAE/RQUT/iterações); council-benchmark HONESTO.

## Phase 4 — ADR + Integration Validation

### T4.1 — ADR-0026 + suite + CHANGELOG

#### Why this step
**Ação:** ADR-0026 (recomendador determinístico, coletor heap, defer auto-tune online). Rodar `cargo pgrx test pg17 recommend_ef scan_stat` + `pytest`. CHANGELOG.
**Raciocínio:** eat-your-own-cooking — M67 completo quando recomendador + coletor + cost + benchmark passam juntos.

#### Files to edit
- `docs/adr/0026-m67-autotune-recommender.md` (NEW)
- `CHANGELOG.md`

#### TDD
- RED: n/a.
- GREEN: suíte verde; ADR + CHANGELOG.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- pg_test + pytest GREEN; ADR-0026 completo; CHANGELOG cita o M67 com o veredito de convergência.

#### DoD
- [ ] Suíte verde; ADR + CHANGELOG; droplet destruído.

## Coverage Matrix

| DoD (ROADMAP M67) | Task(s) | Evidência |
|---|---|---|
| Coletor de estatística de scan (recall est, pages read, latência) por índice — own-code | T2.1 | catálogo `theodb._index_scan_stats` + record/read; reusa `reads`/`visited`/latência existentes |
| Auto-tune (ou recomendação) do ef_search para um alvo de recall; medida de convergência | T1.1, T3.1 | `theodb.recommend_ef` (bisecção monotônica) + benchmark MAE/RQUT/iterações |
| amcostestimate refinado com a estatística real (fecha o gap M48/cost) | T2.1 | `cost.rs` usa avg pages_read observado quando presente (fallback à fórmula) |

Cobertura: 100% dos 3 bullets do DoD. Auto-tune online DEFERIDO por evidência (ADR-0026-a), não gap.

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| **Recomendação (não auto-tune online)** pode parecer "menos" | MÉDIA | É o rung-1 honesto (nenhum vector-DB de produção auto-tuna online; oscilação). O DoD permite "recomendação". ADR-0026-a. | Eng |
| **O recomendador pode não convergir** para alguns alvos (ex. 0.999 exige ef enorme) | MÉDIA | Reportar o ef + o joelho de diminishing-returns; honest-negative se o alvo é inatingível dentro de ef_max. | Eng |
| **Stat write no read path** custa | MÉDIA | Amostragem (o recomendador grava; scans normais não bumpam a cada query). ADR-0026-b. | Eng |

## Unresolved Questions

- Auto-tune online ou recomendação? **Resolvido:** recomendação (rung-1 seguro; ADR-0026-a; DoD permite).
- Recall-est sem GT? **Resolvido:** GT exato amostrado (a base honesta); convergência-do-beam é proxy futuro.
- recall_target GUC ou argumento? **Resolvido:** argumento da função `recommend_ef` (não GUC — só faz sentido acoplado ao recomendador).

## Failure scenarios

- **Índice vazio / GT vazio** — o recomendador retorna o ef default com WARN (não crash); o benchmark aborta com erro tipado no setup se o índice não existe. O recomendador não faz I/O externo (só SPI local).

## Global Definition of Done

- [ ] `theodb.recommend_ef` pg_test GREEN (bisecção monotônica + validação).
- [ ] Coletor `theodb._index_scan_stats` + `index_scan_stats()` pg_test GREEN.
- [ ] amcostestimate refinado (stat empírica quando presente; fallback à fórmula).
- [ ] Benchmark convergência (MAE/RQUT/iterações por alvo) coletado; `.json`+`.md` sem PENDING.
- [ ] Veredito: convergência medida (ou honest-negative onde o alvo é inatingível); council-benchmark HONESTO.
- [ ] ADR-0026 completo (defer auto-tune online por evidência).
- [ ] CHANGELOG atualizado (Regra 6); cada arquivo ≤ 500 LoC delta; droplet destruído.
