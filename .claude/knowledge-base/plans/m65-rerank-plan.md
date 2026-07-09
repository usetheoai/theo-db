---
slug: m65-rerank
milestone_id: M65
created_at: 2026-07-09
goal: Entregar ai.rerank own-code (cross-encoder via HTTP) e medir nDCG@10/MRR em BEIR com vs sem rerank, aceitando honest-negative se não melhorar.
---

# M65 — `ai.rerank` own-code (reranking de 2ª ordem por cross-encoder)

## Goal

Entregar `ai.rerank(query, docs[])` própria em Rust (cross-encoder via HTTP, espelhando `ai.embed`) e **medir** o delta de nDCG@10/MRR@10 em BEIR (SciFact) com vs sem rerank sobre o mesmo top-k, com o veredito em `docs/benchmarks/m65-rerank.{md,json}` (gate: se nDCG@10 melhora → PASS; se ≤ ruído → honest-negative documentado).

## Context

A discovery (`.claude/knowledge-base/discoveries/blueprints/m65-rerank-blueprint.md`, R0 web-citado) mapeou: (a) o padrão SOTA retrieve→rerank (monoBERT/monoT5, arXiv:1901.04085/2003.06713); (b) o shape de API convergente `{query,documents[]}` → `{results:[{index,relevance_score}]}` (Cohere/BGE/mxbai); (c) a superfície `ai.*` a espelhar — HTTP client compartilhado `http.rs::post_json` (`http.rs:41`), padrão `embed.rs::run_batch` (`embed.rs:55`), GUCs livres de sessão. **Construir a superfície é barato** (rung-1 parsimony, reusa http.rs); **o gate real é o benchmark BEIR** — e a literatura mostra que o ganho NÃO é universal (cross-encoders off-the-shelf degradaram nDCG −0.3% a −3.1% em corpora fora de distribuição). Por isso o DoD exige honest-negative se não melhorar.

## Baseline Context

### Files that will be touched

| Arquivo | LoC hoje | Último commit | Papel | Ação M65 |
|---|---|---|---|---|
| `theodb_rs/src/rerank.rs` | 0 (NEW) | — | domínio ai.rerank (espelha embed.rs) | criar (~120 LoC) |
| `theodb_rs/src/lib.rs` | ~110 | — | module root + pg_test offline | +`mod rerank;` + pg_test guards/parsers |
| `theodb_rs/src/api.rs` | 731 | `f5687ef` | pg_extern + extension_sql surface | +`_ai_rerank` + wrapper `ai.rerank` + REVOKE |
| `benchmarks/servers/rerank_server.py` | 0 (NEW) | — | cross-encoder real (espelha embedding_server.py) | criar |
| `benchmarks/tests/test_rerank_sql.py` | 0 (NEW) | — | oracle determinístico contra o stub | criar |
| `benchmarks/run_m65_rerank.py` | 0 (NEW) | — | benchmark BEIR nDCG/MRR com vs sem rerank | criar |
| `docs/benchmarks/m65-rerank.{md,json}` | 0 (NEW) | — | relatório + dados | criar (no droplet) |
| `docs/adr/0024-m65-ai-rerank-cross-encoder.md` | 0 (NEW) | — | ADR (assinatura, nome, honest-negative) | criar |
| `CHANGELOG.md` | — | — | `[Unreleased] § Added` | editar |

### Current callers / dependents

- HTTP client a reusar: `theodb_rs/src/http.rs::post_json` (`http.rs:41`) — minreq, retry 429/502/503 (MAX_RETRIES=2), SSRF `with_max_redirects(0)` (`http.rs:50`), timeout 30s, Bearer header, err tipado 38000.
- Padrão a espelhar: `theodb_rs/src/embed.rs::run_batch` (`embed.rs:55` — guard vazio → `[]`; NULL → 22023 via `err_input`; `resolve_cfg` `embed.rs:129`; payload; post_json; parse index-mapping N-in/N-out).
- Precedente TableIterator: `_hybrid_search_rrf` (`api.rs:108` — `TableIterator<(name!(id,String), name!(score,f32))>`).
- GUC helper: `pg.rs::guc(name)` (`pg.rs:50` = `current_setting(name, true)` via SPI) — GUCs livres de sessão, sem GucRegistry.
- Erros tipados: `pg.rs::err_input` (22023), `pg.rs::err_external` (38000).
- Stub server a espelhar: `benchmarks/servers/embedding_server.py` (modelo REAL fastembed, não mock).

### Domain glossary

- **Cross-encoder** — modelo que recebe (query, doc) JUNTOS e emite um escalar de relevância (mais preciso que bi-encoder; 1 inferência/par → só top-k).
- **Rerank** — reordenar o top-k do retrieval por score de cross-encoder (retrieve→rerank, 2 estágios).
- **nDCG@10** — DCG@10 / IDCG@10; penaliza relevantes em posições baixas (métrica primária do BEIR).
- **MRR@10** — média de 1/rank do 1º relevante (foco no primeiro acerto).
- **Recall@50** — fração de relevantes no top-50 (o rerank NÃO altera — sanity check).
- **honest-negative** — se o rerank não melhora nDCG (≤ ruído) ou piora, declarar com números, não spin.

### Architecture boundaries affected

- `rerank.rs` é domínio (mesma camada de `embed.rs`); reusa `http.rs` (infra) e `pg.rs` (erros/GUC) — sem fronteira DIP nova.
- A superfície pública `ai.rerank` segue o padrão: `#[pg_extern]` interno (schema theodb_rs) + wrapper SQL (schema ai) + REVOKE FROM PUBLIC (least-privilege — HTTP outbound).

## Prior Art & Related Work

- Blueprint M65 (R0): `.claude/knowledge-base/discoveries/blueprints/m65-rerank-blueprint.md`.
- monoBERT (Nogueira & Cho 2019, arXiv:1901.04085); monoT5 (arXiv:2003.06713); BEIR (Thakur et al. NeurIPS 2021, arXiv:2104.08663).
- BGE-reranker-v2-m3 (Apache 2.0): https://huggingface.co/BAAI/bge-reranker-v2-m3
- AlloyDB ai.rank: https://docs.cloud.google.com/alloydb/docs/ai/rank-rerank-search-results-rag
- Padrão interno a espelhar: `benchmarks/servers/embedding_server.py`, `benchmarks/tests/test_ai_sql.py`, `benchmarks/tests/test_m53_beir.py` (harness BEIR do M53).

## ADRs

### ADR-0024-a — `ai.rerank(query, docs[]) RETURNS TABLE(idx, score)`; nome `rerank` (não `rank`)

**Decisão:** `ai.rerank(query text, docs text[], model text DEFAULT NULL, top_n int DEFAULT NULL) RETURNS TABLE(idx int, score real)` — retorna scores por índice (não reordena in-place); nome `rerank` distinto do `ai.rank` existente.

**Rationale:** (1) `TABLE(idx, score)` converge com AlloyDB/Cohere/Voyage/Jina (4 fontes) e permite `ORDER BY score DESC` + join do idx de volta aos docs; precedente `_hybrid_search_rrf` (`api.rs:108`). (2) O repo JÁ tem `ai.rank` (LLM-scoring por-linha, `chat.rs:90`) — semanticamente diferente (1 prompt→1 float generative); `rerank` evita a colisão (divergimos do AlloyDB de propósito).

**Alternativas rejeitadas:**
- **(A) Retornar `text[]` reordenado** — perde o join às linhas de origem e o score para `ORDER BY`. Rejeitada (4 fontes retornam idx+score).
- **(B) Reusar/estender `ai.rank`** — semântica diferente (LLM-judge por item, N round-trips) vs cross-encoder batch; sobrecarregaria um nome com 2 contratos. Rejeitada.

### ADR-0024-b — Reusar `http.rs::post_json` + GUCs livres (rung-1 parsimony)

**Decisão:** `rerank.rs` reusa o client HTTP compartilhado (`http.rs::post_json`) e GUCs livres de sessão (`theodb.rerank_endpoint`/`_model`/`_api_key` via `guc()`); zero client novo, zero GucRegistry.

**Rationale:** parsimony rung-4 (dependência já instalada) — o `http.rs` já tem retry/SSRF/timeout/err tipado; reinventar seria Rule-9 violation. O padrão GUC-livre espelha `embed.rs:129-150`.

**Alternativas rejeitadas:**
- **(A) Novo HTTP client dedicado ao rerank** — duplica retry/SSRF/timeout; Rule 9. Rejeitada.
- **(B) Registrar os GUCs no GucRegistry** — os GUCs ai.* são livres de sessão por design (prefixo com ponto); registrar adicionaria cerimônia sem valor (YAGNI). Rejeitada.

## Dependency Graph

```
Phase 1 (rerank.rs + ai.rerank surface + pg_test offline)  ──→ Phase 2 (stub server + oracle test)  ──┐
                                                                                                        ├─→ Phase 3 (benchmark BEIR)
                                                                                                        │
                                                                                    Phase 3 ──→ Phase 4 (ADR + integration)
```

Phase 2 depende de Phase 1 (a superfície precisa existir para o oracle testar). Phase 3 depende de Phase 1+2 (a superfície + o stub/modelo real). Phase 4 consome tudo.

## Phase 1 — `rerank.rs` + superfície `ai.rerank` + pg_test offline

### T1.1 — `rerank.rs::run` (espelha embed.rs::run_batch) + guards/parsers com pg_test offline

#### Why this step
**Ação:** criar `theodb_rs/src/rerank.rs` com `run(query, docs[]) → Vec<f32>` alinhado por índice (guard docs vazio → `[]` sem HTTP; query/doc NULL → 22023; `resolve_rerank_cfg` lendo GUCs; payload `{"query","documents","model"}`; `post_json`; parse `results[].index`+`relevance_score` com invariante N-in/N-out → mismatch/dup/out-of-range 38000). pg_test offline dos guards/parsers (sem rede).
**Raciocínio:** espelha `embed.rs::run_batch` (`embed.rs:55-124`) — o padrão provado. Os guards são testáveis sem rede (input-path + parse), como `lib.rs:56-100` faz para embed.

#### Files to edit
- `theodb_rs/src/rerank.rs` (NEW, ~120 LoC)
- `theodb_rs/src/lib.rs` (+`mod rerank;` + pg_test offline)

#### Deep file dependency analysis
Reusa `http.rs::post_json` (`http.rs:41`), `pg.rs::{guc,err_input,err_external}` (`pg.rs:8,19,50`). O parse do shape `{"results":[{"index","relevance_score"}]}` é novo (o shape de rerank difere de embeddings/chat).

#### TDD
- RED: `test_rerank_empty_docs_no_http` (docs vazio → `[]`, sem GUC/HTTP), `test_rerank_null_doc_fails_typed` (NULL → 22023), `test_rerank_parse_aligns_by_index` (parser mapeia results[].index → posição, N-in/N-out), `test_rerank_parse_mismatch_fails` (N-out ≠ N-in → 38000), `test_rerank_unset_endpoint_fails` (sem GUC → 22023/38000). Falham antes do código.
- GREEN: implementar `rerank.rs::run` + o parser mínimo.
- REFACTOR: extrair o parser puro (`parse_rerank_results`) testável isolado, como `chat.rs::parse_batch`.

#### Concurrency tests
(none — single-threaded) — cada `ai.rerank` é 1 chamada síncrona num backend; sem estado compartilhado. O retry do http.rs é sequencial.

#### Failure scenarios
- **endpoint de rerank timeout/5xx/connection-reset** — reusa o fail-closed do `http.rs` (retry limitado 429/502/503, depois `err_external` 38000). Teste: apontar para porta fechada → "Connection refused" tipado (espelha `lib.rs:79`). SSRF: endpoint não-http(s) → 22023 no caller (espelha `embed.rs:139`).
- **N-out ≠ N-in** (o endpoint devolve menos/mais scores que docs) → 38000 (a invariante de alinhamento).

#### Acceptance criteria
- `cargo pgrx test pg17 rerank` → todos os testes offline GREEN.
- `rerank.rs::run` reusa `http.rs::post_json` (não novo client — grep confirma 1 só `minreq::post` no crate).
- Guards: docs vazio → `[]` sem HTTP; NULL → 22023; N-mismatch → 38000; unset endpoint → erro tipado.

#### DoD
- [ ] pg_test offline GREEN; parser N-in/N-out provado; SSRF/timeout herda http.rs.

### T1.2 — Superfície `ai.rerank` (pg_extern + wrapper SQL + REVOKE)

#### Why this step
**Ação:** `#[pg_extern] _ai_rerank` no schema theodb_rs (delega a `crate::rerank::run`) + wrapper `extension_sql!` `ai.rerank(query text, docs text[], model text DEFAULT NULL, top_n int DEFAULT NULL) RETURNS TABLE(idx int, score real)` + COMMENT (HTTP-outbound/SSRF) + REVOKE ALL FROM PUBLIC (interno + público).
**Raciocínio:** espelha o padrão `ai.embed`/`_hybrid_search_rrf` (`api.rs:90-108,361-401`). `RETURNS TABLE` via `TableIterator` (precedente `api.rs:108`). Least-privilege (HTTP outbound).

#### Files to edit
- `theodb_rs/src/api.rs` (+`_ai_rerank` ~após 78 + wrapper ~após 401)

#### Deep file dependency analysis
`TableIterator<(name!(idx,i32), name!(score,f32))>` — precedente exato `api.rs:108`. O `top_n` corta o resultado ordenado (ou passa ao endpoint). REVOKE segue `api.rs:388-397`.

#### TDD
- RED: `test_ai_rerank_surface_revoked_from_public` (a função existe e não é PUBLIC-executável) — via pg_test que checa `has_function_privilege`. Falha antes.
- GREEN: adicionar a superfície.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- `ai.rerank` existe, `RETURNS TABLE(idx, score)`, REVOKE FROM PUBLIC (interno + público).
- `cargo pgrx test pg17 rerank` inclui o teste de superfície GREEN.

#### DoD
- [ ] Superfície presente; REVOKE provado; assinatura conforme ADR-0024-a.

## Phase 2 — Stub server real + oracle determinístico

### T2.1 — `rerank_server.py` (cross-encoder real) + `test_rerank_sql.py`

#### Why this step
**Ação:** criar `benchmarks/servers/rerank_server.py` — um servidor HTTP mínimo (stdlib, espelha `embedding_server.py`) servindo um cross-encoder REAL (sentence-transformers `CrossEncoder`, ex. BAAI/bge-reranker-base — Apache 2.0, determinístico) no endpoint `POST /rerank` com shape `{"query","documents"}` → `{"results":[{"index","relevance_score"}]}`. + `test_rerank_sql.py` — oracle que sobe o stub, aponta `theodb.rerank_endpoint`, chama `ai.rerank` e assere o alinhamento por índice + ordenação.
**Raciocínio:** o padrão de teste ai.* usa modelo REAL out-of-process (não mock) — `embedding_server.py` usa fastembed. Reusar (Rule 9). O oracle prova o wire end-to-end sem depender de rede externa.

#### Files to edit
- `benchmarks/servers/rerank_server.py` (NEW, ~90 LoC)
- `benchmarks/tests/test_rerank_sql.py` (NEW, ~100 LoC)

#### Deep file dependency analysis
Espelha `embedding_server.py` (ThreadingHTTPServer stdlib + `/count` seam). O `test_rerank_sql.py` espelha `test_ai_sql.py` (sobe stub, seta GUC, integração — marcado `pytest.mark.integration`, skip sem container).

#### TDD
- RED: `test_rerank_returns_index_aligned_scores` (o stub devolve scores; `ai.rerank` retorna idx alinhados), `test_rerank_orders_by_relevance` (o top_n corta os mais relevantes), `test_rerank_empty_docs_returns_empty`. Falham antes do stub/wire.
- GREEN: o stub server + o wire (Phase 1 já fez a superfície).
- REFACTOR: extrair o shape helper se duplicar embedding_server.

#### Failure scenarios
- **stub devolve `{"results":[]}` para docs não-vazio** (N-mismatch) → `ai.rerank` erra 38000 (testa a invariante do T1.1 end-to-end).
- **stub 500** → `err_external` 38000 (o retry do http.rs esgota).

#### Concurrency tests
(none — single-threaded) — o stub é ThreadingHTTPServer mas o teste é serial; sem asserção de corrida.

#### Acceptance criteria
- `rerank_server.py` sobe um cross-encoder real e responde o shape `{"results":[...]}`.
- `pytest benchmarks/tests/test_rerank_sql.py` (integração, com o stub) → GREEN; `ruff` clean.

#### DoD
- [ ] Stub real + oracle GREEN; o wire ai.rerank→endpoint provado end-to-end offline (localhost).

## Phase 3 — Benchmark BEIR: nDCG@10/MRR com vs sem rerank (o gate real)

### T3.1 — `run_m65_rerank.py` + rodar em BEIR (SciFact) no droplet

#### Why this step
**Ação:** criar `benchmarks/run_m65_rerank.py` — retrieval theodb_hnsw top-50 sobre SciFact (BEIR), 2 braços: **A (baseline)** nDCG@10/MRR@10 sobre o top-50 por distância vetorial; **B (+rerank)** rerankear os 50 via `ai.rerank` → o cross-encoder real, medir nDCG@10/MRR@10 sobre o novo top-10. Reportar Recall@50 pré/pós (sanity: rerank não adiciona evidência) + p95/p99 de latência. ≥3 runs, mean±std. Reusar o harness BEIR do M53 + `pytrec_eval` (métricas). Rodar no droplet, coletar `docs/benchmarks/m65-rerank.json`.
**Raciocínio:** a superfície que roda ≠ ganho provado — o gate do DoD é o delta de nDCG. Aritmética claim-bearing (ndcg, mrr, delta, verdict) pura e unit-testável.

#### Files to edit
- `benchmarks/run_m65_rerank.py` (NEW, ~250 LoC)
- `benchmarks/tests/test_run_m65_rerank.py` (NEW, ~100 LoC — aritmética ndcg/mrr/delta/verdict)
- `docs/benchmarks/m65-rerank.{md,json}` (NEW, coletado no droplet)

#### Deep file dependency analysis
Reusa o loader BEIR do M53 (`test_m53_beir.py`) + `theodb_bench`. O `ai.rerank` (Phase 1) é o SUT; o `rerank_server.py` (Phase 2) é o cross-encoder.

#### TDD
- RED: `test_ndcg_at_k_formula` (nDCG conhecido para um ranking sintético), `test_mrr_formula`, `test_rerank_delta_positive_is_pass`, `test_rerank_delta_within_noise_is_honest_negative` (delta ≤ tol → HONEST_NEGATIVE, não PASS), `test_recall_conserved_sanity` (Recall@50 pré==pós). Falham antes.
- GREEN: a aritmética mínima.
- REFACTOR: reusar helpers do M53.

#### Failure scenarios
- **o endpoint de rerank cai no meio do benchmark** — o harness aborta com erro tipado (não mede lixo); reproduz apontando para porta fechada no setup → SystemExit claro.

#### Concurrency tests
(none — single-threaded) — o benchmark mede serialmente; variância de carga reportada (load_avg por-run).

#### Acceptance criteria
- `docs/benchmarks/m65-rerank.json` com `per_arm` (A/B: nDCG@10, MRR@10, Recall@50, p50/p95/p99), `verdict` (delta + PASS/HONEST_NEGATIVE).
- Recall@50 pré==pós (sanity — o rerank não adiciona evidência).
- Se nDCG@10 melhora > ruído → PASS; senão HONEST_NEGATIVE documentado com números.
- council-benchmark: veredito HONESTO.

#### DoD
- [ ] `.json`+`.md` coletados; delta nDCG medido; PASS ou honest-negative honesto; council-benchmark HONESTO.

## Phase 4 — ADR + Integration Validation

### T4.1 — ADR-0024 + suite completa + CHANGELOG

#### Why this step
**Ação:** escrever `docs/adr/0024-m65-ai-rerank-cross-encoder.md` (assinatura, nome rerank≠rank, reuso http.rs, o veredito do benchmark). Rodar `cargo pgrx test pg17 rerank` + `pytest` completos. CHANGELOG `[Unreleased] § Added`.
**Raciocínio:** o "eat your own cooking" gate — o M65 não está completo até os pg_test + pytest + benchmark passarem juntos e o ADR registrar o veredito (PASS ou honest-negative).

#### Files to edit
- `docs/adr/0024-m65-ai-rerank-cross-encoder.md` (NEW)
- `CHANGELOG.md` (`[Unreleased] § Added`)

#### TDD
- RED: n/a (validação de integração).
- GREEN: suíte verde; ADR + CHANGELOG completos.
- REFACTOR: n/a.

#### Concurrency tests
(none — single-threaded).

#### Acceptance criteria
- `cargo pgrx test pg17 rerank` → GREEN; `pytest` rerank tests → GREEN.
- ADR-0024 completo (2 decisões + alternativas + veredito honesto do benchmark).
- CHANGELOG cita o M65 com o veredito (PASS ou honest-negative).

#### DoD
- [ ] Suíte verde; ADR + CHANGELOG; droplet destruído.

## Coverage Matrix

| DoD (ROADMAP M65) | Task(s) | Evidência |
|---|---|---|
| `ai.rerank(query, docs[])` própria (Rust), integrável com híbrida (M53) + vector join (M63) | T1.1, T1.2 | rerank.rs + superfície `ai.rerank` RETURNS TABLE(idx,score); pg_test offline + oracle; compõe via `ORDER BY score DESC` + join do idx (como hybrid/M63) |
| Qualidade medida: nDCG@10/MRR em BEIR com vs sem rerank → `docs/benchmarks/m65-rerank.{md,json}` (gate: melhora nDCG) | T2.1, T3.1 | cross-encoder real (rerank_server.py) + benchmark 2-braços SciFact; delta nDCG@10 medido |
| Honestidade: se não melhorar, honest-negative + decisão | T3.1, T4.1 | verdict HONEST_NEGATIVE se delta ≤ ruído (com números); ADR-0024 registra a decisão |

Cobertura: 100% dos 3 bullets do DoD mapeados a tasks.

## Drawbacks & Risks

| Risco | Severidade | Mitigação | Owner |
|---|---|---|---|
| **O rerank pode NÃO melhorar nDCG** (honest-negative) — a literatura mostra ganho não-universal | ALTA | O DoD já prevê honest-negative; medimos o delta real e declaramos com números (não spin). O valor é a superfície mensurável + model-agnostic, não um ganho universal. | Eng |
| **Latência do rerank HTTP pode dominar o pipeline** (560-2100ms na literatura) | MÉDIA | Reportar p95/p99 obrigatório; o rerank é opt-in (o usuário decide rerankear ou não). | Eng |
| **SSRF/timeout do HTTP outbound** | MÉDIA | Reusa o fail-closed do `http.rs` (SSRF max_redirects=0, timeout, err tipado) + REVOKE FROM PUBLIC; council-security confere no /review. | Eng |
| **Shape do endpoint de rerank pode divergir** do assumido | BAIXA | O stub `rerank_server.py` fixa o shape `{"results":[{"index","relevance_score"}]}` (Cohere/BGE/TEI); o parser é trivial de ajustar; o benchmark usa o mesmo shape. | Eng |

## Unresolved Questions

- Qual reranker default (BGE-reranker-v2-m3 vs mxbai-rerank-v2)? **Resolvido:** para o benchmark, BGE-reranker-base (Apache 2.0, determinístico, CPU-viável em SciFact); a escolha de produção é configurável por GUC (model-agnostic) — não fixamos um default de produto no M65.
- Top-k de retrieval para rerankear (50)? **Resolvido:** top-50 (padrão BEIR retrieve-then-rerank); rerankear para top-10 (nDCG@10).

## Failure scenarios

- **Endpoint de rerank (HTTP outbound): timeout / 5xx / connection-reset / SSRF** — o único I/O externo. Reusa o fail-closed do `http.rs::post_json` (retry limitado 429/502/503 → `err_external` 38000; SSRF `with_max_redirects(0)`; endpoint não-http(s) → 22023). Testado em T1.1 (`#### Failure scenarios`: porta fechada → "Connection refused" tipado) e T2.1 (stub 500 / N-mismatch → 38000). O benchmark (T3.1) aborta com erro claro se o endpoint cair no setup (não mede lixo).

## Global Definition of Done

- [ ] pg_test offline de `rerank.rs` (guards/parsers/SSRF/N-mismatch) GREEN.
- [ ] Superfície `ai.rerank` presente, REVOKE FROM PUBLIC, RETURNS TABLE(idx,score).
- [ ] `rerank_server.py` (cross-encoder real) + `test_rerank_sql.py` oracle GREEN.
- [ ] Benchmark BEIR (SciFact) coletado: nDCG@10/MRR com vs sem rerank + Recall@50 sanity + p95/p99; `.json`+`.md` sem PENDING.
- [ ] Veredito: PASS (nDCG melhora) OU HONEST_NEGATIVE (com números) — nunca spin.
- [ ] ADR-0024 completo; council-benchmark HONESTO.
- [ ] CHANGELOG `[Unreleased] § Added` atualizado (Regra 6).
- [ ] Cada arquivo tocado ≤ 500 LoC de delta; droplet destruído ao fim.
