# E2 — SymphonyQG co-located quantized graph in-PostgreSQL: SIFT1M A/B verdict

**Date:** 2026-07-18 · **Module:** `theodb_rs/src/am/{scan,build,page/symqg,options}.rs` + `ann/symqg_spike.rs`
(own-code, clean-room from arXiv:2411.12229 — the NTUITIVE-licensed reference C++ was **study-only, never copied**, D1).
**What is measured:** the same 1M SIFT base vectors indexed on two custom index AMs on the SAME table, queried with
the SAME 200 official queries, scored against the official groundtruth — the only variable is the AM:

- **`theodb_symqg`** — SymphonyQG's co-located quantized graph: each vertex row stores its rotated vector `P·x`
  plus, for each of its ≤32 neighbours, a **1-bit RaBitQ sign code**. Beam search estimates all neighbours per hop
  from the co-located codes (no separate rerank); the popped centre's exact distance is a local read.
- **`theodb_hnsw`** — our mature HNSW AM (M35–M46: copy-free page reads, SIMD distance, tuned beam).

**Hardware / method:** DigitalOcean dedicated droplet, 8 vCPU, PostgreSQL 17.10 + pgrx 0.19.0, `shared_buffers=2GB`,
SIFT1M (`sift-128-euclidean`, L2), N=1,000,000, 200 queries × best-of-3 (warm), recall@10 vs official GT,
`degree_bound=32`. Reproduce: `benchmarks/e2_symqg_inpg.py`. Raw log: `e2-symqg-inpg-verdict.json`.

**Gate:** `theodb_symqg QPS ≥ 1.5× theodb_hnsw at matched recall@10 ≥ 0.95`. **Result: NOT MET.**

---

## The page tax was real, dominant, and mitigable — but a residual gap remains

The off-PG spike (`e2-symqg-spike.md`) measured `theodb_symqg` **1.8–2.66× faster** than the reference at recall
parity, in pure RAM. This in-PG A/B settles what the spike could not: the **per-hop random-page-read tax** of a real
persisted index. It manifested in two stages.

### Stage 1 — v1 layout (one page per row): the page tax dominates (8.5× loss)

The first layout wrote each 1408-byte vertex row on its own 8 KB page (`write_chunks` per row, ~82 % waste),
inflating the 1 M-vertex index to **7828 MB** — far past `shared_buffers`, so every hop faulted a page from disk.

| matched recall@10 | theodb_symqg v1 (7828 MB) | theodb_hnsw (724 MB) | hnsw faster |
|---|---:|---:|---:|
| ~0.95 | ef=80 → **32.3 qps** | ef=40 → **274.5 qps** | **8.5×** |
| ~0.98 | ef=160 → 29.9 qps | ef=80 → 179.6 qps | 6.0× |
| ~0.999 | ef=640 → 21.3 qps | ef=640 → 37.1 qps | 1.7× |

This is the **opposite** of the off-PG spike — the page tax, not the algorithm, dominated. An unfair benchmark:
it measured the v1 page-waste, not the traversal.

### Stage 2 — v2 layout (contiguous packing): fair comparison, still a loss (2.6–3.9×)

Rows are fixed-size, so their byte offset is `ord · row_bytes` — the per-vertex `(first_block, npages)` directory was
pure page-waste. v2 packs all rows into one contiguous byte region (CHUNK bytes/page, arithmetic addressing, no
directory), folding the index **5.66× to 1383 MB** (fits `shared_buffers` → warm) and building **faster**
(1421 s vs 1968 s — 5.7× fewer page extends).

| matched recall@10 | theodb_symqg v2 (1383 MB) | theodb_hnsw (724 MB) | hnsw faster |
|---|---:|---:|---:|
| ~0.95 | ef=80 → **73.3 qps** (r=0.939) | ef=40 → **287.4 qps** (r=0.946) | ~3.9× |
| ~0.98 | ef=160 → 67.5 qps (r=0.978) | ef=80 → 199.5 qps (r=0.983) | ~3.0× |
| ~0.994 | ef=320 → 50.1 qps (r=0.996) | ef=160 → 131.6 qps (r=0.994) | ~2.6× |
| ~0.999 | ef=640 → 33.3 qps | ef=640 → 43.3 qps | ~1.3× |

Packing gave symqg a **+2.3× QPS** lift (32→73 at recall ~0.95) and a fair, warm comparison. The gate still fails:
`theodb_hnsw` is **2.6–3.9× faster at matched recall** across the practical range (0.95–0.994); only at recall ~0.999
does the gap narrow to ~1.3× (still hnsw-favoured).

---

## Verdict (honest)

- **The gate is NOT met.** `theodb_hnsw` outperforms `theodb_symqg` in-PG at every matched recall point on SIFT1M.
- **The page tax (the plan's risk #1) was real and dominant, and mitigable.** Contiguous packing folded the index
  5.66× and lifted QPS 2.3×, turning an 8.5× loss into a 2.6–3.9× loss — but did not close it.
- **The off-PG spike advantage did NOT transfer in-PG.** In RAM against a reference, symqg's co-located traversal was
  1.8–2.66× faster; in-PG against the **mature** `theodb_hnsw` AM it is 2.6–3.9× slower. The residual gap is
  (a) scan maturity — the first-cut `gather_symqg_candidates` allocates a fresh Vec per hop, uses a `HashSet` visited
  set and `f64` heaps, with no SIMD; `theodb_hnsw` has had M35–M46 optimization passes — and (b) the fundamental
  per-hop cost of decoding a 1408-byte co-located row + 32 scalar sign-estimates per pop vs a lean neighbour-list hop.
- **The AM itself is correct and complete.** `CREATE INDEX … USING theodb_symqg`, beam-search scan, `INSERT`→pending,
  `VACUUM`, and MVCC-delete are all validated on real distinct SIFT1M data; recall tracks ef cleanly (0.857 → 0.9995).
  Only the QPS gate is unmet.

## Path forward (separate scope — not this gate)

The identified lever to potentially close the residual gap is the **FastScan 1-bit SIMD sign-estimate kernel**
(reuses `vec/ah.rs::ah_score_block`, the pshufb block32 kernel already tested) + copy-free row reads
(`with_page_item`) — the per-hop cost is exactly what the off-PG spike shows is compressible. That is an explicit
**separate task** (per the plan and the E2 goal), not part of the page-tax gate this document settles. Until it is
measured, this project makes **no claim** of a symqg QPS win over HNSW (`public-copy.md`, `CLAUDE.md` rule 5).

## Caveats

- Warm (in-`shared_buffers`) regime only. A genuine out-of-RAM (dataset ≫ RAM) run would re-introduce a page tax the
  1383 MB index partly avoids here; direction measured for v1↔v2, absolute billion-scale numbers not.
- Build cost is single-thread HNSW-adjacency-bound (~1421 s at 1M on this steal-noisy shared droplet); not optimized
  (out of scope for the page-tax gate).
- L2-only (`theodb_symqg_l2_ops`; the 1-bit sign estimator is L2-only).
