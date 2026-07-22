# M104 / #99 — bounded columnar write memory (benchmark)

**Date:** 2026-07-16 · **Host:** droplet `theo-m104-pgrx19` (8 vCPU) · **PG:** 17.10 / pgrx 0.19.0 · **maintenance_work_mem:** 4MB
**Harness:** `theodb_rs/isolation/bench_m104_write.sh` · **Raw:** [`m104-write-envelope.json`](./m104-write-envelope.json)

## What is measured

The incremental stripe flush (M104): a columnar INSERT flushes a stripe once pending bytes exceed
`maintenance_work_mem`, so peak write memory is **O(maintenance_work_mem)**, not O(rows-in-xact) (#99). The
deterministic signal is **stripe-count linearity** — if memory were O(N), a single INSERT would buffer everything
and produce ONE stripe; with the bound, it produces N/(mwm/rowbytes) stripes and the pending set never exceeds mwm.

## Results (fixed mwm = 4MB)

| Rows | Stripes | Peak pending set (≈ total_bytes / stripes) |
|---|---|---|
| 50 000 | 1 | ~2.1 MB |
| 200 000 | 3 | — |
| 800 000 | 12 | — |
| 3 200 000 | 46 | ~2.9 MB |

**64× more rows → 46× more stripes (linear in N)**, while the **peak pending set stays ~constant (~2–3 MB ≈
maintenance_work_mem)**. Write memory is bounded by `maintenance_work_mem`, independent of the row count.

## Crash-safety (not regressed)

`theodb_rs/isolation/crash_columnar_incremental.sh` (GREEN): an aborted multi-stripe INSERT leaves **0 visible rows /
0 stripes** (atomic discard); a committed 60 000-row / **4-stripe** INSERT survives an immediate crash + WAL replay
**byte-identical**. The per-stripe pages→catalog-row-LAST invariant is preserved (the #46/#47 proofs stay GREEN) — a
crash/abort at any point of a multi-stripe INSERT yields the whole INSERT or none, never a torn partial set.

## Honest note

A `/proc` VmHWM sample was attempted but is dominated by the shared-library + planner RSS baseline, so it is not the
primary evidence; the stripe-count linearity + the ~constant per-stripe pending bound is the deterministic,
reproducible proof.
