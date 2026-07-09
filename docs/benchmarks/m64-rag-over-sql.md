# M64 — RAG-sobre-SQL unificado: "1 SQL vs N app-calls" medido (round-trips + latência a recall-igualado)

**Date:** 2026-07-09 · **Milestone:** M64 · **Métrica primária:** round-trips/query (estrutural) · **Suporte:** p50/p95/p99
**Harness:** `benchmarks/run_m64_rag_over_sql.py` (reusa `theodb_bench.metrics`, espelha `run_m63_vector_join.py`) · **JSON:** `docs/benchmarks/m64-rag-over-sql.json`
**ADR:** [`0023-m64-rag-unified-not-columnar-planner.md`](../adr/0023-m64-rag-unified-not-columnar-planner.md) (D1 Path 1 row-store / Path 2 columnar dois-statements; D2 helper rejeitado)

> **Veredito estrutural (o gate do DoD) — CUMPRIDO e PROVADO por `#[pg_test]`:** a query RAG unificada
> (`WITH retrieved AS (WHERE cat ORDER BY emb <=> $q LIMIT k) SELECT string_agg(content) …`) recupera
> **exatamente** o top-k filtrado do oráculo exato (recall preservado — compor não degrada) e é
> **read-your-writes** na mesma SQL/snapshot MVCC. 2 `#[pg_test]` GREEN. O benchmark abaixo mede o valor que
> o campo não publica: o head-to-head **"1 SQL vs N app-calls"** — round-trips economizados a recall-igualado.

---

## 1. O gate de correção — 2 `#[pg_test]` GREEN

`cargo pgrx test pg17 rag_unified` → **2 passed** (contra o stack real: theodb_rs + vector + vectorscale + theodb):

- **`rag_unified_query_preserves_recall`** — a query composta (filtro `WHERE cat` + retrieval `ORDER BY emb <=> $q LIMIT k` + `string_agg` contexto) recupera o set `retrieved.id` **idêntico** ao oráculo exato `SELECT id WHERE cat=$c ORDER BY emb <=> $q LIMIT k` (recall preservado — a composição não perde vizinhos); e o `string_agg` concatena **exatamente K** docs.
- **`rag_unified_read_your_writes`** — uma linha INSERTada dentro da txn é recuperável pela RAG-query na **mesma SQL e no mesmo snapshot MVCC** (via pending region do `theodb_hnsw`). Nota de rigor: um cliente app-layer também obtém read-your-writes se abrir uma txn explícita; o diferencial do Path 1 é fazê-lo numa SQL única, num snapshot único, **sem coordenar múltiplas chamadas**.

## 2. O benchmark — dois braços, mesma recuperação (recall idêntico por construção)

| Braço | O que faz | round-trips | p50 (ms) | p95 (ms) | p99 (ms) |
|---|---|---|---|---|---|
| **A — unified** | 1 statement: `WITH retrieved AS (WHERE cat ORDER BY v <=> q LIMIT k) SELECT array_agg(id), string_agg(content)` — filtro+retrieval+assemble server-side | **1** | **6.721** | 8.849 | 9.479 |
| B — app-layer | 2 statements: (1) retrieve ids via vetor SQL, (2) hydrate content `WHERE id = ANY(ids)` (PK-served), depois assemble client-side | **2** | 7.284 | 9.400 | 9.919 |

**Método:** n=5000, dim=128, k=10, cosine, 3 runs × 50 reps/run, gaussian-mixture (5 clusters, `cat` filter), droplet c-8. Ambos os braços usam o MESMO retrieval (o `theodb_hnsw` index-served) → **recall idêntico por construção**. O `recall-match gate` confirma antes de comparar latência.

## 3. Veredito (por-eixo, honesto)

- **Recall-match gate — PASS** (jaccard **1.0**, "same top-k set"). Os dois braços recuperam o mesmo conjunto → a latência É comparável (a comparação não é de capacidade, é de estrutura).
- **Round-trips (o mecanismo estrutural) — 1 vs 2 (saved 1, ratio 2.0).** O unified faz filtro+retrieval+assemble em 1 ida; o app-layer hidrata numa 2ª ida. **Esta é a vitória estrutural** — independe de escala/carga.
- **Latência p50 — UNIFIED_FASTER, mas MODESTO co-located: 6.721 vs 7.284 ms (~1.08×, 8%).** Co-located, o custo do 2º round-trip + hydrate é pequeno (exatamente o esperado — a nota do harness previu isso). **A economia de round-trip AMPLIFICA sobre um hop de rede real** (onde cada ida custa o RTT); reportamos o co-located (isola CPU) e declaramos que o ganho cresce com a latência de rede.

## 4. O que o benchmark NÃO mede (anti-interpretação-errada)

- **NÃO** demonstra superioridade algorítmica de retrieval — **ambos os braços usam exatamente o mesmo top-k** (recall idêntico, jaccard 1.0). Mede APENAS a diferença estrutural: compor in-SQL (1 round-trip, 1 snapshot MVCC) vs compor no cliente (2 round-trips, coordenação de múltiplas chamadas).
- A leg **columnar NÃO é planner-integrada** (ADR-0023 D1): pg_duckdb proíbe DuckDB em função (ADR-0021), row-store + Parquet são 2 engines que 1 planner não unifica. O columnar RAG é o padrão M62 de **dois statements** (honesto, não mascarado).

## 5. Caveats honestos

1. **Box não idle:** `load_per_run` = [3.21, 4.44, 4.63] (a build do pgvectorscale ainda finalizava numa c-8). Os **absolutos** (6.7ms) estão inflados pela carga; o **delta A-vs-B** é robusto (os dois braços foram medidos interleaved na MESMA box, mesma carga, mesma run).
2. **Co-located:** o cliente e o servidor no mesmo host → o round-trip custa ~microssegundos. Por isso o gap de latência é modesto (8%); o valor estrutural (round_trips 1 vs 2) é o que cresce sobre rede real. Honesto (public-copy.md §4): não afirmamos um ganho de latência grande co-located.
3. **Dados sintéticos** (gaussian-mixture, 5 clusters). A direção (menos round-trips, mesmo recall) é mecânica; os absolutos movem com dados reais.
4. **Rerank de 2ª ordem (cross-encoder)** é M65 (`ai.rerank`, não existe ainda); o rerank disponível hoje é o score RRF (M53) ou `ai.rank` (LLM). Fora do escopo M64.

## 6. Reprodução

```
# no droplet, instância pgrx pg17 com CREATE EXTENSION theodb_rs CASCADE (vector+vectorscale+theodb):
PGHOST=localhost PGPORT=<pgrx_port> PGUSER=theo \
  python3 benchmarks/run_m64_rag_over_sql.py --n 5000 --dim 128 --k 10 --runs 3 \
    --out docs/benchmarks/m64-rag-over-sql.json
```

Dados brutos: `docs/benchmarks/m64-rag-over-sql.json`.
