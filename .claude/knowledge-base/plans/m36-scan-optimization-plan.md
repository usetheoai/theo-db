---
slug: m36-scan-optimization
milestone_id: M36
created_at: 2026-07-02
goal: Replace the O(C·log C) full sort of all scan candidates with a lazy min-heap (O(C + k·log C)) in the theodb index scan, cutting the measured ~38% sort phase with byte-identical top-K results, measured by benchmarks/tests + a THEODB_SCAN_PROFILE + harness artifact showing the QPS gain at unchanged recall.
---

# M36 — Otimização do scan: sort top-K lazy heap (Phase 1) + I/O quantizada (Phase 2)

## Goal

Substituir o **sort O(C·log C) de TODOS os candidatos** no scan do índice (`am/scan.rs:109` ivf, `:188` hnsw) por
um **heap min lazy** (heapify O(C) no `amrescan` + pop O(log C) por `amgettuple` → O(C + k·log C)), cortando a fase
`sort` que a medição measurement-first do M36 mostrou ser **~35–41%** do custo de scan — com **top-K byte-idêntico**
(zero risco de recall) — medido por `#[pg_test]` + `THEODB_SCAN_PROFILE` + o harness (`benchmarks/theodb_bench/`)
mostrando o ganho de QPS a recall inalterado.

## Context

O gate measurement-first do M36 (blueprint `.claude/knowledge-base/discoveries/blueprints/m36-quantization-in-index-blueprint.md`)
FALSIFICOU a premissa original (quantizar a distância): a distância é ~15% do custo, não o gargalo. Os gargalos
medidos (`THEODB_SCAN_PROFILE`, 200k×128, estável em 3 pontos de probes) são **reads ~44–51%** e **sort ~35–41%**.
Este plano ataca o `sort` primeiro (Phase 1 — heap top-K, zero risco de recall, ADR-2 do blueprint), depois o
`reads` (Phase 2 — códigos SBQ menores + rerank f32, com gate de recall).

## Baseline Context

### Files that will be touched

| File | LoC | git sha | Why |
|---|---|---|---|
| `theodb_rs/src/am/scan.rs` | 255 | (M35) | `ScanState` vira um min-heap; `amrescan` heapifica em vez de ordenar; `amgettuple` faz pop; remove os 2 `results.sort_by` (`:109`,`:188`); profiler mede heapify |
| `benchmarks/run_m36_scan.py` | (NEW) | — | driver: profiler (sort→heapify) + harness QPS a recall inalterado vs baseline pré-M36 |
| `docs/benchmarks/m36-scan-optimization.{md,json}` | (NEW) | — | o artefato de evidência |

### Current callers / dependents

- `am/scan.rs:15` `struct ScanState { results: Vec<(i64,f64)>, pos }` — consumido por `ambeginscan:27`,
  `amrescan:43`, `amgettuple:233`. A mudança troca a representação (heap) mantendo o contrato de `amgettuple`
  (um TID por chamada, ordem crescente de distância).
- `am/scan.rs:109` (`scan_ivf_structured`) e `:188` (`scan_hnsw_structured`) — os 2 `results.sort_by` a remover;
  ambas as funções passam a retornar candidatos NÃO ordenados (o heapify acontece no `amrescan`).
- `amgettuple:236` lê `state.results[state.pos]` — vira `state.heap.pop()`.

### Domain glossary

- **C** = número de candidatos varridos (~50k a probes=50/200k). **k** = o LIMIT do executor (tipicamente 10).
- **heap min lazy** — `BinaryHeap<Reverse<Scored>>`; heapify O(C) (`BinaryHeap::from`), cada `pop` O(log C). O
  executor puxa ~k vezes → O(C + k·log C) em vez de O(C·log C).
- **top-K byte-idêntico** — o heap emite exatamente a mesma sequência crescente de `(dist, tid)` que o sort; o
  recall é intocado (é a MESMA ordenação, só o custo muda).

### Architecture boundaries affected

Nenhuma nova. Toda a mudança é interna ao `am/scan.rs` (a camada de scan do index-AM), atrás do contrato
`amgettuple` da Index Access Method API (Cap. 14 do handbook). `std::collections::BinaryHeap` (rung 2 da
parsimony-ladder — stdlib).

## Prior Art & Related Work

- Blueprint (este ciclo): `m36-quantization-in-index-blueprint.md` (a medição que reformulou o milestone).
- In-repo: o `traverse` do M35 (`am/hnsw_page.rs`) já usa `BinaryHeap` limitado — o mesmo padrão, aqui aplicado ao
  scan do ivf. `Cand` com `Ord` em `am/hnsw_page.rs` é o precedente de ordenação de f64.

## ADRs

### ADR-1 — heap min lazy no amgettuple (não bounded-k), porque o scan não conhece k
**Decisão:** heapify todos os candidatos em O(C) no `amrescan`; `amgettuple` faz `pop` sob demanda.
**Rationale:** o index-AM NÃO conhece o LIMIT `k` no `amrescan` (o executor aplica o LIMIT puxando `amgettuple` k
vezes). Um heap-de-tamanho-k exigiria saber k. O heapify-todos + pop-lazy dá O(C + k·log C) sem conhecer k, e é
byte-idêntico ao sort. **Rejeitado:** bounded top-k heap no scan (não sabemos k); manter o full sort (é o gargalo
medido).

### ADR-2 — Phase 1 (heap) primeiro, medir, depois Phase 2 (I/O quantizada)
**Decisão:** entregar o heap como slice 1 (zero risco de recall), medir o ganho, depois a quantização de I/O
(risco de recall) como slice 2 gated por recall. **Rationale:** risco crescente; o heap é correção pura de
complexidade. **Rejeitado:** fazer os dois num slice só (mistura um win de recall-zero-risco com um de recall-risco).

## Dependencies

### Existing — use as-is
| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `std::collections::BinaryHeap` | (std) | Rust | heapify O(C) + pop O(log C) — rung 2 da parsimony-ladder |

### New — to be introduced
| Package | Version | Ecosystem | Rule 9 rationale | Why |
|---|---|---|---|---|
| (none) | | | stdlib resolve | — |

### Removed
| Package | Last version | Why |
|---|---|---|
| (none) | | |

## Dependency graph

```
Phase 1 (heap min lazy — remove o full sort; top-K idêntico; benchmark do ganho de sort)
   ──▶ Phase 2 (códigos SBQ menores no scan + rerank f32 — corta reads; gate de recall)  [gated pela medição do Phase 1]
```

## Phase 1 — heap min lazy (o win de sort, zero risco de recall)

### T1.1 — ScanState vira min-heap; amrescan heapifica; amgettuple faz pop

#### Why this step
É o gargalo medido de ~38% (sort de TODOS os candidatos) atacado com uma correção de complexidade pura: o mesmo
top-K, custo O(C + k·log C) em vez de O(C·log C). Zero risco de recall porque a sequência emitida é idêntica.

#### Files to edit
- `theodb_rs/src/am/scan.rs`

#### TDD
- RED: `scan_heap_emits_same_order_as_sort` (`#[pg_test]`) — para um conjunto de candidatos `(tid, dist)` com
  distâncias e ties, o heap min emite EXATAMENTE a mesma sequência `(dist asc, tid asc)` que o `sort_by` atual.
  Given candidatos com empates de distância, when consumidos via pop, then a ordem == a ordem do sort estável.
- RED: `scan_heap_sql_knn_identical` (`#[pg_test]`, SQL) — `ORDER BY <-> q LIMIT k` num índice `theodb_ivfflat` e
  `theodb_hnsw` retorna os MESMOS k ids (mesma ordem) que antes da mudança (recall preservado; comparar contra o
  seqscan exato).
- GREEN: `struct ScanState { heap: BinaryHeap<Reverse<Scored>> }` com `Scored{d:f64,tid:i64}` + `Ord` por
  (`d.total_cmp`, depois `tid`); `scan_ivf_structured`/`scan_hnsw_structured` retornam candidatos não ordenados
  (removidos os `sort_by` `:109`/`:188`); `amrescan` faz `BinaryHeap::from(...)` (heapify); `amgettuple` faz
  `heap.pop()`. O profiler mede o heapify no lugar do sort.
- REFACTOR: fatorar `Scored` + a construção do heap num helper compartilhado pelas duas funções de scan (DRY).

#### Concurrency tests
(none — single-threaded). O scan é um snapshot share-locked (`index_shared`, `scan.rs:56`), contrato inalterado.

#### Failure scenarios
- Índice vazio / `pending` vazio → heap vazio → `amgettuple` retorna false na 1ª chamada (mesmo comportamento).
- Empates de distância (NaN impossível — distâncias são finitas) → `total_cmp` é total; desempate por `tid` mantém
  a ordem estável do sort anterior.

#### Acceptance criteria
- `cargo pgrx test` verde: `scan_heap_emits_same_order_as_sort` + `scan_heap_sql_knn_identical` (ivf + hnsw).
- `cargo pgrx install --release` 0 warnings; coexistência M20–M35 verde (mesmos ids retornados).

#### DoD
- Nenhum `results.sort_by` no caminho de scan (grep vazio); `ScanState` é o heap; `amgettuple` faz pop.

## Phase 2 — códigos SBQ menores no scan + rerank f32 (o win de reads, gated por recall)

### T2.1 — persistir códigos SBQ nas páginas de lista; pontuar por Hamming; rerank f32 do top over_fetch

#### Why this step
Ataca o gargalo medido de ~44% (`reads`): 16 bytes/candidato (SBQ 1-bit) vs 512 (f32 dim=128) → 32× menos dados
lidos por página. Reusa `sbq.rs` (M22). Rerank f32 do top over_fetch recupera o recall.

#### Files to edit
- `theodb_rs/src/am/page.rs` (persistir códigos por lista), `theodb_rs/src/am/build.rs` (quantizar no build),
  `theodb_rs/src/am/scan.rs` (pontuar por Hamming + rerank), `theodb_rs/src/sbq.rs` (reusar o quantizer)

#### TDD
- RED: `scan_sbq_recall_preserved` — o scan com códigos SBQ + rerank f32 atinge recall ≥ o baseline f32 no ponto
  casado (tolerância medida) num corpus de fixture.
- GREEN: persistir `SbqQuantizer` + códigos nas páginas de lista; scan lê códigos (menos bytes), rankeia por
  Hamming, rerank f32 do top `over_fetch`.
- REFACTOR: reusar `sbq::hamming` + `SbqQuantizer::quantize` (não reimplementar).

#### Concurrency tests
(none — single-threaded).

#### Failure scenarios
- SBQ-1bit regride recall abaixo do gate → escalar bits ou over_fetch; se estagnar, ADR documenta e Phase 2 vira
  um follow-up (honesto), Phase 1 (heap) permanece o win entregue.

#### Acceptance criteria
- `scan_sbq_recall_preserved` verde; recall ≥ baseline no ponto casado.
- 0 warnings; coexistência verde.

#### DoD
- Bytes lidos por candidato caem (medido via `THEODB_SCAN_PROFILE` reads_us); recall preservado.

## Phase 3 — benchmark (a evidência)

### T3.1 — m36-scan-optimization.{md,json}: profiler (sort→heapify) + QPS a recall inalterado

#### Why this step
A evidência measurement-first: provar o ganho de QPS a recall inalterado, honesto sobre quanto do gap do M33 fecha
(sort+reads, não distância).

#### Files to edit
- `benchmarks/run_m36_scan.py` (NEW), `docs/benchmarks/m36-scan-optimization.{md,json}` (NEW), `CHANGELOG.md`

#### TDD
(none — artefato de medição sobre o container, como M32/M34/M35)

#### Concurrency tests
(none — single-threaded).

#### Failure scenarios
- Se o heap não mostrar ganho mensurável de QPS → honesto no artefato (a fase sort caiu mas o wall-clock é
  dominado por reads); o profiler localiza. Não inflar.

#### Acceptance criteria
- `docs/benchmarks/m36-scan-optimization.json`: `THEODB_SCAN_PROFILE` mostra sort_us→heapify_us caindo; QPS ≥
  baseline a recall IDÊNTICO (Phase 1) / preservado (Phase 2); hardware + repro + veredito honesto de quanto do
  gap M33 fecha.

#### DoD
- Artefato reproduzível via `python3 benchmarks/run_m36_scan.py`; CHANGELOG linka.

## Coverage Matrix

| Goal / DoD item | Task(s) |
|---|---|
| Sort → heap top-K lazy (O(C+k·log C)); top-K idêntico (recall inalterado) | T1.1 |
| Reads → códigos SBQ menores + rerank f32 (recall preservado, gated) | T2.1 |
| Benchmark do ganho de QPS a recall inalterado; honesto sobre o gap M33 | T3.1 |
| Measurement-first (pré-requisito) | ✅ concluído no discover (blueprint) |
| Coexistência M20–M35 verde; sem nova dependência | T1.1, T2.1 |
| CHANGELOG (Rule 6) | T3.1 |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| O heap muda a ordem emitida (recall) por bug de Ord/tie | ALTO | `scan_heap_emits_same_order_as_sort` compara byte-a-byte com o sort; `total_cmp` total + desempate por tid | paulohenriquevn |
| SBQ-1bit regride recall (Phase 2) | MÉDIO | rerank f32 + gate de recall; Phase 2 gated pela medição do Phase 1; escalar via ADR se estagnar | paulohenriquevn |
| O ganho de QPS do heap é pequeno (reads ainda domina) | MÉDIO | honesto no artefato; o profiler localiza; Phase 2 ataca reads | paulohenriquevn |

## Unresolved Questions

- A ~44% de reads pode dominar o wall-clock mesmo com o sort resolvido → Phase 2 (I/O) é o que ataca isso;
  resolvido por: medir o Phase 1 antes de comprometer o Phase 2 (ADR-2).

## Failure scenarios

- **Heap emite ordem diferente do sort** → bug de recall; pego por `scan_heap_emits_same_order_as_sort`. (T1.1)
- **SBQ regride recall** → gate de recall bloqueia; escalar ou honestamente adiar Phase 2. (T2.1)
- **Ganho de QPS pequeno** → honesto no artefato, profiler localiza. (T3.1)

## Final Phase — Integration Validation

- `cargo pgrx test` verde (ordem idêntica ivf+hnsw; SQL kNN idêntico; recall SBQ preservado).
- `cargo pgrx install --release` 0 warnings; coexistência M20–M35 verde no container.
- `docs/benchmarks/m36-scan-optimization.{md,json}` committado: profiler + QPS a recall inalterado + veredito
  honesto. CHANGELOG atualizado.
