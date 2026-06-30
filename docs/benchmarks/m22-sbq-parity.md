# M22 — Own SBQ scalar quantization recall@k + memory parity vs pgvectorscale

**Verdict:** `PARITY_REACHED`  
**Method:** measurement-first (ROADMAP-v2 M22). TheoDB's own SBQ quantizer + quantized search (`theodb.sbq_knn`, theodb_rs M22 — Hamming over the M21 IVFFlat carrier + full-precision f32 rerank) vs pgvectorscale's SBQ (`diskann`, storage_layout=memory_optimized). recall@10 on an exact brute-force ground truth (`theodb_bench.recall`), distance-thresholded.  
**Config:** n=1500, dim=32, queries=60, runs=3 (mean ± std), tolerance=0.05, bits=1, ivf lists=38, metric=l2, seed=2026.  
**Build:** the `theo-db` container image (postgres:17 + pgvector + pgvectorscale + theodb_rs).  

## Memory (bytes/vector)

| representation | bytes/vector | vs f32 |
|---|---|---|
| f32 (baseline) | 128 | 1× |
| **own SBQ (1-bit)** | **8** | **16.0× smaller** |
| pgvectorscale SBQ (1-bit) | 8 | 16.0× smaller |

Memory is **parity with pgvectorscale** (`own 8 ≤ pgvectorscale 8` — identical `ceil(dim·bits/64)·8` formula, EC-1) and a **16× reduction vs f32**. Honest: this is memory PARITY with pgvectorscale, not a memory win over it — the recall below is the substantive comparison.

## Recall@10 (own, mean ± std) — pgvectorscale diskann SBQ baseline = **0.6278 ± 0.0044**

| over_fetch | probes | own recall | parity (own ≥ pgvectorscale − tol) |
|---|---|---|---|
| 8 | 8 | 0.5000 ± 0.0000 | ❌ fail |
| 8 | 16 | 0.5467 ± 0.0000 | ❌ fail |
| 8 | 32 | 0.5083 ± 0.0000 | ❌ fail |
| 16 | 8 | 0.6250 ± 0.0000 | ✅ PASS |
| 16 | 16 | 0.7033 ± 0.0000 | ✅ PASS |
| 16 | 32 | 0.6783 ± 0.0000 | ✅ PASS |
| 32 | 8 | 0.6717 ± 0.0000 | ✅ PASS |
| 32 | 16 | 0.8433 ± 0.0000 | ✅ PASS |
| 32 | 32 | 0.8550 ± 0.0000 | ✅ PASS |

## Reproduce

```bash
python3 benchmarks/bench_sbq_index.py --n 1500 --dim 32 --nq 60 --runs 3 --tolerance 0.05 --write-doc
pytest benchmarks/tests/test_sbq_index.py::test_recall_memory_parity_gate -v
```

## Migration decision (coexistence, measurement-first)

Per blueprint ADR D1/D3 + plan ADR D1/D3: the own SBQ quantizer + search **coexist** with pgvectorscale — they read `embedding::real[]` and never touch pgvectorscale's `diskann`/SBQ or pgvector. RaBitQ (vectorchord) is **AGPL** and was NOT borrowed (D1); the own SBQ is permissive std-only code (zero new deps). Recall parity is reached at the operating points marked PASS (with rerank) at memory parity, so the own quantizer is a viable opt-in path; the on-disk planner-integrated AM is M22b. No pgvectorscale index is replaced by M22.
