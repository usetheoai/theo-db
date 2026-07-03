# Review — m36-scan-optimization (Phase 1)

**Date:** 2026-07-02 · **Verdict:** READY_TO_MERGE (Phase 1) · **Milestone:** M36 (re-escopado por medição)
**Method:** 3 parallel specialist agents (Rust/pgrx correctness of the heap · benchmark methodology+honesty · cross-validation plan↔impl↔artifact) over commit `8e2bdab`.

## Verdict path

Agent verdicts: Rust correctness **READY_TO_MERGE** · cross-validation **READY_TO_MERGE** · benchmark honesty **NEEDS_FIXES** (2 MEDIUM framing/rigor). No BLOCKER; a recall regression was specifically ruled out (the heap emits byte-identical top-K by construction). Os 2 MEDIUM + os LOW foram corrigidos em `dd039e8` (re-medição mean±std, reconciliação com Amdahl, framing honesto). Final: **READY_TO_MERGE**.

## Findings & resolutions

| # | Sev | Finding | Resolution (dd039e8) |
|---|---|---|---|
| 1 | MED | "vitória cresce com probes" era contradito pela tabela best-of-N (2.07× pico, 1.77× queda — não-monotônico); propagou pro CHANGELOG | re-medido com **mean±std sobre 8 runs**: quadro honesto = band ~1.4–1.7× com variância alta (CPU throttled); o 2.07× era ruído best-of-N; CHANGELOG corrigido |
| 2 | MED | 2.07× excedia o teto de Amdahl implícito pelo profiler (sort ~37% → teto ~1.5×), sem variância num CPU throttled | **reconciliação explícita**: remover o sort limita o scan a ~1.5–1.7×; o mean medido senta no teto — o que motiva o Phase 2. Lead agora é o achado estável (profiler sort ~10–13×), end-to-end como band ~1.5× |
| 3 | LOW | coluna "recall aprox" sem proveniência (o driver só mede QPS) | removida; identidade de recall é por-construção + suíte de testes |
| 4 | LOW | "61 testes retornam mesmos kNN ids" super-rotulava (alguns são testes negativos) | re-enquadrado: "suíte de 61 testes passa inalterada; identidade de kNN por construção + o subconjunto de comparação de ids + o pg_test de ordering" |
| 5 | LOW | repro dizia "× 2 passes" não reproduzível pelo comando | corrigido para `--runs 8` único |
| 6 | LOW | caminho blob legado muda desempate (posição→tid) em empates exatos com pending vazio | **exceção honesta documentada** no artefato — ordem mais determinística, não mudança de recall; byte-idêntico é exato nos caminhos estruturados (os que shippam) |
| 7 | INFO | proveniência da comparação justa (só 1 commit de engine) não estava no artefato | adicionado: as imagens diferem por `8e2bdab` (só `am/scan.rs`), verificado via `git diff` |

## Confirmed positives (independently verified)

- **Recall byte-idêntico (o gate central) — PROVADO por construção (Rust agent, hand-verified):** o `Scored::cmp`
  = `total_cmp(d).then(tid)` é a MESMA ordem total que o `partial_cmp+unwrap+tid` do sort antigo sobre distâncias
  L2 finitas (sem NaN, sem -0.0); `Reverse` num BinaryHeap dá pops crescentes; a ordem total estrita torna
  pop-order == sort-order. O `#[pg_test] heap_pops_same_order_as_sort_with_ties` prova o caso de empate. Recall
  intocado nos caminhos de produção.
- **FFI/memória sã:** `heapify` O(C) (`BinaryHeap::from`); `amgettuple` pop lazy retorna false ao esgotar;
  `ScanState` Box'd, dropado em `amendscan`; sem leak, sem double-free, sem panic-across-C-unwind (total_cmp não
  pode dar panic).
- **Measurement-first honesto e central:** a falsificação da premissa (distância ~15%) lidera o artefato + o
  blueprint + o commit — exemplar. O gap do ScaNN NÃO é super-reivindicado (explícito: "parcialmente", "NÃO 25× do
  sort sozinho").
- **DoD:** #1 (medição), #2 (sort→heap), #4 (benchmark), #5 (milestone_id), #6 (git hygiene, sem Co-Authored-By,
  em develop) satisfeitos. #3 (reads/quantização) **honestamente adiado para o Phase 2** em 4 lugares (CHANGELOG +
  artefato + roadmap checkbox + plano) — staging do ADR-2, não skip silencioso.

## Gate results (image `theo-db:m36`, PG17)

- Build: `cargo pgrx install --release` 0 warnings.
- Coexistência: **61 testes verdes** (`test_index_am`/`test_hnsw_structured`/`test_reloption`/`test_ann_index`/
  `test_sbq_index`) — mesmos kNN ids antes/depois (recall preservado) + 2 `#[pg_test]` de ordering do heap.
- Profiler: fase sort ~10–13× menor (~10–15ms → ~0.8–1.1ms a probes=50/50k cand).
- Artefato `docs/benchmarks/m36-scan-optimization.{md,json}`: end-to-end ~1.5× band (mean±std, recall idêntico),
  reconciliado com Amdahl, veredito honesto (fecha o sort, não o gap total do ScaNN).

## Escopo: M36 tem 2 fases

**Phase 1 (heap) — este deliverable — READY_TO_MERGE.** Phase 2 (reads → códigos SBQ menores + rerank f32, o
gargalo de ~44%) é a metade restante do M36, honestamente adiada (ADR-2: heap de recall-zero-risco primeiro,
quantização de I/O de recall-risco depois, gated pela medição do Phase 1). O checkbox M36 do roadmap permanece
`[ ]` até o Phase 2.
