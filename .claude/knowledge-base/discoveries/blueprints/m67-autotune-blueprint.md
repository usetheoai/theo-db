# Blueprint M67 — Índices vetoriais auto-tunados (ef_search/probes por workload)

**Milestone:** M67 · **Data:** 2026-07-09 · **Método:** R0 (WebSearch/WebFetch — papers/OSS/blogs, ≥2 fontes por claim) + council-index-storage (mapa file:line do scan/cost/guc do AM).

## Veredito de topo (honesto)

**Quase nenhum sistema de produção auto-tuna ef/probes ONLINE para um alvo de recall vivo.** Milvus AUTOINDEX/Pinecone auto-selecionam params de BUILD; Qdrant/Weaviate/pgvector expõem GUC manual. O "auto-tune para alvo de recall" vive na academia como **early-termination query-adaptativo** (DARTH SIGMOD'26 [arXiv:2505.19001], Ada-ef [arXiv:2512.06636]) — probabilístico. O único auto-tuner shipado (VDTuner ICDE'24 [arXiv:2404.10413]) é **Bayesian offline recomendador**. **Rung-1 honesto TheoDB: coletor de stats + recomendador determinístico** (sugere ef, operador aplica com `SET`). Auto-tune online de GUC persistente tem risco de oscilação que nenhum vector-DB de produção assumiu.

## Coverage Corner 1 — Integration Tests

- Testar o coletor (stats corretas por scan) + o recomendador (dado alvo → ef mínimo) via pg_test. O sinal de recall (convergência do beam) é derivável do scan existente sem GT.

## Coverage Corner 2 — Dependencies

Nenhuma dep nova. **Grande parte já existe** (council-index-storage, file:line):
- `reads` counter (pages read) — `hnsw_page.rs:1515`, threaded pela descida, já logado sob `THEODB_SCAN_PROFILE`.
- `visited` HashSet (candidatos vistos) — `ann/scan_core.rs:109` (hoje descartado; retornar `visited.len()`).
- amcostestimate honesto f(ef) — `am/cost.rs:33` (`hnsw_visit_ratio`), `am/mod.rs:123`.
- GUCs ef_search/probes Userset — `am/guc.rs:25,17`.
- **Sinal de convergência M52** — `scan.rs:335`: se dobrar ef não traz candidato novo (`fresh.is_empty()`) → recall=1.0 (oráculo barato).

## Coverage Corner 3 — Tools

- Benchmark de convergência: medir que o recomendador atinge o alvo de recall (MAE |R*−R|, RQUT tail-safety, iterações-até-convergir). Reusar o harness recall (SIFT/BEIR) do projeto.

## Coverage Corner 4 — Techniques

**(a) Estimativa de recall SEM ground-truth:** a única família confiável é **exact-scan amostrado periódico** (Qdrant `exact=True`, Milvus vs FLAT, Elastic `flat`) — o GT verdadeiro numa amostra. Proxy online barato: **distância do k-ésimo vizinho / convergência do beam** (rodar em ef e 2·ef, comparar k-ésima distância — o M52 já faz o 2× re-search). DARTH ([arXiv:2505.19001]) usa 11 features internas → GBDT (MSE 0.003, R² 0.88) mas treina offline contra GT. Fontes: [Qdrant](https://qdrant.tech/documentation/tutorials-search-engineering/retrieval-quality/), [Milvus PR#39410](https://github.com/milvus-io/milvus/pull/39410), [Elastic](https://www.elastic.co/search-labs/blog/recall-vector-search-quantization).

**(b) Recomendador de ef (bisection monotônica):** recall(ef) é **monotônico não-decrescente** (a lista de ef+1 é superset da de ef — Malkov & Yashunin [arXiv:1603.09320]) e QPS(ef) não-crescente, com **diminishing returns** (0.9→0.99→0.999 exige ef super-linear). Algoritmo determinístico: doubling `[k,2k,4k,…]` até recall_amostrado(ef)≥R*, depois bisecta o bracket → **mínimo ef** que atinge R*. Confirmação pgvector: [Supabase](https://supabase.com/blog/increase-performance-pgvector-hnsw) (ef 40→100 acc 0.98→...), [OSC](https://opensourceconnections.com/blog/2025/02/27/vector-search-navigating-recall-and-performance/).

**(c) amcostestimate refinado:** o cost.rs já é f(ef) (M48, porta a fórmula do pgvector hnsw.c). Refino: quando há stat empírica para o índice, usar o `pages_read` médio observado como base do custo (calibra pela realidade); senão fallback à fórmula. Corrige o gap [pgvector #784](https://github.com/pgvector/pgvector/issues/784) (dimensionalidade). Contrato: [PG index-cost-estimation](https://www.postgresql.org/docs/current/index-cost-estimation.html).

## ADR-1 — Escopo: coletor + recomendador determinístico; auto-tune online DEFERIDO

**FAZER (v1):** (1) coletor de stats de scan (visited/reads/latência) persistido num catálogo `theodb._index_scan_stats` (heap regular, key indexrelid — **FORA das páginas do índice**, crash-safety: escrever nas páginas via GenericXLog a cada scan violaria partial-read + imutabilidade M35); amostragem (bump 1-em-N ou flush agregado no amendscan — evita custo por-scan no read path). (2) recomendador determinístico `theodb.recommend_ef(index, recall_target) → int` (probe scan em ef crescente, convergência do beam como recall-est, retorna o menor ef que atinge o alvo — read-only). (3) amcostestimate refinado (usar pages_read observado).

**NÃO FAZER:** auto-tune online que muta ef_search vivo (oscilação, colide com o SET do usuário, difícil de tornar crash-safe/observável — nenhum vector-DB de produção faz).

**DEFERIR (v2, com evidência):** early-termination query-adaptativo (Ada-ef rule-based é a entrada de menor risco; DARTH GBDT exige modelo+treino) — bet medido antes de shipar.

O DoD permite "auto-tune **ou** recomendação" — o recomendador é o rung-1 seguro alinhado ao P7.

## ADR-2 — Persistência das stats fora das páginas do índice (crash-safety)

Molde `theodb.vectorizer_worker_stats` (`vectorizer.rs:150`): catálogo heap `theodb._index_scan_stats(indexrelid oid PK, n_scans, sum_pages_read, sum_candidates, sum_latency_us, last_ef, last_updated)`. Bump via SPI amostrado. O scan das páginas do índice continua read-only; contrato IndexAmRoutine intacto.

## O gate REAL — benchmark de convergência

O DoD pede "medida de convergência". Metodologia: alvo R* (ex. 0.95), o recomendador sugere ef; medir |recall_medido(ef) − R*| (MAE), RQUT (% queries abaixo do alvo — tail safety, não só a média), iterações-até-convergir (banda ±2%). Reportar vs baseline (ef fixo). Se o recomendador não converge, honest-negative.

## Débito honesto

- Recall-est por convergência-do-beam é proxy (não GT) — a base-de-verdade é o exact-scan amostrado.
- Early-termination adaptativo (Ada-ef/DARTH) é v2 (ganho 6.8-13.6× DARTH, mas probabilístico + treino).
- amcostestimate do pgvectorscale não foi verificável na varredura (semente futura).
