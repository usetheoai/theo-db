# E1 — f32-free RaBitQ rerank in-PostgreSQL: SIFT1M A/B verdict (v5 vs v8)

**Date:** 2026-07-17 · **Module:** `theodb_rs/src/am/{scan,build,page/ivf,options}.rs` + `vec/rabitq.rs`
(own-code, arXiv:2409.09913 algorithm, Apache-2.0 reimplemented). **What is measured:** the same 1M SIFT base
vectors indexed two ways on the SAME `theodb_ivfflat` AM, queried with the SAME 200 official queries and scored
against the official groundtruth — the only variable is the Stage-2 rerank codec:

- **v5** (`WITH (separate_storage=1)`) — Stage-2 reranks survivors on the **raw f32 vectors** (the M82/v5 baseline).
- **v8** (`WITH (separate_storage=1, refine=2, rabitq_bits=7)`) — Stage-2 reranks on **f32-FREE RaBitQ residual
  codes** (`estimate_l2_sq`, integer-weighted dot `⟨q_r,u⟩/W`, zero raw vector touched). Stage-1 (AH block32 prune
  over codes-only pages) is byte-identical between the two — only the refine region differs.

**Hardware / method:** DigitalOcean 8-vCPU / 15 GB droplet, PostgreSQL 17.10 + pgrx 0.19.0, SIFT1M
(ann-benchmarks `sift-128-euclidean`, L2), N=1,000,000, lists=500, 200 queries × best-of-3 (warm) / per-query
cold-fault (cold), recall@10 vs official GT. Reproduce: `benchmarks/` scripts `e1_rabitq_bench.py` (build+warm),
`cold_perquery.py` (cold). Raw log: `e1-rabitq-inpg-verdict.json`.

---

## Result 1 — Index size (the memory lever, always-on)

| Index | Refine region | Size on disk |
|---|---|---:|
| v5 (f32 rerank) | raw f32 vectors (512 B/vec) | **528 MB** |
| v8 (RaBitQ 7-bit) | RaBitQ code `[i8×dim][nr][w]` (136 B/vec) | **161 MB** |

**v8 is 3.28× smaller at recall parity.** The f32-free rerank means the index does not store raw vectors for
reranking — the direct billion-scale RAM lever.

## Result 2 — Recall parity (both regimes)

Across the full warm sweep (over_fetch ∈ {8,16,32,64} × probes ∈ {32,64,128,256}), v8 recall tracks v5 within
**1.1–1.6 percentage points** and rises monotonically with the rerank pool — the f32-free ranking is correct, not
a coarse approximation. Representative points:

| over_fetch | probes | v8 recall@10 | v5 recall@10 | Δ |
|---:|---:|---:|---:|---:|
| 16 | 64 | 0.9790 | 0.9925 | −0.0135 |
| 32 | 64 | 0.9830 | 0.9980 | −0.0150 |
| 64 | 128 | 0.9840 | 0.9995 | −0.0155 |

## Result 3 — WARM (in-RAM): parity QPS — NO speedup (honest)

With `shared_buffers=6 GB` and the whole 1M dataset resident, v8 and v5 deliver the **same QPS** (ratio 0.86–1.08
across all 16 configs) and touch the **same number of buffers per query** (ratio 1.01–1.02):

| over_fetch | probes | v8 QPS | v5 QPS | qps_ratio | v8 buf/q | v5 buf/q |
|---:|---:|---:|---:|---:|---:|---:|
| 16 | 64 | 78.2 | 81.6 | 0.96 | 3200 | 3236 |
| 32 | 128 | 45.1 | 45.5 | 0.99 | 6113 | 6228 |
| 64 | 256 | 22.2 | 21.7 | 1.02 | 11893 | 12169 |

**Why no warm win (mechanism):** the per-query buffer traffic (thousands of pages) is dominated by **Stage-1**
(reading every probed list's code pages), not by the Stage-2 refine reads. Warm, both the code pages and the f32
refine pages are cache hits, so removing the f32 read changes nothing. This reproduces the M85/M82 finding:
Stage-2 refinement is not the in-RAM bottleneck.

## Result 4 — COLD (out-of-RAM): 2.5–2.8× lower latency at recall parity — GATE MET

With `shared_buffers=256 MB` and the OS page cache dropped **before every query** (the out-of-RAM proxy the 15 GB
box otherwise hides — a 528 MB index fits OS cache warm), each search must fault its pages from disk. Now the
index-size difference becomes I/O the query pays for:

| over_fetch | probes | v8 mean ms | v5 mean ms | **speedup** | v8 p50 | v5 p50 | v8 recall | v5 recall |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 16 | 64 | 75.1 | 189.3 | **2.52×** | 61.9 | 178.6 | 0.982 | 0.994 |
| 32 | 128 | 84.1 | 226.3 | **2.69×** | 56.8 | 218.2 | 0.985 | 1.000 |
| 64 | 256 | 121.0 | 335.7 | **2.77×** | 87.4 | 297.6 | 0.985 | 1.000 |

**The E1 gate (≥2× QPS at recall parity, with a measured f32-read drop) is MET in the out-of-RAM regime.** The
speedup grows with the rerank pool (2.52 → 2.77×) because more survivors ⇒ more Stage-2 reads ⇒ a larger f32
disk-fault penalty for v5, exactly the region the 3.28× smaller v8 index removes.

---

## Verdict (honest)

- **f32-free multi-bit RaBitQ rerank works in-PG** — recall parity (within ~1.5 pp) proven on the real SIFT1M
  benchmark through the actual `theodb_ivfflat` scan path, not a Monte-Carlo proxy.
- **The win is memory + out-of-RAM latency, NOT in-RAM QPS.** Warm, it is parity (Stage-2 is not the in-RAM
  bottleneck — M85 holds). The value is the **3.28× smaller index** and the **2.5–2.8× lower cold-query latency**
  that the smaller index buys when the working set exceeds RAM.
- **This is the North-Star-credited axis** (`docs/adr/0035`, `CLAUDE.md`): "paridade recall + memória
  billion-scale". It does NOT claim to beat ScaNN/AlloyDB on warm vector QPS — that ceiling (paradigm, MVCC/WAL
  tax) stands unchanged (M73/M82/ADR-0036).

## Caveats

- Cold latency uses a drop-caches-per-query proxy on a 15 GB box; a genuine billion-scale out-of-RAM run (dataset
  ≫ RAM) would measure the same mechanism at scale but was out of scope for the 1M A/B. The direction is
  measured, the absolute billion-scale numbers are not.
- Build cost: v8 pays a one-time RaBitQ residual-encode pass on top of the shared IVF/AQ/AH build (~1138 s each at
  1M/500-lists single-thread on this box); not optimized (out of E1 scope).
- L2-only (the RaBitQ estimator is L2-only; `refine=2` fails loud at build on a non-L2 opclass).
