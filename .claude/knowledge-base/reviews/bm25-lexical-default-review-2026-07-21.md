# Review — M138 (BM25 como perna lexical default) — 2026-07-21

**Verdict:** READY_TO_MERGE (honest-negative). Nenhum BLOCKER, nenhum HIGH.

Slice de medição: harness `run_m138_bm25_fusion.py` + `db.py::bm25_query` (fix 2-arg) + artefato
`docs/benchmarks/m138-bm25-fusion.md`. O deliverable é a **decisão medida** (não trocar o default), não código de produto.

## Revisor

- `council-benchmark` (lente "você mediu ou está supondo?") — auditoria adversarial da medição, significância,
  cherry-picking, degeneração de dados, reprodutibilidade.

## Veredito do revisor

O honest-negative **se sustenta**. Auto-consistência p↔effect-size confirmada por reconstrução independente
(t-approx bate o permutation em ambos corpora). Desenho A/B pareado limpo (mesma perna-vetor nas duas fusões,
mesmo RRF/k/top). Perna BM25 legítima: nfcorpus 0,325 = bullseye com o baseline BM25 publicado do BEIR;
scifact 0,688 ≈ 0,665 publicado. NFCorpus (o corpus que MAIS favorece BM25) foi deliberadamente rodado contra
a própria tese — o oposto de cherry-picking.

## Findings e disposição

| # | Sev | Finding | Disposição |
|---|---|---|---|
| MEDIUM-1 | MEDIUM | Arrays per-query não persistidos → p não recomputável do artefato | **CORRIGIDO** — `run_m138` persiste `per_query.{qids,hybrid_tsrank_ndcg10,hybrid_bm25_ndcg10}` no JSON |
| MEDIUM-2 | MEDIUM | Equivalência twin↔in-DB para a perna BM25 é assumida (in-DB quebrado, #146) | **DÍVIDA HONESTA** — divulgada no artefato §"Achado colateral" + issue #146; não vira o veredito (gap sem direção que transforme empate/perda em vitória) |
| LOW-1 | LOW | Pernas lexicais sem tie-break determinístico no ORDER BY | **CORRIGIDO** — `, doc_id` adicionado a `fts_query`, `bm25_query` e `vector_query_docs` |
| LOW-2 | LOW | Explicação "complementaridade/diversidade" é hipótese não medida | **ACEITO** — já enquadrada como explicação, não medição (artefato §"Por que a perna...") |
| LOW-3 | LOW | Generalização a 3º corpus | **ACEITO** — decisão por-corpus, com empate + perda-significativa a barra do flip é intransponível; não superinterpretado |

## Re-medição pós-fix

Os fixes de determinismo (LOW-1) alteram o desempate na fronteira do top-N; re-rodei os dois corpora e
atualizei os números do artefato + os JSONs versionados. O veredito `flip = false` é invariante ao tie-break.

## Conclusão

Merge-ready. O maior buraco era de reprodutibilidade do artefato (MEDIUM-1), fechado. Nada autoriza reverter
`flip = false`. O default lexical permanece `ts_rank_cd`; `pg_textsearch` não é embarcado (Phase 2 vetada pela
Phase 1). Issue #146 rastreia a fusão in-DB BM25 quebrada para trabalho futuro.
