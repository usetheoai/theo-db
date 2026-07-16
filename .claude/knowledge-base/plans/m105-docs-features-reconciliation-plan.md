---
slug: m105-docs-features-reconciliation
milestone_id: M105
created_at: 2026-07-16
goal: Reconcile every docs/features/*.md SQL example to the shipped surface (or label it target-API), verified by a grep gate that zero runnable symbol is fabricated.
---

# Plan: M105 — docs/features reality reconciliation (docs-only)

## Discovery (source of truth)

The 3-agent feature audit (spec↔code↔tests) + this extracted **ground-truth shipped SQL surface**:

### Real AI surface (schema `ai`)
- `ai.generate(prompt text, model text DEFAULT NULL) RETURNS text`
- `ai.generate_batch(prompts text[], model text DEFAULT NULL) RETURNS text[]`
- `ai.summarize(content text, model text DEFAULT NULL) RETURNS text`
- `ai.analyze_sentiment(content text, model text DEFAULT NULL) RETURNS text`
- `ai.rank(prompt text, model text DEFAULT NULL) RETURNS real`  ← scalar relevance score
- `ai.rerank(query text, documents text[], model text DEFAULT NULL, top_n int DEFAULT NULL) RETURNS TABLE(idx int, score real)`  ← batch reranker (idx 0-based)
- `ai.if_batch(condition text, vals text[], model) RETURNS boolean[]`, `ai.if_costly(condition text, val text, model) RETURNS boolean`  (NO bare `ai.if`)
- `ai.hybrid_search(config jsonb) RETURNS TABLE(id text, score real)`, `ai.hybrid_search_rrf(...)`
- `ai.nl_to_sql(question text, allowed_relations text[], model) RETURNS text`, `ai.nl_query(question text, allowed_relations text[], model, max_rows int) RETURNS jsonb`, `ai.nl_add_config/nl_add_template/nl_set_value_index/nl_query_cfg/...`

### Real vector/embed surface (schema `theodb`)
- `theodb.embed(content text, model text DEFAULT NULL) RETURNS vector`, `theodb.embed_batch(content text[], model) RETURNS vector[]`  ← NOT `theodb_ml.embedding(...)`
- `theodb.l2_distance/inner_product/cosine_distance`, operators `<->` `<#>` `<=>` on the own `vector` type
- AMs: `theodb_ivfflat`, `theodb_hnsw`, `theodb_columnar`  ← NO `theodb_scann`, NO `ivf`
- opclasses: default `theodb_ivfflat_l2_ops`/`theodb_hnsw_l2_ops` + `theodb_{ivfflat,hnsw}_{cosine,ip}_ops` (+ `theodb_ivfflat_label_ops`)
- `theodb_ml` is a **schema** (registry: `create_model/apply_model/drop_model/list_models`), NOT an extension. No `theodb_ml.embedding`.

## Per-file correction directives

| File | Correction |
|---|---|
| 01-busca-similaridade-vetorial | `theodb_ml.embedding('theodb-embedding-005','TEXT')` → `theodb.embed('TEXT', 'model')` (arg order + schema + name); drop fictional model id; keep `<->`/`<=>` (real) |
| 02-indice-hnsw | own-AM examples use `USING theodb_hnsw (embedding theodb_hnsw_l2_ops)`; pgvector-surface examples labeled as coexistence |
| 03-indice-ivfflat | own-AM examples use `USING theodb_ivfflat (embedding theodb_ivfflat_l2_ops) WITH (lists=N)`; pgvector labeled |
| 04-indice-ivf | NO literal `ivf` AM → move `USING ivf`/`quantizer=` under **🎯 API-alvo / roadmap (não-shipped)**; shipped path = `theodb_ivfflat` + reloptions (`lists`, `pq_subspaces`, `pq_bits`, `separate_storage`) first |
| 05-indice-scann | NO `theodb_scann`/`USING scann` → whole SQL body under **🎯 API-alvo / roadmap (não-shipped)**; document the SHIPPED ScaNN-inspired path (`theodb_ivfflat WITH (pq_subspaces=M)`, IVF-AQ+AH); ScaNN-QPS-superiority = **measured-negative** (ADR-0035/0036), not a gap |
| 06-busca-hibrida | remove/label unimplemented JSON keys (`weight`,`distance_operator`,`ranking_function`,`include_json_output`,`id_type`) + `g_to_tsquery`/`theodb_scann`; document the REAL contract keys (`table/id_col/content_tsv_col/vector_col/query_*/k/per_leg_limit/result_limit/language/filter_sql/lexical_engine`) |
| 07-funcoes-ia-sql | `CREATE EXTENSION theodb_ml` → it's a schema + registry; `ai.if` → `ai.if_batch`/`ai.if_costly` (real names) |
| 08-acelerar-consultas | Proxy Model (`ai.if(prompt,embedding)`, `enable_ai_query_engine`, `runtime_accuracy_check`) under **🎯 API-alvo / roadmap**; shipped acceleration = `ai.generate_batch` (N→1 batching) documented first |
| 09-ranquear-resultados | phantom `ai.rank(model_id, search_string, documents, top_n)` → real `ai.rerank(query, documents[], model, top_n) RETURNS TABLE(idx,score)`; fix the RAG-join off-by-one (idx 0-based); the scalar `ai.rank(prompt,model)→real` documented as the distinct scalar-scorer |
| 10-analise-sentimento | already IMPLEMENTED — verify `ai.analyze_sentiment(content, model)` matches; fix any `theodb_ml.embedding`/`CREATE EXTENSION theodb_ml` |
| 11-sumarizacao-conteudo | already IMPLEMENTED — verify `ai.summarize` + `ai.agg_summarize`; fix any theodb_ml extension DDL |
| 12-linguagem-natural | `theodb_ai_nl.*` (50+ AlloyDB funcs) under **🎯 API-alvo / roadmap (não-shipped)**; shipped path = `ai.nl_to_sql`/`ai.nl_query` + `ai.nl_*` config, with the 4-layer safety posture documented first |

## GATE (acceptance evidence — the "benchmark" for a docs milestone)

Grep every runnable (non-labeled) SQL symbol in `docs/features/*.md` against the real surface: **zero fabricated symbol** outside a **🎯 API-alvo / roadmap (não-shipped)** banner. Specifically: no `theodb_ml.embedding`, no `CREATE EXTENSION theodb_ml`, no `USING scann`/`USING ivf`, no `theodb_scann`, no phantom `ai.rank(4-arg)`, no `theodb_ai_nl.*`, no unimplemented hybrid JSON keys — except under a labeled target-API section.

## DoD

Per the ROADMAP M105 block (7 items). Boundary: **docs-only, zero code change**. Verification: the GATE grep + a fresh review agent re-greps.

## Risks

(a) claiming shipped what isn't (honesty) → grep-verify per example before marking runnable; (b) scope creep into rewriting whole specs → correct+label, don't rewrite.
