# M103 — vector + columnar unified substrate (benchmark)

**Date:** 2026-07-16 · **Host:** DigitalOcean droplet `theo-m98-pgrx19` (8 vCPU, 15 GiB) · **PG:** 17.10 / pgrx 0.19.0
**Harness:** `theodb_rs/isolation/bench_m103.sh` (reproducible) · **Raw:** [`m103-vector-columnar.json`](./m103-vector-columnar.json)

## What is measured

The co-resident filtered top-k (`theodb.vindex_knn_columnar`) reads a `theodb_columnar` table that stores the IVF
vector index (`tid`, `part_id`, `label`, `vec`) **co-resident with the analytical columns**. It decodes ONLY the 4
index columns — the analytical columns are never touched. The cleanest measurable proof of **column pruning**: the
knn latency is INVARIANT to the analytical-column width. Two co-resident indexes are built on the SAME vectors —
one NARROW (1 float8 payload), one WIDE (16 float8 payload columns) — and the knn latency is compared.

The **recall correctness** (DoD 3) is NOT a benchmark number — it is the byte-identity `pg_test`
(`vindex::tests::m103_full_probe_byte_identical_to_exact_filtered`, GREEN): the co-resident filtered top-k is a
byte-identical `(tid, dist)` sequence to the exact filtered brute-force, sharing the M90-M95 `Scored` tie-break +
`vec::l2_dist_from_bytes` kernel. Recall is EQUAL by construction — never a claim.

## Results — column pruning (N = 50 000, dim = 8, 5 runs)

| Index | Analytical payload | knn latency (mean) | on-disk size |
|---|---|---|---|
| `idx_narrow` | 1 × float8 | **79.43 ms** | 0.32 MB |
| `idx_wide` | 16 × float8 | **79.27 ms** | 1.49 MB |
| **wide / narrow latency ratio** | | **0.998** | 4.67× larger on disk |

The WIDE index is **4.67× larger on disk** (the analytical columns), yet the filtered vector scan latency is
**unchanged (ratio 0.998)**. This is column pruning: the co-resident scan decodes only `tid`/`part_id`/`label`/`vec`
and never pays the zstd-decode cost of the analytical columns. In a row-store the analytical columns would be
interleaved in every heap tuple and read regardless.

## Composed filtered-knn + analytical aggregation

`SELECT avg(i.p0) FROM theodb.vindex_knn_columnar(idx, q, 10, 64, 0) knn JOIN idx i USING(tid)` — the scalar-prefiltered
vector top-k + the analytical aggregation compose in **one plan** (**225.7 ms**), reading the vector columns for the
search and only the projected analytical column for the aggregate.

## Honest ceiling (ADR D4)

- This is a **cost / scale / composability** win, **orthogonal to vector recall**. Recall is EQUAL by construction
  (the byte-identity GATE), never framed as a win.
- **NO QPS-vs-ScaNN claim.** The M73/M74 paradigm ceiling (anisotropic AH-LUT + no MVCC/WAL tax) is untouched by
  co-residence — this milestone does not close it and makes no throughput-superiority claim.
- **Not measured:** out-of-RAM behavior at billion-scale (the honest projection: the column-pruned scan reads a
  fraction of the segment, so it degrades more gracefully than a full-row scan when the working set exceeds RAM —
  the M57/M88 lesson says this must be measured on a dedicated large box before any at-scale claim). The pruning
  ratio here is the deterministic in-RAM proof; the out-of-RAM extrapolation is stated, not claimed.
- **Reduced-probe recall:** at `probes < nlist` the co-resident scan is an IVF approximation of the exact set (the
  `part_id` column IS the real IVF partition); the in-memory `m103_reduced_probe_is_subset_recall_of_exact` test
  covers correctness of ordering. The GATE claim is the full-probe byte-identity.

## Reproduce

```bash
# on the droplet, after `cargo pgrx install --features pg17 --no-default-features`
cd theodb_rs/isolation && bash bench_m103.sh
```
