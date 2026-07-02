---
slug: m34-ivfflat-reloption
milestone_id: M34
created_at: 2026-07-02
goal: theodb_ivfflat lists/probes configurable -> p50 <= pgvector at 1M
verdict: IMPLEMENTATION_COMPLETE
---

# M34 — theodb_ivfflat configurable lists/probes — implementation summary

## Goal (met)

Make `theodb_ivfflat` `lists` (build reloption) + `probes` (scan GUC) configurable so its Index Scan p50 reaches
**≤ pgvector** at 1M×128 (recall ≥ parity). **MET** — at matched probes (50/100, recall 0.99+) theodb p50 is
below pgvector (`docs/benchmarks/m34-ivfflat-reloption.{md,json}`).

## What shipped

| Task | Change | Files |
|---|---|---|
| T1.1 | `WITH (lists=N)` reloption (pgrx `amoptions`, pgvectorscale pattern); default 100; bounds [1,32768] rejected at DDL; `lists_from_relation` used by ambuild + VACUUM fold | `theodb_rs/src/am/options.rs` (NEW), `mod.rs`, `lib.rs` (`_PG_init`), `build.rs` |
| T2.1 | `SET theodb_ivfflat.probes` GUC (`GucRegistry`); default 10; scan reads `guc::probes().clamp(1,nlists)` | `theodb_rs/src/am/guc.rs` (NEW), `lib.rs`, `scan.rs` |
| fix-1 | k-means++ init O(k²·n·d) → O(k·n·d) (incremental min-distance) — a `WITH (lists=1000)` build was stuck > 26 min; result byte-identical (proven) | `theodb_rs/src/ann/ivf.rs` |
| fix-2 | structured IVFFlat directory page-chunked (format v2) — single-page dir capped lists at ~665; a `WITH (lists=1000)` build paniced | `theodb_rs/src/am/page.rs` |
| fix-3 | harness isolates each spec's measurement (drop other indexes) — two ivfflat-family indexes on one column let the planner cross-use them, flattening the pgvector sweep | `benchmarks/theodb_bench/harness.py` |
| T3.1 | reloption/GUC behavioral gate + 1M artifact driver | `benchmarks/tests/test_reloption.py`, `benchmarks/run_m34_ivfflat.py` |

## Evidence (SIFT1M, n=1M, lists=1000, matched probes, i7-1355U, single-thread builds)

| probes | theodb recall/p50 | pgvector recall/p50 |
|---|---|---|
| 10 | 0.874 / 2.99 ms | 0.866 / 2.72 ms |
| 50 | 0.992 / **12.77 ms** | 0.992 / 13.48 ms |
| 100 | 0.999 / **25.38 ms** | 0.999 / 28.32 ms |

At the high-recall points (probes 50/100) theodb p50 ≤ pgvector at parity recall — **the M32 ~8× gap (theodb 30.7
QPS / 32.5 ms at fixed lists=100) is closed**. Index size 537 MB < pgvector 550 MB.

## Honest residual (named future lever, not a defect)

theodb build is slower (575 s vs pgvector 33 s) — single-thread scalar k-means over the FULL 1M corpus (pgvector
samples + parallelizes). M34 already fixed the init O(k²)→O(k) + the directory page-chunking (both were hard
blockers to lists=1000); build-time parity (sampled/parallel k-means) is a separate future lever, not M34's
scan-latency DoD.

## Gates

- Build: `cargo pgrx install --release` 0 warnings; `CREATE EXTENSION` smoke OK (`_PG_init` registers reloption+GUC).
- Coexistence: 80+ tests green (M20-M22, ann, recall, sbq, index-AM M26/M31 with format v2, latency) — zero
  regression (defaults preserve behavior; k-means byte-identical proven standalone; v2 format read/written by the
  same tests).
- Reloption gate: `test_reloption.py` 4/4 (lists=1 exact vs lists=50 partial; probes monotone recall; default;
  lists=0 rejected).
- 1M artifact: `docs/benchmarks/m34-ivfflat-reloption.{md,json}` — mean±std ≥3 runs, hardware, isolated measurement,
  honest per-knob verdict, reproduction command.

## Discovery-driven scope

M34 was originally "both QPS levers" (ivfflat knobs + structured HNSW scan). Discovery sized the structured HNSW
scan at ~3-4× the M31 effort (page-native graph rewrite, high risk) — split to **M35** to avoid a rushed,
rework-prone bundle. M34 delivers the higher-leverage ivfflat lever fully.
