# M103 — vector + columnar unified substrate (benchmark)

**Date:** 2026-07-16 · **Host:** DigitalOcean droplet `theo-m98-pgrx19` (8 vCPU, 15 GiB) · **PG:** 17.10 / pgrx 0.19.0
**Harness:** `theodb_rs/isolation/bench_m103.sh` (reproducible; 5 runs + 1 discarded warmup, mean ± population stddev) ·
**Raw:** [`m103-vector-columnar.json`](./m103-vector-columnar.json)

## What is measured — and what it does NOT claim

The co-resident filtered top-k (`theodb.vindex_knn_columnar`) reads a `theodb_columnar` table that stores the IVF
vector index (`tid`, `part_id`, `label`, `vec`) **co-resident with the analytical columns**, decoding ONLY the 4
index columns. Column pruning is **architecturally guaranteed** (the `decode_columns` projection skips the zstd of
unprojected columns — see `am/columnar.rs`) and proven by the `pg_test`s. This benchmark **quantifies the pruning
win** and states its honest scale limits.

- **Recall correctness (DoD 3)** is NOT a benchmark number — it is the byte-identity `pg_test`
  (`vindex::tests::m103_full_probe_byte_identical_to_exact_filtered`, GREEN): the co-resident filtered top-k is a
  byte-identical `(tid, dist)` sequence to the exact filtered brute-force (shared `am/scan.rs::Scored` tie-break +
  `vec::l2_dist_from_bytes` kernel). Recall is EQUAL by construction — never a claim.

## Primary result — isolated column-decode cost (the pruning win, free of the L2 confound)

The end-to-end knn is dominated by the full-probe L2 rerank (identical regardless of payload width), so it cannot
quantify the pruning win. The honest control isolates the **decode** cost: decode only the 4 index columns vs ALL
columns, on the WIDE index (16 analytical cols), same rows, no rerank.

| Decode | Columns | Bytes decoded | Time (mean ± stddev, 5 runs) |
|---|---|---|---|
| pruned (index only) | tid, part_id, label, vec | 2.4 MB | **49.6 ms ± 0.3** |
| full | + 16 × float8 payload | 8.8 MB | **219.8 ms ± 1.8** |

**Column pruning saves 77.4 % of the decode time** — a real, above-the-floor win (the stddevs, ~0.3–1.8 ms, are far
smaller than the 170 ms gap). Decoding only the index columns is **4.4× faster** than decoding the full row.

## Secondary — end-to-end knn latency invariance to analytical width

| Index | Analytical payload | knn latency (mean ± stddev) |
|---|---|---|
| `idx_narrow` | 1 × float8 | 77.6 ms ± 0.4 |
| `idx_wide` | 16 × float8 | 78.7 ms ± 0.9 |
| **wide / narrow ratio** | | **1.014** |

The end-to-end filtered vector scan latency is ~unchanged (ratio 1.014, within a stddev) as the payload grows from 1
to 16 columns. This shows pruning **adds no width-dependent cost** to the query — but it is the isolated decode
control above, not this ratio, that quantifies the win (the knn latency is L2-dominated). On-disk size (wide 1.49 MB
vs narrow 0.32 MB, 4.67×) is stated as a separate fact; **on-disk size is not decode cost** — the measured decode
delta (77.4 %) is the real magnitude.

## Composed filtered-knn + analytical aggregation

`SELECT avg(i.p0) FROM theodb.vindex_knn_columnar(idx, q, 10, 64, 0) knn JOIN idx i USING(tid)` — the scalar-prefiltered
vector top-k + the analytical aggregation compose in **one plan** (**224.7 ms ± 2.1**).

## Honest ceiling (ADR D4)

- A **cost / scale / composability** win, **orthogonal to vector recall**. Recall is EQUAL by construction (the
  byte-identity GATE), never framed as a win.
- **Column pruning is measured (77.4 % of decode time saved)** at N=50 000 / dim=8 / in-RAM. The **VALUE at scale**
  — where pruning avoids reading whole segments and the row-store pays the full-row cost — is unproven until
  measured **out-of-RAM** with realistic payload widths / dims on a dedicated large box (the M57/M88 discipline: a
  crossover is direction-only until measured on a non-saturated box). This benchmark measures the in-RAM decode
  delta; the out-of-RAM extrapolation is stated, not claimed.
- **NO QPS-vs-ScaNN claim.** The M73/M74 paradigm ceiling (anisotropic AH-LUT + no MVCC/WAL tax) is untouched by
  co-residence — this milestone does not close it and makes no throughput-superiority claim.
- **Reduced-probe:** at `probes < nlist` the co-resident scan is an IVF approximation (the `part_id` column IS the
  real IVF partition); the columnar query path currently runs full-probe (the GATE), and the `probes` argument is
  reserved (a documented follow-up). Reduced-probe ordering correctness is covered by the in-memory `pg_test`.

## Reproduce

```bash
# on the droplet, after `cargo pgrx install --features pg17 --no-default-features`
cd theodb_rs/isolation && bash bench_m103.sh
```
