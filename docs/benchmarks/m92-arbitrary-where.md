# M92/M93 — arbitrary-WHERE filtered vector search: INLINE (Custom Scan node) vs POST (native), MEASURED

**Date:** 2026-07-13 · **Host:** DO 8-vCPU (Intel Xeon Gold 6548N, 15 GB) · **Verdict:** `GO — INLINE dominates POST`

The Custom Scan node (M92/M93) pushes an arbitrary scalar `WHERE` into the IVF-AQ vector scan: it runs the planner's
native bitmap sub-plan over the scalar column's btree, materializes the matching TIDs into a membership set, and the
vector scan's Stage-1 skips non-members inline (+ M91 adaptive probing). The gate: does inline-by-bitmap beat the
native post-filter? **It does — on BOTH recall and QPS, at the selective regime where the post-filter starves.**

## Result (SIFT1M, real neighbors, `cat = id%1000` btree-indexed, k=10, 100 queries, v5 plain-vector index)

| selectivity | POST (native post-filter) | INLINE (node, probes=64) | Δ recall | QPS ratio |
|---|---:|---:|---:|---:|
| **1%** (`cat<10`) | recall 0.673 @ 21.2 QPS | recall **0.953** @ **265.7** QPS | **+0.28** | **~12×** |
| **5%** (`cat<50`) | recall 0.593 @ 91.9 QPS | recall **0.915** @ **125.9** QPS | **+0.32** | **~1.4×** |

INLINE recall-QPS frontier (probe sweep — INLINE sits on a strictly better frontier than POST):

| | probes=64 | 128 | 256 | 500 |
|---|---:|---:|---:|---:|
| **1%** recall/QPS | 0.953 / 266 | 0.969 / 206 | 0.968 / 147 | 0.969 / 93 |
| **5%** recall/QPS | 0.915 / 126 | 0.910 / 107 | 0.910 / 82 | 0.912 / 56 |

Ground truth = exact seqscan-filtered top-10 (`WHERE <scalar> ORDER BY e <-> q LIMIT 10`), same 100 SIFT queries.

## Why INLINE wins

- **POST (native post-filter):** the vector index scan scores candidates in distance order; the executor Filter drops
  the non-matching ones AFTER. At a selective filter, the probed lists' rerank pool is dominated by non-matching rows,
  so the filtered top-k is **starved** (recall 0.59–0.67) and the M87 iterative re-search thrashes (21–92 QPS). This is
  the exact problem M90 identified for label filters, here for arbitrary columns.
- **INLINE (Custom Scan node):** the bitmap membership is consulted in Stage-1 — non-matching candidates are **skipped
  before** they cost a rerank slot, so the pool fills with matching candidates → recall stays high (0.92–0.97); the
  M91 adaptive probing keeps it fast. Result: **higher recall AND higher QPS**.

## Correctness (the gate, first)

The `m93_t2` pg_tests prove the INLINE result is **byte-identical to the exact seqscan-filtered top-k** on a non-label
column; the **pending region** (post-build INSERTs) and **lossy bitmap blocks** are rechecked out by the vector child's
own qpqual Filter (the MVCC recheck). **262 pg_tests GREEN**; the GUC-off path is byte-identical (zero regression). The
node is OFF by default (`theodb.enable_vecfilter`).

## Honest boundary

- The inline pre-filter engages on both the **v5 plain-vector** and **v7 label** IVF-AQ layouts (the realistic
  plain-`(e)`-index case).
- **Single run** per point — the margin is large (+0.28 recall, up to 12× QPS), so the direction is unambiguous; a
  multi-run mean±std is a follow-up if a tighter claim is needed.
- The node's **cost model is a spike heuristic** (cost below the cheapest base path to force selection); an honest
  cost model (membership-reduced candidate count) is a follow-up.
- **NOT a QPS-superiority claim vs ScaNN/AlloyDB** — the paradigm ceiling (M73/M82) stands. This is a **capability +
  recall-QPS-Pareto** result vs the native post-filter, measured. Boundary: scalar `WHERE` over existing indexes; NOT
  the AlloyDB core cross-index mid-query re-plan (tier ④).

## Verdict

**`GO`.** Arbitrary-`WHERE` filtered vector search via the Custom Scan node beats the native post-filter on both recall
(+0.28 to +0.32) and QPS (1.4–12×) at 1–5% selectivity — the AlloyDB "inline filtering" tier ③ mechanism, delivered in
a permissive OSS Postgres extension, measured.

## Provenance

- Harness: `benchmarks/m92_arbitrary_where_bench.py`. Raw log: `docs/benchmarks/m92-arbitrary-where.log`. JSON: `docs/benchmarks/m92-arbitrary-where.json`.
- Impl: commits `4d542a7` (2-child node), `c8db947` (v5 inline + adaptive). Blueprint: `knowledge-base/discoveries/blueprints/arbitrary-where-custom-scan-blueprint.md`.
