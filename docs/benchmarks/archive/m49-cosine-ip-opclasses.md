# M49 — cosine + inner-product opclasses: recall & crash-safety

Caracterização do recall@10 dos índices `cosine`/`ip` de `theodb_hnsw` e `theodb_ivfflat` vs o oracle **seqscan da MESMA métrica** (ground truth EXATO — não é um head-to-head vs pgvector; o oracle exato é um gate mais forte que approx-vs-approx, e um comparativo pgvector fica como follow-up).

**Ambiente:** CPU `13th Gen Intel(R) Core(TM) i7-1355U`; código `git 1af9da1`. Dataset: 2000 vetores dim=16 uniform[-1,1] seed=42; 20 queries; k=10.

## recall@10 (vs oracle exato same-metric)

| AM / métrica | recall@10 |
|---|---|
| theodb_hnsw/cosine | 1.0 |
| theodb_hnsw/ip | 1.0 |
| theodb_ivfflat/cosine | 0.89 |
| theodb_ivfflat/ip | 0.83 |

Todos ≥ 0.80 (o gate do M49). HNSW = 1.0 (perfeito); IVF 0.83–0.89.

## Crash-safety (edge #4)

Provado por **teste committado** `benchmarks/tests/test_am_crash.py::test_cosine_crash_safe (committed, reproducible)`: theodb_hnsw cosine, 500 rows, SIGKILL+restart → top-5 under <=> identical pre/post (metric preserved, not corrupted to L2). Cosine/IP usam o formato de página IDÊNTICO ao L2 (raw f32, ADR-2), então a maquinaria crash-safe do M48 (meta-pivot, WAL do INIT fork, fold) cobre por construção — e o teste prova end-to-end para cosine.

## Caveats honestos

- **MIPS:** Inner product is NOT a metric (no triangle inequality). HNSW-over-IP works empirically (pgvector ships vector_ip_ops for hnsw); build+scan use the same negative-IP comparator → the graph is self-consistent.
- **Recall IVF (honesto):** IVF cosine/ip recall (0.83-0.89 vs HNSW's 1.0) has TWO components, honestly: (1) inherent list-probing approximation (only `probes` lists scanned), AND (2) a build-quality gap — our IVF k-means seeds with L2 (ivf.rs:59) and uses arithmetic-mean centroids (ivf.rs:101), NOT the spherical k-means pgvector uses for cosine (ivfkmeans.c:33). Part of the gap is (2), not purely (1). Tracked follow-up: spherical k-means for IVF cosine/ip (or Design-A normalize escalation).
- **Kernel:** IP/cosine use scalar-from-bytes fused kernels (zero-alloc, the M49 DoD). L2 keeps AVX2+FMA; AVX2 for IP/cosine is a future optimization if a latency benchmark shows a lag — not measured here.
- **Escopo:** Correctness (recall vs exact oracle) + crash-safety. NOT a QPS/latency claim.
