# Blueprint M68 — Observabilidade do query vetorial (EXPLAIN + métricas)

**Milestone:** M68 (o último de v3) · **Data:** 2026-07-09 · **Método:** R0 (WebSearch/WebFetch — pgvector/Milvus/Qdrant/PG18 docs) + council-index-storage (mapa file:line). Depende M67 (entregue).

## Coverage Corner 1 — Integration Tests

- Reusa a infra M67 (`scan_stats`, thread_local, catálogo). Testar via pg_test: `explain_scan` retorna pages_read+candidates>0; candidates persiste no catálogo.

## Coverage Corner 2 — Dependencies

Nenhuma dep nova. Reusa: `scan_stats`/thread_local `SCAN_PAGES_READ` (M67, `autotune.rs`), catálogo `theodb._index_scan_stats`, o `visited` HashSet (`scan_core.rs:109`).

## Coverage Corner 3 — Tools

Sem benchmark de performance (observabilidade → validado por pg_test). Métrica runtime = o catálogo consultável (padrão `vectorizer_worker_stats`, não Prometheus — honesto para v1).

## Coverage Corner 4 — Techniques

**Achado central (R0, ≥2 fontes):**
- **NÃO há hook `amexplain` no PostgreSQL 18** — o `IndexAmRoutine` não tem callback para o AM injetar linhas custom no EXPLAIN ([PG18 Index AM Interface](https://www.postgresql.org/docs/current/indexam.html), [Index Functions](https://www.postgresql.org/docs/current/index-functions.html)). O padrão dos peers é uma **função/endpoint diagnóstico separado** (Qdrant `/telemetry`+`usage` [qdrant.tech/documentation/search](https://qdrant.tech/documentation/search/search/); Milvus métricas Prometheus [milvus.io/docs/metrics_dashboard.md](https://milvus.io/docs/metrics_dashboard.md)).
- **pgvector/pgvectorscale NÃO expõem pages_read/candidates por-query** ([pgvector README](https://github.com/pgvector/pgvector), [dbi-services pgvector DBA guide](https://www.dbi-services.com/blog/pgvector-a-guide-for-dba-part-2-indexes-update-march-2026/)) — o operador infere via `EXPLAIN (BUFFERS)` `shared read` (proxy grosseiro) + `pg_statio_user_indexes`. **O `theodb.scan_stats` do M67 já supera esse baseline.**
- **Métricas canônicas:** Recall, Latency, QPS ([Milvus](https://milvus.io/ai-quick-reference/how-do-i-evaluate-vector-search-performance)). Sinal nº1 de degradação: o **planner não escolher o índice** (seqscan fallback — pgvector [#771](https://github.com/pgvector/pgvector/issues/771)) e o "indexed vectors ratio < 1.0" do Qdrant.

## ADR-1 — `theodb.explain_scan` (função diagnóstica), NÃO hook do EXPLAIN

`theodb.explain_scan(index_table regclass, vector_col text, query text, ef int, k int) RETURNS TABLE(index_name text, ef_effective int, pages_read bigint, candidates_seen bigint, latency_us bigint, results bigint)` — reusa o motor de medição do `scan_stats` (M67) + adiciona candidates + o nome do índice + o ef. NÃO tentar hook do EXPLAIN (não há ponto de extensão estável no PG18 — complexidade acidental).

## ADR-2 — candidates_seen via retorno puro de ground_search (invariante do bench)

O `visited.len()` (candidatos navegados) vive em `scan_core.rs:109`, descartado em `:164`. Expor via **retorno**: `ground_search_nodes` (`:101`) retorna `(Vec, visited.len())`. NÃO chamar `crate::am::autotune::bump` de DENTRO de `scan_core` — ele tem o invariante "no `crate::` / no `pg_sys`" (`:8, :176`) para o criterion bench standalone. O bump (`bump_scan_candidates`, thread_local espelhando o pages_read) acontece no lado de produção (`hnsw_page.rs:1645`, ao lado de `bump_scan_pages`). ~7 call sites mecânicos (destructure da tupla). Recall-neutro (só lê `len()`).

## Métrica runtime (wiring pillar c)

O catálogo `theodb._index_scan_stats` (M67) ganha `sum_candidates`; `record_scan_stat` + `scan_stats` propagam. `theodb.index_scan_stats(rel)` expõe `avg_candidates`. Consultável (o padrão `vectorizer_worker_stats`), não Prometheus (honesto para v1 — histograma Prometheus exigiria um registry no processo, YAGNI).

## Doc de operação (esqueleto)

`docs/ops/vector-scan-diagnostics.md`: (1) 1º passo — `explain_scan`: o índice foi escolhido? (senão `ORDER BY <dist> LIMIT` asc); (2) recall baixo → subir ef (sweep, menor que bate a meta, começar 64-128) → se filtrado, iterative scan → se teto não sobe, rebuild m/ef_construction; (3) latência alta → ef alto demais (0.95→0.99 = 3-5×) → se não cede, memória (grafo em RAM, ver pages_read) → max_scan_tuples → cold start pós-restart; (4) tabela sinal→causa→ação. Fontes: [ParadeDB tuning](https://www.paradedb.com/learn/postgresql/tuning-pgvector), [Nerd Level Tech](https://nerdleveltech.com/pgvector-hnsw-postgres-18-production-tuning-tutorial), [ClickHouse scale vector search](https://clickhouse.com/resources/engineering/scale-vector-search-postgres).

## Caveats honestos (o M68 documenta, não esconde)

- **Approximate path (AQ/SBQ):** candidates reflete `walk_ef = ef·over_fetch` (`hnsw_page.rs:1611`) — a verdade do que o scan navegou, não o ef do result.
- **`explain_scan` reporta o ef PASSADO**, não o ef **crescido pelo iterative scan M52** (`scan.rs:325-330`, que vive no amgettuple fora do coletor) — capturar o ef iterative é uma 2ª complexidade, YAGNI.
- **Métrica runtime = catálogo consultável**, não Prometheus histogram (v1 honesto).

## Gap / rung-1

Reusar M67 + (a) expor candidates via retorno de `ground_search` + thread_local; (b) `theodb.explain_scan`; (c) doc de operação. Sem benchmark de performance (observabilidade). A única essential complexity é a assinatura de `ground_search_nodes` (~7 call sites mecânicos).
