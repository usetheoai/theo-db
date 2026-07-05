# Blueprint — M46: fechar o gap de QPS do theodb_hnsw no alto recall (scan hot-path hygiene, benchmark-gated)

**Slug:** m46-hnsw-highrecall-qps
**Data:** 2026-07-04
**Milestone:** M46 (V2 — primeiro milestone após ROADMAP V1 completo)
**Tipo:** measurement-first + scan-path optimization (recall-neutra)
**phd-rigor:** P2 (pilar vetorial) — SOTA-anchored, ≥2 fontes primárias/técnica, benchmark-gated (PRD D3, `public-copy.md`)

## Contexto e evidência que dispara o milestone

`docs/benchmarks/m45-pareto-sift1m.md` (mean±std, SIFT1M 1M×128, exact GT, 3 runs) mede **PARIDADE**
theodb_hnsw vs pgvector_hnsw, com um déficit no alto recall:

| recall | QPS theodb | QPS pgvector | margem | effect>variância |
|---|---|---|---|---|
| 0.9932 | 43.5 ± 19.1 | 75.0 | 0.58× | sim |
| 0.9956 | 44.4 | 62.8 | 0.71× | sim |

O GOTO P0 do CTO (superioridade vetorial) exige fechar esse regime.

## Achado central da discovery (honestidade — Regra 3): parte do "gap" é ruído de medição

Três especialistas (council-vector-ann, council-index-storage, council-performance-simd) leram o código
real + os peers SOTA. Convergência + um achado que refuta parcialmente o framing:

1. **O scan a 1M é `theodb_rs/src/am/hnsw_page.rs::traverse` (:479-552)**, não `hnsw.rs::search`
   (esse é o path in-memory de build). O read-count do ground loop é **O(ef·M) correto** — o
   `visited.insert` guard (`hnsw_page.rs:530`) dedup antes de todo `load`; **não há read-amplification**.
2. **Não é compute-bound.** A distância já é AVX2+FMA alloc-free lendo bytes da página pinada
   (M41: `vec.rs:133-160`, `hnsw_page.rs:419-420`). SIMD já gasto; <5% do tempo/query.
3. **É memory-bound + overhead acidental per-query que escala com ef.** As 3 estruturas per-query são
   alocadas fresh a cada `traverse`, **sem pre-size**:
   - `visited: HashSet::new()` (`hnsw_page.rs:518`) — **SipHash** (lento, DoS-resistant) + capacity 0
     → ~11-12 rehashes numa busca ef=200 (cada rehash re-hasheia todas as chaves + realoca).
   - `cands`/`result: BinaryHeap::new()` (`:519-520`) — realloc grow-by-doubling.
   - `decode_neighbors` (`:200`, via `neighbors_of` `:461`) — **1 `Vec<Addr>` novo por nó expandido**.
   O working set (~532MB element + ~200MB neighbor a 1M) cruza `shared_buffers` a ef alto → page-miss.
4. **⚠️ O ponto ef=200 do M45 é dominado por RUÍDO.** `m45-pareto-sift1m.md:17-18`: theodb
   **ef=400 → 44.8 QPS é MAIS RÁPIDO que ef=200 → 43.5 QPS**. Mais trabalho **não pode** ser mais rápido:
   isso é fisicamente impossível para custo estrutural/I/O real. É outlier de medição numa dev box
   contendida (o próprio M45 cita co-tenant containers, `:58-65`); pgvector na mesma box também explode a
   ef=400 (13.9 ± 8.6). O std de 44% (vs pgvector ±1.7%) é fingerprint de cache/allocator, não de código.
   **O sinal confiável é o mid-band** (ef=100: theodb **139.9 ± 2.8 vence** pgvector 108.6 ± 1.6).

**Conclusão honesta:** o gap de alto-recall é parcialmente artefato de variância. A causa de código real é
overhead acidental (SipHash unsized + realloc + alloc-por-nó) que injeta a variância e um custo que escala
com ef — **complexidade acidental que os peers SOTA não pagam** (essencial vs acidental: `CLAUDE.md`).

## Coverage Corner 1 — Integration tests
Os 8 pg_tests do padrão SBQ/scan são o modelo. O M46 é recall-neutro: o teste-âncora é
**recall/ordem idênticos antes-e-depois** por seed fixa (a otimização NÃO pode mudar resultado), +
coexistência M20–M22 verde + bordas (ef fora de range → erro tipado, não crash).

## Coverage Corner 2 — Dependencies
Nenhuma dependência nova de engine obrigatória. Hash rápido: **stdlib não tem** open-addressing rápido;
opções permissivas já no ecossistema Rust — `rustc-hash` (FxHashSet, MIT/Apache, usado pelo compilador) ou
`ahash` (MIT/Apache). Parsimony-ladder rung 4: se nenhuma já estiver no `Cargo.toml`, avaliar 1 dep
permissiva mínima (FxHashSet é a mais enxuta; determinística; sem DoS-resistance — irrelevante para índice).
`smallvec`/`arrayvec` (MIT) para o scratch de neighbors (m0≤32) — ou um `Vec` reusado (rung 5, zero dep).

## Coverage Corner 3 — Tools
`benchmarks/run_m45_pareto.py` (o harness Pareto mean±std já existe) + `THEODB_SCAN_PROFILE=1`
(`hnsw_page.rs:547` emite `pages_read`/query) para a medição decisiva compute-vs-memory.
Docker container + pytest integration (o padrão validado).

## Coverage Corner 4 — Techniques (SOTA-anchored, ≥2 fontes/técnica)

| Lever | Técnica | Fonte 1 | Fonte 2 | Recall |
|---|---|---|---|---|
| **L1** | Pre-size + fast-hash visited + pre-size heaps | pgvector `tidhash_create(CurrentMemoryContext, ef*m*2, NULL)` + murmur `hash_tid` (`references/pgvector/src/hnswutils.c:675,54`) | pgvectorscale `HashSet::with_capacity(search_list_size*neigbors)` + `BinaryHeap::with_capacity(...)` (`references/pgvectorscale/.../graph/mod.rs:109-111`) | neutra (ordem idêntica) |
| **L2** | Eliminar alloc-por-nó (scratch reusado / smallvec) | pgvector stack `ItemPointerData indextids[HNSW_MAX_M*2]` + `unvisited` palloc-once (`hnswutils.c:799,834`) | pgvectorscale scratch reuse (`graph/mod.rs`) | neutra |
| **L3** | Per-query buffer reuse (hoist p/ ScanState + clear) | o próprio IVF path do TheoDB já faz (`scan.rs:42-44,53,76`) | pgvector `unvisited` reusado por SearchLayer (`hnswutils.c:834`) | neutra |
| **L4** (next-seed) | Prefetch p/ esconder page-miss | vectorchord `RelationPrefetch`/`RelationReadStream` (`references/vectorchord/src/index/scanners.rs:18,52`) | pgvector iterative-scan prefetch | neutra |
| **L5** (next-seed) | SBQ-in-graph: traversar em código 1-bit, rerank f32 | pgvectorscale StreamingDiskANN+SBQ | ScaNN anisotropic (`docs/benchmarks/m39-pq.md`, M33) | risco (precisa rerank gate) |

## ADR (decisões da síntese)

**ADR-1: Escopo M46 = L1+L2+L3 (recall-neutros) + re-medição rigorosa. L4/L5 são next-seeds.**
- Alternativa A (rejeitada): ir direto a L5 (SBQ-in-graph) — deep leverage mas risco de recall + esforço
  ~M22; grande demais p/ um ciclo sem re-trabalho. É o próximo bet se L1-3 não fecharem.
- Alternativa B (rejeitada): L4 prefetch primeiro — essencial mas só ajuda quando o working set excede
  cache; ataca a variância, não o throughput warm. Depende de L1-3 medidos antes.
- Escolhida: **L1+L2+L3** — pura complexidade acidental, recall-neutra, baixo risco, ataca exatamente
  o overhead-que-escala-com-ef + a variância do allocator. Justificados INDEPENDENTE do artefato de
  medição (higiene de performance real com âncora SOTA). Parsimony-ladder respeitada.

**ADR-2: Measurement-first é parte do DoD (anti-sunk-cost, princípio TheoDB).**
Como o ponto ef=200 do M45 é ruidoso (ef200<ef400), o DoD NÃO pode ser cegamente "theodb ≥ pgvector a
recall 0.993". O DoD honesto: re-medir com metodologia endurecida (≥5 runs, median, drop outliers,
`THEODB_SCAN_PROFILE` p/ separar pages_read de wall-clock) e veredito por **effect>variância**. Vitória
válida = (a) fechar/superar o gap de alto-recall E/OU (b) reduzir a variância de 44%→<10% (estabilidade =
qualidade real medida) preservando o win de mid-band. Honest negative aceito se L1-3 não moverem a agulha
→ next-seed L4/L5.

## Riscos
- **Ruído de medição na dev box (MÉDIO):** a mesma box contendida limita o sinal. Mitigação: median de ≥5
  runs, drop de outliers, reportar pages_read (determinístico) além de QPS; caveat de hardware explícito.
- **Dep nova (BAIXO):** FxHashSet/smallvec são permissivas (MIT/Apache) e mínimas; passar `/deps-audit`.
- **Regressão de recall (BAIXO→nulo):** L1-3 são recall-neutros por construção; teste-âncora prova ordem
  idêntica por seed fixa.

## Referências
- Evidência: `docs/benchmarks/m45-pareto-sift1m.md`, `docs/benchmarks/sift1m-carrier-verdict.md` (retratado)
- Código: `theodb_rs/src/am/hnsw_page.rs`, `am/scan.rs`, `am/page.rs`, `vec.rs`, `sbq.rs`
- Peers: `.claude/knowledge-base/references/{pgvector,pgvectorscale,vectorchord}/`
- Regras: `.claude/rules/discover-phd-rigor.md`, `parsimony-ladder.md`, `public-copy.md`, `testing.md`
