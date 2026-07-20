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

## What is TRUE — the achievable win (own-path A/B, MEASURED)

The pgvector-parity DoD is falsified, but the **re-scoped, achievable** goal IS met: resume-from-discarded is a
measured improvement to theodb's OWN filtered iterative scan, at matched recall. Toggled via `theodb_hnsw.resume`
on the SAME index/data/settings:

| theodb path | avg latency (ms) | recall@10 |
|---|---|---|
| **resume ON (M118)** | **14.33** | 0.9967 |
| resume OFF (M52 re-search) | 27.94 | 0.9967 |

**At matched recall (0.9967), M118 resume is ~1.95× FASTER than the M52 re-search it replaces** — the re-search
re-traverses the whole graph with a doubled `ef` on each exhaustion; resume continues from the retained frontier.

- **Correct.** recall@10 = 1.0 vs brute-force exact kNN under the selective filter (A/B in-PG, `Index Scan using
  theodb_hnsw`). The resume returns the true top-k.
- **~1.95× faster than the path it replaces** (own-path, matched recall — the honest achievable metric).
- **Bounded + operator-controlled** (`theodb_hnsw.resume_max_mb` fail-safe; `theodb_hnsw.resume` kill-switch).

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

## Consequence — M118 RE-SCOPED (owner-approved 2026-07-20)

The original DoD (≤ 1.2× vs pgvector) is **FALSIFIED** — structurally unachievable (page-native vs in-memory graph),
consistent with the repositioned North Star (ADR-0033). Per `public-copy.md` + Unbreakable Rule 5, **no
"closes-the-gap-vs-pgvector" claim is made** — the evidence refutes it.

M118 is **re-scoped** to the achievable, MEASURED outcome and shipped on it:

> **Resume-from-discarded: correct (recall@10 = 1.0 vs brute-force under a selective filter) + ~1.95× faster than
> the M52 re-search it replaces, at matched recall (own-path A/B) + memory-bounded + operator-toggleable.**

This is a genuine improvement to theodb's OWN filtered iterative scan. It does NOT beat pgvector's in-memory graph
and never claims to. Code: `50eb574` (T1.1+T2.1), `764ecaa` (T2.2), + the `theodb_hnsw.resume` toggle.
