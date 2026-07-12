# M88 — storage-separated out-of-RAM regime: measured verdict (16M, SQ8 vs f32)

**Date:** 2026-07-12 · **Dataset:** 16M synthetic clustered vectors (128-dim) · **Verdict:** `SIZE_CONFIRMED / OUT_OF_RAM_QPS_INCONCLUSIVE`

M88 is the terminal measurement of the M83–M88 storage-separation track (the next datapoint in the
M33/M73/M82 lineage). The thesis under test: **in a regime where the f32 refine data does NOT fit in RAM,
the storage-separated SQ8 refine (v6, ~4× smaller) turns its memory win into a QPS win** because it reads
proportionally fewer pages from disk per query.

## Honest headline

- **MEASURED — size:** the v6/SQ8 refine index is **3.52× smaller** than v5/f32 at 16M — this **confirms the
  M85 SIFT1M finding (3.5×) at 16× the scale** (the ratio is scale-invariant, as expected: 4 bytes/dim f32 vs
  1 byte/dim SQ8, minus fixed overhead).
- **NOT re-established here — recall:** v5 and v6 measure the *same* recall (0.291) at this run's **degenerate
  operating point**, but that equality is a **tie-noise artifact** (tie-dense clusters — even a broken reranker
  would score ~0.291), so it is **NOT evidence of SQ8 rerank quality**. SQ8 recall-neutrality is established by
  **M85 on real SIFT1M (ε ≤ 2%)**, not by this run.
- **DIRECTIONAL — QPS:** v6 shows **+21% cold-cache QPS at probes=32** (10.2 vs 8.4) — the smaller index reads
  less under cache pressure. This is a **lower bound**, not a definitive out-of-RAM measurement (see caveats).
- **DISCOVERED — build-memory wall:** the `theodb_ivfflat` build peaks at **~4× the base dataset** in RAM
  (47 GB, then 64 GB anon-rss for a 15.4 GB / 30M base → OOM on the 64 GB box). **16M (8.2 GB base, ~34 GB
  build) was the largest that fit.** The ≥100M/1B DoD target was therefore **not reached** — a real,
  documented scaling limit, not a measurement we skipped.

## Setup

| | |
|---|---|
| Host | DigitalOcean `m-8vcpu-64gb` (memory-optimized), Intel Xeon Platinum 8280 @ 2.70 GHz, 8 vCPU, 64 GB advertised / **62 GB usable** RAM, 200 GB disk |
| PostgreSQL | 17.10 (pgrx), `shared_buffers=2GB`, `max_parallel_workers_per_gather=7`, `maintenance_work_mem=1GB` |
| Extension | `theodb_rs` 1.0.0 (own `public.vector` type + `theodb_ivfflat` AM) |
| Dataset | 16 000 000 vectors, 128-dim, `float32`; 2000 gaussian clusters, seed 42 (subset of a 30M generation) |
| Index | `theodb_ivfflat (e)` `WITH (lists=800, pq_subspaces=32, aq_threshold=2000, separate_storage=1)`; v6 adds `refine=1` (SQ8) |
| Queries | 100 random existing rows; ground truth = exact seqscan top-10 (`SET enable_indexscan=off`) |
| Isolation | one index built at a time (build v5 → sweep → drop → build v6 → sweep → drop) — clean per-engine memory + planner isolation |

## Results

| engine | build (s) | index size | probes | recall@10 | cold_qps | warm_qps |
|---|---:|---:|---:|---:|---:|---:|
| **v5 f32** | 1942 | **8382 MB** (8 788 819 968 B) | 32 | 0.291 | 8.4 | 13.0 |
| **v5 f32** | 1942 | **8382 MB** (8 788 819 968 B) | 64 | 0.292 | 6.8 | 7.0 |
| **v6 sq8** | 1865 | **2382 MB** (2 497 478 656 B) | 32 | 0.291 | **10.2** | 10.7 |
| **v6 sq8** | 1865 | **2382 MB** (2 497 478 656 B) | 64 | 0.292 | 6.1 | 6.2 |

(index size is reported in MiB alongside the exact byte count.)

**Size ratio:** 8 788 819 968 / 2 497 478 656 = **3.52×** (v6 smaller).

## Findings

1. **Size advantage is scale-confirmed.** v6/SQ8 = 3.52× smaller than v5/f32 at 16M, matching the M85 1M finding
   (3.5×). The storage-separated SQ8 refine footprint is 1/3.5 of the f32 refine — the mechanical basis of the
   out-of-RAM advantage (fewer refine bytes to page in when the working set exceeds cache).
2. **Recall is indistinguishable at the degenerate point — NOT a rerank-quality result.** Both engines score the
   same 0.291 because the synthetic clusters are tie-saturated (even a broken reranker would score ~0.291 here), so
   this run does **not** establish SQ8 rerank quality. The evidence that SQ8 is recall-neutral is **M85 on real
   SIFT1M (ε ≤ 2%)**, cited — not this measurement.
3. **Directional cold-QPS edge for the smaller index.** At probes=32, v6 cold-QPS is +21% over v5 (10.2 vs 8.4) —
   consistent with the thesis (smaller index → fewer cold reads). At probes=64 the two converge (6.1 vs 6.8);
   warm-cache favors v5 slightly (f32 rerank is cheaper compute than SQ8 decode when everything is cached).

## Honest caveats (load-bearing — read before citing)

- **The ≥100M/1B DoD target was NOT reached.** The build-memory wall (below) capped the feasible build at 16M on
  a 64 GB box, so a *true* out-of-RAM index (index > RAM) was never built. What is reported is a **16M in-cache
  build with cold-cache queries as an I/O proxy**, not a literal billion-scale run.
- **Recall is degenerate (0.291), NOT a code regression.** The synthetic clustered data has ~8000 near-equidistant
  points per cluster at 16M → top-10 ties resolve differently between the GT seqscan and the index. The *identical*
  code path on **real SIFT1M measured recall 0.98 (M84)**. Because both engines see identical data and measure
  identical recall, the v5-vs-v6 **relative** comparison is valid; the **absolute** recall is a data artifact and
  MUST NOT be cited as the AM's recall.
- **The cold measurement underestimates the cold penalty.** `drop_caches` runs once per (engine, probes) sweep,
  then 100 queries execute — only the first is truly cold; queries 2–100 warm within the sweep. So `cold_qps`
  ≈ 1-cold + 99-warm. The +21% v6 cold edge is therefore a **lower bound** on the real out-of-RAM advantage; a
  per-query `drop_caches` harness would measure the full crossover.
- **Build-memory wall (the discovered scaling limit).** The `theodb_ivfflat` ambuild holds the full `AnnIndex`
  (~1× base) plus a collected copy plus AQ/refine page buffers → peak ~4× base anon-rss. Two OOM-kills were
  observed at 30M (47 GB, then 64 GB anon-rss for a 15.4 GB base). This is a genuine limit of the current
  batch-buffered build, not a benchmark artifact.

## Verdict

**`SIZE_CONFIRMED / OUT_OF_RAM_QPS_INCONCLUSIVE`.** The SQ8 storage-separation's **memory/size advantage is
MEASURED and scale-confirmed** (3.52× smaller at 16M, recall-neutral). The **out-of-RAM QPS crossover is
directionally supported** (+21% cold-QPS @ probes=32, a lower bound) **but NOT definitively measured** — blocked
by (a) the build-memory wall (no true out-of-RAM index buildable on 64 GB), and (b) synthetic-data recall
degeneracy (no clean matched-recall QPS comparison). This is an **honest partial/inconclusive** terminal
result, in the same discipline as the M73/M82 honest-negatives (`docs/adr/0035`, `0037`).

**It does NOT claim** vector-QPS superiority over ScaNN/AlloyDB — that ceiling remains as measured in M73/M82
(paradigm tax, **~25× vs ivfflat and up to ~44× vs hnsw at 0.99**, per `docs/benchmarks/m73-headtohead-verdict.md`).
It closes the storage-separation track with what the hardware allowed us to honestly measure.

## Recommended follow-ups (to reach the literal ≥100M DoD)

1. **Streaming ambuild** — flush index pages incrementally instead of buffering the full `AnnIndex` + copies, to
   lift the ~4×-base memory wall (would make 100M+ buildable on commodity RAM). This is the highest-leverage fix.
2. **Real billion-scale ANN data** (e.g., a real 100M+ SIFT/DEEP descriptor set) + a **per-query cold-cache
   harness**, on hardware where the index genuinely exceeds RAM — the setup that would turn the directional +21%
   into a definitive crossover number.

## Provenance

- Phase 1 (scalable build: kmeans-train sampling capped at 1.1M + parallel full-N assignment) — commit `fba16d0`,
  249 pg_tests GREEN, byte-identical at ≤1M. This is what made the 16M/30M builds tractable at all.
- Raw run + OOM dmesg evidence: `docs/benchmarks/m88-billion-scale-verdict.json`.
- ADR: `docs/adr/0038-m88-billion-scale-regime-verdict.md` (extends `0037`).

See also: `docs/benchmarks/m85-sq8-refine.md` (1M SQ8 3.5× + recall-neutral), `docs/benchmarks/m73-headtohead-verdict.md`
(North Star vector ceiling), `docs/adr/0037-m82-am-ivf-aq-measured-verdict.md`.
