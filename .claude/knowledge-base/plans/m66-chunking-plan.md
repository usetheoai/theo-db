---
slug: m66-chunking
milestone_id: M66
created_at: 2026-07-09
goal: Entregar chunking declarativo own-code (fixed/sentence/recursive+overlap) no vectorizer com chunk-table opt-in e medir recall@k por estratégia em BEIR.
---

# M66 — Estratégias de chunking declarativas no vectorizer

## Goal

Entregar um chunker own-code (`chunk.rs`: fixed/sentence/recursive + overlap, Unicode-safe) wireado no vectorizer via modo opt-in `WITH (chunk_strategy=…, chunk_size=…, overlap=…)` (1 doc → N chunks → N embeddings numa chunk-table), e **medir** o recall@k/nDCG@10 por estratégia em BEIR, com o veredito em `docs/benchmarks/m66-chunking.{md,json}` (gate: recall por estratégia medido; honest-negative se uma estratégia não move o recall).

## Context

A discovery (`.claude/knowledge-base/discoveries/blueprints/m66-chunking-blueprint.md`, R0 web-citado) concluiu: (a) implementar fixed/sentence/recursive + overlap; **deferir semantic** com ADR honest-negative (arXiv:2410.13070 — ganho 0-4pp, frequentemente negativo, 14× custo); (b) o **recall-mover real é o schema 1-doc→N-chunks** — hoje o vectorizer é 1→1 in-place (`vectorizer.rs:361`), e `theodb.chunk_text` (`:120`) é **código morto**; (c) chunking é lógica de string (own-code, char-based v1, sem tokenizer pesado; `sentence` delega segmentação Unicode a dep permissiva). O gate real é o benchmark de recall por estratégia (o `chunk_text` morto não-medido é o sintoma a evitar).

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Último commit | Papel | Ação M66 |
|---|---|---|---|---|
| `theodb_rs/src/chunk.rs` | 0 (NEW) | — | chunker Rust puro (fixed/sentence/recursive + overlap) | criar (~180 LoC) |
| `theodb_rs/src/lib.rs` | ~170 | `604184b` | module root + pg_test | +`mod chunk;` |
| `theodb_rs/src/vectorizer.rs` | ~906 | — | vectorizer M54 (catálogo, worker, chunk_text morto) | +colunas catálogo + chunk-table mode + wire chunk.rs |
| `theodb_rs/src/api.rs` | ~750 | — | superfície pg_extern | +`_chunk_text` (a superfície SQL do chunker) |
| `sql/theodb--1.4--1.5.sql` | 0 (NEW) | — | migração (ALTER theodb.vectorizer + chunk cols) | criar |
| `benchmarks/run_m66_chunking.py` | 0 (NEW) | — | benchmark recall@k por estratégia (BEIR) | criar |
| `benchmarks/tests/test_run_m66_chunking.py` | 0 (NEW) | — | aritmética (doc-recall from chunk-hits, verdict) | criar |
| `docs/benchmarks/m66-chunking.{md,json}` | 0 (NEW) | — | relatório + dados | criar (no droplet) |
| `docs/adr/0025-m66-chunking-strategies.md` | 0 (NEW) | — | ADR (estratégias, chunk-table opt-in, defer semantic) | criar |
| `CHANGELOG.md` | — | — | `[Unreleased] § Added` | editar |

### Current callers / dependents

- Vectorizer worker: `vectorizer.rs:516-665` (`theodb_embed_worker_main`); per-job upsert `_vectorizer_process_upsert` (`:347-369`, o `embed::run` 1→1 `:360`); batch `:392-429` (`embed::run_batch` `:413`).
- Catálogo: `theodb.vectorizer` (`vectorizer.rs:25-35`); `lookup_config` (`:304-327`); `create_vectorizer` (`:86-94`).
- Chunk morto a substituir/reusar: `theodb.chunk_text` (`vectorizer.rs:120-144`, plpgsql fixed+overlap — só nos testes `:842-861`).
- Embed a reusar: `embed::run_batch` (`embed.rs:55` — N chunks → N vetores num round-trip).
- Migração precedente: `sql/theodb--1.3--1.4.sql`.

### Domain glossary

- **chunk** — um fragmento de um documento; a unidade que é embedada e indexada.
- **chunk_strategy** — `fixed` (corte por N chars) | `sentence` (agrupa sentenças até size) | `recursive` (hierarquia de separadores `\n\n`→`\n`→`. `→` `).
- **overlap** — N chars repetidos entre chunks adjacentes (contexto nas bordas; ~10-20%).
- **chunk-table** — tabela separada `{target}_chunks (source_pk, chunk_index, chunk_text, embedding)` — 1 doc → N linhas.
- **doc-recall** — no BEIR os qrels são por-doc; um doc é "recuperado" se QUALQUER chunk dele está no top-k.
- **k-adaptativo** — igualar o budget de contexto entre estratégias (`k ≈ target_tokens / avg_chunk_size`).

### Architecture boundaries affected

- `chunk.rs` é domínio puro (lógica de string, sem I/O) — a camada mais testável. Sem fronteira DIP nova.
- O modo chunk-table é **opt-in** (`WITH chunk_strategy`) — o modo 1→1 in-place é preservado (não-breaking).

## Prior Art & Related Work

- Blueprint M66 (R0): `.claude/knowledge-base/discoveries/blueprints/m66-chunking-blueprint.md`.
- pgai chunking: https://github.com/timescale/pgai/blob/released/docs/utils/chunking.md
- LangChain RecursiveCharacterTextSplitter: https://docs.langchain.com/oss/python/integrations/splitters/recursive_text_splitter
- "Is Semantic Chunking Worth the Cost?" (Qu 2024): arXiv:2410.13070
- `benbrandt/text-splitter` (Rust, MIT): https://github.com/benbrandt/text-splitter (avaliado; v1 own-code char-based)
- Vectorizer M54: `theodb_rs/src/vectorizer.rs`; ADR `docs/adr/0016`.

## ADRs

### ADR-0025-a — Chunker own-code char-based (fixed/sentence/recursive + overlap); NÃO adotar tokenizer no v1

**Decisão:** `chunk.rs` Rust puro, char-based (fronteira de char UTF-8), com fixed/sentence/recursive + overlap. `sentence` usa split em `.!?` boundaries char-safe (v1 simples); NÃO adotar tiktoken/BPE nem `text-splitter` inteiro no v1.

**Rationale:** parsimony — char-based resolve o caso comum (pgai também começou char-based); token-based (tiktoken) é complexidade acidental para o v1. Rust `String` UTF-8 garante char-boundary (nunca corta multibyte). Own-code justifica-se porque a API SQL/reloptions do TheoDB precisa de controle que o crate não dá diretamente; o diff é lógica de string.

**Alternativas rejeitadas:**
- **(A) Adotar `text-splitter` (MIT) inteiro** — resolve sentence/recursive/Unicode/tokens de uma vez, mas traz uma dep + a API do crate não casa 1:1 com os reloptions; para o v1 char-based own-code é menor diff e sem dep nova. (Reavaliar em v2 se token-based for pedido.)
- **(B) Token-based (tiktoken-rs)** — mais correto (o gerador tem budget em tokens) mas exige o tokenizer BPE; complexidade acidental para o v1. Deferido.

### ADR-0025-b — Modo chunk-table OPT-IN (não-breaking); semantic DEFERIDO por evidência

**Decisão:** o chunking é opt-in via `WITH (chunk_strategy=…)`; escreve numa chunk-table separada `{target}_chunks`. O modo 1→1 in-place atual é preservado (default). `semantic` NÃO é implementado.

**Rationale:** mudar in-place → chunk-table é breaking no query contract (retrieval passa a join/agregar sobre chunks); torná-lo opt-in preserva os vectorizers existentes. Semantic deferido por evidência (arXiv:2410.13070 — ganho não-universal, custo alto), não por falta de tempo.

**Alternativas rejeitadas:**
- **(A) Trocar o modo in-place por chunk-table (breaking)** — quebraria os vectorizers/queries existentes; rejeitada (opt-in é KISS + não-breaking).
- **(B) Implementar semantic** — 0-4pp recall, frequentemente negativo, 14× custo (evidência). Rejeitada.

## Dependency Graph

```
Phase 1 (chunk.rs + superfície + pg_test)  ──→ Phase 2 (chunk-table mode no vectorizer + migração + pg_test)  ──┐
                                                                                                                 ├─→ Phase 3 (benchmark recall por estratégia)
                                                                                                                 │
                                                                                       Phase 3 ──→ Phase 4 (ADR + integration)
```

## Phase 1 — `chunk.rs` (chunker Rust puro) + superfície + pg_test offline

### T1.1 — `chunk.rs`: fixed/sentence/recursive + overlap, Unicode-safe, typed errors

#### Why this step
**Ação:** criar `theodb_rs/src/chunk.rs` com `chunk(text, strategy, size, overlap) → Vec<String>`: `fixed` (janelas de `size` chars com `overlap`), `sentence` (agrupa sentenças `.!?`-delimitadas até `size`), `recursive` (hierarquia `\n\n`→`\n`→`. `→` `, força char-cut no fim). Edge: vazio→`[]`, doc<size→1 chunk, palavra gigante→char-cut. Negative: `overlap>=size`→err_input, `size<=0`→err_input, strategy desconhecida→err_input. Char-boundary UTF-8 sempre.
**Raciocínio:** o chunking DOMINA o recall do RAG; a lógica de string é o own-code central e unit-testável offline (o antídoto ao `chunk_text` morto não-medido). Char-based é rung-1 (blueprint).

#### Files to edit
- `theodb_rs/src/chunk.rs` (NEW, ~180 LoC)
- `theodb_rs/src/lib.rs` (+`mod chunk;` + pg_test)

#### Deep file dependency analysis
Reusa `pg.rs::err_input` (`pg.rs:8`) para os typed errors. Rust `String`/`char_indices` garante char-boundary (nunca corta multibyte). Sem I/O — domínio puro.

#### TDD
- RED: `test_fixed_windows_overlap` (janelas+overlap corretos), `test_recursive_prefers_paragraph_then_sentence`, `test_sentence_groups_until_size`, `test_empty_returns_no_chunks`, `test_doc_smaller_than_size_single_chunk`, `test_giant_word_forces_char_cut` (sem loop infinito, nenhum chunk>size), `test_multibyte_never_splits_char` (emoji/CJK — cada chunk é UTF-8 válido), `test_overlap_ge_size_rejected` (err_input), `test_size_zero_rejected`, `test_unknown_strategy_rejected`. Falham antes.
- GREEN: implementar as 3 estratégias + validação.
- REFACTOR: extrair o helper de char-boundary window.

#### Concurrency tests
(none — single-threaded) — chunking é função pura sem estado compartilhado.

#### Acceptance criteria
- `cargo pgrx test pg17 chunk` → todos GREEN.
- Todo chunk é UTF-8 válido (multibyte nunca cortado); nenhum chunk > size (exceto palavra atômica indivisível, documentada); vazio→`[]`.
- `overlap>=size`/`size<=0`/strategy desconhecida → err_input tipado.

#### DoD
- [ ] pg_test GREEN; edge + negative cobertos (testing.md §4.1); char-boundary provado com multibyte.

### T1.2 — Superfície `theodb.chunk(text, strategy, size, overlap)` + deprecar `chunk_text` morto

#### Why this step
**Ação:** `#[pg_extern] _chunk_text` → `crate::chunk::chunk` + wrapper SQL `theodb.chunk(content text, strategy text DEFAULT 'recursive', chunk_size int DEFAULT 512, overlap int DEFAULT 64) RETURNS text[]`. Substituir o `theodb.chunk_text` plpgsql morto (`vectorizer.rs:120`) por este (KISS — um só chunker).
**Raciocínio:** expõe o chunker como superfície testável + reusável pelo vectorizer; elimina o código morto (Rule: dead code).

#### Files to edit
- `theodb_rs/src/api.rs` (+`_chunk_text` + wrapper)
- `theodb_rs/src/vectorizer.rs` (remover o `chunk_text` plpgsql morto + seus testes SPI)

#### Deep file dependency analysis
`RETURNS text[]` (Vec<String>). O `chunk_text` morto (`:120-144`) e seus testes (`:842-861`) removidos; os casos migram para os pg_test de `chunk.rs`.

#### TDD
- RED: `test_theodb_chunk_surface` (chama `theodb.chunk('a. b. c', 'sentence', 4, 0)` e assere os chunks). Falha antes.
- GREEN: a superfície.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- `theodb.chunk(...)` existe, `RETURNS text[]`; o `chunk_text` morto foi removido (grep confirma 0 refs fora dos testes migrados).

#### DoD
- [ ] Superfície GREEN; código morto `chunk_text` eliminado.

## Phase 2 — Modo chunk-table opt-in no vectorizer + migração

### T2.1 — Catálogo + migração + chunk-table + wire chunk.rs no worker

#### Why this step
**Ação:** (a) adicionar `chunk_strategy text, chunk_size int, chunk_overlap int` à `theodb.vectorizer` (`:25-35`) + params default em `create_vectorizer` (`:86`) + `lookup_config` (`:304`); (b) migração `sql/theodb--1.4--1.5.sql` (ALTER ADD COLUMN); (c) quando `chunk_strategy` não-NULL, o worker cria/usa a chunk-table `{target}_chunks (source_pk, chunk_index, chunk_text, embedding)`, chama `chunk::chunk` no conteúdo → N chunks → `embed::run_batch` → N INSERTs (DELETE os chunks antigos do PK antes — evita órfãos); o modo 1→1 in-place é preservado quando `chunk_strategy` é NULL.
**Raciocínio:** é o **recall-mover** (1-doc→N-chunks); opt-in preserva o não-breaking (ADR-0025-b). Reusa `embed::run_batch` (N chunks num round-trip).

#### Files to edit
- `theodb_rs/src/vectorizer.rs` (catálogo + create_vectorizer + lookup_config + worker chunk-table path)
- `sql/theodb--1.4--1.5.sql` (NEW)
- `theodb.control` (default_version 1.4 → 1.5)

#### Deep file dependency analysis
`_vectorizer_process_upsert*` (`:347`,`:392`) ganha o branch chunk-table. `_vectorizer_process_delete` (`:375`) deleta as N linhas do PK no modo chunk. A migração espelha `sql/theodb--1.3--1.4.sql`.

#### TDD
- RED: `test_vectorizer_chunk_mode_writes_n_rows` (um doc com chunk_strategy='fixed' size pequeno → N linhas na chunk-table, chunk_index 0..N-1) — via pg_test com a fila fatiada (sem worker/OpenAI, como os testes M54 `:667`); `test_vectorizer_default_mode_still_in_place` (sem chunk_strategy → 1→1 preservado); `test_reembed_deletes_old_chunks` (UPDATE não acumula órfãos). Falham antes.
- GREEN: o branch chunk-table.
- REFACTOR: extrair o helper de chunk-table upsert.

#### Concurrency tests
(none — single-threaded) — o worker processa a fila sequencialmente (o M54 já é single-worker; sem novo estado concorrente).

#### Failure scenarios
- **embed do chunk falha (HTTP)** — o job vai para dead-letter (o mecanismo M54 existente); os chunks daquele PK não são escritos parcialmente (DELETE+INSERT numa txn). Teste: stub-fail → nenhuma linha órfã.

#### Acceptance criteria
- `cargo pgrx test pg17 vectorizer` → GREEN (chunk mode escreve N linhas; default mode preservado; re-embed não deixa órfãos).
- Migração `theodb--1.4--1.5.sql` aplica ALTER sem erro; `default_version` = 1.5.

#### DoD
- [ ] pg_test GREEN; chunk-table 1-doc→N-chunks; modo default não-breaking; migração presente.

## Phase 3 — Benchmark: recall@k por estratégia (o gate real)

### T3.1 — `run_m66_chunking.py` + rodar em BEIR no droplet

#### Why this step
**Ação:** criar `benchmarks/run_m66_chunking.py` — para cada estratégia (fixed/sentence/recursive) × config (size/overlap): chunkar o corpus BEIR (NFCorpus — pequeno, 3633 docs), embeddar os chunks (OpenAI cache), indexar na chunk-table, retrieval top-k, medir **doc-recall@k** (doc recuperado se qualquer chunk seu ∈ top-k) + nDCG@10, com **k-adaptativo** (igualar budget de contexto). ≥ a estratégia default (recursive) como âncora. Aritmética (doc_recall_from_chunk_hits, verdict) pura e unit-testável.
**Raciocínio:** o gate do DoD é "recall por estratégia" — o antídoto ao chunk_text morto não-medido. Honesto: reportar por-corpus, k-adaptativo (comparação justa).

#### Files to edit
- `benchmarks/run_m66_chunking.py` (NEW, ~250 LoC)
- `benchmarks/tests/test_run_m66_chunking.py` (NEW, ~100 LoC)
- `docs/benchmarks/m66-chunking.{md,json}` (NEW, coletado no droplet)

#### Deep file dependency analysis
Reusa `theodb_bench.beir`/`metrics`/`openai_embed` (M53/M65) + `theodb.chunk` (Phase 1). NFCorpus (menor que SciFact) para tratabilidade.

#### TDD
- RED: `test_doc_recall_any_chunk_hit` (doc recuperado se ≥1 chunk seu no top-k), `test_k_adaptive_equalizes_budget`, `test_strategy_verdict_reports_per_config`, `test_no_improvement_is_honest_negative`. Falham antes.
- GREEN: a aritmética.
- REFACTOR: reusar helpers M65.

#### Concurrency tests
(none — single-threaded).

#### Failure scenarios
- **OpenAI embed falha no setup** → SystemExit tipado (não mede lixo). `THEODB_RERANK_ENDPOINT` n/a (M66 não usa rerank).

#### Acceptance criteria
- `docs/benchmarks/m66-chunking.json` com `per_strategy` (recall@k, nDCG@10, avg_chunk_size, k_adaptive) por config.
- Comparação k-adaptativa (budget igualado); reportado por-corpus.
- council-benchmark: HONESTO (sem cherry-pick; honest-negative se uma estratégia não move).

#### DoD
- [ ] `.json`+`.md` coletados; recall por estratégia medido; council-benchmark HONESTO.

## Phase 4 — ADR + Integration Validation

### T4.1 — ADR-0025 + suite completa + CHANGELOG

#### Why this step
**Ação:** ADR-0025 (estratégias, chunk-table opt-in, defer semantic com evidência). Rodar `cargo pgrx test pg17 chunk vectorizer` + `pytest`. CHANGELOG.
**Raciocínio:** eat-your-own-cooking — M66 completo quando chunker + vectorizer + benchmark passam juntos e o ADR registra o veredito.

#### Files to edit
- `docs/adr/0025-m66-chunking-strategies.md` (NEW)
- `CHANGELOG.md`

#### TDD
- RED: n/a (validação).
- GREEN: suíte verde; ADR + CHANGELOG.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- `cargo pgrx test pg17 chunk vectorizer` GREEN; pytest GREEN.
- ADR-0025 completo (2 decisões + alternativas + veredito benchmark + defer semantic).
- CHANGELOG cita o M66 com o veredito de recall por estratégia.

#### DoD
- [ ] Suíte verde; ADR + CHANGELOG; droplet destruído.

## Coverage Matrix

| DoD (ROADMAP M66) | Task(s) | Evidência |
|---|---|---|
| Chunking configurável no vectorizer (`WITH (chunk_strategy, chunk_size, overlap)`), own-code | T1.1, T1.2, T2.1 | chunk.rs (fixed/sentence/recursive+overlap) + superfície theodb.chunk + modo chunk-table opt-in no vectorizer + migração |
| Benchmark: recall de RAG por estratégia num corpus real → `docs/benchmarks/m66-chunking.{md,json}` | T3.1 | benchmark BEIR/NFCorpus, doc-recall@k/nDCG@10 por estratégia, k-adaptativo |
| Edge/negative: documentos degenerados (vazio, gigante, 1 token) → typed error/handling | T1.1 | pg_test: vazio→[], gigante→chunka, 1-token→1 chunk, palavra gigante→char-cut, multibyte→char-boundary, overlap>=size/size<=0/strategy desconhecida→err_input |

Cobertura: 100% dos 3 bullets do DoD mapeados. Semantic deferido por evidência (ADR-0025-b), não gap.

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| **Chunk-table muda o query contract** (retrieval passa a join/agregar sobre chunks) | ALTA | Modo **opt-in** (`WITH chunk_strategy`); o 1→1 in-place é preservado (não-breaking). ADR-0025-b. | Eng |
| **Uma estratégia pode não mover o recall** (honest-negative) | MÉDIA | O DoD pede medição, não ganho garantido; reportar o delta real por-corpus (a literatura mostra dependência de corpus). | Eng |
| **Benchmark de recall exige re-embeddar chunks** (custo OpenAI × estratégias) | MÉDIA | Corpus pequeno (NFCorpus 3633 docs); cache de embeddings; ≤ 3 estratégias × 2 configs. | Eng |
| **Char-based (não token) pode divergir do budget do gerador** | BAIXA | v1 char-based declarado (ADR-0025-a); token-based é v2 (débito honesto). | Eng |

## Unresolved Questions

- Char-based ou token-based no v1? **Resolvido:** char-based (rung-1, sem tokenizer; ADR-0025-a). Token = v2.
- Corpus do benchmark? **Resolvido:** NFCorpus (BEIR, 3633 docs — menor que SciFact, tratável para re-embeddar chunks por estratégia).
- Implementar semantic? **Resolvido:** NÃO (deferido por evidência, ADR-0025-b).

## Failure scenarios

- **Embed do chunk (HTTP outbound) falha** — no worker, o job vai para dead-letter (mecanismo M54); DELETE+INSERT numa txn evita chunks órfãos parciais (testado em T2.1). No benchmark, falha no setup → SystemExit tipado (T3.1). O chunker (`chunk.rs`) é I/O-puro (sem I/O externo — só string).

## Global Definition of Done

- [ ] `chunk.rs` pg_test (3 estratégias + edge + negative + multibyte) GREEN.
- [ ] Superfície `theodb.chunk` presente; `chunk_text` morto eliminado.
- [ ] Modo chunk-table opt-in: 1-doc→N-chunks; default 1→1 preservado; migração `theodb--1.4--1.5.sql`.
- [ ] Benchmark recall@k/nDCG@10 por estratégia (BEIR/NFCorpus) coletado; k-adaptativo; `.json`+`.md` sem PENDING.
- [ ] Veredito por estratégia (qual move o recall; honest-negative onde não move); council-benchmark HONESTO.
- [ ] ADR-0025 completo (defer semantic por evidência).
- [ ] CHANGELOG `[Unreleased] § Added` atualizado (Regra 6).
- [ ] Cada arquivo tocado ≤ 500 LoC de delta; droplet destruído ao fim.
