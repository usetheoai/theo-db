# Blueprint — M118 Filtered ANN: resume-from-discarded (fechar o ~3× vs pgvector 0.8)

- **Slug:** `filtered-ann-resume-discarded` · **Milestone:** M118 · **Date:** 2026-07-20
- **Objetivo:** substituir o iterative-scan por **re-search com ef dobrado** (M52) por um **resume a partir do frontier descartado** (à la pgvector 0.8.5), fechando o déficit de QPS medido no caso seletivo **sem regredir recall**.
- **Gap medido (fonte):** `docs/benchmarks/m52-filtered-ann.md:25` — *"theodb ~3× mais lento que pgvector no caso seletivo (42.8 vs 14.6 ms) porque o iterative scan re-busca o grafo inteiro com ef dobrado a cada esgotamento, enquanto o pgvector 0.8 resume do `discarded` set"*.

## Coverage Corner 4 — Techniques (o mecanismo, SOTA-anchored)

**Prior art — pgvector 0.8.5 `ResumeScanItems` (permissivo, PostgreSQL License):**
- `knowledge-base/references/pgvector/src/hnswscan.c:55` — a busca ground-level passa `&so->discarded` a `HnswSearchLayer`: candidatos explorados mas expulsos do beam `w` vão para `so->discarded` (um **pairing heap** ordenado por distância).
- `hnswscan.c:59-86` (`ResumeScanItems`) — no esgotamento, popa os melhores `batch_size = hnsw_ef_search` candidatos de `so->discarded` como **novos entry points** e continua `HnswSearchLayer(..., initial=false, &so->discarded, ...)` — **não re-percorre do topo**.
- `hnswscan.c:174,255-280` — `so->discarded` + `so->v` (visited) vivem no scan opaque state; bounded por `hnsw_max_scan_tuples` **e** `work_mem` (`MemoryContextMemAllocated > maxMemory`, L259).

**Nosso lado (o que joga fora o estado resumível):**
- `theodb_rs/src/am/hnsw_page.rs:1502` — `traverse(rel, &meta, query, ef)` é o traversal **page-native** (M35 partial-read, O(ef·M)); constrói frontier+visited+beam e **retorna só o beam, descartando frontier/visited**.
- `theodb_rs/src/am/scan.rs:238` — o iterative-scan M52 chama `hnsw_page::traverse(...)` de novo com **ef crescente** a cada esgotamento (dedup via `state.emitted: HashSet<i64>`, L60).
- (Referência do algoritmo puro: `ann/hnsw.rs::search_layer` mostra a mesma estrutura `cand`/`visited`/`result` descartada no return — é a versão in-memory, NÃO o hot path do scan.)

**Design escolhido:** tornar `hnsw_page::traverse` **resumível** — expor um estado (frontier min-heap por distância + `visited` bitset) persistido no `Scan` opaque state entre chamadas de `amgettuple`; no esgotamento, retomar a expansão do frontier retido em vez de re-buscar do entry point. Espelha `ResumeScanItems`. Bounded por `max_scan_tuples` (já existe) + um teto de memória do frontier/visited (equivalente ao `work_mem`/`maxMemory` do pgvector).

## Coverage Corner 1 — Integration tests (correção sob MVCC/rescan)
- Teste de terminação por `max_scan_tuples` (cap) — já rastreado no backlog M52; agora sobre o resume.
- Self-join/nested-loop provando `emitted` + `visited` resumíveis **sem skip/dup** entre chamadas e entre rescans (`rescan` reseta o estado — `scan.rs:118-122`).
- **Recall paridade**: o top-k emitido sob resume == top-k sob re-search (recall ≥ o atual) — ablação mesmo-índice.
- Teto de memória: frontier/visited bounded (não explode em query seletiva a 1M).

## Coverage Corner 2 — Dependencies
- Nenhuma nova crate. Reusa `std::collections::BinaryHeap` (frontier) + um `visited` bitset (hoje `Vec<bool>` em `search_layer`; no page-native usar um set esparso keyed por node-id — o grafo é on-demand, não cabe `vec![false; N]` a 1M). Rung-2/4 da parsimony ladder (stdlib).

## Coverage Corner 3 — Tools (a evidência — DoD)
- `benchmarks/run_m52_filtered_ann.py` (já existe) — estender para **multi-seed** (ex.: [42,99,7]) reportando mean±std do delta de QPS por seletividade **1% / 10% / 50%** vs pgvector 0.8.5 a **recall casado**.
- Rodar em **droplet quieto** (box da dev é poluída — `docs/benchmarks/m52` já alerta variância). `parity_gate` tolerância 0.01 (documentada).

## ADRs
1. **Resume vs re-search:** escolhido resume-from-discarded (fecha o ~3×); re-search (M52 status quo) era KISS-first com recall em paridade, agora superado por evidência de custo.
2. **Onde vive o estado:** no `Scan` opaque state (`scan.rs`), o traversal `hnsw_page::traverse` ganha uma variante resumível; o `ann/hnsw.rs` in-memory NÃO é o hot path (não tocar).
3. **Bound de memória:** frontier+visited bounded por um teto análogo ao `work_mem` do pgvector — fail-safe (parar e devolver o que tem) em vez de OOM.

## Open questions (measure-time, não discover-time)
1. O `visited` do traversal page-native a 1M seletivo: `Vec<bool>` O(N) por chamada é o custo escondido? (resume elimina as re-alocações repetidas — parte do ganho). Medir alloc.
2. O ganho real do resume no nosso layout page-native (I/O de página on-demand) pode diferir do pgvector (que tem o grafo em buffer cache) — medir, não supor (UNBENCHMARKED até o bench do droplet).

## Prior art citado (todos resolvem no disco)
- `knowledge-base/references/pgvector/src/hnswscan.c` (0.8.5 — `META.json`)
- `theodb_rs/src/am/hnsw_page.rs:1502` (traverse), `theodb_rs/src/am/scan.rs:238` (iterative M52)
- `docs/benchmarks/m52-filtered-ann.md` (o gap medido), `.claude/knowledge-base/backlog.md` (M52 follow-up)
