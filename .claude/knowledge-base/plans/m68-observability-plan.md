---
slug: m68-observability
milestone_id: M68
created_at: 2026-07-09
goal: Entregar theodb.explain_scan (índice + ef efetivo + pages_read + candidates + latência) + candidates_seen na métrica runtime + doc de operação, reusando a infra do M67.
---

# M68 — Observabilidade do query vetorial (EXPLAIN + métricas)

## Goal

Entregar `theodb.explain_scan(index, vec_col, query, ef, k)` que mostra índice + ef efetivo + pages_read + candidates_seen + latência por scan (diagnóstico own-code, não hook do EXPLAIN — inexistente no PG18), a métrica runtime `candidates_seen` no catálogo `theodb._index_scan_stats`, e o doc de operação `docs/ops/vector-scan-diagnostics.md`, validado por `cargo pgrx test pg17 explain_scan` GREEN.

## Context

A discovery (`.claude/knowledge-base/discoveries/blueprints/m68-observability-blueprint.md`, R0 web-citado) concluiu: (a) **não há hook `amexplain` no PG18** — o padrão é uma função diagnóstica separada (Qdrant/Milvus); (b) pgvector/pgvectorscale **não expõem** pages_read/candidates por-query — o `theodb.scan_stats` do M67 **já supera** o baseline; (c) a única peça essential é expor `visited.len()` (candidates) via **retorno** de `ground_search_nodes` (o `scan_core.rs` tem invariante "no `crate::`" para o criterion bench); (d) observabilidade → **sem benchmark de performance** (validado por pg_test). O M68 reusa a infra M67 (scan_stats, thread_local, catálogo).

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Papel | Ação M68 |
|---|---|---|---|
| `theodb_rs/src/ann/scan_core.rs` | ~180 | ground_search (visited HashSet) | `ground_search_nodes` retorna `(Vec, visited.len())` |
| `theodb_rs/src/am/hnsw_page.rs` | ~3200 | traverse (call sites do ground_search) | destructure a tupla + `bump_scan_candidates` |
| `theodb_rs/src/am/autotune.rs` | ~330 | thread_local + catálogo + scan_stats | +thread_local SCAN_CANDIDATES + sum_candidates no catálogo |
| `theodb_rs/src/api.rs` | ~800 | superfície | +`theodb.explain_scan` + candidates nas wrappers scan_stats |
| `docs/ops/vector-scan-diagnostics.md` | 0 (NEW) | doc de operação | criar |
| `docs/adr/0027-m68-vector-observability.md` | 0 (NEW) | ADR | criar |
| `CHANGELOG.md` | — | — | editar |

### Current callers / dependents

- `visited` HashSet: `scan_core.rs:109` (populado `:127,:144`, descartado `:164`); invariante "no `crate::`/`pg_sys`" `:8,:176`.
- Call sites de `ground_search`/`ground_search_nodes`: produção `hnsw_page.rs:1612,:1638` + `~4` testes (`:278,:296,:310,:323`).
- Infra M67 a reusar: `bump_scan_pages`/`read_scan_pages` (`autotune.rs:24-41`), catálogo `theodb._index_scan_stats` (`autotune.rs:48-60`), `scan_stats` (`autotune.rs:127`), `record_scan_stat` (`autotune.rs:64`), superfície `_scan_stats` (`api.rs`).
- ef efetivo: `ScanState.ef` (`scan.rs:50`, crescido pelo iterative M52 `:325-330`).

### Domain glossary

- **candidates_seen** — nº de nós únicos navegados no beam search (o `visited.len()` — o pool navegado, não o result set ef).
- **ef efetivo** — o ef realmente usado no walk (no approximate path = ef·over_fetch; o explain reporta o ef passado).
- **explain_scan** — função diagnóstica que mostra o que o scan fez (índice, ef, pages, candidates, latência).
- **seqscan fallback** — o planner não escolheu o índice (sinal nº1 de recall/latência ruim).

### Architecture boundaries affected

- `scan_core.rs` mantém o invariante "no `crate::`" — o count sai por retorno puro; o bump fica em `hnsw_page.rs` (produção). Sem tocar páginas de índice / crash-safety (M35). Catálogo heap (M67 ADR-0026-b).

## Prior Art & Related Work

- Blueprint M68 (R0): `.claude/knowledge-base/discoveries/blueprints/m68-observability-blueprint.md`.
- Infra M67: `theodb_rs/src/am/autotune.rs` (scan_stats, catálogo), ADR `docs/adr/0026-m67-autotune-recommender.md`.
- pgvector tuning (ops doc): https://github.com/pgvector/pgvector · https://www.paradedb.com/learn/postgresql/tuning-pgvector

## ADRs

### ADR-0027-a — `theodb.explain_scan` (função diagnóstica); NÃO hook do EXPLAIN

**Decisão:** `theodb.explain_scan(index_table regclass, vector_col text, query text, ef int DEFAULT 100, k int DEFAULT 10) RETURNS TABLE(index_name text, ef_effective int, pages_read bigint, candidates_seen bigint, latency_us bigint, results bigint)` — reusa o motor do `scan_stats` (M67) + candidates + o nome do índice. NÃO tentar um hook do EXPLAIN.

**Rationale:** o PG18 não tem callback `amexplain` no `IndexAmRoutine` (verificado) — não há ponto de extensão estável para o AM injetar linhas no EXPLAIN. Os peers (Qdrant/Milvus) usam função/endpoint diagnóstico separado. Tentar um hook seria complexidade acidental sem ponto de apoio.

**Alternativas rejeitadas:**
- **(A) Hook no EXPLAIN (amexplain)** — não existe no PG18 (há patch em pgsql-hackers não-landed). Rejeitada.
- **(B) Só logar sob GUC (THEODB_SCAN_PROFILE)** — já existe (log), mas não é consultável/estruturado; o operador precisa de uma função que retorna dados. Rejeitada (o log é complementar).

### ADR-0027-b — candidates_seen via retorno puro de ground_search (preserva o invariante do bench)

**Decisão:** `ground_search_nodes` (`scan_core.rs:101`) retorna `(Vec<(Node,f64)>, usize)` — a tupla com `visited.len()` capturado antes do drop (`:164`). Um novo thread_local `SCAN_CANDIDATES` (espelha `SCAN_PAGES_READ`) é bumpado no lado de PRODUÇÃO (`hnsw_page.rs:1645`, ao lado de `bump_scan_pages`), NÃO de dentro de `scan_core`.

**Rationale:** `scan_core.rs` tem o invariante "no `crate::`/`pg_sys` references" (`:8,:176`) porque é incluído por `#[path]` no criterion bench standalone. Chamar `crate::am::autotune::bump` de dentro quebraria o link do bench. O count sai por retorno puro; o bump acontece em produção. Recall-neutro (só lê `len()` de uma estrutura que já existe).

**Alternativas rejeitadas:**
- **(A) Bump de dentro de scan_core** — quebra o invariante de link do bench. Rejeitada.
- **(B) Não expor candidates** — o DoD pede "candidatos vistos". Rejeitada.

## Dependency Graph

```
Phase 1 (candidates: ground_search retorno + thread_local + catálogo)  ──→ Phase 2 (explain_scan + wrappers)  ──→ Phase 3 (doc de operação + ADR + integration)
```

## Phase 1 — candidates_seen (retorno de ground_search + thread_local + catálogo)

### T1.1 — `ground_search_nodes` retorna candidates + thread_local + catálogo sum_candidates

#### Why this step
**Ação:** (a) `ground_search_nodes` (`scan_core.rs:101`) retorna `(Vec, visited.len())`; `ground_search` (`:92`) propaga; os ~6 call sites (produção `hnsw_page.rs:1612,:1638` + testes) destructuram a tupla. (b) novo thread_local `SCAN_CANDIDATES` + `bump_scan_candidates`/`read_scan_candidates`/`reset_scan_candidates` em `autotune.rs` (espelha `:24-41`); bump em `hnsw_page.rs:1645`. (c) catálogo `theodb._index_scan_stats` ganha `sum_candidates`; `record_scan_stat` + `scan_stats` propagam.
**Raciocínio:** candidates_seen é observabilidade genuína que o M67 deixou de fora. A mudança é mecânica (~7 call sites) e recall-neutra (só lê `len()`). O invariante do bench é preservado (retorno puro).

#### Files to edit
- `theodb_rs/src/ann/scan_core.rs`, `theodb_rs/src/am/hnsw_page.rs`, `theodb_rs/src/am/autotune.rs`

#### Deep file dependency analysis
`ground_search_nodes` é chamado no approximate path (`hnsw_page.rs:1612`, walk_ef=ef·over_fetch) e o exact (`:1638`). O `visited.len()` no approximate reflete o pool alargado (a verdade honesta). Catálogo: `ALTER` não necessário no fresh install (theodb_rs regenera); nota de upgrade honesta como M66.

#### TDD
- RED: `scan_records_candidates_seen` (após um scan, `read_scan_candidates() > 0` + o catálogo tem sum_candidates). Falha antes.
- GREEN: o retorno + thread_local + catálogo.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded) — o thread_local é backend-local; sem estado compartilhado entre backends.

#### Acceptance criteria
- `cargo pgrx test pg17` (os testes de scan/autotune) GREEN após a mudança de assinatura (nenhum call site quebrado).
- `read_scan_candidates()` > 0 após um scan; catálogo persiste sum_candidates.

#### DoD
- [ ] pg_test GREEN; candidates_seen exposto + persistido; invariante do bench preservado.

## Phase 2 — `theodb.explain_scan` + candidates nas wrappers

### T2.1 — `theodb.explain_scan` + scan_stats/index_scan_stats ganham candidates

#### Why this step
**Ação:** (a) `#[pg_extern] _explain_scan` → `TableIterator(index_name, ef_effective, pages_read, candidates_seen, latency_us, results)` reusando `scan_stats` + candidates + o nome do índice (resolvido do regclass); wrapper `theodb.explain_scan(...)`. (b) `_scan_stats` + `theodb.scan_stats`/`index_scan_stats` (`api.rs`) ganham a coluna candidates. REVOKE FROM PUBLIC.
**Raciocínio:** a função diagnóstica é o padrão dos peers (Qdrant/Milvus) — o operador vê o que o scan fez. Reusa o motor M67 (rung-4: não reinventar o medidor).

#### Files to edit
- `theodb_rs/src/am/autotune.rs` (scan_stats retorna candidates), `theodb_rs/src/api.rs` (explain_scan + wrappers)

#### Deep file dependency analysis
`scan_stats` (`autotune.rs:127`) retorna `(pages, latency, results)` → passa a `(pages, candidates, latency, results)`. `_scan_stats` (`api.rs`) + wrappers ganham a coluna. `explain_scan` é uma projeção com o nome do índice + ef.

#### TDD
- RED: `explain_scan_shows_index_and_candidates` (retorna index_name não-vazio + pages_read>0 + candidates_seen>0); `scan_stats_returns_candidates`. Falham antes.
- GREEN: a superfície.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded).

#### Failure scenarios
- **Índice inexistente / query mal-formada** → erro tipado (regclass valida; a query SQL falha com erro claro no SPI, não crash).

#### Acceptance criteria
- `cargo pgrx test pg17 explain_scan` GREEN; `theodb.explain_scan` retorna as 6 colunas; REVOKE FROM PUBLIC.

#### DoD
- [ ] explain_scan GREEN (índice + ef + pages + candidates + latência); scan_stats/index_scan_stats com candidates.

## Phase 3 — Doc de operação + ADR + Integration Validation

### T3.1 — `docs/ops/vector-scan-diagnostics.md` + ADR-0027 + suite + CHANGELOG

#### Why this step
**Ação:** doc de operação (o playbook recall-baixo/latência-alta com `explain_scan`), ADR-0027, `cargo pgrx test` completo, CHANGELOG.
**Raciocínio:** o DoD pede o doc de operação; o ADR registra a decisão (função diagnóstica, não hook).

#### Files to edit
- `docs/ops/vector-scan-diagnostics.md` (NEW), `docs/adr/0027-m68-vector-observability.md` (NEW), `CHANGELOG.md`

#### TDD
- RED: n/a (doc + validação).
- GREEN: suíte verde; doc + ADR.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- `cargo pgrx test pg17 explain_scan` + scan/autotune GREEN.
- `docs/ops/vector-scan-diagnostics.md` cobre: índice foi escolhido? recall baixo → ef sweep/iterative/rebuild; latência alta → ef/memória/max_scan_tuples/cold-start; tabela sinal→causa→ação.
- ADR-0027 completo; CHANGELOG cita o M68.

#### DoD
- [ ] Suíte verde; doc de operação + ADR + CHANGELOG.

## Coverage Matrix

| DoD (ROADMAP M68) | Task(s) | Evidência |
|---|---|---|
| `EXPLAIN` do scan mostra: índice, ef/probes efetivo, pages read, candidatos vistos | T1.1, T2.1 | `theodb.explain_scan` (função diagnóstica — não há hook amexplain no PG18) retorna index_name + ef + pages_read + candidates_seen |
| Métricas runtime (counter/histogram) do scan vetorial expostas (pilar c) | T1.1, T2.1 | catálogo `theodb._index_scan_stats` (n_scans + sum_pages_read + sum_candidates + sum_latency_us) consultável via `index_scan_stats` |
| Doc de operação: diagnosticar recall baixo / latência alta | T3.1 | `docs/ops/vector-scan-diagnostics.md` (playbook + tabela sinal→causa→ação) |

Cobertura: 100% dos 3 bullets. Hook do EXPLAIN N/A (não existe no PG18 — função diagnóstica é o padrão dos peers).

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| **Não há hook amexplain** — "EXPLAIN do scan" é uma função separada, não linhas no EXPLAIN do plano | BAIXA | É o padrão dos peers (Qdrant/Milvus); documentado (ADR-0027-a). O operador usa `theodb.explain_scan`. | Eng |
| **candidates no approximate path é walk_ef alargado** | BAIXA | É a verdade honesta (o que o scan navegou); documentado no doc de operação. | Eng |
| **Mudança de assinatura de ground_search** quebra call sites | MÉDIA | ~7 call sites mecânicos (destructure); o pg_test cobre; o invariante do bench é preservado (retorno puro, sem `crate::` em scan_core). | Eng |

## Unresolved Questions

- Hook do EXPLAIN ou função? **Resolvido:** função diagnóstica (não há hook no PG18; ADR-0027-a).
- Métrica Prometheus ou catálogo? **Resolvido:** catálogo consultável (v1 honesto; Prometheus histogram exigiria registry no processo — YAGNI).
- Capturar o ef crescido pelo iterative M52? **Resolvido:** NÃO no v1 (o explain reporta o ef passado; o iterative vive no amgettuple — 2ª complexidade, YAGNI). Documentado como caveat.

## Failure scenarios

- **Query mal-formada / índice inexistente no explain_scan** — regclass valida o índice; a query SQL falha com erro tipado no SPI (não crash). Sem I/O externo (o scan é I/O-local).

## Global Definition of Done

- [ ] candidates_seen exposto (retorno de ground_search) + persistido no catálogo; pg_test GREEN.
- [ ] `theodb.explain_scan` retorna índice + ef efetivo + pages_read + candidates_seen + latência; pg_test GREEN.
- [ ] `theodb.scan_stats`/`index_scan_stats` ganham candidates.
- [ ] `docs/ops/vector-scan-diagnostics.md` (playbook recall-baixo/latência-alta + tabela sinal→causa→ação).
- [ ] ADR-0027 completo; CHANGELOG atualizado (Regra 6).
- [ ] Suíte completa GREEN; invariante do bench (no `crate::` em scan_core) preservado; cada arquivo ≤ 500 LoC delta.
