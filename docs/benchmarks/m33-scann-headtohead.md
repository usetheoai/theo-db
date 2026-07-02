# M33 — head-to-head vs AlloyDB/ScaNN (North Star vector superiority)

**Dataset:** sift-128-euclidean (SIFT1M) · n=1,000,000 dim=128 k=10 · queries=1000 (seed 42) · runs=3 · host=paulohenriquevn

**Baseline:** AlloyDB is GCP-managed (no local run); ScaNN OSS is the DoD-sanctioned proxy — the algorithm behind AlloyDB's vector index (arXiv:1908.10396).

> **CAVEAT (library vs database):** ScaNN is a pure IN-MEMORY ANN library (no persistence/transactions/SQL); theodb_ivfflat is a persistent transactional PostgreSQL index. Raw-search numbers compare the ALGORITHM axis; they do NOT make ScaNN a database. ScaNN peak-RSS includes the full in-memory corpus; theodb index bytes are on-disk index structure — different measures.

**ScaNN:** v1.4.2 · build 20337 ms · peak RSS 1194 MB (incl. in-memory corpus) · config num_leaves=1000 training_sample=250000 pre_reorder=256

## Recall–QPS frontier (partition-fraction matched)

| system | params | recall@10 | QPS | p50 (ms) | p95 (ms) | p99 (ms) |
|---|---|---|---|---|---|---|
| scann | leaves_to_search=1 | 0.3758 | 19738.1 | 0.05 | 0.08 | 0.11 |
| scann | leaves_to_search=10 | 0.8733 | 8044.4 | 0.12 | 0.19 | 0.22 |
| scann | leaves_to_search=50 | 0.9909 | 2866.7 | 0.33 | 0.52 | 0.75 |
| scann | leaves_to_search=100 | 0.9970 | 1615.8 | 0.57 | 0.93 | 1.1 |
| scann | leaves_to_search=200 | 0.9985 | 816.5 | 1.84 | 2.44 | 2.82 |
| scann | leaves_to_search=400 | 0.9985 | 312.3 | 4.41 | 9.79 | 13.42 |
| theodb_ivfflat | probes=1 | 0.3734 | 1627.4 | 0.60 | 1.0190780492848714 | 1.3588220770907318 |
| theodb_ivfflat | probes=10 | 0.8737 | 331.1 | 2.99 | 4.705635452774003 | 5.798276020577758 |
| theodb_ivfflat | probes=50 | 0.9924 | 77.9 | 12.77 | 17.581593999784673 | 20.85089138468901 |
| theodb_ivfflat | probes=100 | 0.9991 | 38.8 | 25.38 | 33.68442374485311 | 37.76926271115371 |
| pgvector_ivfflat | probes=1 | 0.3744 | 2485.8 | 0.37 | 0.6973169965931446 | 1.0346412543003676 |
| pgvector_ivfflat | probes=10 | 0.8661 | 348.7 | 2.72 | 4.607763750391312 | 5.276304920116665 |
| pgvector_ivfflat | probes=50 | 0.9923 | 71.8 | 13.48 | 20.317285899727718 | 23.58218436107563 |
| pgvector_ivfflat | probes=100 | 0.9993 | 36.0 | 28.32 | 35.9793695010012 | 41.11573080655942 |

## Verdict @ recall ≥ 0.99 (matched high-recall operating point)

| dimension | verdict | ScaNN | theodb_ivfflat | pgvector_ivfflat |
|---|---|---|---|---|
| best QPS | GAP | 2866.7 | 77.91121151983064 | 71.7756129412639 |
| p50 latency (ms) | GAP | 0.33 | 12.765203999151709 | 13.481656496878713 |
| recall@10 reachable | PARITY | 0.9909 | 0.9924000000000001 | 0.9923000000000001 |
| memory | INDETERMINATE (different measures — see caveat: ScaNN peak-RSS vs theodb index bytes) | 1194 MB RSS | 537075712 B index | — |

**Overall:** North Star (vector superiority vs AlloyDB): theodb_ivfflat vs the ScaNN algorithm on raw ANN search — see verdict_per_dimension. theodb's value is vector search INSIDE a transactional database; ScaNN's is a specialized in-memory library. A GAP on raw QPS is honest and expected (ScaNN uses learned anisotropic quantization + SIMD AH; theodb_ivfflat is full-precision IVFFlat).

## Reproduction

```
pip install scann   # dev-only, Apache-2.0, needs AVX2
python3 benchmarks/run_m33_scann.py --n-queries 1000 --runs 3
```

theodb/pgvector rows reused verbatim from `docs/benchmarks/m34-ivfflat-reloption.json` (same SIFT1M, hardware, neighbors-GT). ScaNN measured on the SAME 1000-query subsample (seed 42).
