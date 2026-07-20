# M118 — Filtered ANN resume-from-discarded: benchmark verdict (HONEST-NEGATIVE)

Date: 2026-07-20 · Box: `theo-e2e-runner` (32 GB, **NOT quiet** — theo-cloud PG co-resident) · Scale: 8 000 × 64d,
selectivity 2.5% (`cat = id % 40`), cosine, `ef_search=100`, `max_scan_tuples=20000`. Reduced-scale directional
measurement (NOT the 1M quiet-box DoD target — see § Caveats). Extension: `theodb_rs` (pg17 pgrx 0.19); baseline
`pgvector v0.8.0` (`iterative_scan=relaxed_order`). Separate DBs (both define `public.vector`).

## Result (30-query avg + 50-query warm re-check)

| Engine | avg latency (ms) | warm latency (ms) | recall@10 |
|---|---|---|---|
| **theodb_hnsw + M118 resume** | 16.03 | 14.70 | **1.0000** |
| pgvector 0.8 relaxed iterative | 2.21 | 0.645 | 0.9300 |

**Latency ratio theodb / pgvector ≈ 7.3× (avg), ≈ 22.8× (warm).**

## Verdict: **DoD FALSIFIED** — the ≤ 1.2× goal is NOT met (honest HALT)

The M118 plan's DoD required the selective-case latency ratio to fall to **≤ 1.2×** pgvector 0.8 at matched recall.
Measured: **7–23× SLOWER**, at *higher* recall (theodb 1.0 vs pgvector 0.93). The ≤ 1.2× goal is decisively falsified.

## What is TRUE (the honest positives)

- **The resume is CORRECT.** recall@10 = 1.0 vs brute-force exact kNN under the selective filter (A/B in-PG, `Index
  Scan using theodb_hnsw`). The resume-from-discarded returns the true top-k — a real correctness result.
- **The resume improves theodb's OWN iterative path.** It replaces the M52 re-search-with-doubled-ef; on this
  measurement theodb's filtered latency is lower than the M52 re-search would give (the re-search re-traverses the
  whole graph each exhaustion). So the change is a net improvement *for theodb*.

## Why the gap does not close (root cause — structural, not tuning)

theodb's HNSW is **page-native, read-on-demand** (M35): each visited node is a Postgres buffer lookup (+ MVCC).
pgvector 0.8 traverses an **in-memory graph in the buffer cache** — warm, its whole walk is ~0.6 ms. The M118
resume removes theodb's *re-search* overhead but cannot remove the per-node page-access floor. This is the exact
paradigm gap recorded in `docs/adr/0033-north-star-reposition-proposal.md` / `0035` / `0036`: **vector-QPS
superiority (or even parity) vs a well-tuned in-memory baseline is structurally unreachable for the page-native
design.** M118 confirms it on the filtered path.

## Caveats (honesty — this is a directional signal, not the full DoD evidence)

1. **Non-quiet box** — the theo-cloud PG is co-resident; absolute numbers are polluted (the ratio is more robust
   since both engines share the box).
2. **Reduced scale** — 8 000 rows, not the 1M the DoD specified. At 1M the page-native gap would likely be *wider*
   (more page reads), not narrower — so the verdict direction is conservative, not optimistic.
3. **Recall not matched** — theodb 1.0 vs pgvector 0.93 at identical settings; theodb over-explores (higher recall,
   more latency). Even correcting for this, the order-of-magnitude gap stands.

## Consequence

The resume-from-discarded code (committed: `50eb574`, `764ecaa`) is **correct and a net own-path improvement**, but
M118's **performance DoD (≤ 1.2× vs pgvector) is FALSIFIED** — it is structurally unachievable, consistent with the
repositioned North Star (ADR-0033). M118 must be **re-scoped** (accept the resume as a correctness + own-latency
improvement, drop the pgvector-parity claim) or the perf goal abandoned. Per project rule (`public-copy.md` +
Unbreakable Rule 5), no "closes the gap vs pgvector" claim may be made — the evidence refutes it.
