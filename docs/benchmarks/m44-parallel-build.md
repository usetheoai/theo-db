# M44 — theodb_hnsw parallel build: 2.82× @ 50k (rigorous), 1.95× @ 1M, recall parity

**Date:** 2026-07-03
**Verdict:** **WIN** — the theodb_hnsw graph build is **2.82× faster** (rigorous 3-sample back-to-back A/B @ 50k,
std bands separated) at **recall parity**, and **8.4 min → 4.3 min @ 1M** (1.95×). Completes the build lineage
24 min (scalar) → 8.4 min (M43 SIMD) → 4.3 min (M44 parallel).
**Type:** A/B benchmark (build-time + recall parity — the honest oracle). Recall is PARITY, not deterministic
(racy parallel insert → a different but recall-equivalent graph each run — ADR D2).

## What changed

The in-memory HNSW graph build (`ann/hnsw_parallel.rs`, new) runs concurrently across CPU cores with
`std::thread::scope` (borrows the read-only corpus without `Arc`; a worker panic re-raises on join → fail-loud) +
a per-node `RwLock` on the neighbor lists (readers/search share, writers/link exclusive; deadlock-free by
construction — one node lock at a time). `HnswIndex::build` dispatches on corpus size: `< 4096` builds
sequentially (deterministic; the tiny AM test corpora are unchanged), `≥ 4096` builds in parallel. No new
dependency (std only). No PG-parallel-worker machinery (the pgvector approach) — the graph build is pure Rust over
the already-loaded corpus, so worker threads never touch Postgres.

## A/B @ 50k (rigorous — 3-sample back-to-back, dim=128, mean±std)

Sequential = `theo-db:m43`; parallel = `theo-db:m44`; same corpus, `max_parallel_maintenance_workers=0` on both.

| build | mean±std | recall@10 |
|---|---|---|
| sequential (m43) | 33.0 ± 6.3 s | 0.6845 |
| **parallel (m44)** | **11.7 ± 2.6 s** | **0.6900** |

**Build speedup: 2.82×** — std bands separated (33±6 vs 12±3 → significant). **Recall parity** (Δ +0.0055, within
±0.03 — marginally higher, consistent with the build/scan SIMD-metric alignment). D3 verdict: `PARALLEL_WINS`.

## @ 1M (target scale, SIFT1M, real GT)

| build | wall-clock | recall@10 |
|---|---|---|
| sequential (M43 baseline) | 503.7 s (8.4 min) | ~0.96 |
| **parallel (m44)** | **257.9 s (4.3 min)** | **0.9730** |

**1.95×** at 1M — **lower than the 50k 2.82×**, honestly: lock contention grows with scale (more threads competing
on a denser graph's back-links). Amdahl + contention cap the speedup below the 12-core count. The 1M figure is
approximate (the 503.7 s baseline was measured in the M43 cycle, not back-to-back), so the 50k **2.82×** is the
rigorous controlled number; 1M confirms the build dropped 8.4 min → 4.3 min at recall parity (0.9730 ≈ M42/M43).

## Correctness & safety

- **Recall parity:** 8/8 `benchmarks/tests/test_index_am.py` green on `theo-db:m44` (sequential path, tiny corpora);
  parallel recall @50k 0.69 vs seq 0.685, @1M 0.9730 (parity). Live: a 6000-node parallel build in 0.4 s with a
  correct self-match (top-1 of a corpus point is itself).
- **Race-freedom by construction:** Rust's `RwLock` forbids `&mut` aliasing across threads (no data race compiles);
  the build is deadlock-free (each insert holds at most one node lock at a time, no nesting).
- **Panic-safety:** `std::thread::scope` re-raises a worker panic on join (fail-loud, no silent-wrong graph); the
  pure-Rust workers hold no PG state.
- **Non-determinism (honest cost):** the parallel insert order races → a different graph each run (level assignment
  stays deterministic). No test asserts build determinism; recall parity is the gate (ADR D2). Reproducibility
  regresses — documented.

Reproduce: `python3 benchmarks/run_m44_parallel_build.py --n 50000 --dim 128 --runs 3 --seq-port <m43> --par-port <m44>`.

## Honest bottom line

A real, recall-preserving **2.82× (controlled @50k) / 1.95× (@1M)** parallel-build speedup — the theodb_hnsw build
drops from 8.4 min to 4.3 min at 1M (24 min → 4.3 min across M43+M44). The speedup is Amdahl+contention-limited
(not 12×) and shrinks with scale. Combined with M41 (scan) + M42 (SIFT1M superiority signal) + M43 (SIMD build),
the theodb_hnsw carrier is now competitive on build, scan, AND recall×QPS.

## Next (evidence-based)

- Contention reduction (lock striping / per-layer locks) could lift the 1M speedup — a measured follow-up if build
  time becomes critical again.
- The bigger open item remains the mean±std + independent reproduction of the theodb_hnsw-vs-pgvector-hnsw margin
  (M42) for a publishable superiority claim.
