# ADR 0024 — M65 `ai.rerank`: cross-encoder reranking via HTTP (own-code, reusa o client ai.embed)

**Status:** Accepted · **Data:** 2026-07-09 · **Milestone:** M65 · **Owner:** Eng
**Relacionado:** blueprint `.claude/knowledge-base/discoveries/blueprints/m65-rerank-blueprint.md`,
plan `.claude/knowledge-base/plans/m65-rerank-plan.md`, ADR `0006` (superfície ai.* em Rust/pgrx),
`.claude/rules/error-handling.md` (fail-closed tipado), `.claude/rules/parsimony-ladder.md` (rung-4 reuso),
`.claude/rules/public-copy.md §4` (performance é claim), Unbreakable Rule 9 (não reinventar).

## Contexto

O RAG SOTA rerankeia o top-k do retrieval com um **cross-encoder** (query+doc juntos no modelo → escalar de
relevância) — mais preciso que o bi-encoder do retrieval, mas 1 inferência/par, só para top-k pequeno. A
discovery (blueprint, R0 web-citado, ≥2 fontes por claim) mapeou: (a) o padrão retrieve→rerank
(monoBERT/monoT5, arXiv:1901.04085/2003.06713); (b) o shape de API convergente `{query,documents[]}` →
`{results:[{index,relevance_score}]}` (Cohere/Jina/Voyage/BGE/TEI); (c) a superfície `ai.*` a espelhar. O M65
fecha o lifecycle `retrieve→rerank` da superfície `ai.*` (embed/chat/rank/hybrid) com `ai.rerank`.

## Decisão D1 — Assinatura `ai.rerank(query, docs[]) RETURNS TABLE(idx, score)`; nome `rerank` ≠ `rank`

**Decisão:** `ai.rerank(query text, documents text[], model text DEFAULT NULL, top_n int DEFAULT NULL) RETURNS TABLE(idx int, score real)`,
ordenada por score DESC; `idx` 0-based no array `documents` de entrada (join de volta às linhas de origem).

**Rationale:**
- **`TABLE(idx, score)` (não reordena in-place)** converge com AlloyDB `ai.rank`/Cohere/Voyage/Jina (4 fontes);
  permite `ORDER BY score DESC` + join do `idx`. Precedente interno exato: `_hybrid_search_rrf` usa
  `TableIterator<(name!(id,String), name!(score,f32))>` (`api.rs`).
- **Nome `rerank` (não `rank`)** — o repo JÁ tem `ai.rank` (LLM-scoring por-linha, `chat.rs:90` — 1 prompt→1
  float via generative, semanticamente diferente). Divergimos do AlloyDB (que chama o dele `ai.rank`) DE
  PROPÓSITO para não colidir com o `ai.rank` existente.

**Alternativas rejeitadas:**
- **(A) Retornar `text[]` reordenado** — perde o join às linhas de origem e o score para `ORDER BY`. Rejeitada.
- **(B) Reusar/estender `ai.rank`** — semântica diferente (LLM-judge por item, N round-trips) vs cross-encoder
  batch (query+docs num shape dedicado); sobrecarregar um nome com 2 contratos é confuso. Rejeitada.

## Decisão D2 — Reusar `http.rs::post_json` + GUCs livres (rung-1/rung-4 parsimony)

**Decisão:** `rerank.rs::run` reusa o client HTTP compartilhado (`http.rs::post_json`) e GUCs livres de sessão
(`theodb.rerank_endpoint`/`_model`/`_api_key` via `guc()`); zero client novo, zero GucRegistry. Espelha
`embed.rs::run_batch`.

**Rationale:** parsimony rung-4 (dependência já instalada) — o `http.rs` já tem retry (429/502/503), SSRF
(`with_max_redirects(0)`), timeout 30s, err tipado (38000). Reinventar seria Rule-9 violation. O padrão
GUC-livre espelha `embed.rs:129-150`. O parser N-in/N-out (results[].index → posição; mismatch/dup/out-of-range
/non-numeric → 38000) espelha a lógica de alinhamento do `embed::run_batch`.

**Alternativas rejeitadas:**
- **(A) Novo HTTP client dedicado** — duplica retry/SSRF/timeout; Rule 9. Rejeitada.
- **(B) Registrar os GUCs no GucRegistry** — os GUCs ai.* são livres de sessão por design (prefixo com ponto);
  cerimônia sem valor (YAGNI). Rejeitada.

## Segurança (herda o fail-closed do ai.embed)

`ai.rerank` faz HTTP outbound síncrono — herda a mesma superfície SSRF/timeout/5xx do `ai.embed`: endpoint
http(s)-only (não-http → 22023), `with_max_redirects(0)` (sem seguir 30x para metadata interno), timeout, err
tipado. REVOKE ALL FROM PUBLIC (interno `theodb_rs._ai_rerank` + público `ai.rerank`) — least-privilege
(NIST AC-6). council-security confere no /review.

## O gate REAL (o que importa) — o benchmark, não a superfície

**A superfície que roda ≠ ganho de retrieval provado.** O DoD do M65 é: `ai.rerank` só é aceito se **melhorar
nDCG@10/MRR mensuravelmente em BEIR** (`docs/benchmarks/m65-rerank.{md,json}`), com **honest-negative se não
melhorar**. A literatura é explícita que o ganho NÃO é universal — cross-encoders off-the-shelf degradaram
nDCG −0.3% a −3.1% + 560-2100ms de latência em corpora fora de distribuição ([pgai report]). Por isso o
benchmark mede o delta real (mesmo top-k, rerankear, medir nDCG@10) e o `rerank_verdict` retorna PASS
(delta > ruído) OU HONEST_NEGATIVE (delta ≤ ruído) — nunca spin.

## Evidência (medida)

- **14 pg_test GREEN** (`cargo pgrx test pg17 rerank` — 14 passed, na stack real theodb_rs+vector+vectorscale+theodb):
  guards (NULL query/doc, empty→no-HTTP, unset endpoint, SSRF non-http, connrefused tipado) + parser
  (align-by-index, size-mismatch, dup, out-of-range, non-numeric → 38000).
- 11 pytest aritmética (mrr_at_k, rerank_verdict) GREEN, ruff clean.
- **Benchmark BEIR/SciFact (100 queries, 3 runs determinísticos) — VEREDITO HONEST-NEGATIVE:** o rerank
  (BGE-reranker-base) **degradou** o nDCG@10 em **−3.8%** (baseline 0.7327 → rerank 0.6947), ao custo de ~1.96 s
  p50/query. Recall@50 conservado (0.92 == 0.92, sanity ✓). Exatamente o previsto pela literatura (cross-encoder
  off-the-shelf regride em corpus fora de distribuição — SciFact é fact-checking científico). `docs/benchmarks/m65-rerank.{md,json}`.

## Decisão pós-benchmark (o DoD exige "honest-negative + decisão")

- **`ai.rerank` embarca** — a superfície own-code está correta, testada e medível (o valor é fechar o lifecycle
  retrieve→rerank de forma mensurável e model-agnostic, NÃO um ganho universal).
- **NÃO se afirma ganho de qualidade** (public-copy.md §4) — o benchmark mostra regressão no par (BGE-base, SciFact).
  O operador escolhe o reranker adequado ao seu corpus por GUC; um reranker in-domain pode ganhar onde este perdeu,
  mas isso exige o próprio benchmark, não extrapolação.
- **Rerank é opt-in** — custo ~2 s/query sem ganho garantido; não é default.

## Consequências

- **`ai.rerank` fecha o lifecycle retrieve→rerank** da superfície `ai.*`, model-agnostic (endpoint por GUC —
  BGE/mxbai Apache 2.0 self-host, ou Cohere/Voyage API).
- **O ganho de qualidade NÃO é garantido** — é medido e honesto (PASS ou honest-negative com números). O valor
  é a superfície mensurável, não um claim universal.
- **Latência do rerank** pode dominar o pipeline (a literatura reporta 560-2100ms) — o rerank é opt-in (o
  usuário decide); p95/p99 reportados.
- **Zero client HTTP novo** (reusa http.rs — Regra 9).

## Caveats honestos

Dados BEIR/SciFact (fact-checking científico) podem estar fora da distribuição de treino do reranker default
(honest-negative mais provável — e é um resultado válido). O reranker default do benchmark é declarado; a
escolha de produto é configurável por GUC. Sem claim de ganho universal — os números são os medidos.
