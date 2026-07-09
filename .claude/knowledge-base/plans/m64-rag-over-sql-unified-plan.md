---
slug: m64-rag-over-sql-unified
milestone_id: M64
created_at: 2026-07-09
goal: Medir e documentar o RAG-sobre-SQL unificado — a query única de retrieval — provando round-trips economizados vs app-layer com recall igualado.
---

# M64 — RAG-sobre-SQL unificado (a query única)

## Goal

Provar e medir que o padrão RAG do TheoDB (filtro relacional + retrieval vetorial + context-assembly numa **única** SQL) economiza round-trips vs a orquestração app-layer **com recall@k igualado**, entregando o benchmark em `docs/benchmarks/m64-rag-over-sql.{md,json}` (métrica primária: round-trips/query = 1 no braço unified vs N no app-layer, com p50/p95/p99 e recall-matched).

## Context

A discovery (`.claude/knowledge-base/discoveries/blueprints/m64-rag-over-sql-unified-blueprint.md`, R0 web-citado) concluiu que **as peças do RAG unificado já existem** e são planner-integradas: filtered ANN (M52, `am/scan.rs:127-316`), híbrida RRF com `filter_sql` nas 2 pernas (M53, `hybrid.rs:34-74`), vector JOIN LATERAL (M63, `hnsw_page.rs:2976`), embed/chat in-SQL (`api.rs:306`, `chat.rs:19`). O M64 é **composição + medição + documentação** (rung-1 parsimony: NÃO construir `theodb.rag_query`, precedente ADR-0022). O achado de honestidade central: o DoD pede "agregação columnar planner-integrada", mas o pg_duckdb proíbe DuckDB em função (ADR-0021) e a engine columnar é separada do índice row-store — **um planner não os unifica**. Entregamos o Path 1 (uma query real, row-store) e documentamos o Path 2 (columnar = dois statements) honestamente. A lacuna que o campo não cobre e o M64 preenche: o head-to-head medido **"1 SQL vs N app-calls"**.

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Último commit | Papel | Ação M64 |
|---|---|---|---|---|
| `benchmarks/run_m64_rag_over_sql.py` | 0 (NEW) | — | harness unified-vs-app-layer | criar |
| `benchmarks/tests/test_run_m64_rag_over_sql.py` | 0 (NEW) | — | unit-test da aritmética (round-trip count, recall-match gate, verdict) | criar |
| `theodb_rs/src/am/hnsw_page.rs` | ~3150 | `10851fc` (M63) | pg_test suite do AM | +2 `#[pg_test]` (query composta recall + read-your-writes) |
| `docs/benchmarks/m64-rag-over-sql.md` | 0 (NEW) | — | relatório do padrão + números | criar |
| `docs/benchmarks/m64-rag-over-sql.json` | 0 (NEW) | — | dados brutos | criar (no droplet) |
| `docs/adr/0023-m64-rag-unified-not-columnar-planner.md` | 0 (NEW) | — | ADR: columnar não é planner-integrado; veredito unified-vs-app | criar |
| `CHANGELOG.md` | — | — | `[Unreleased] § Added` | editar |

### Current callers / dependents

- `theodb.embed` → `theodb_rs/src/api.rs:306` (síncrono, HTTP por linha — `embed.rs:24`).
- `ai.hybrid_search_rrf` → `theodb_rs/src/hybrid.rs:90`; `filter_sql` inlineado (`hybrid.rs:34,42,66,74`).
- Filtered ANN recall provado: `filtered_scan_preserves_recall_via_iterative` (`hnsw_page.rs:2283`).
- Vector JOIN Index Scan: `vector_join_uses_index_scan` (`hnsw_page.rs:2976`).
- Columnar codegen: `theodb.olap_sql` (`sql/85-theodb-htap.sql:126`) — cliente executa (ADR-0021).

### Domain glossary

- **RAG unificado** — retrieval (filtro + vetor) + context-assembly numa única SQL, sem sair do banco.
- **Context-assembly** — `string_agg(content, sep ORDER BY score)` sobre o top-k → um blob de contexto.
- **Round-trip** — uma ida-e-volta cliente↔servidor; o braço unified faz 1, o app-layer faz N (retrieve, filter, rerank, assemble).
- **Recall-matched** — igualar recall@k dos dois braços ANTES de comparar latência (senão a comparação é injusta).
- **Path 1 / Path 2** — Path 1 = uma query row-store (real); Path 2 = retrieval + agregação columnar = dois statements (honesto).

### Architecture boundaries affected

- `benchmarks/` é out-of-tree do crate Rust; reusa `theodb_bench.metrics` (Rule 9 — sem infra de harness nova).
- Os `#[pg_test]` correm dentro do crate (`am/hnsw_page.rs`), padrão M52/M63.
- Nenhuma fronteira DIP nova — zero código de produção novo (composição).

## Prior Art & Related Work

- Blueprint interno M64 (R0): `.claude/knowledge-base/discoveries/blueprints/m64-rag-over-sql-unified-blueprint.md`.
- AlloyDB AI (rerank RAG in-SQL): https://docs.cloud.google.com/alloydb/docs/ai/rank-rerank-search-results-rag
- pgvector-python hybrid RRF: https://github.com/pgvector/pgvector-python/blob/master/examples/hybrid_search/rrf.py
- RRF (Cormack SIGIR 2009): DOI 10.1145/1571941.1572114
- Harness precedente: `benchmarks/run_m63_vector_join.py` (espelhado).

## ADRs

### ADR-0023-a — Entregar Path 1 (uma query, row-store); Path 2 columnar documentado como dois statements

**Decisão:** o M64 entrega a query de referência RAG unificada como Path 1 (`WITH retrieved AS (WHERE <filtro> ORDER BY vec LIMIT k) SELECT string_agg(...) FROM retrieved`), planner-integrada, row-store. A leg columnar (Path 2) é documentada honestamente como **dois statements** (o retrieval + o `SELECT theodb.olap_sql()` que o cliente roda), NÃO um plano híbrido.

**Rationale:** o DoD (ROADMAP M64) pede "agregação columnar planner-integrada", mas isso é estruturalmente inalcançável — pg_duckdb proíbe DuckDB em função (ADR-0021, medido) e o índice `theodb_hnsw` (row-store) e o Parquet (DuckDB) são duas engines que um planner não unifica. Honestidade (Regra 5, `public-copy.md §4`): entregar o Path 1 real + documentar o Path 2 é o "veredito honesto" que o 3º bullet do DoD exige.

**Alternativas rejeitadas:**
- **(A) Fingir uma query columnar planner-integrada** — desonesto; a agregação sobre k linhas roda no executor PG, a engine columnar é irrelevante nessa escala. Viola Regra 5.
- **(B) Construir um custom scan que una row-store + Parquet** — PhD-level, exigiria reescrever o planner; fora de escopo (o columnar first-class exige uma engine única, decisão de produto do PRD). Viola "Esforço ≠ Complexidade" (complexidade acidental).

### ADR-0023-b — NÃO construir `theodb.rag_query()`; padrão documentado + benchmark

**Decisão:** zero código de produção novo. O RAG unificado é um padrão de query (CTE/LATERAL + `string_agg`) que o usuário escreve; o M64 entrega o guia + o benchmark, não uma função helper.

**Rationale:** rung-1 da parsimony-ladder ("isto precisa existir?") — precedente ADR-0022 (M63 rejeitou `theodb.vector_join` porque o raw idiom já é first-class e o helper com SQL dinâmico arriscaria o pushdown). O mesmo se aplica.

**Alternativas rejeitadas:**
- **(A) `theodb.rag_search(filter, qtext, k)` função** — açúcar puro sobre first-class; SQL dinâmico arriscaria o recall/pushdown; contrato SemVer para zero ganho de capacidade (YAGNI).
- **(B) VIEW canônica** — a query varia por schema (nomes de coluna, filtro); uma view fixa não generaliza. Documentar o padrão é mais KISS.

## Dependency Graph

```
Phase 1 (query de referência + pg_test recall/read-your-writes)  ──┐
                                                                    ├─→ Phase 3 (ADR + veredito honesto)
Phase 2 (benchmark harness unified-vs-app + unit-tests)  ──────────┘         │
                                                                              ↓
                                                                   Phase 4 (Integration Validation)
```

Phase 1 e Phase 2 podem paralelizar (o pg_test não depende do harness). Phase 3 consome os dois. Phase 4 fecha.

## Phase 1 — Query de referência + prova de correção (pg_test)

### T1.1 — `#[pg_test]` prova a query RAG composta preserva o recall das peças

#### Why this step
**Ação:** um `#[pg_test] rag_unified_query_preserves_recall` que monta a query de referência (`WITH retrieved AS (SELECT id, content FROM t WHERE cat=$1 ORDER BY emb <=> $2 LIMIT k) SELECT string_agg(content, ...) , count(*) FROM retrieved`) e assere que o conjunto `retrieved.id` == o oráculo exato `SELECT id FROM t WHERE cat=$1 ORDER BY emb <=> $2 LIMIT k`.
**Raciocínio:** a query composta não pode degradar o recall já provado das peças (M52). Espelha o oráculo de `filtered_scan_preserves_recall_via_iterative` (`hnsw_page.rs:2283`), citado no Baseline. Prova que "compor não quebra".

#### Files to edit
- `theodb_rs/src/am/hnsw_page.rs` (+~40 LoC, `#[pg_test]`)

#### Deep file dependency analysis
Reusa os helpers de fixture do módulo de teste (mesmo padrão de `vector_join_*`, `hnsw_page.rs:2976`). Não toca produção.

#### TDD
- RED: `test rag_unified_query_preserves_recall` — monta a query composta, compara `retrieved.id` set vs oráculo exato; assert igualdade (recall==1.0 no subset tratável). Falha antes do teste existir.
- GREEN: o teste É o deliverable (a query já funciona — composição); escrever o teste que a exercita.
- REFACTOR: extrair o helper de fixture se duplicar `vector_join_*`.

#### Concurrency tests
(none — single-threaded) — o pg_test roda num backend; sem estado compartilhado mutável.

#### Acceptance criteria
- `cargo pgrx test pg17 rag_unified` → `test rag_unified_query_preserves_recall ... ok`.
- O set `retrieved.id` == oráculo exato (recall 1.0 no subset).

#### DoD
- [ ] Teste GREEN; asserção de igualdade de set presente (não só "não-vazio").

### T1.2 — `#[pg_test]` prova read-your-writes na mesma transação (correção, não latência)

#### Why this step
**Ação:** `#[pg_test] rag_unified_read_your_writes` — dentro de UMA transação: INSERT de uma linha nova (content+embedding) → roda a query RAG unificada → assere que a linha nova é recuperável no top-k (com um embedding construído para ser o vizinho mais próximo da query).
**Raciocínio:** o blueprint marcou "no dual-write / read-your-writes" como **propriedade de correção** (não número de latência) — é o ganho transacional real do RAG-no-banco vs app-layer (que veria a linha só após commit + re-index). Prova a consistência que o app-layer não tem de graça.

#### Files to edit
- `theodb_rs/src/am/hnsw_page.rs` (+~35 LoC, `#[pg_test]`)

#### Deep file dependency analysis
O AM `theodb_hnsw` tem uma pending region (aminsert) que serve linhas não-fold ainda visíveis na txn (M40/M48). Este teste exercita esse caminho por-txn.

#### TDD
- RED: `test rag_unified_read_your_writes` — INSERT + RAG-query na mesma txn; assert a nova linha no resultado. Falha antes de existir.
- GREEN: escrever o teste (o comportamento já existe via pending region).
- REFACTOR: n/a se limpo.

#### Concurrency tests
(none — single-threaded) — o invariante é intra-transação single-backend; a corrida cross-backend é coberta pelos testes de crash-safety do M48 (fora de escopo).

#### Acceptance criteria
- `cargo pgrx test pg17 rag_unified` → `test rag_unified_read_your_writes ... ok`.
- A linha inserida na txn aparece no top-k da RAG-query da mesma txn.

#### DoD
- [ ] Teste GREEN; a asserção verifica a presença da linha nova (por id).

## Phase 2 — Benchmark harness unified-vs-app-layer

### T2.1 — Harness `run_m64_rag_over_sql.py` + aritmética unit-testável

#### Why this step
**Ação:** criar `benchmarks/run_m64_rag_over_sql.py` com 2 braços: **A (unified)** = 1 statement (o CTE retrieve + `string_agg` contexto); **B (app-layer)** = N statements simulando a orquestração (retrieve → filter no cliente → assemble no cliente). Mede p50/p95/p99 end-to-end (excluindo a chamada LLM de generate, idêntica nos 2), **round-trips/query** (1 vs N) e **recall@k igualado** (gate antes de comparar). Espelha `run_m63_vector_join.py`.
**Raciocínio:** a lacuna que o campo não publica é o head-to-head "1 SQL vs N app-calls" (blueprint corner 4). A aritmética claim-bearing (round-trip counter, recall-match gate, verdict por-eixo) é stdlib pura → unit-testável sem container (disciplina M63).

#### Files to edit
- `benchmarks/run_m64_rag_over_sql.py` (NEW, ~300 LoC)
- `benchmarks/tests/test_run_m64_rag_over_sql.py` (NEW, ~120 LoC)

#### Deep file dependency analysis
Reusa `theodb_bench.metrics.latency_percentiles` (Rule 9). O braço B usa o MESMO retrieval SQL do braço A, só quebrado em N idas — a comparação é justa (mesmos dados/embeddings/k/seletividade).

#### TDD
- RED: `test_recall_match_gate_blocks_unequal` — se recall_A != recall_B (tol), o verdict é `RECALL_MISMATCH` (não compara latência); `test_round_trip_count` — braço A conta 1, braço B conta N; `test_verdict_honest_per_axis`. Falham antes do código.
- GREEN: implementar a aritmética mínima (contadores, gate, verdict).
- REFACTOR: extrair helpers compartilhados com run_m63 se aplicável.

#### Failure scenarios
- **embed HTTP timeout/5xx** (`theodb.embed` é síncrono HTTP, `embed.rs`): o harness pré-computa os embeddings da query FORA do loop de medição (a chamada embed não está na âncora de latência); se o endpoint falhar no setup, o harness aborta com erro tipado claro (não mede lixo). Reproduz: endpoint inválido → assert `SystemExit`/erro explícito no setup, não um número silencioso.

#### Concurrency tests
(none — single-threaded) — o harness mede serialmente (1 cliente); a variância de carga é reportada (load_avg por-run, lição M46/M63), não paralelizada.

#### Acceptance criteria
- `pytest benchmarks/tests/test_run_m64_rag_over_sql.py` → todos GREEN; `ruff check` clean.
- O recall-match gate bloqueia comparação de latência quando recall difere > tol.
- round-trips: braço A == 1, braço B == N (medido/instrumentado, não hardcoded).

#### DoD
- [ ] Testes GREEN; recall-match gate presente; round-trip counter instrumentado.

## Phase 3 — Veredito honesto + ADR

### T3.1 — Rodar o benchmark no droplet + ADR-0023 + relatório

#### Why this step
**Ação:** rodar `run_m64_rag_over_sql.py` num droplet com a imagem (pgvector control), coletar `docs/benchmarks/m64-rag-over-sql.json`, escrever `docs/benchmarks/m64-rag-over-sql.md` e o `docs/adr/0023-m64-rag-unified-not-columnar-planner.md` (Path 1 real / Path 2 columnar honesto / veredito unified-vs-app).
**Raciocínio:** performance é claim, não opinião (Regra 5) — o número (round-trips economizados, latência a recall-igualado) só vira release com artefato em `docs/benchmarks/`. O ADR registra o achado de honestidade do columnar.

#### Files to edit
- `docs/benchmarks/m64-rag-over-sql.md` (NEW)
- `docs/benchmarks/m64-rag-over-sql.json` (NEW, coletado no droplet)
- `docs/adr/0023-m64-rag-unified-not-columnar-planner.md` (NEW)

#### Deep file dependency analysis
Espelha a estrutura de `docs/benchmarks/m63-vector-join.md`. O ADR referencia ADR-0021 (constraint pg_duckdb) e ADR-0022 (precedente helper-rejeitado).

#### TDD
- RED: n/a (documentação/medição — o gate é o `.json` existir com os campos + o council-benchmark HONESTO).
- GREEN: coletar os números reais; preencher o .md sem PENDING.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded) — a coleta é serial (1 cliente); a variância de carga é reportada (load_avg por-run), não paralelizada.

#### Acceptance criteria
- `docs/benchmarks/m64-rag-over-sql.json` existe com `per_arm` (A/B), `round_trips`, `recall_matched`, `verdict`.
- O .md não tem PENDING; cita os números do .json.
- ADR-0023 completo com as 2 decisões + alternativas.
- council-benchmark: veredito HONESTO (sem fabricação/spin).

#### DoD
- [ ] `.json` + `.md` + ADR presentes; council-benchmark HONESTO; números rastreáveis ao `.json`.

## Phase 4 — Integration Validation

### T4.1 — Suite completa + wiring + CHANGELOG

#### Why this step
**Ação:** rodar a suíte de teste completa (`cargo pgrx test pg17 rag_unified` + `pytest benchmarks/tests/`), confirmar o CHANGELOG `[Unreleased]` atualizado, e o guia do padrão documentado.
**Raciocínio:** o "eat your own cooking" gate — o M64 não está completo até os pg_test + pytest passarem juntos e o padrão estar documentado.

#### Files to edit
- `CHANGELOG.md` (`[Unreleased] § Added`)

#### TDD
- RED: n/a (validação de integração).
- GREEN: suíte verde; CHANGELOG com entry M64.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded) — validação de integração serial; sem novo estado concorrente introduzido pelo M64 (composição de peças existentes).

#### Acceptance criteria
- `cargo pgrx test pg17 rag_unified` → 2 testes ok.
- `pytest benchmarks/tests/test_run_m64_rag_over_sql.py` → verde.
- CHANGELOG `[Unreleased] § Added` cita o M64 com os arquivos + o veredito.

#### DoD
- [ ] Suíte verde; CHANGELOG atualizado (Regra 6).

## Coverage Matrix

| DoD (ROADMAP M64) | Task(s) | Evidência |
|---|---|---|
| Query de referência (WHERE filtro + ORDER BY vetor + LIMIT k) + agregação, planner-integrado, recall+latência medidos | T1.1, T2.1, T3.1 | pg_test recall==exact + harness round-trips/latência + benchmark artefato. **Nota honesta (ADR-0023-a):** agregação columnar planner-integrada é inalcançável (pg_duckdb constraint); entregamos Path 1 (row-store, real) + Path 2 (columnar dois-statements) documentado. |
| Doc do padrão RAG-nativo (retrieval + rerank + contexto) em SQL + benchmark | T1.1, T3.1 | guia do padrão no .md + query de referência; rerank de 2ª ordem (cross-encoder) é M65 — hoje RRF/ai.rank documentados honestamente |
| Veredito honesto vs pgvector + app-layer | T2.1, T3.1 | benchmark unified-vs-app-layer (round-trips economizados a recall-igualado) + ADR-0023 |

Cobertura: 100% dos 3 bullets do DoD mapeados a tasks, com a ressalva de honestidade do columnar registrada em ADR (não mascarada).

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| **DoD literal (columnar planner-integrado) é inalcançável** — pode ser lido como M64 falho | ALTA | ADR-0023-a documenta o achado; entregamos Path 1 real + Path 2 honesto; o 3º bullet do DoD ("veredito honesto") é exatamente isso. Flip do checkbox anota o que foi entregue (como M63). | Eng |
| **Benchmark app-layer pode ser um straw-man** (medir um braço B artificialmente lento) | MÉDIA | O braço B usa o MESMO SQL de retrieval, só quebrado em N idas reais; referência LangChain/LlamaIndex; medir com rede real (1 hop), não só co-located (senão o round-trip não aparece). council-benchmark audita. | Eng |
| **Zero código de produção pode parecer "M64 não entregou"** | BAIXA | Esforço ≠ Complexidade (CLAUDE.md): o valor é a medição honesta (a lacuna que o campo não publica) + a prova de correção (read-your-writes). Composição é o design correto (Regra 9). | Eng |

## Unresolved Questions

- O braço B (app-layer) deve simular um reranker cross-encoder (que o unified não tem até M65)? **Resolvido:** NÃO — igualar capacidade (ambos sem cross-encoder) para a comparação ser de latência/round-trips, não de capacidade. O cross-encoder é M65.
- Medir com rede real (1 hop) ou co-located? **Resolvido:** ambos — o round-trip economizado só aparece com rede real; reportar os dois (co-located isola CPU, rede isola round-trip).

## Failure scenarios

- **`theodb.embed` HTTP (endpoint de embedding) timeout/5xx/connection-reset** — o único I/O externo. Mitigação: o harness pré-computa embeddings da query no SETUP (fora da âncora de latência); falha no setup → erro tipado explícito + `SystemExit`, nunca um número de latência silencioso (testado em T2.1 `#### Failure scenarios`). O caminho de query em si (retrieval) é I/O-local (índice + heap), sem I/O externo.

## Global Definition of Done

- [ ] Os 2 `#[pg_test]` (recall composta + read-your-writes) GREEN.
- [ ] Harness + unit-tests GREEN; `ruff` clean.
- [ ] Benchmark `.json` + `.md` coletados no droplet; sem PENDING; números rastreáveis.
- [ ] ADR-0023 completo (2 decisões + alternativas).
- [ ] council-benchmark: veredito HONESTO.
- [ ] CHANGELOG `[Unreleased] § Added` atualizado (Regra 6).
- [ ] Cada arquivo tocado ≤ 500 LoC de delta.
- [ ] Droplet destruído ao fim.
