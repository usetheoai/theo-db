# Review — M128 official-benchmark columnar/ClickBench pilot

**Date:** 2026-07-20 · **Slug:** official-benchmark-columnar · **Milestone:** M128 · **Commit:** 79e5014 (+ fixes)
**Verdict:** READY_TO_MERGE

## Scope

Adversarial review (council-benchmark, measurement-honesty lens) of the M128 ClickBench-over-`theodb_columnar`
entry + driver + measured artifact + the filed planner-hang bug (#135).

## Consolidated findings

| # | Severity | Finding | Resolution |
|---|---|---|---|
| 1 | MEDIUM | The doc claimed "n=100,000 corroborated" but only the n=1,000 JSON was committed — a measurement statement without its reproducible artifact. | **FIXED** — the n=100,000 run completed (43/43, 0 err, geomean 5.567s, **byte-identical A/B PASS 43/43**); its JSON is now the committed primary artifact (`docs/benchmarks/m128-clickbench-columnar.json`); the doc table shows both scales with the 100k backed. |
| 2 | LOW | The hot geomean is storage-path (native-executor) latency, not columnar acceleration; a hurried reader could over-read the bolded number. | **FIXED** — the doc labels it explicitly "storage-path latency, NOT columnar-accelerated — agg off, customscan=0". |
| 3 | LOW | Naive `.replace("hits","hits_heap")` is latent-fragile (currently safe — 0 of 43 queries contain "hits" beyond the table ref). | **FIXED** — inline comment noting it is safe + why (the council-benchmark verification). |

No BLOCKER, no HIGH.

## What the review verified (measured, not supposed)

- **Real measurement:** the 43 queries run against a real `theodb_columnar` table (loaded via INSERT-SELECT from a
  heap copy), 3× each, wall-clock timed; all 43 timing triples distinct; the geomean recomputes exactly (5.567 /
  0.0668). Not degenerate.
- **Genuine A/B:** `assert_byte_identical` does a real element-wise per-query row compare; the LIMIT-tie handling
  (compare the full unlimited aggregation) is a **stronger** storage-correctness check, not a cover-up — a real
  value bug would still diverge; the earlier "9 divergences" were provably tie-order artifacts. PASS 43/43 at both
  scales.
- **#135 real + open:** the planner hang (not execution; uninterruptible by statement_timeout) on the wide
  mixed-type real hits is a genuine bug with a confirmed repro; the agg-off scope is defensible + disclosed
  (`columnar_customscan_count: 0` visible); no comparative / "Nx faster" claim is made (public-copy § 4 untriggered).
- **D1 clean:** `git ls-files` confirms only our Apache-2.0 files (benchmark.sh/template.json/README) are tracked;
  the CC-BY-NC-SA create.sql/queries.sql/results.json are `git check-ignore`-confirmed untracked; `hits` streamed
  CI-only, never committed. No secrets.

## Verdict

**READY_TO_MERGE.** council-benchmark: "M128 mediu" — real hits, INSERT-SELECT columnar load, wall-clock timing,
value-exact A/B, honest scope. The MEDIUM (n=100k artifact) is now committed and backs the claim; the two LOWs are
tidied. The adopt-and-wrap pattern is proven for the columnar pillar (storage path); the vectorized pushdown is a
tracked follow-up on #135.
