---
slug: m140-3-bm25-production-engine
milestone_id: M140.3
created_at: 2026-07-22
goal: Entregar a superfície BM25 own-code de produção (bm25_build + bm25_search) com cache do Directory MVCC-correto que mata o reload-por-query do spike.
---

# Plan: M140.3 — Engine BM25 de produção own-code (cache + superfície)

> **Version 1.0** — O spike M139 provou a viabilidade mas recarregava o índice inteiro do heap a **cada** busca
> (o único "naive" declarado). O M140.3 entrega a engine **usável em produção**: (a) um **cache do Directory**
> process-local, MVCC-correto (por geração), que mata o reload-por-query; (b) a superfície own-code
> `theodb.bm25_build` (indexa uma tabela) + `theodb.bm25_search` (retorna `(id, score)`), sobre heap (ADR-0052,
> não index AM); (c) supersede a exceção permissiva do ADR-0013 (`pg_textsearch`) com own-code Tantivy MIT. Bate
> a baseline do M138 na forma final, com o cache medido vs o reload-por-query.

## Goal

> Enable buscas BM25 own-code repetidas a NÃO pagarem mais o reload-do-índice-por-query do spike, entregando a
> superfície `theodb.bm25_search(index_id, query, k)` com cache do Directory MVCC-correto, measured by o gate
> `theodb_rs/lexical_core` (teste do cache) verde + um benchmark em `docs/benchmarks/m140-3-bm25-engine.md` que
> mostra latência de busca com cache **< 50%** da latência reload-por-query no mesmo corpus E nDCG@10 ≥ a
> baseline `pg_textsearch` do M138.

## Context

M140.1 (`docs/adr/0052`) decidiu **heap buffer-then-flush** (não index AM) e mediu que a BM25 own-engine bate
`ts_rank_cd` em retrieval lexical puro. M140.2 (`docs/adr/0053`) extraiu o núcleo pgrx-free (`theodb_lexical`:
`PgDirectory`/`MemStore`/`SegmentStore`), testável com `cargo test` stock. O spike (M139, `pg_backing.rs`) tem o
write path (`flush`) + read path (`load`) + `lexical_spike_search` — mas `load` **reconstrói um `MemStore` do
heap a cada busca** (`pg_backing.rs:42`). Em produção, buscas repetidas sobre um índice estável não podem pagar
esse reload; e a superfície `lexical_spike_*` é demo (retorna contagem `i64`), não produção (precisa retornar
`(id, score)` de linhas reais). O M140.3 fecha essas duas lacunas.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/lexical_core/src/lib.rs` | ~300 | `ebfd4ff`~ (M140.2) | Núcleo pgrx-free: `PgDirectory`/`MemStore`/`SegmentStore` | Continua pgrx-free (`cargo tree` zero pgrx); 6 testes verdes |
| `theodb_rs/src/lexical/pg_backing.rs` | 211 | M139/M140.2 | `flush`/`load` + `#[pg_extern]` do spike | `flush`/`load` mantêm o contrato do heap MVCC; o novo surface é aditivo |
| `theodb_rs/src/lexical/mod.rs` | ~13 | M140.2 | Re-export do núcleo + `pub mod pg_backing` | Aditivo (novos módulos re-exportados) |
| `theodb_rs/lexical_core/src/cache.rs` (NEW) | 0 | — | (novo) o cache puro do índice por geração (pgrx-free, testável) | — |
| `theodb_rs/src/lexical/engine.rs` (NEW) | 0 | — | (novo) a superfície de produção: `bm25_build` + `bm25_search` (pgrx, SPI + cache) | — |
| `benchmarks/run_m140_3_engine.py` (NEW) | 0 | — | (novo) benchmark: latência cache-vs-reload + nDCG vs M138 | — |
| `docs/adr/0054-m140-3-bm25-supersede-textsearch.md` (NEW) | 0 | — | (novo) ADR-2: supersede ADR-0013 | — |
| `docs/benchmarks/m140-3-bm25-engine.md` (NEW) | 0 | — | (novo) o artefato de medição | — |

### Current callers / dependents

- **Symbol:** `load(index_id)` / `flush(index_id, store)` em `pg_backing.rs:27,42`
  - **Callers (produção):** os `#[pg_extern]` do spike (`lexical_spike_*`); o novo `engine.rs` os reusa.
  - **Callers (tests):** os testes do spike em `pg_backing.rs`.
  - **External:** não — atrás da feature `spike-lexical`, não shipado.
- **Symbol:** `PgDirectory`/`MemStore` (núcleo `theodb_lexical`)
  - **Callers:** `pg_backing.rs` (flush/load/spike). O novo `cache.rs` (no núcleo) e `engine.rs` (pgrx) os consomem.

### Domain glossary

- **geração (generation)** — contador por índice, bumpado a cada `bm25_build`; lido sob o snapshot da txn na busca. O cache serve o `Index` construído para uma geração; se a geração visível mudou, reconstrói (invalidação MVCC-correta).
- **cache do Directory** — mapa process-local (por-backend PG) `index_id -> (geração, tantivy::Index)`; evita o `load`+rebuild do heap a cada busca.
- **reload-por-query** — o comportamento naive do spike: `load(index_id)` reconstrói o `MemStore` do heap a **cada** `lexical_spike_search` (`pg_backing.rs:42,183`).
- **SRF (set-returning function)** — `#[pg_extern]` que retorna `TableIterator<(id, score)>` (padrão `graph_extract.rs:277`).

### Architecture boundaries affected

Mantém a fronteira do M140.2: o **cache é lógica pura** → vive em `theodb_lexical` (pgrx-free, testável). A
**superfície SQL** (`bm25_build`/`bm25_search`, SPI + geração do catálogo) vive em `theodb_rs` (pgrx). DIP
(`architecture.md §2`): o núcleo define o cache + `SegmentStore`; a camada pgrx wira a geração e o heap.

## Prior Art & Related Work

- **Internal ADR:** `docs/adr/0052` (heap, não AM — a forma da superfície), `docs/adr/0053` (o núcleo pgrx-free reusado), `docs/adr/0013` (a exceção `pg_textsearch` a superseder), `docs/adr/0051` (o spike + o "naive" reload-per-query).
- **Internal benchmark:** `docs/benchmarks/m140-1-lexical-measurement.md` (a baseline M138 + o harness `theodb_bench` a reusar p/ nDCG).
- **Reference project:** ParadeDB `pg_search` (AGPL — estudo apenas) usa um IndexReader cacheado invalidado por versão; a IDEIA (cache por versão) é aprendível, o código reimplementa-se do zero (Rule 9 / [[vectorchord-agpl-study-only]]).
- **Skill de patterns:** nenhuma `skills/*-patterns/` casa (engine BM25) — verificado; nada a citar/sobrepor.
- **External:** Tantivy 0.26 `IndexReader`/`ReloadPolicy` docs (MIT); Okapi BM25 (Robertson & Zaragoza 2009). SRF pgrx: `graph_extract.rs:277` (padrão interno).

## Objective

- [ ] Sub-goal 1 — `IndexCache` puro no núcleo (`theodb_lexical`): `get_or_build(id, generation, build_fn)`, MVCC-correto por geração, testável com `cargo test` stock.
- [ ] Sub-goal 2 — catálogo de geração (`theodb.lexical_index_meta`) bumpado no build, lido sob snapshot na busca.
- [ ] Sub-goal 3 — `theodb.bm25_build(index_id, table, id_col, text_col)` — indexa uma tabela real (id+text) no heap.
- [ ] Sub-goal 4 — `theodb.bm25_search(index_id, query, k)` → `TableIterator(id, score)`, usando o cache (sem reload-por-query).
- [ ] Sub-goal 5 — benchmark medido: latência com cache < 50% do reload-por-query + nDCG@10 ≥ baseline M138.
- [ ] Sub-goal 6 — ADR-0054 supersede a exceção do ADR-0013 (`pg_textsearch`) + plano de saída.

## ADRs

### D1 — Cache do Directory process-local, invalidado por geração (MVCC-correto)

- **Decision:** um cache por-backend PG (`static Mutex<HashMap<i64, (u64, Index)>>` na camada pgrx; a LÓGICA de
  get-or-build vive no núcleo pgrx-free como `IndexCache`) keyed por `index_id`, guardando o `tantivy::Index`
  aberto + a geração em que foi construído. Na busca: lê a geração **visível sob o snapshot** do catálogo; se
  `cache.generation == visible_generation`, reusa; senão `load` (snapshot-visível) + rebuild + cache.
- **Rationale:** mata o reload-por-query para buscas repetidas na geração estável (o caso read-heavy comum —
  theo-lens busca traces muito mais do que ingere). MVCC-correto: um leitor com snapshot antigo vê uma geração
  antiga → reconstrói do estado heap **que seu snapshot enxerga** (o `load` já respeita o snapshot, M139 gate 2);
  nunca serve uma versão que o snapshot não deveria ver. Cada backend PG é um processo separado → o cache é
  naturalmente per-conexão (sem sharing cross-backend), single-thread (a busca roda na main thread — #153).
- **Alternatives considered:** (a) `IndexReader` do Tantivy com `ReloadPolicy::OnCommitWithDelay` — rejeitado:
  o reload do Tantivy observa o filesystem, não o heap PG/geração; não é MVCC-aware do snapshot. (b) sem cache
  (reload sempre) — é o naive do spike que o milestone existe para matar. (c) cache compartilhado cross-backend
  (shared memory) — YAGNI + quebra o isolamento por-snapshot; um `static` por-processo é correto e mais simples.
- **Consequences:** habilita busca rápida repetida; custo = memória do cache (um Index aberto por índice ativo
  por backend); a consistência flush-sob-merge em escala é risco residual (#153) → **M140.4** prova.

### D2 — Geração num catálogo heap (`theodb.lexical_index_meta`), bumpada no build

- **Decision:** `theodb.lexical_index_meta(index_id bigint PRIMARY KEY, generation bigint NOT NULL)`; `bm25_build`
  faz `INSERT ... ON CONFLICT DO UPDATE SET generation = generation + 1`; `bm25_search` lê `generation` sob o
  snapshot corrente. É o token de invalidação do D1.
- **Rationale:** o heap dá MVCC de graça ao token de geração (mesma razão do ADR-0051 para os bytes) — um leitor
  vê a geração que seu snapshot vê. Simples (uma linha por índice), Rule 9 (heap, não estrutura custom).
- **Alternatives considered:** derivar a geração de `max(xmin)`/`count(*)` de `lexical_files` — rejeitado:
  frágil (xmin wraparound; count não muda em updates in-place). Um contador explícito é claro e correto.
- **Consequences:** `bm25_build` e `bm25_search` compartilham o token; o cache invalida-se corretamente.

### D3 — Schema do índice: `id` (i64) stored/fast + `body` TEXT; a busca retorna `(id, score)`

- **Decision:** o schema Tantivy da produção = `id: i64 STORED|FAST` + `body: TEXT` (tokenizer default, como o
  spike + o harness M140.1). `bm25_search` retorna os `id` armazenados + o score BM25.
- **Rationale:** produção precisa retornar QUAIS documentos casaram (o spike só contava). `id` como i64 stored é
  o mínimo para o join de volta à tabela do usuário. Fidelidade de ranking: mesmo tokenizer do spike/M140.1.
- **Alternatives considered:** id como text — rejeitado (i64 é o caso do theo-lens/PK numérica; text é follow-up
  se um consumidor precisar). Armazenar o body — sim (o M140.1 mediu com body stored; consistente).
- **Consequences:** o índice guarda o id; `bm25_search` retorna linhas úteis; join trivial na tabela do usuário.

### D4 — Superfície = funções (`bm25_build`/`bm25_search`), não index AM (consome ADR-0052)

- **Decision:** a superfície é `#[pg_extern]` funções sobre o heap, como o ADR-0052 decidiu (heap, não AM custom).
- **Rationale:** ADR-0052 mediu que o heap não tem inversão de custo que justifique o AM; funções são o mínimo
  (KISS) que entrega a capacidade sem o AM (registro/reloption/cost/vacuum próprios).
- **Alternatives considered:** index AM `USING theodb_bm25` — rejeitado pelo ADR-0052 (over-engineering medido).
- **Consequences:** M140.4 prova MVCC/VACUUM/crash sobre esta superfície de funções + heap.

### D5 — ADR-0054 supersede a exceção permissiva do ADR-0013 (`pg_textsearch`) para BM25

- **Decision:** o ADR-0054 registra que a BM25 own-code (Tantivy MIT) torna a exceção do ADR-0013
  (`pg_textsearch` como BM25 v1-legacy) obsoleta; plano de saída para quem a adotou.
- **Rationale:** o DoD-3 do milestone. O ADR-0013 mantinha `pg_textsearch` como exceção medição-throwaway; agora
  há own-code permissivo viável (o spike deu GO, M139; o M140.1 mediu que bate ts_rank).
- **Alternatives considered:** manter `pg_textsearch` — rejeitado: o M138 mediu o leg in-DB dele **quebrado**
  (#146) + é dep externa; own-code é o mandato v2 (ADR-0006).
- **Consequences:** o roadmap lexical passa a own-code; `pg_textsearch` é referência de benchmark, não produção.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Cache serve versão errada sob snapshot (bug de MVCC) | High | D1/D2: geração lida sob snapshot; teste MVCC (leitor snapshot antigo não vê build novo) no pg-test; M140.4 prova a fundo | dev |
| Cache thread-safety (#153): busca toca o cache de múltiplas threads | Medium | A busca roda na main thread; o `Mutex` protege; o Tantivy usa threads internas que NÃO tocam o cache/PG. Probe de threads é regressão M140.4 | dev |
| Memória do cache cresce com nº de índices ativos por backend | Medium | Um Index por índice ativo por backend; começar sem evição (YAGNI); documentar; evição LRU é follow-up se medido | dev |
| Invalidação sob escrita concorrente (flush durante busca) | Medium | Geração bumpada atomicamente no build (ON CONFLICT); consistência flush-sob-merge em escala é #153 → M140.4 | dev |
| `bm25_build` sobre tabela grande via SPI pode ser lento/memória | Low | Streaming por cursor SPI; medir ingest; escala bilhão-scale é fora de escopo (o corpus do theo-lens é modesto) | dev |

## Unresolved Questions

- Q1 — O cache deve ser `thread_local!` (por-thread) ou `static Mutex` (por-processo)? → `static Mutex` por-processo (D1): um backend PG é single-thread na main; o Mutex cobre o acesso; `thread_local` perderia o cache entre statements. Resolver no T1.
- Q2 — A geração deve ser lida uma vez por busca ou por statement? → uma vez por `bm25_search` (o snapshot é estável na chamada); resolver no T2.
- Q3 — `bm25_build` deve dropar o índice antigo do heap antes de reconstruir? → sim: `DELETE FROM lexical_files WHERE index_id=$1` + reinsere, na mesma txn (atômico), bumpa geração. Resolver no T2.

## Dependency Graph

```
Phase 1 (IndexCache puro no núcleo + testes stock) ──▶ Phase 2 (superfície pgrx: build+search+geração+cache)
                                                              │
                                                              ▼
                                                        Phase 3 (benchmark cache-vs-reload + nDCG + ADR-0054)
```

Phase 1 é pgrx-free (testa local stock). Phase 2 precisa do toolchain pgrx (valida no e2e-runner). Phase 3 mede.

---

## Phase 1: `IndexCache` puro no núcleo (pgrx-free, testável)

**Objective:** a lógica de cache por-geração como tipo puro em `theodb_lexical`, testável com `cargo test` stock.

### T1.1 — `IndexCache::get_or_build` por geração

#### Objective
Adicionar `theodb_lexical::cache::IndexCache` — um mapa `index_id -> (generation, Index)` com
`get_or_build(id, gen, build_fn)` que reconstrói iff a geração mudou. Puro, sem pgrx.

#### Why this step (action + reasoning)
1. **What this step does** — cria `lexical_core/src/cache.rs` com `IndexCache` (a lógica de invalidação por geração), testável com um `build_fn` fake.
2. **Why it is necessary now** — é a fundação do D1; mantém a testabilidade do M140.2 (o cache é lógica pura → núcleo pgrx-free). Sem ela, a invalidação MVCC viveria só no pgrx (não testável stock).

#### Evidence
`docs/adr/0053` (o núcleo pgrx-free onde a lógica pura vive); `theodb_rs/lexical_core/src/lib.rs:29` (o `SegmentStore`/`MemStore` que o `Index` usa).

#### Files to edit
```
theodb_rs/lexical_core/src/cache.rs — (NEW) IndexCache + get_or_build + testes
theodb_rs/lexical_core/src/lib.rs — pub mod cache; pub use cache::IndexCache
```

#### Deep file dependency analysis
- `cache.rs` (NEW) usa `std::collections::HashMap` + `tantivy::Index`; sem pgrx. `lib.rs` re-exporta.
- Downstream: `engine.rs` (Phase 2, pgrx) consome o `IndexCache`.

#### Deep Dives
- `IndexCache { map: HashMap<i64, (u64, Index)> }`. `get_or_build(id, gen, build: impl FnOnce() -> Index) -> &Index`: se `map[id].0 == gen`, retorna o cacheado; senão `build()`, insere `(gen, index)`, retorna.
- Invariante: a mesma `(id, gen)` retorna o MESMO Index sem chamar `build`; uma `gen` diferente chama `build` uma vez.
- Edge case: id ausente → build; gen decresce (leitor snapshot antigo após um build) → build (reconstrói do estado antigo — correto).

#### Pseudo-code / Signatures
```pseudocode
pub struct IndexCache { map: HashMap<i64,(u64, tantivy::Index)> }
impl IndexCache {
  pub fn new() -> Self
  pub fn get_or_build(&mut self, id: i64, gen: u64, build: impl FnOnce()->tantivy::Index) -> &tantivy::Index {
    let stale = self.map.get(&id).map(|(g,_)| *g != gen).unwrap_or(true);
    if stale { self.map.insert(id, (gen, build())); }
    &self.map.get(&id).unwrap().1
  }
}
```

#### Tasks
1. RED tests (cache hit não chama build; geração nova chama build 1×; geração decrescente reconstrói).
2. Implementar `IndexCache`.
3. REFACTOR: None.

#### TDD
```
RED:  test_cache_hit_same_generation_does_not_rebuild() — build chamado 1× em 2 gets na mesma gen
RED:  test_new_generation_rebuilds_once() — gen muda → build chamado de novo
RED:  test_decreasing_generation_rebuilds() — gen antiga (leitor snapshot antigo) → reconstrói (não serve versão nova)
RED:  test_absent_id_builds() — id novo → build
GREEN: implementar cache.rs
REFACTOR: None expected
VERIFY: cd theodb_rs && cargo test -p theodb_lexical
```

#### Concurrency tests

O `IndexCache` em si é single-thread por design (D1: acessado da main thread do backend, protegido por `Mutex` na camada pgrx). O tipo puro não tem locking próprio (o Mutex é da camada pgrx). O probe de threads (que a busca não toca PG/cache de worker thread) é regressão do M140.4.

(none — single-threaded)

#### Acceptance Criteria
- [ ] `cargo test -p theodb_lexical` exit code 0 incluindo os 4 testes novos do cache.
- [ ] `cargo tree -p theodb_lexical | grep -c pgrx` retorna 0 (o cache continua pgrx-free).
- [ ] `grep -c "build" theodb_rs/lexical_core/src/cache.rs` confirma o build_fn injetado (testável).
- [ ] `wc -l theodb_rs/lexical_core/src/cache.rs` ≤ 120.

#### DoD
- [ ] `cd theodb_rs && cargo test -p theodb_lexical` exit code 0.
- [ ] `cargo tree -p theodb_lexical | grep -c pgrx` retorna 0.
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Phase 2: Superfície de produção (pgrx: build + search + cache + geração)

**Objective:** os `#[pg_extern]` de produção sobre o heap + cache, MVCC-correto.

### T2.1 — `bm25_build` + catálogo de geração

#### Objective
`theodb.bm25_build(index_id, table, id_col, text_col)`: lê a tabela via SPI, indexa `(id, body)` no Tantivy,
flush ao heap (drop+reinsere atômico), bumpa a geração em `theodb.lexical_index_meta`.

#### Why this step (action + reasoning)
1. **What this step does** — o write path de produção: indexa uma tabela real e versiona.
2. **Why it is necessary now** — sem o build de produção não há o que buscar; a geração é o token do cache (D2).

#### Evidence
`pg_backing.rs:27` (`flush` a reusar), `:19` (o catálogo `lexical_files`), ADR-0052 (heap), `graph_extract.rs` (SPI read pattern).

#### Files to edit
```
theodb_rs/src/lexical/engine.rs — (NEW) bm25_build (#[pg_extern]) + ensure_meta + bump_generation
theodb_rs/src/lexical/mod.rs — pub mod engine (sob cfg spike-lexical)
```

#### Deep file dependency analysis
- `engine.rs` (NEW) usa `theodb_lexical::{PgDirectory, MemStore}` + `flush` de `pg_backing` + SPI. `mod.rs` adiciona `pub mod engine`.
- Downstream: `bm25_search` (T2.2) lê a geração que este bumpa.

#### Deep Dives
- Schema Tantivy: `id: i64 STORED|FAST`, `body: TEXT` (D3). Lê `SELECT <id_col>, <text_col> FROM <table>` via SPI, `add_document(id, body)` por linha, commit, `flush` ao heap (após `DELETE FROM lexical_files WHERE index_id` — drop+reinsere atômico, Q3), `INSERT ... ON CONFLICT DO UPDATE generation+1`.
- Invariante: build é atômico (uma txn); a geração só bumpa após o flush completo.
- Edge case: tabela vazia → índice vazio, geração bumpa; `table`/`col` inválidos → erro tipado (validar identificadores — SQL injection: usar `quote_ident`).

#### Pseudo-code / Signatures
```pseudocode
#[pg_extern] fn bm25_build(index_id: i64, table: &str, id_col: &str, text_col: &str) -> i64 {
  ensure_table(); ensure_meta();
  let (schema, id_f, body_f) = build_schema();       // id STORED|FAST, body TEXT
  let store = Arc::new(MemStore::default());
  let index = Index::create(PgDirectory::with_store(store.clone()), schema, ...);
  let mut w = index.writer_with_num_threads(1, 50MB);
  for (id, txt) in spi_select(quote_ident(table), quote_ident(id_col), quote_ident(text_col)) {
    w.add_document(doc!(id_f => id, body_f => txt));
  }
  w.commit();
  Spi::run("DELETE FROM theodb.lexical_files WHERE index_id=$1", &[index_id]);
  flush(index_id, &store);                            // reusa pg_backing::flush
  bump_generation(index_id);                          // INSERT..ON CONFLICT generation+1
  store.files().len() as i64
}
```

#### Tasks
1. RED pg-test (build indexa N docs; geração vira 1; rebuild → geração 2).
2. Implementar `bm25_build` + `ensure_meta` + `bump_generation` + `quote_ident` (anti-injeção).
3. REFACTOR: extrair `build_schema` compartilhado com search.

#### TDD
```
RED:  test_bm25_build_indexes_table_and_bumps_generation() — pg-test: build sobre tabela de 3 linhas → 3 files, gen=1
RED:  test_bm25_build_rebuild_bumps_generation_to_2()
RED:  test_bm25_build_quotes_identifiers() — nome de tabela com aspas não injeta
GREEN: implementar engine.rs bm25_build
REFACTOR: extrair build_schema
VERIFY: (e2e-runner) cargo pgrx test --features "pg18 spike-lexical pg_test" bm25_build
```

#### Concurrency tests

O `bm25_build` roda numa txn (main thread); o writer do Tantivy usa 1 thread (`writer_with_num_threads(1, ...)`), e as threads internas do Tantivy NÃO tocam SPI/pg_sys (a disciplina #153 do spike, preservada — o flush é main-thread pós-commit). Escrita concorrente de dois builds no MESMO index_id: serializada pelo `ON CONFLICT` da geração + o `DELETE`/`flush` na txn (o segundo espera o lock de linha). O caminho que toca o PG é single-threaded.

(none — single-threaded)

#### Acceptance Criteria
- [ ] pg-test: `bm25_build` sobre tabela de 3 linhas persiste 3 files + geração=1 (`cargo pgrx test`).
- [ ] Rebuild bumpa geração para 2 (test).
- [ ] Identificadores citados (`quote_ident`) — teste de não-injeção passa.
- [ ] `cargo clippy --features "pg18 spike-lexical" --no-deps -- -D warnings` zero warnings novos.

#### DoD
- [ ] (e2e-runner) `cargo pgrx test --features "pg18 spike-lexical pg_test"` verde p/ bm25_build.
- [ ] `cargo check --features "pg18 spike-lexical"` exit code 0.
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

### T2.2 — `bm25_search` com cache MVCC-correto

#### Objective
`theodb.bm25_search(index_id, query, k)` → `TableIterator(id, score)`: lê a geração sob snapshot, usa o
`IndexCache` (rebuild só se a geração mudou), busca e retorna `(id, score)`.

#### Why this step (action + reasoning)
1. **What this step does** — o read path de produção com cache (mata o reload-por-query) e retorno de linhas.
2. **Why it is necessary now** — é o coração do milestone (o Goal: cache < 50% da latência reload).

#### Evidence
`pg_backing.rs:42` (`load` reusado no build_fn do cache), `:183` (`lexical_spike_search` — o reload-por-query a substituir), `graph_extract.rs:277` (SRF pattern), T1.1 (`IndexCache`).

#### Files to edit
```
theodb_rs/src/lexical/engine.rs — bm25_search (#[pg_extern] SRF) + o CACHE static Mutex + read_generation
```

#### Deep file dependency analysis
- `bm25_search` usa `theodb_lexical::IndexCache` (T1.1) num `static CACHE: Mutex<HashMap<i64, IndexCache-por-id>>`... na verdade um `static CACHE: Lazy<Mutex<IndexCache>>` (o IndexCache já é o mapa por-id). Lê a geração via SPI sob snapshot; o build_fn = `load(index_id)` + `Index::open`.
- Downstream: o benchmark (Phase 3) e o consumidor theo-lens (M140.4).

#### Deep Dives
- `static CACHE: Lazy<Mutex<IndexCache>>` (per-backend process). `bm25_search`: `gen = read_generation(index_id)` (SPI, snapshot); `let mut c = CACHE.lock(); let index = c.get_or_build(index_id, gen, || Index::open(PgDirectory::with_store(Arc::new(load(index_id)))))`; abre reader, `parse_query` sanitizado, `TopDocs(k)`, coleta `(id stored, score)`; retorna `TableIterator`.
- Invariante MVCC (D1): a geração é lida sob o snapshot → um leitor com snapshot antigo lê a geração antiga → o cache reconstrói do `load` que o snapshot enxerga (nunca serve docs de um build que o snapshot não vê).
- Edge case: índice inexistente (geração ausente) → 0 linhas; query vazia → 0 linhas; k≤0 → erro tipado.

#### Pseudo-code / Signatures
```pseudocode
static CACHE: Lazy<Mutex<IndexCache>> = ...;
#[pg_extern] fn bm25_search(index_id: i64, query: &str, k: i32)
    -> TableIterator<'static, (name!(id, i64), name!(score, f64))> {
  if k <= 0 { error!("k must be > 0"); }
  let gen = read_generation(index_id);                 // SPI, sob snapshot; None -> vazio
  let mut cache = CACHE.lock().unwrap();
  let index = cache.get_or_build(index_id, gen, || open_from_heap(index_id));  // load só se gen mudou
  let hits = search_index(index, query, k as usize);   // (id_i64, score_f64)
  TableIterator::new(hits.into_iter())
}
```

#### Tasks
1. RED pg-test (search retorna o id certo; 2ª search na mesma geração NÃO recarrega — via counter de load; search após rebuild vê a nova geração).
2. Implementar `bm25_search` + o `static CACHE` + `read_generation` + `search_index`.
3. REFACTOR: compartilhar `open_from_heap` / schema com o build.

#### TDD
```
RED:  test_bm25_search_returns_matching_id() — pg-test: build 3 docs, search termo raro → o id certo em rank 0
RED:  test_bm25_search_cache_avoids_reload_same_generation() — 2 searches, load() chamado 1× (counter/log)
RED:  test_bm25_search_sees_new_generation_after_rebuild() — rebuild → search vê os novos docs
RED:  test_bm25_search_mvcc_old_snapshot() — leitor com snapshot antes do build não vê os docs novos
RED:  test_bm25_search_empty_query_returns_no_rows()
GREEN: implementar bm25_search
REFACTOR: extrair open_from_heap
VERIFY: (e2e-runner) cargo pgrx test --features "pg18 spike-lexical pg_test" bm25_search
```

#### Concurrency tests

O `static CACHE: Mutex<IndexCache>` é acessado da main thread do backend; o `Mutex` serializa (um backend é
single-thread, mas o Mutex é o contrato de segurança). As threads internas do Tantivy (busca) NÃO tocam o cache
nem o PG (#153 — a disciplina do spike). Teste: um `Mutex` guarda o acesso; o probe de threads é regressão M140.4.

Atomic-counter invariant: um contador de `load()` prova que N buscas na mesma geração chamam `load` 1× (o cache hit).

#### Acceptance Criteria
- [ ] pg-test: `bm25_search` retorna o `id` correto para um termo raro (rank 0).
- [ ] Cache hit: 2 buscas na mesma geração chamam `load()` 1× (contador prova).
- [ ] MVCC: um leitor com snapshot anterior ao build não vê os docs novos (test).
- [ ] `cargo clippy --features "pg18 spike-lexical" -- -D warnings` limpo.

#### DoD
- [ ] (e2e-runner) `cargo pgrx test --features "pg18 spike-lexical pg_test"` verde p/ bm25_search (incl. MVCC + cache-hit).
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Phase 3: Benchmark + ADR-0054

**Objective:** medir o ganho do cache + confirmar nDCG ≥ M138, e superseder o ADR-0013.

### T3.1 — Benchmark: latência cache-vs-reload + nDCG vs M138

#### Objective
`run_m140_3_engine.py`: mede a latência de `bm25_search` (cache) vs o `lexical_spike_search` (reload-por-query)
no MESMO corpus, e o nDCG@10 da engine de produção vs a baseline `pg_textsearch` do M138 (reusa `theodb_bench`).

#### Why this step (action + reasoning)
1. **What this step does** — o artefato de evidência (o Goal: cache < 50% da latência reload + nDCG ≥ M138).
2. **Why it is necessary now** — DoD-5; sem número medido, "cache mata o reload" é opinião (`public-copy.md`).

#### Evidence
`benchmarks/theodb_bench/` (harness reusado), `docs/benchmarks/m140-1-lexical-measurement.md` (a baseline M138 + o corpus), `pg_backing.rs:183` (o reload-por-query a comparar).

#### Files to edit
```
benchmarks/run_m140_3_engine.py — (NEW) latência cache vs reload + nDCG vs M138 (docker PG18 c/ o build)
```

#### Deep file dependency analysis
- Reusa `theodb_bench.{beir,metrics,significance}` + o corpus. Fala com um PG com o `theodb_rs` (feature spike-lexical) instalado (e2e-runner ou docker com o build).

#### Deep Dives
- Latência: indexa o corpus via `bm25_build`; roda K buscas repetidas via `bm25_search` (cache) e via `lexical_spike_search` (reload) — mede p50/mean sobre ≥3 runs. Gate: cache < 50% do reload.
- nDCG: BEIR scifact/nfcorpus via `bm25_search`, compara com a baseline M138 (`pg_textsearch`) — deve ser ≥.
- Invariante de rigor: ≥3 runs, mean±std, sem cherry-pick (council-benchmark).

#### Tasks
1. Escrever o runner (reusa harness).
2. Rodar no box com o build; emitir JSON.
3. Confirmar o gate (cache < 50% reload; nDCG ≥ M138).

#### TDD
```
RED:  test_m140_3_smoke() — o runner --smoke emite JSON {latency_cache, latency_reload, ndcg} bem-formado
GREEN: implementar o runner
REFACTOR: None expected
VERIFY: cd benchmarks && python3 run_m140_3_engine.py --smoke --out /tmp/m140_3_smoke.json
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] JSON emitido com `latency_cache_ms`, `latency_reload_ms`, `ndcg` sobre números reais.
- [ ] `latency_cache_ms < 0.5 * latency_reload_ms` (o Goal do cache).
- [ ] nDCG@10 da produção ≥ a baseline `pg_textsearch` do M138 (mesmo corpus).
- [ ] `ruff check benchmarks/run_m140_3_engine.py` zero warnings.

#### DoD
- [ ] `python3 run_m140_3_engine.py --smoke` exit code 0.
- [ ] Números reais no JSON; `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

### T3.2 — ADR-0054 (supersede ADR-0013) + report

#### Objective
`docs/adr/0054-m140-3-bm25-supersede-textsearch.md` (supersede a exceção `pg_textsearch` do ADR-0013 + plano de
saída) e `docs/benchmarks/m140-3-bm25-engine.md` (o report medido).

#### Why this step (action + reasoning)
1. **What this step does** — documenta o veredito (DoD-6) e o artefato de medição.
2. **Why it is necessary now** — fecha o milestone; M140.4 (consumidor) constrói sobre esta superfície.

#### Evidence
`docs/adr/0013` (a exceção a superseder), o JSON de T3.1.

#### Files to edit
```
docs/adr/0054-m140-3-bm25-supersede-textsearch.md — (NEW)
docs/benchmarks/m140-3-bm25-engine.md — (NEW)
```

#### Deep file dependency analysis
- Documentos; sem downstream de código. M140.4 cita.

#### Deep Dives
- ADR: Contexto (ADR-0013 mantinha pg_textsearch; agora own-code viável), Decisão (supersede; own-code é a superfície BM25), Alternativas (manter pg_textsearch — rejeitado, #146 quebrado + dep externa), Consequências + plano de saída.
- Report: latência cache-vs-reload (tabela), nDCG vs M138, reprodução. Honestidade: sem framing banido.
- Edge case: se o cache NÃO atingir < 50% (improvável) → honest-negative + re-escopo.

#### Tasks
1. Escrever ADR-0054 + report a partir dos números de T3.1.

#### TDD
```
RED:  (docs — validado por check_xrefs + presença de tabelas/números)
GREEN: escrever ADR + report
REFACTOR: None expected
VERIFY: python3 .claude/scripts/check_xrefs.py 2>&1 | tail -3
```

#### Concurrency tests

(none — single-threaded)

#### Acceptance Criteria
- [ ] ADR-0054 tem Decisão + ≥1 alternativa + Consequências + plano de saída do `pg_textsearch`.
- [ ] Report tem tabela de latência (cache vs reload) + nDCG vs M138 + reprodução, com números reais.
- [ ] `python3 .claude/scripts/check_xrefs.py` retorna Overall PASS.

#### DoD
- [ ] ADR + report escritos a partir dos números reais.
- [ ] `git diff CHANGELOG.md` mostra entrada `[Unreleased]`.

---

## Coverage Matrix

| # | Gap / Requirement (DoD ROADMAP M140.3) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Superfície BM25 own-code operante (AM ou bm25_search) com cache do Directory — latência não paga reload-por-query | T1.1, T2.1, T2.2, T3.1 | `IndexCache` + `bm25_build`/`bm25_search`; medido cache < 50% reload |
| 2 | Bate a baseline do M138 (pg_textsearch) em nDCG@10 no mesmo corpus, forma final | T3.1 | nDCG@10 da produção ≥ baseline M138 (BEIR, harness reusado) |
| 3 | ADR-2 supersede a exceção do ADR-0013 + plano de saída do pg_textsearch | T3.2 | ADR-0054 |
| 4 | MVCC-correto (o cache não serve versão errada sob snapshot) | T1.1, T2.2 | geração sob snapshot + pg-test MVCC (leitor snapshot antigo) |

**Coverage: 4/4 gaps covered (100%)**

## Global Definition of Done

- [ ] Todas as fases completas.
- [ ] `cd theodb_rs && cargo test -p theodb_lexical` exit code 0 (cache pgrx-free).
- [ ] `cargo tree -p theodb_lexical | grep -c pgrx` retorna 0.
- [ ] (e2e-runner) `cargo pgrx test --features "pg18 spike-lexical pg_test"` verde (build + search + MVCC + cache-hit).
- [ ] `cargo check --features "pg18 spike-lexical"` + `cargo build` (default) exit code 0.
- [ ] `cargo clippy --features "pg18 spike-lexical" -- -D warnings` (baseline M136) limpo.
- [ ] Benchmark: `latency_cache_ms < 0.5 * latency_reload_ms` E nDCG@10 ≥ baseline M138.
- [ ] File-size budget respeitado (cache ≤ 120, engine ≤ 400 LoC; ADR ≤ 500).
- [ ] CHANGELOG.md atualizado sob `[Unreleased]`.
- [ ] Backward compatibility — o núcleo continua pgrx-free; o spike (`lexical_spike_*`) intacto; M140.2 verde.
- [ ] Plan-specific: ADR-0054 + report escritos a partir dos números reais.
- [ ] Plan archived após merge.

## Failure scenarios (I/O external)

O `bm25_build`/`bm25_search` falam com o Postgres (SPI) e o `bm25_build` lê uma tabela do usuário.

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| PostgreSQL (SPI) | tabela/coluna inexistente no `bm25_build` | pg-test com nome inválido | erro tipado (`error!`) claro, sem crash do backend; nada persistido (txn aborta) |
| PostgreSQL (SPI) | `bm25_search` sobre index_id sem build (geração ausente) | pg-test | 0 linhas (não erro) — índice vazio é um estado válido |
| Heap `lexical_files` | flush parcial interrompido (crash mid-build) | (M140.4 prova a fundo com crash real) | a txn do build aborta atômica → nenhuma geração nova visível; o snapshot vê o índice antigo |

## Final Phase: Integration Validation (MANDATORY)

**Objective:** validar a engine ponta-a-ponta no toolchain pgrx real.

### Execution
```
cd theodb_rs
cargo test -p theodb_lexical                                    # cache pgrx-free (stock)
cargo tree -p theodb_lexical | grep -c pgrx                     # 0
# no e2e-runner (pgrx 0.19 + PG18):
cargo pgrx test --features "pg18 spike-lexical pg_test"         # build + search + MVCC + cache-hit
cargo check --features "pg18 spike-lexical" && cargo build      # cdylib + default
cargo clippy --features "pg18 spike-lexical" --no-deps -- -D warnings ...   # M136
cd ../benchmarks && python3 run_m140_3_engine.py --out ../docs/benchmarks/m140-3-data/result.json
```

### Acceptance Criteria
- [ ] `cargo test -p theodb_lexical` verde (cache).
- [ ] `cargo pgrx test` verde (build/search/MVCC/cache-hit) no box.
- [ ] `cargo check` spike+default + clippy RC=0.
- [ ] Benchmark: cache < 50% reload + nDCG ≥ M138 (números reais).
- [ ] Failure scenarios exercidos (tabela inexistente → erro tipado; index vazio → 0 linhas).

### If Validation Fails
1. Separar falhas do plano vs pré-existentes.
2. Corrigir as do plano (o cache MVCC é o ponto de maior risco — priorizar o test de snapshot antigo).
3. Re-rodar a cadeia.
