# M95 — honest vecfilter cost model — measured verdict

**Hardware:** Intel Xeon Platinum 8358 @ 2.60GHz (DO 8-vCPU) · **Dataset:** SIFT1M (128-d, real neighbors),
100 queries, k=10, lists=1000, probes=64 · **Date:** 2026-07-13 · Raw: `m95-cost-model.{json,log}`.

## What was measured

The forced `total_cost = min_cost × 0.1` selection heuristic was replaced with an honest cost =
`term_B` (the bitmap sub-plan's `bitmapqual.total_cost` — the membership-production cost, no heap double-count) +
`term_V` (`cost::vecfilter_scan_cost`, re-derived from the bitmap selectivity via `cost::effective_probes` —
imaging the M91 adaptive loop). The sweep asks, at each selectivity: does the planner (honest cost,
`vecfilter_force=off`) **auto-pick** the node? And how does the FORCED node (INLINE) compare to the native
post-filter (POST) — the recall the planner cannot see?

## Results

| Selectivity | planner auto-picks node? | POST recall | POST QPS | INLINE (forced) recall | INLINE QPS |
|---|---|---:|---:|---:|---:|
| 0.1% | no | 1.000 | 261 | 0.921 | 251 |
| 0.5% | no | 1.000 | 78 | 0.947 | 233 |
| 1% | no | 0.673 | 16 | **0.953** | **211** |
| 2% | no | 0.638 | 31 | **0.937** | **171** |
| 5% | no | 0.593 | 71 | **0.915** | 104 |
| 8% | no | 0.574 | 116 | 0.894 | 82 |
| 12% | no | 0.558 | 177 | 0.890 | 53 |
| 15% | no | 0.551 | 227 | 0.881 | 48 |
| 25% | no | 0.577 | 339 | 0.878 | 28 |
| 50% | no | 0.700 | 359 | 0.850 | 14 |

## Verdict — `HONEST_NEGATIVE on auto-select, POSITIVE on value` (the blueprint's R4, measured)

1. **The honest cost model does its primary job: it PREVENTS over-selection.** The forced `× 0.1` hack that made
   the node hijack EVERY filtered query (even loose ones where it is the wrong plan) is gone. The node no longer
   auto-hijacks anything — the real harm is closed.
2. **The planner never AUTO-selects the node (`chosen_node=False` everywhere) — R4, measured.** With M48's
   `amcostestimate` unchanged, the native post-filter competitor is **probe-blind**: it prices a default-probe
   scan and is blind to the fact that, under a selective filter, it must scan far more of the ordered stream. So
   its cost is systematically UNDER-stated, and the honest node — which correctly prices its higher work — always
   looks more expensive on the planner's cost-only comparison.
3. **But the node is correctness-critical, not just faster.** Across 1–25% selectivity the native POST is
   **recall-broken (0.55–0.67 — losing a third to nearly half of the true neighbors)**, while the forced INLINE
   node recovers recall to **0.88–0.95**. At 1% INLINE is also **13× the QPS** (211 vs 16). The planner cannot see
   recall, so it cannot know the cheap plan it prefers is wrong.

## Resolution (shipped)

`theodb.vecfilter_force` (default OFF) — an explicit user override, the same rationale as Postgres's `enable_*`
knobs — prices the node below the cheapest base path for a selective filter whose recall the planner is blind to.
The **honest cost is the safe default** (never hijacks); a user who needs the node's recall forces it. The GUC
stays opt-in. Making M48's `amcostestimate` probe-aware for the filtered case — which would let the planner
auto-select the node where it wins — is the honest follow-up (it is out of M95's scope: M95's job was to remove the
hack and price the NODE honestly, which it does).

Positioning: **honest — never claims QPS-superiority over ScaNN/AlloyDB** (the paradigm ceiling M73/M82 stands).
The INLINE-vs-POST comparison is our own filtered-search node vs our own native post-filter.
