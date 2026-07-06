# M49 — cosine + inner-product opclasses: recall parity

Caracterização do recall@10 dos índices `cosine`/`ip` de `theodb_hnsw` e `theodb_ivfflat` vs o oracle seqscan da MESMA métrica (ground truth exato). Prova que o kernel fused pontua a métrica certa E que a estrutura ANN a preserva.

**Ambiente:** CPU `13th Gen Intel(R) Core(TM) i7-1355U`; código `git 2d8a465`. Dataset: 2000 vetores dim=16 uniform[-1,1] seed=42; 20 queries seeded; k=10.

## recall@10 (vs oracle same-metric)

| AM / métrica | recall@10 |
|---|---|
| theodb_hnsw/cosine | 1.0 |
| theodb_hnsw/ip | 1.0 |
| theodb_ivfflat/cosine | 0.89 |
| theodb_ivfflat/ip | 0.83 |

Todos ≥ 0.80 (o gate do M49). HNSW = 1.0 (perfeito); IVF 0.83–0.89 (aproximação inerente ao list-probing — não é defeito de métrica; ajustável por `probes`/`lists`).

## Crash-safety (edge #4)

theodb_hnsw cosine, 500 rows, SIGKILL + restart → top-5 identical pre-crash and post-recovery (metric preserved, not corrupted to L2). Cosine/IP usam o formato de página IDÊNTICO ao L2 (raw f32, ADR-2), então a maquinaria crash-safe do M48 (meta-pivot, WAL do INIT fork, fold) cobre por construção — provado aqui end-to-end.

## Caveats honestos

- **MIPS:** Inner product is NOT a metric (no triangle inequality). HNSW-over-IP works empirically (pgvector ships vector_ip_ops for hnsw); build and scan use the same negative-IP comparator, so the graph is self-consistent. No triangle-inequality assumption exists in the traverse.
- **IVF aproximado:** IVF recall (<1.0) is inherent to list-probing (only `probes` lists scanned), not a metric defect — HNSW achieves 1.0. Tune `probes`/`lists` for higher IVF recall.
- **Kernel:** IP/cosine use scalar-from-bytes fused kernels (zero-alloc, the M49 DoD). L2 keeps AVX2+FMA; adding AVX2 to IP/cosine is a future optimization if a latency benchmark shows a lag — not measured here.
- **Escopo:** Recall parity vs the SAME-metric seqscan oracle (exact ground truth). Not a QPS/latency claim.
