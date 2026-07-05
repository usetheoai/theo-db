---
slug: fu1-samegraph-scan-microbench
milestone_id: M47
created_at: 2026-07-05
goal: Medir de forma limpa (same-graph, box-noise-immune) o custo de alocação que a mudança M46 remove, extraindo o ground-loop do HNSW scan para a camada pura ann/scan_core.rs e benchando presized vs ::new() sobre um grafo seeded fixo via criterion.
---

# Plano — FU-1: micro-benchmark same-graph do ground-loop do HNSW scan (isolar o efeito de alocação do M46)

## Goal

Extrair o ground-loop do HNSW scan para a camada pura `ann/scan_core.rs` (trait `NeighborSource` + `ground_search<S>(…, presize: bool)`) e medir o custo de alocação que o M46 remove via um criterion micro-bench `presized` vs `::new()` sobre um grafo **seeded fixo** — meta observável única: **`cargo bench --features bench_internals` produz duas medições criterion (presized/unsized) sobre o MESMO grafo com CIs reportados, e o delta é persistido em `docs/benchmarks/fu1-samegraph-scan-microbench.{md,json}` com o caveat EC-2 (upper bound), recall-neutralidade preservada (M46 oracle verde).**

## Context

Disparado por FU-1 (`knowledge-base/implementations/m46-hnsw-highrecall-qps-followups.md`): o ganho de QPS do M46
(pre-size + scratch reuse em `traverse`) não pôde ser medido — o A/B de dois containers foi confundido por
contenção do box (controle pgvector +122%) E pelo build paralelo M44 que produz grafos diferentes. Blueprint da
discovery: `.claude/knowledge-base/discoveries/blueprints/fu1-samegraph-scan-microbench-blueprint.md` (SHIPPABLE
97.6). Decisões resolvidas na discovery:
- **D1 (blueprint):** extrair o ground-loop para a camada PURA `ann/` (sem pg_sys) — padrão vectorchord (crate
  pura benchável) aplicado via a fronteira de layer; satisfaz `architecture.md § 1` (domínio não depende de infra)
  e evita o problema de link pg_sys que forçou pgvectorscale a benchar uma cópia divergente.
- **D2 (blueprint):** fixture seeded determinístico (`HnswIndex::build(seed=42)`, N≥50k), um grafo compartilhado
  pelas duas bench fns — same-graph é o motivo de FU-1 existir.
- **D3 (blueprint):** guard de equivalência obrigatório (o path benchado == oráculo `brute()` exato), mais o
  oracle recall-neutro do M46 no path de produção.
- **EC-2:** o micro-bench sem I/O de página magnifica a fração de alocação → o delta criterion é UPPER BOUND do
  ganho de produção (o custo de alocação removido), não o número de produção. Honestidade (`public-copy.md`).

## Prior Art & Related Work

- Blueprint FU-1 (acima) — Coverage Corners 1-4 + ADRs D1-D3.
- **pgvectorscale** `references/pgvectorscale/pgvectorscale/benches/lsr.rs` — criterion (`0.5.1`) benchando duas
  estratégias de candidate-storage (`benchmark_lsr` vs `benchmark_lsr_min_heap`, `:156/:172`) via re-implementação
  (cópia divergente, `thread_rng` não-seeded); `Cargo.toml` `[[bench]] harness=false`. **A técnica base; FU-1
  melhora com código real + grafo seeded + guard.**
- **vectorchord** `references/vectorchord/crates/simd/Cargo.toml` — `[[bench]] harness=false required-features=["internal"]`
  — **o padrão de feature-gate que expõe internals ao bench** (benches são crates externas, só veem API `pub`).
- **M46** — `am/hnsw_page.rs::traverse` (o alvo) + `traverse_presize_is_recall_neutral_end_to_end` (o oráculo
  recall-neutro reusado como guard de produção).

## Baseline Context

### Files that will be touched

| File | LoC today | Last commit (sha) | Why / Invariants |
|---|---|---|---|
| `theodb_rs/src/ann/scan_core.rs` | (NEW) | — | camada pura: `NeighborSource` trait + `ground_search<S>(…, presize)`. Sem `pg_sys` (invariante de link — Q5). |
| `theodb_rs/src/ann/hnsw.rs` | 481 | `31baf39` 2026-07-03 | expõe `node_neighbors:282`, `node_vector:278`, `node_level:275`, `entry:269`, `params:260`, `node_id:272` — a base do `MemNeighborSource`. Só leitura; `mod scan_core` declarado em `ann/mod.rs`. |
| `theodb_rs/src/am/hnsw_page.rs` | 781 | `2a1d609` 2026-07-04 | `traverse:516` ground loop `:571-592` (`Cand:410` puro: f64/u32/u16/u8/i64). Refatorar p/ chamar `ground_search` via `PageNeighborSource`. Invariante: recall-neutro (M46 oracle `:741`). |
| `theodb_rs/src/lib.rs` | — | — | declarar `#[cfg(feature="bench_internals")] pub mod bench_support` (re-export de `ann::scan_core` + `HnswIndex` + seeded corpus). |
| `theodb_rs/Cargo.toml` | 42 | `386d073` 2026-06-29 | `[dev-dependencies] criterion="0.5.1"`; `[features] bench_internals=[]`; `[[bench]] name="scan_hot_path" harness=false required-features=["bench_internals"]`. |
| `theodb_rs/benches/scan_hot_path.rs` | (NEW) | — | criterion: fixture seeded, presized vs unsized, ef sweep. |
| `docs/benchmarks/fu1-samegraph-scan-microbench.{md,json}` | (NEW) | — | o artefato de dado (delta + CIs + caveat EC-2). |
| `CHANGELOG.md` | — | — | entry `[Unreleased] § Added`. |
| `ROADMAP.md` | — | — | milestone M47. |

### Current callers / dependents

`traverse` (`hnsw_page.rs:516`) é chamado só por `scan_hnsw_structured` (`scan.rs:131`) — único caller de
produção. `neighbors_into`/`load`/`decode_neighbors_into` são internos ao `hnsw_page.rs`. A extração cria
`ground_search` em `ann/scan_core.rs`, chamado por (a) `traverse` via `PageNeighborSource` (produção), (b) o bench
+ testes via `MemNeighborSource`. `ann/hnsw.rs` accessors (`node_*`) já existem, sem novos callers cross-repo.

### Domain glossary
- **ground_search**: o loop do ground layer (min-heap cands + max-heap result + visited HashSet + scratch) que o
  M46 otimizou; a unidade a ser extraída e benchada.
- **NeighborSource**: a fronteira DIP — o domínio (`ground_search`) depende dela; produção (page reads) e bench
  (in-memory) a implementam. Expõe `neighbors_into(node,&mut Vec<u64>)` + `distance(node)->f64`.
- **NodeId (u64)**: handle opaco de nó. Produção empacota `(blk<<16)|off`; bench/in-memory usa o índice do nó.
- **presize**: o eixo único do bench — `true` = pre-size das 3 estruturas + scratch (M46); `false` = `::new()`.
- **same-graph**: o bench constrói UM grafo seeded e o compartilha entre as duas medições → o delta é só alocação.
- **EC-2 caveat**: micro-bench sem I/O = upper bound do ganho de produção (I/O amortiza a alocação).

### Architecture boundaries affected
Cria a fronteira DIP `ann::scan_core::NeighborSource` (domínio) implementada por `am/hnsw_page.rs`
(infra/adapter) — exatamente `architecture.md § 1` (domínio não importa infra; adapter implementa a interface do
domínio). `ann/scan_core.rs` NÃO importa `pg_sys` (invariante de link do bench). `criterion` é dev-only (rung 4,
zero impacto no cdylib).

## ADRs

### D1 — Extrair `ground_search` para a camada pura `ann/scan_core.rs` (não uma cópia de bench)
**Decisão:** o ground-loop + `NeighborSource` vivem em `ann/scan_core.rs` (sem `pg_sys`); `traverse` e o bench
ambos o chamam.
**Rationale (cita blueprint D1, `architecture.md § 1`, `parsimony-ladder.md`):** vectorchord prova que benchar
código puro evita o problema de link pg_sys E o risco de divergência da cópia do pgvectorscale; `architecture.md`
manda o domínio não depender de infra — esta extração É essa fronteira.
**Alternativas rejeitadas:** (a) cópia estilo pgvectorscale (`benches/lsr.rs` re-implementa `ListSearchResult`) —
risco de divergência, exige teste de equivalência separado (fallback D3 só); (b) benchar o `traverse` pg-coupled
direto — não linka sem runtime pg (Q5 do blueprint).

### D2 — Um grafo construído UMA vez e compartilhado por referência pelas duas bench fns (same-graph within-run)
**Decisão:** `HnswIndex::build(seed=42)`, N≥50k, construído **uma vez por invocação** de `cargo bench` (fora do
timing) e o MESMO `&HnswIndex` compartilhado por `presized` e `unsized` → o delta é medido sobre um grafo
byte-idêntico DENTRO do run.
**Rationale (cita blueprint D2, edge-case EC-1):** same-graph é o ponto de FU-1. **Correção EC-1:** N≥50k dispara
o build paralelo M44 (`ann/hnsw.rs:44`), que é NÃO-determinístico entre runs (linking races — o mesmo motivo do
M46 não medir). Portanto NÃO se depende de reprodutibilidade cross-run; a garantia same-graph vem de **"built
once, shared by reference"** (as duas fns veem o MESMO objeto de grafo no mesmo run), não de determinismo de seed.
O bench compara presized vs unsized num único grupo/run (não usa `--baseline` cross-run), então within-run
same-graph é suficiente e correto. N≥50k porque `ef*m0*2` (ef=200, m0=32 → 12.800 slots) é onde o rehash do
`::new()` HashSet importa (EC-3); um grafo de 30 nós mostra ~zero delta.
**Alternativa rejeitada:** snapshot/restore de índice PG — mais pesado, reintroduz o acoplamento de formato de
storage que a extração `ann/` evita. Forçar build sequencial (determinístico) a 50k — não exposto por `build()`
e desnecessário (within-run same-graph já basta).

### D3 — Guard de equivalência obrigatório (estrutural + oráculo)
**Decisão:** o path benchado (`ground_search` sobre `MemNeighborSource`) é coberto por teste asseverando que
retorna a mesma ordem/resultado que o oráculo `brute()` (kNN exato, 100% recall no fixture); produção mantém o
`traverse_presize_is_recall_neutral_end_to_end` do M46.
**Rationale (cita blueprint D3, `testing.md §4.1`):** mesmo com função compartilhada, um mapeamento errado
node-id↔vetor no `MemNeighborSource` mediria um grafo bogus; o oráculo pega isso. Recall-neutro é o contrato.
**Alternativa rejeitada:** confiar na função compartilhada sem oráculo (o gap não-guardado do pgvectorscale — Q7).

## Dependencies

### Existing — use as-is
| Package | Version | Ecosystem | Why |
|---|---|---|---|
| `HnswIndex` (interno) | — | rust | fixture seeded + accessors `node_*` (base do `MemNeighborSource`) |

### New — to be introduced
| Package | Version | Ecosystem | Rule 9 rationale | Why this one |
|---|---|---|---|---|
| `criterion` | `0.5.1` | rust (dev-only) | benchmark harness padrão do ecossistema Rust; não reimplementar timing/CI/outlier (Regra 9) | mesma pin do pgvectorscale (peer same-stack, pgrx 0.16.1); dev-dep → zero cdylib |

### Removed
| Package | Last version | Why removed |
|---|---|---|
| (none) | | |

## Dependency Graph

```
Phase 1 (extração pura ann/scan_core + testes+oráculo) ─→ Phase 2 (rewire traverse, recall-neutro)
        │                                                        │
        └────────────────→ Phase 3 (bench harness + link-check) ─┘
                                        │
                                        └─→ Phase 4 (medição + report EC-2)
```
Phase 1 é a base (a função pura). Phase 2 (produção) e Phase 3 (bench) dependem dela e podem paralelizar. Phase 4
depende de 3 (roda o bench). Phase 2 valida recall-neutro; Phase 3 valida o link (Q5).

## Phase 1 — Extração pura do ground-loop + oráculo de equivalência

### Task 1.1 — `ann/scan_core.rs`: `NeighborSource` + `ground_search<S>(…, presize)` + `MemNeighborSource`

#### Why this step
**Ação:** criar `theodb_rs/src/ann/scan_core.rs` com o trait `NeighborSource` (`neighbors_into(node:u64,&mut Vec<u64>)`,
`distance(node:u64)->f64`), a função `ground_search<S: NeighborSource>(src,&S, entry:u64, entry_dist:f64, ef, m0,
presize:bool) -> Vec<(u64,f64)>` (o ground-loop do M46 com `presize` togglando `with_capacity` vs `::new()`), e um
`MemNeighborSource<'a>{ idx:&'a HnswIndex, query:&'a [f32] }` que implementa o trait via `idx.node_neighbors`/
`idx.node_vector`+`metric.dist`. Declarar `mod scan_core` em `ann/mod.rs`.
**Raciocínio:** blueprint D1 — a lógica de alocação que o M46 mudou vira uma função pura parametrizada pela
fronteira `NeighborSource`, benchável sem pg_sys e sem cópia divergente. `Cand` de produção embute (blk,off); a
função pura usa `NodeId=u64` opaco (o adapter empacota), mantendo a lógica de heaps/visited/scratch byte-idêntica.

#### Files to edit
- `theodb_rs/src/ann/scan_core.rs` (NEW) — trait + `ground_search` + `MemNeighborSource` + testes.
- `theodb_rs/src/ann/mod.rs` — `mod scan_core;` (1 linha).

#### Deep file dependency analysis
`ground_search` usa só `std::collections::{HashSet,BinaryHeap}` + um `Cand`-equivalente puro `(f64,u64)`. Depende
de `NeighborSource` (novo, no mesmo arquivo). `MemNeighborSource` depende de `HnswIndex` accessors (`node_neighbors:282`,
`node_vector:278`) — já `pub(crate)`, visíveis dentro do crate. Nenhum consumidor externo ainda (Phase 2/3 os adicionam).

#### TDD
- **RED:** `ground_search_matches_brute_exact_knn` (teste em `scan_core.rs`, `#[cfg(test)]`): construir
  `HnswIndex::build(&rand_corpus(2000,16,42),16,64,L2,42)`, rodar `ground_search` via `MemNeighborSource` a ef=200,
  asseverar que o top-10 por distância == `brute(corpus,q,10,L2)` (100% recall no fixture pequeno). Prova que a
  função pura traverse corretamente (guard D3). `assert_eq!` de sets ordenados.
- **RED (equivalência presize):** `ground_search_presize_is_result_neutral` — `ground_search(...,presize=true)` ==
  `ground_search(...,presize=false)` byte-idêntico (mesma ordem+distâncias) para seed fixa. Prova que `presize` é
  só alocação (o eixo do bench não muda resultado).
- **RED (borda):** `ground_search_ef_zero_clamped` — ef=0 → `ef.max(1)`, retorna ≤1, sem panic (negative case).
- **RED (EC-2, borda):** `ground_search_ef_exceeds_node_count_returns_all` — grafo de 5 nós, ef=200 → retorna ≤5
  resultados (sem panic, sem padding de ef). Assevera `result.len() <= node_count`.
- **RED (EC-3, negativo):** `mem_neighbor_source_out_of_range_node_is_typed_err` — `neighbors_into(u64::MAX,&mut v)`
  → `Err("scan_core: node id out of range")` tipado, NÃO panic de index-out-of-bounds (`error-handling.md`).
- **GREEN:** implementar o trait + `ground_search` (mover a lógica do ground-loop do M46) + `MemNeighborSource`.
- **REFACTOR:** garantir que `ground_search` não referencia `pg_sys` (invariante Q5); doc-comment citando o M46.
  NaN distance (cosine zero-norm) cai por último via o `Ord` existente de `Cand`/`Scored` (`ann/mod.rs:116`, EC-5)
  — sem novo handling; o fixture usa L2 (sem NaN).

#### Concurrency tests
(none — single-threaded) — `ground_search` opera sobre estruturas per-query stack-local; o fixture usa o build
paralelo M44 EXISTENTE (já testado por `hnsw_parallel_build_produces_valid_searchable_graph`), não é concorrência nova.

#### Acceptance criteria
- `ground_search` via `MemNeighborSource` retorna top-k == `brute()` exato no fixture (guard D3).
- `presize=true` e `presize=false` retornam resultado byte-idêntico (`assert_eq!`) — presize é só alocação.
- `ground_search` não importa `pg_sys` (`grep -L pg_sys` no arquivo).
- `cargo pgrx test` (ou `cargo test -p theodb_rs ann::scan_core`) verde.

#### DoD
- `cargo build` limpo; testes de `scan_core` verdes; arquivo ≤ 300 LoC (cirúrgico, budget 500).

## Phase 2 — Rewire de produção (recall-neutro)

### Task 2.1 — `traverse` chama `ground_search` via `PageNeighborSource` adapter

#### Why this step
**Ação:** em `am/hnsw_page.rs`, criar `PageNeighborSource{ rel, nblocks, metric, is_l2, q, meta, reads:Cell<usize>,
tid_of:RefCell<HashMap<u64,(i64,...)>> }` implementando `NeighborSource` (via `neighbors_into`/`load` existentes,
empacotando `(blk,off)` em `u64`); refatorar o ground-loop de `traverse:571-592` para delegar a
`ann::scan_core::ground_search(&pg_src, entry_packed, ep.d, ef, m0, true)`, mapeando os `u64` de volta a tids.
**Raciocínio:** blueprint D1 — produção passa a exercitar EXATAMENTE a função benchada (zero divergência). O
upper-layer descent (`neighbors_of`, `:538`) fica intocado (fora de escopo — só o ground loop). Recall-neutro por
construção (mesma ordem de visita); o oráculo M46 prova.

#### Files to edit
- `theodb_rs/src/am/hnsw_page.rs` (`traverse:554-592` → chamada a `ground_search`; novo `PageNeighborSource`
  `impl NeighborSource`; o ground-loop inline sai).

#### Deep file dependency analysis
`traverse` é chamado só por `scan.rs:131`. O `PageNeighborSource` reusa `load` (`:452`) e `neighbors_into` (`:497`)
existentes — sem nova lógica de page-read. O resultado `Vec<(tid,d)>` de `traverse` (`:598`) permanece idêntico. Os
testes M46 (`traverse_presize_is_recall_neutral_end_to_end:741`) exercitam o path — devem continuar verdes.

#### TDD
- **RED:** o teste M46 existente `traverse_presize_is_recall_neutral_end_to_end` deve continuar passando APÓS o
  rewire (index-scan == seqscan exato). Se divergir 1 tid, o rewire quebrou recall-neutralidade → BUG.
- **RED (novo):** `traverse_via_ground_search_matches_pre_refactor` — capturar o output de `traverse` num índice
  seeded pequeno ANTES do rewire (golden do rev atual `2a1d609`) e asseverar byte-idêntico DEPOIS (recall-neutro
  do refactor). Deriva o golden do binário pré-refactor OU do oráculo brute (anti-circular).
- **RED (EC-4, borda):** `page_neighbor_source_nodeid_roundtrip` — empacotar `(blk=u32::MAX, off=u16::MAX)` em
  u64 via `(blk<<16)|off` e desempacotar; assevera `unpack(pack(x)) == x` no máximo (pega off-by-shift).
- **GREEN:** implementar `PageNeighborSource` + a delegação; mínimo que passa.
- **REFACTOR:** remover o ground-loop inline duplicado; `traverse` fica ~um adapter + 1 chamada.

#### Concurrency tests
(none — single-threaded) — o `index_shared` lock (`scan.rs`) já serializa contra VACUUM e é intocado; nenhum
estado compartilhado novo. `PageNeighborSource` é per-scan stack-local.

#### Acceptance criteria
- `traverse_presize_is_recall_neutral_end_to_end` (M46) verde após o rewire.
- `traverse` retorna `Vec<(tid,d)>` byte-idêntico ao pré-refactor por seed fixa.
- `pages_read` idêntico ao pré-refactor (ordem de visita preservada).
- Suíte pg_test + coexistência M20-M22 verde (`cargo pgrx test` 0 falhas — validado em container).

#### DoD
- `cargo build`/`cargo pgrx install --release` limpo; imagem builda; recall-neutro provado (index==seqscan).
- Diff cirúrgico; `hnsw_page.rs` não cresce materialmente (o inline vira chamada).

## Phase 3 — Bench harness + validação de link

### Task 3.1 — Cargo.toml (criterion + feature + [[bench]]) + `bench_support` + `benches/scan_hot_path.rs`

#### Why this step
**Ação:** em `Cargo.toml`: `[dev-dependencies] criterion="0.5.1"`; `[features] bench_internals=[]`; `[[bench]]
name="scan_hot_path" harness=false required-features=["bench_internals"]`. Em `lib.rs`:
`#[cfg(feature="bench_internals")] pub mod bench_support { pub use crate::ann::{HnswIndex,Metric,scan_core::*};
pub fn seeded_corpus(n,dim,seed)->Vec<(i64,Vec<f32>)>{...} }`. Criar `benches/scan_hot_path.rs` (criterion):
fixture `HnswIndex::build(seeded_corpus(50_000,128,42),...,42)` construído UMA vez; duas `bench_function`
(`scan/presized`, `scan/unsized`) sobre o MESMO grafo via `MemNeighborSource`, ef sweep {100,200,400}.
**Raciocínio:** blueprint Q4/Q5/Q6 — `harness=false` + `required-features` é o padrão vectorchord que expõe
internals ao bench (crates externas só veem `pub`); `bench_support` re-exporta só código puro `ann/` → o bench não
puxa pg_sys (invariante de link). `criterion` dev-only = rung 4.

#### Files to edit
- `theodb_rs/Cargo.toml` (dev-dep + feature + [[bench]]).
- `theodb_rs/src/lib.rs` (`#[cfg(feature="bench_internals")] pub mod bench_support`).
- `theodb_rs/benches/scan_hot_path.rs` (NEW).

#### Deep file dependency analysis
`bench_support` re-exporta `ann::scan_core` (puro) + `HnswIndex` (puro) — sem `am/` (pg-coupled). O bench linka só
esses símbolos → sem pg_sys (Q5). `seeded_corpus` reusa a lógica de `ann/mod.rs::rand_corpus` (hoje `#[cfg(test)]`)
promovida a `bench_support` (ou duplicada minimamente — a RNG `Rng` é `pub(super)`).

#### TDD
- **RED (o link é o teste):** `cargo bench --no-run --features bench_internals` DEVE compilar+linkar sem runtime
  pg. Este é o gate Q5 — se falhar (símbolo pg_sys não resolvido), o bench puxou código pg-coupled → violação do
  invariante de D1 (corrigir a fronteira, NÃO adicionar workaround). Sucesso = binário de bench linkado.
- **RED (estrutura):** `bench_emits_two_functions_same_graph` — um teste leve (em `scan_core.rs` ou um smoke)
  asseverando que o fixture é construído uma vez e as duas fns veem o mesmo `HnswIndex` (ponteiro/hash do grafo).
- **GREEN:** implementar o bench + `bench_support`.
- **REFACTOR:** extrair o setup do fixture numa fn helper; DRY.

#### Concurrency tests
(none — single-threaded) — o bench é criterion single-thread; o build do fixture usa o build paralelo M44 existente
(não concorrência nova). criterion serializa as iterações.

#### Failure scenarios
(none — no external I/O touched) — o bench é puramente in-memory (sem container, sem página, sem rede). A única
"falha" possível é o link pg_sys (coberto pelo RED de link acima), que é erro de compilação, não I/O runtime.

#### Acceptance criteria
- `cargo bench --no-run --features bench_internals` linka sem erro de símbolo pg_sys (gate Q5).
- `cargo bench --features bench_internals` roda e emite 2 medições criterion (presized/unsized) × 3 ef, cada uma
  com CI, sobre o MESMO grafo seeded.
- `cargo build --release` (sem bench_internals) NÃO inclui criterion no cdylib (dev-only; `cargo tree` prova).

#### DoD
- Bench roda no host (sem container — é puro); CIs reportados; feature dev-only confirmada.

## Phase 4 — Medição + veredito honesto (EC-2)

### Task 4.1 — Rodar o bench + `docs/benchmarks/fu1-samegraph-scan-microbench.{md,json}`

#### Why this step
**Ação:** rodar `cargo bench --features bench_internals` no box (agora quieto), capturar o delta presized-vs-unsized
por ef com CIs, e escrever o report com: a tabela de delta, a metodologia (same-graph seeded, same-process
interleaved), e o **caveat EC-2** (upper bound do ganho de produção — sem I/O de página, a alocação é fração maior).
**Raciocínio:** blueprint EC-2 + `public-copy.md` — o número é honesto: quantifica o custo de alocação que o M46
remove (box-noise-immune, same-graph), MAS não é o QPS de produção (I/O amortiza). Nenhuma afirmação de
superioridade de produto sem o número de produção (que continua sendo o SQL quiet-box).

#### Files to edit
- `docs/benchmarks/fu1-samegraph-scan-microbench.{md,json}` (NEW).
- `CHANGELOG.md` (`[Unreleased] § Added`).

#### TDD
- **RED:** `test_fu1_report_present_and_grounded` (doc-check em `benchmarks/tests/` ou `scan_core` smoke): o `.md`
  contém a tabela presized-vs-unsized por ef, os CIs, o caveat EC-2 explícito, e NÃO afirma superioridade de
  produto sem qualificação.
- **GREEN:** gerar o report a partir do output real do criterion (`target/criterion/`).

#### Acceptance criteria (o DoD do milestone)
- Delta presized-vs-unsized medido por ef {100,200,400} com CIs criterion, sobre o MESMO grafo seeded.
- Caveat EC-2 explícito (upper bound; produção I/O-amortizada).
- Sem cherry-pick; se o delta for dentro do CI (não-significativo), reportar honestamente (o pre-size pode ser
  ruído mesmo isolado — resultado válido).
- Recall-neutro reafirmado (Phase 2 oracle verde).

#### DoD
- Report em `docs/benchmarks/`; CHANGELOG atualizado; sem `Co-Authored-By`.

## Coverage Matrix

| # | Gap / Requirement (Goal/blueprint) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Extrair ground-loop p/ camada pura (D1) | T1.1 | `ann/scan_core.rs` `ground_search<S>` |
| 2 | Trait `NeighborSource` (fronteira DIP) | T1.1 | trait + `MemNeighborSource` |
| 3 | Rewire `traverse` via adapter (recall-neutro) | T2.1 | `PageNeighborSource` + delegação |
| 4 | Recall-neutro do refactor provado | T2.1 | M46 oracle + golden byte-idêntico |
| 5 | Fixture seeded same-graph N≥50k (D2, EC-3) | T3.1 | `HnswIndex::build(seed=42)` compartilhado |
| 6 | criterion dev-dep + [[bench]] + feature (Q4/Q6) | T3.1 | Cargo.toml + `bench_support` |
| 7 | Link sem pg_sys (Q5, invariante D1) | T3.1 | `cargo bench --no-run` gate |
| 8 | Guard de equivalência (D3) | T1.1 | `ground_search == brute` oráculo |
| 9 | Bench presized vs unsized + CIs (Q8) | T3.1 | 2 bench_function × ef sweep |
| 10 | Medição + report + caveat EC-2 | T4.1 | `docs/benchmarks/fu1-*.{md,json}` |
| 11 | Caveat EC-2 honesto (upper bound) | T4.1 | seção caveat |
| 12 | CHANGELOG + milestone M47 no ROADMAP | T4.1 | Global DoD |

**Coverage: 12/12 gaps covered (100%)**

## Drawbacks & Risks

| Risco | Sev | Mitigação | Owner |
|---|---|---|---|
| Link pg_sys falha (bench puxa código pg-coupled) | MÉDIO | `ann/scan_core.rs` puro (invariante grep -L pg_sys); `bench_support` re-exporta só `ann/`; gate `cargo bench --no-run` na T3.1; fallback D3 (cópia guardada) se estrutural | eng |
| Rewire de `traverse` regride recall | BAIXO→nulo | recall-neutro por construção (mesma lógica movida); M46 oracle + golden byte-idêntico bloqueiam | eng |
| Delta de alocação dentro do CID (não-significativo mesmo isolado) | MÉDIO | resultado honesto válido (o pre-size pode não mover a agulha sem o rehash de 1M); reportar com CI, sem spin | eng |
| Micro-bench ≠ produção (EC-2) | MÉDIO (esperado) | caveat EC-2 explícito (upper bound); produção QPS continua sendo o SQL quiet-box, não reivindicado aqui | eng |
| `seeded_corpus` duplica `rand_corpus` (`#[cfg(test)]`) | BAIXO | promover a `bench_support` (uma fonte) OU reusar a RNG `pub(super)`; DRY | eng |

## Unresolved Questions

(none — every decision is resolved at plan/blueprint time: extração pura (D1/D1), fixture seeded (D2/D2),
guard (D3/D3), link via feature-gate (Q5), caveat (EC-2). A única incerteza empírica — o link pg_sys — é
resolvida pelo gate `cargo bench --no-run` da T3.1, com fallback D3 documentado.)

## Failure scenarios

(none — no external I/O touched) — todo o novo código (`ann/scan_core.rs`, o bench) é puramente in-memory. A
produção `traverse` mantém seu error path existente (`with_page_item` → `Result`), intocado pelo rewire (o
`PageNeighborSource` reusa `load`/`neighbors_into` que já retornam `Result`). O único "modo de falha" é o link
pg_sys em compile-time (T3.1 gate), não I/O runtime.

## Global DoD

- Todos os pg_tests + `ann::scan_core` tests verdes no container; `cargo build` + `cargo pgrx install --release` limpos.
- Recall-neutro do rewire provado (M46 oracle index==seqscan; golden byte-idêntico).
- `cargo bench --no-run --features bench_internals` linka sem pg_sys (gate Q5); `cargo bench` emite 2 medições × ef com CI.
- Benchmark reproduzível em `docs/benchmarks/fu1-*.{md,json}` com delta + CIs + caveat EC-2 honesto.
- `/code-quality` ∉ {FAIL_HARD, INVALID}; `/review` READY_TO_MERGE.
- CHANGELOG `[Unreleased]` atualizado; milestone M47 no ROADMAP; sem `Co-Authored-By`.
- File-size: `scan_core.rs` ≤ 300 LoC; `hnsw_page.rs` não cresce materialmente.

## Final Phase — Integration Validation

Rodar a suíte completa no container (pg_tests + `ann::scan_core` tests) + `cargo bench --features bench_internals`
end-to-end (fixture → 2 medições → delta → report). O plano NÃO está completo até: (a) recall-neutro do rewire
provado (index==seqscan byte-idêntico), (b) bench linka sem pg_sys e roda com CIs, (c) report honesto com caveat
EC-2 escrito com dados reais do criterion, (d) suíte verde. "Eat your own cooking": se o rewire mudar 1 tid, é BUG
e o milestone falhou.
