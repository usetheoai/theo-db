---
slug: ambuild-streaming
milestone_id: M89
created_at: 2026-07-12
goal: "Reescrever o ambuild do theodb_ivfflat para build streaming via tuplesort, pico bounded por maintenance_work_mem, medido ≤1.5× base a 30M."
---

# Plano — M89 ambuild streaming (tuplesort/spool, bounded-memory IVF build)

## Goal

Reescrever o `ambuild` do `theodb_ivfflat` para **build streaming via `tuplesort_begin_heap`** (nunca materializar o corpus inteiro em RAM), com **pico de anon-rss ≤ ~1.5× o dataset base MEDIDO num build de 30M num box de 64 GB** (o cenário que OOMou 2× no M88), **zero regressão** (249 pg_tests GREEN + recall byte-idêntico a ≤1M) e **on-disk format inalterado** (sem magic bump / sem REINDEX).

## Context

Origem: achado MEDIDO do M88 (`docs/benchmarks/m88-billion-scale-verdict.md`, `docs/adr/0038-m88-billion-scale-regime-verdict.md`) — o ambuild pica ~4× o base (2 OOM-kills a 30M). Blueprint DISCOVER: `knowledge-base/discoveries/blueprints/ambuild-streaming-blueprint.md` (deep research code+web-grounded contra `knowledge-base/references/pgvector/src/ivfbuild.c` + verificação dos bindings pgrx 0.16.1 pg17). Grill: `knowledge-base/grills/ambuild-streaming-feature-grill.md`.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/am/build.rs` | 1153 | `d601513` (2026-07-12) | `ambuild`/`ambuild_hnsw`, `collect_corpus` (`:46`), buffers AQ/SQ8 (`:126,:135,:137`), vacuum rebuild, ambuildempty | `extern "C-unwind"` boundary (pânico não atravessa C); on-disk page format v3/v4/v5/v6 inalterado; crash-safety via GenericXLog |
| `theodb_rs/src/ann/ivf.rs` | 404 | `fba16d0` (2026-07-12) | `IvfflatIndex::build` (kmeans++, assign_all_parallel `:59`, list_entries, with_soar_spill `:88`); clona corpus→self.vectors `:28` | recall byte-idêntico a ≤1M; `assign_all_parallel` determinístico |
| `theodb_rs/src/am/page.rs` | 1723 | `6229d1a` (2026-07-12) | writers de página v5/v6 (`write_ivf_aq_split`/`_split_sq8`) que o streaming alimenta por-lista | formato on-disk byte-idêntico; GenericXLog durável |
| `benchmarks/m89_membench.py` (NEW) | 0 | — | (a criar) harness de medição de pico de RSS old vs new | — |
| `docs/benchmarks/m89-ambuild-streaming.md` (NEW) | 0 | — | (a criar) artefato de evidência | — |
| `docs/benchmarks/m89-ambuild-streaming.json` (NEW) | 0 | — | (a criar) dados brutos | — |

Todo arquivo em qualquer `#### Files to edit` abaixo aparece nesta tabela.

### Current callers / dependents

- **Symbol:** `IvfflatIndex::build(corpus: &[(i64,Vec<f32>)], lists, metric, seed)` in `theodb_rs/src/ann/ivf.rs:26`
- **Callers (production):** `theodb_rs/src/am/build.rs:102` (ambuild — corpus GRANDE, o alvo), `theodb_rs/src/am/build.rs:533` (vacuum rebuild), `theodb_rs/src/am/build.rs:560` (ambuildempty), `theodb_rs/src/sbq.rs:211` (carrier pequeno), `theodb_rs/src/pq.rs:216` (carrier pequeno), `theodb_rs/src/ann/ivf_aqah.rs:50` (carrier pequeno), `theodb_rs/src/ann/ivf.rs:245` (rebuild interno).
- **Callers (tests):** `theodb_rs/src/ann/ivf.rs:381,391,398`.
- **External (public API consumed by other repos):** no — `pub(crate)`.

### Domain glossary

- **tuplesort** — external merge sort do Postgres; spilla p/ temp files, pico bounded por `maintenance_work_mem`.
- **virtual slot** — `TupleTableSlot` preenchido via `tts_values`/`tts_isnull` + `ExecStoreVirtualTuple`.
- **list#** — índice da lista IVF (a sort key do build streaming).
- **carrier build** — uso de `IvfflatIndex::build` sobre um corpus pequeno (sbq/pq/ivf_aqah) só p/ obter centróides, não p/ persistir um índice grande.
- **corpus** — `Vec<(i64 heap-tid, Vec<f32> vector)>` de todos os tuplos vivos não-NULL.

### Architecture boundaries affected

- Cruza a fronteira C do Postgres (`am/build.rs` `extern "C-unwind"` ↔ `pg_sys` tuplesort/slot) — mesma direção já existente (o `collect_corpus` já chama `pg_sys::table_index_build_scan`). Não introduz nova camada; mantém o AM como adapter sobre o core PG (per `architecture.md`, DIP: domínio define, adapter satisfaz). A lógica de algoritmo (`ann/ivf.rs`) permanece pura; a orquestração streaming vive em `am/build.rs` (composition root do build).

## Prior Art & Related Work

- **Internal blueprint** — `knowledge-base/discoveries/blueprints/ambuild-streaming-blueprint.md` (o pipeline tuplesort, feasibility pgrx, decisão A+B).
- **Reference project** — pgvector `knowledge-base/references/pgvector/src/ivfbuild.c:162,216,272,614,1024` (o pipeline sample→assign-into-tuplesort→sort→stream-write; PostgreSQL License, permissiva).
- **External** — Postgres `tuplesort.h`/`tuplesortvariants.h` (REL_17): `tuplesort_begin_heap` API; bindings confirmados em `pgrx-pg-sys-0.16.1/src/include/pg17.rs`.
- **M88 Phase 1** (`fba16d0`) — kmeans-train sampling + parallel assignment, a base do streaming.

## Objective

Trocar o build "coleta-tudo-em-RAM" por um pipeline streaming espelhando o pgvector, mantendo formato/recall e derrubando o pico de ~4× para O(maintenance_work_mem).

## ADRs

### D1 — tuplesort_begin_heap vs disk-spill caseiro
Decisão: usar o `tuplesort` do Postgres. Rationale: é o mecanismo que pgvector/btree usam; FFI confirmada disponível (`pgrx-pg-sys-0.16.1/src/include/pg17.rs`). Alternatives considered: (a) temp-file manual — REJEITADO: reinventa external merge sort (Regra 9); (b) só clone-elimination — REJEITADO: não torna 100M construível, falha o propósito da linhagem M89. Consequences: habilita 100M+ em RAM commodity; custo = `unsafe` bounded p/ virtual slot (`ExecClearTuple` inline → escrever `tts_values` direto).

### D2 — fast-path in-RAM preservado p/ N pequeno
Decisão: quando o corpus cabe folgado em `maintenance_work_mem`, usar o build in-RAM atual (byte-idêntico). Rationale: garante testes ≤1M + benchmarks 1M byte-idênticos (zero regressão) e não regride latência de build pequeno. Alternatives considered: streaming sempre — REJEITADO: regride latência a ≤1M sem ganho. Consequences: dois paths com seleção explícita por N.

### D3 — SOAR e parallel-workers fora de escopo
Decisão: M89 = f32(v5)+SQ8(v6) serial. Rationale: SOAR (`ivf.rs:88`) precisa do residual por-vetor (2ª olhada) → não cabe no single-stream barato; parallel-workers (`Sharedsort`) é otimização. Alternatives considered: incluir SOAR agora — REJEITADO: dobra o escopo + risco. Consequences: `soar_lambda>0` força path in-RAM com WARN explícito (nunca resultado errado silencioso); SOAR-streaming vira follow-up.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| FFI do virtual slot (`ExecClearTuple`/`slot_getattr` inline) exige `unsafe` manual | High | Espelhar `ivfbuild.c` exato + pg_test roundtrip (T2.1) + sign-off council-rust-pgrx | implementador |
| Regressão de tempo de build a ≤1M (spill em disco mais lento) | Medium | Fast-path in-RAM (T2.4) — streaming só p/ N grande | implementador |
| SOAR incompatível com o single-stream | Medium | D3: path explícito + WARN + follow-up documentado | implementador |
| Validação exige droplet 64GB (~$0.50/h, ~1-2h) | Low | Aceito — DoD exige evidência medida; destruir o droplet ao fim | operador |

## Unresolved Questions

- Q1 — O threshold exato do fast-path (`N·base_bytes` vs `maintenance_work_mem`) — a calibrar empiricamente em T2.4/T3.1; não bloqueia o design (default conservador: streaming quando `N·dim·4 > 0.5·maintenance_work_mem`).
- Q2 — O `bytea` payload do vetor no tuplo de sort deve ser o f32 cru ou já o código AQ? (residual: definir em T2.2 — provável f32 cru p/ manter o AQ-encode no stream-write, evitando re-decode.)

## Dependency Graph

```
Phase 1 (clone-elimination, sem FFI, un-regride hoje)
      │
      ▼
Phase 2 (tuplesort streaming — o milestone)
      │
      ▼
Phase 3 (validação droplet 30M + benchmark — evidência do DoD)
```

## Phase 1: Clone-elimination (borrow/move o corpus; sem FFI)

### T1.1 — `IvfflatIndex::build` consome o corpus (move) em vez de clonar

#### Objective
Eliminar a cópia interna `self.vectors = corpus.iter().map(clone)` (`ivf.rs:28`) movendo os vetores do corpus quando o caller pode ceder posse.

#### Why this step (action + reasoning)
Ação: adicionar `IvfflatIndex::build_owned(corpus: Vec<(i64,Vec<f32>)>, ...)` que faz `into_iter()` movendo `v` (sem clone); `build(&corpus,...)` existente delega clonando (mantém os carriers sbq/pq/ivf_aqah pequenos inalterados). Raciocínio: o clone #2 (16 GB @30M) é a cópia mais barata de matar (cita Baseline "Current callers": só o ambuild `build.rs:102` tem corpus grande + cede posse; os outros são carriers pequenos onde clonar é negligível). Preserva a assinatura pública `pub(crate)`.

#### Files to edit
`theodb_rs/src/ann/ivf.rs` (`:26-28` — novo `build_owned`, `build` delega), `theodb_rs/src/am/build.rs:102` (usa `build_owned(corpus)` movendo o corpus).

#### Pseudo-code / Signatures
```rust
pub(crate) fn build_owned(corpus: Vec<(i64, Vec<f32>)>, lists: usize, metric: Metric, seed: u64) -> Self {
    let (ids, vectors): (Vec<i64>, Vec<Vec<f32>>) = corpus.into_iter().unzip(); // move, no clone
    Self::from_parts(ids, vectors, lists, metric, seed)
}
pub(crate) fn build(corpus: &[(i64, Vec<f32>)], ...) -> Self { Self::build_owned(corpus.to_vec(), ...) } // carriers
```

#### TDD
Teste `ivfflat_build_owned_byte_identical` (RED primeiro): `build_owned(corpus.clone())` produz `to_bytes()` **byte-idêntico** a `build(&corpus)` sobre o mesmo corpus fixo. Given corpus determinístico / When ambos os paths / Then `assert_eq!(a.to_bytes(), b.to_bytes())`.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
- 249 pg_tests GREEN + o novo teste; recall byte-idêntico ≤1M.
- `build.rs:102` não mantém o corpus vivo após `build_owned` (o corpus é movido).

#### DoD
`cargo pgrx test` (pg17) GREEN no droplet; `assert_eq` byte-idêntico passa.

### T1.2 — SQ8 encoda do corpus emprestado (deleta `corpus_vecs`)

#### Objective
Remover o 3º clone (`build.rs:135` `corpus_vecs`) no path v6/SQ8.

#### Why this step (action + reasoning)
Ação: o encode SQ8 lê do corpus já disponível (a `IvfflatIndex` retém os vetores movidos em T1.1), não de um clone novo. Raciocínio: o clone #3 (16 GB @30M no v6) é redundante — os vetores já existem no índice construído; expor um acessor `idx.vector_at(ord)` remove a cópia.

#### Files to edit
`theodb_rs/src/am/build.rs:135-146` (encoda de `idx`/`&corpus`, deleta `corpus_vecs`), `theodb_rs/src/ann/ivf.rs` (acessor `vector_at` se necessário).

#### TDD
Teste `sq8_build_no_redundant_clone_byte_identical`: v6 build `to_bytes()` byte-idêntico ao atual sobre um corpus fixo (o clone era puro overhead, resultado inalterado).

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
v6 recall byte-idêntico; pico teórico cai de ~3× p/ ~2× base (medido em T3.1).

#### DoD
Teste GREEN no droplet; nenhum `corpus_vecs` remanescente (`grep` limpo).

## Phase 2: tuplesort streaming build (o milestone)

### T2.1 — FFI wrapper seguro do tuplesort

#### Objective
Um módulo `stream` que expõe put/sort/get ordenado por list# com payload íntegro.

#### Why this step (action + reasoning)
Ação: `tuplesort_begin_heap` com tupdesc `{list# INT4, tid INT8, vec BYTEA}`, put via virtual slot (`tts_values` + `ExecStoreVirtualTuple`), `performsort`, `gettupleslot`, `end`. Raciocínio: espelha `ivfbuild.c:614/216/1024`; o `unsafe` fica isolado + comentado num só módulo (SRP), testável por roundtrip antes de tocar o build.

#### Files to edit
`theodb_rs/src/am/build.rs` (novo `mod stream`).

#### TDD
`tuplesort_roundtrip_sorts_by_list` (pg_test — precisa do backend PG): put `(list#, tid, vec)` fora de ordem → performsort → get → volta ordenado por list# ascendente, tid+vec íntegros. `assert` ordem + payload.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
Roundtrip GREEN; nenhum leak de slot/tupdesc (end libera).

#### DoD
pg_test GREEN no droplet.

### T2.2 — Streaming assignment na callback do heap scan

#### Objective
Atribuir cada vetor à lista inline e empurrar p/ o sorter, sem acumular o corpus.

#### Why this step (action + reasoning)
Ação: a callback do `table_index_build_scan` computa nearest-centroid inline (read-only sobre os centróides pequenos) e `stream.put(list#, tid, vec)`, substituindo `state.corpus.push` (`build.rs:281`). Raciocínio: a atribuição é per-vetor-independente (cita blueprint Q4) → não precisa do corpus em RAM; espelha `AddTupleToSort` (`ivfbuild.c:162`).

#### Files to edit
`theodb_rs/src/am/build.rs` (callback + o loop de kmeans sample-train sobre um sample bounded, não sobre o corpus inteiro).

#### TDD
`streaming_build_scans_identical_forced_spill` (pg_test): `SET maintenance_work_mem='1MB'` força external merge num corpus que excede o budget; recall do index-scan == exact seqscan top-k (correto sob spill).

#### Failure scenarios (ver § global)
`maintenance_work_mem` mínimo → external merge → resultado correto; erro de I/O do temp-file do tuplesort → propaga (fail-fast, sem índice parcial).

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
Recall correto sob spill forçado; pico do backend não cresce com N (observável no teste via VmHWM em T3.1).

#### DoD
pg_test GREEN no droplet.

### T2.3 — Stream-write das páginas por-lista (performsort → get → page::write)

#### Objective
Ler ordenado por list#, empacotar AQ/SQ8 por lista, escrever via os writers existentes.

#### Why this step (action + reasoning)
Ação: após `performsort`, iterar `gettupleslot` agrupando por list#, empacotar códigos por lista e chamar `page::write_ivf_*`. Raciocínio: só uma lista "em voo" (`ivfbuild.c:272`); reusa os writers v5/v6 → formato on-disk byte-idêntico → sem magic bump.

#### Files to edit
`theodb_rs/src/am/build.rs` (stream-write), reuso de `theodb_rs/src/am/page.rs` (writers inalterados).

#### TDD
`streaming_build_restart_scan_identical` (pg_test): build streaming → re-abre o índice (simula restart) → scan retorna idêntico (crash-safety; páginas GenericXLog-durables inalteradas).

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
Layout on-disk byte-idêntico ao build in-RAM p/ o mesmo corpus (mesmo v5/v6); scan pós-restart idêntico.

#### DoD
pg_test GREEN; `grep` confirma nenhum magic/version novo.

### T2.4 — Fast-path in-RAM p/ N pequeno + seleção de path

#### Objective
Rotear N pequeno p/ o build atual (byte-idêntico) e N grande p/ o streaming; SOAR força in-RAM.

#### Why this step (action + reasoning)
Ação: `if streaming_needed(n, mwm, soar_lambda) { stream } else { inram }` com `streaming_needed = soar_lambda==0 && n*dim*4 > 0.5*mwm`. Raciocínio: D2 (byte-identidade ≤1M) + D3 (SOAR in-RAM). Preserva os 249 testes + benchmarks 1M.

#### Files to edit
`theodb_rs/src/am/build.rs` (branch de seleção + WARN no path SOAR).

#### TDD
`small_n_uses_inram_path_byte_identical` (pg_test): N pequeno usa in-RAM byte-idêntico; N grande (com mwm baixo) usa streaming; `soar_lambda>0` usa in-RAM + emite WARN.

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
Todos os 249 testes + benchmarks 1M inalterados (fast-path); WARN observável no path SOAR.

#### DoD
Suite completa GREEN no droplet; byte-idêntico ≤1M confirmado.

## Phase 3: Validation (droplet) + benchmark

### T3.1 — Medição de pico de memória old vs new (16M, 30M)

#### Objective
Provar o DoD: build de 30M ≤ ~1.5× base num box de 64 GB (old-build OOMa).

#### Why this step (action + reasoning)
Ação: harness que mede o pico de anon-rss do backend durante `CREATE INDEX` via `/proc/<backend_pid>/status VmHWM` (ou cgroup `memory.peak`), old-build vs new-build, a 16M e 30M. Raciocínio: a medição empírica é a única evidência aceita (Regra 5); reproduz o cenário exato do M88 (30M/62GB).

#### Files to edit
`benchmarks/m89_membench.py` (NEW), `docs/benchmarks/m89-ambuild-streaming.md` (NEW), `docs/benchmarks/m89-ambuild-streaming.json` (NEW).

#### TDD
N/A (medição empírica — não é lógica de negócio testável por unit; a corretude do build é coberta por T2.2/T2.3).

#### Concurrency tests
(none — single-threaded)

#### Acceptance Criteria
**Build de 30M completa com pico ≤ ~1.5× base (~23 GB) MEDIDO num box 64 GB; o build antigo OOMa (reproduz M88).** Pico do streaming ≈ O(maintenance_work_mem), não escala ~4× (curva pico-vs-N plana).

#### DoD
`docs/benchmarks/m89-*.{md,json}` com VmHWM old vs new a 16M/30M; sign-off council-benchmark.

## Coverage Matrix

| # | Gap / Requirement | Task(s) | Resolution |
|---|---|---|---|
| 1 | Pico ≤1.5× base MEDIDO a 30M num box 64GB | T3.1 | Harness VmHWM old vs new; 30M completa ≤~23 GB |
| 2 | Streaming via tuplesort (bounded por maintenance_work_mem) | T2.1, T2.2, T2.3 | FFI wrapper + assign-into-sorter + stream-write |
| 3 | Zero regressão (249 tests + byte-idêntico ≤1M) | T1.1, T1.2, T2.4 | build_owned byte-idêntico + fast-path in-RAM |
| 4 | Format on-disk inalterado (sem REINDEX) | T2.3 | reusa writers v5/v6; sem magic bump |
| 5 | maintenance_work_mem respeitado | T2.1, T2.2, T3.1 | workMem = mwm; spill forçado testado |
| 6 | Sem novas deps (tuplesort é core PG) | T2.1 | wrapper usa só `pg_sys` (D1, Regra 9); nenhuma crate nova |
| 7 | Evidência benchmark (pico vs N, old vs new) | T3.1 | m89-*.{md,json} |
| 8 | Sign-off council-rust-pgrx + index-storage + benchmark | T2.1, T3.1 | FFI revisada (T2.1) + medição revisada (T3.1) |
| 9 | SOAR não regride (path explícito) | T2.4, D3 | soar_lambda>0 → in-RAM + WARN |

**Coverage: 9/9 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `cargo pgrx test` (pg17) green (249 + novos testes de streaming)
- [ ] Zero type errors / lint — `cargo clippy` clean
- [ ] File-size budget respeitado (cada arquivo tocado ≤ ~600 LoC de delta; `unsafe` isolado)
- [ ] CHANGELOG.md atualizado sob `[Unreleased]` (Regra 6)
- [ ] Backward compatibility — formato on-disk inalterado (sem REINDEX); recall byte-idêntico ≤1M
- [ ] Plan-specific: build 30M ≤~1.5× base MEDIDO (old-build OOMa) — `docs/benchmarks/m89-*.{md,json}`
- [ ] Streaming scan-identical + correto sob spill forçado
- [ ] Sign-off council-rust-pgrx (FFI) + council-index-storage (build/page) + council-benchmark (medição)
- [ ] Plan archived após `/review` READY_TO_MERGE + PR merged

## Failure scenarios (I/O externo: Postgres tuplesort temp-files + heap scan)

- **tuplesort spill (maintenance_work_mem excedido):** external merge → resultado correto — T2.2 (`SET maintenance_work_mem='1MB'`).
- **temp-file I/O error do tuplesort:** propaga como erro tipado (fail-fast), nunca índice parcial silencioso — asserção em T2.2.
- **restart pós-build:** scan idêntico (páginas GenericXLog-durables) — T2.3.

## Final Phase: Integration Validation (MANDATORY)

### Execution
`cargo pgrx test` (pg17) completo no droplet; harness m89_membench a 16M/30M; A/B same-data byte-idêntico ≤1M; spill-forced + restart-scan-identical.

### Acceptance Criteria
Suite GREEN; 30M ≤~1.5× base MEDIDO; formato inalterado; 3 sign-offs de council.

### If Validation Fails
Loop de volta ao `/implement` (validation halt-loop); nunca emitir completude sobre falha (Regra 3).
