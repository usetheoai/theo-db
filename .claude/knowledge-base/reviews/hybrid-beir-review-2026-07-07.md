# /review — M53 Híbrida de verdade (WHERE + BM25 + i18n + BEIR real)

Date: 2026-07-07 · Slug: `hybrid-beir` · milestone_id: M53 · Range: `v0.43.0..HEAD`

## Verdict: READY_TO_MERGE (após review-fixes)

Três council specialists (security, rust-pgrx, benchmark). rust-pgrx direto READY; security + benchmark NEEDS_FIXES → todos os fixes aplicados e re-verdes.

## DoD (4 itens) — todos cumpridos com evidência

1. **Filtro relacional na fusão** (`ai.hybrid_search_rrf(…, filter_sql)`) — confinado a ambos os legs, SECURITY INVOKER, rejeita `;`+comentários. pg_test: `hybrid_search_accepts_filter_and_language`, `hybrid_filter_rejects_statement_terminator`, `hybrid_filter_rejects_sql_comment`.
2. **Gate de adoção BM25** — leg `pg_textsearch` opt-in (`lexical_engine='bm25'`) + fallback `ts_rank_cd` preservado (default). Erros tipados 22023 (engine inválido / content_text_col ausente); 0A000 quando pg_textsearch ausente. pg_test: `hybrid_bm25_without_text_col_errors`, `hybrid_invalid_lexical_engine_errors`, `hybrid_bm25_without_extension_raises_unsupported`. Gate de medição executado (item 3).
3. **Benchmark BEIR real** — `docs/benchmarks/m53-hybrid-beir.{md,json}`: scifact 5183 docs/300 queries, OpenAI text-embedding-3-small, 3 runs determinísticos. hybrid 0.7337 = vector 0.7296 (recall paridade 0.9733); bm25 0.6881 vs ts_rank_cd 0.0703. Decision-grade.
4. **i18n** — `language` parametriza `plainto_tsquery` (antes 'english' fixo). Coberto por `hybrid_search_accepts_filter_and_language` (language='simple').

## Reviewers + findings

**council-rust-pgrx: READY_TO_MERGE** (direto) — sem blockers. Lifetimes dos binds condicionais sound (`.into()` faz palloc/owned, não empresta dos temporários → sem dangling); aridade 13 bate 1:1 (pg_extern ↔ wrapper SQL ↔ CREATE FUNCTION ↔ COMMENT/REVOKE); `err_input`/`err_unsupported` são o mecanismo canônico pgrx de raise tipado (não panic cru atravessando C); sem Spi-em-Spi::connect; zero `unsafe` no diff.

**council-security: NEEDS_FIXES → READY_TO_MERGE** — F1 (HIGH): o guard `;` NÃO confina `filter_sql` (subquery de leitura passa: `(SELECT count(*) FROM t) >= 0`). NÃO é escalonamento hoje (INVOKER + read-only SPI + REVOKE FROM PUBLIC se sustentam — endossado), MAS o código enviava garantia falsa "injection-safe" p/ esse path (Regra 3) e é BLOCKER latente sob SECURITY DEFINER/GRANT. **FIXED**: (1) module docstring + `COMMENT ON FUNCTION` corrigidos (filter_sql declarado SQL cru caller-privilege, nunca de input não-confiável/nunca SECURITY DEFINER); (2) guard estende p/ `--`,`/*`,`*/`; (3) teste `hybrid_filter_rejects_sql_comment`; (4) ADR de filtro estruturado no backlog. Resto da superfície (lexical_engine whitelist, colunas `%I`, language/query_text/qvec binds) endossado SAFE.

**council-benchmark: NEEDS_FIXES → READY_TO_MERGE** — MEDIUM-1 (header dizia "SUPERA" no +0.004 sem teste de significância entre queries) + MEDIUM-2 (gap 9.8× bm25-vs-ts_rank_cd vendido antes do confound de candidate-set). Zero HIGH: nenhum número fabricado/invertido; agregados .md↔.json batem ao dígito; determinism medido (spread 0.0), não asserido; product-path in-DB real. **FIXED**: header + §2 reescritos ("IGUALA", edge marginal não-conclusivo; gap 9.8× qualificado como ranker+candidate-set; sinal limpo bm25 0.688 ≈ vector 0.730); follow-ups (significância pareada, hybrid-com-bm25, pytrec_eval) no §4 + backlog.

## Hard gates
Failing tests: NENHUM (6 pg_test hybrid + 7 pytest lógica pura verdes). Sem secrets (OPENAI_API_KEY só em header Authorization, .env gitignored, cache BEIR/embeddings gitignored). Sem commit em main; sem Co-Authored-By; CHANGELOG atualizado; artefato + backlog registrados.

**Verdict:** READY_TO_MERGE
