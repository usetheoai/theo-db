# Review — M127 official-benchmark vector pilot

**Date:** 2026-07-20 · **Slug:** official-benchmark-vector · **Milestone:** M127 · **Commit:** cf38d8b (+ LOW fixes)
**Verdict:** READY_TO_MERGE

## Scope

Adversarial review (council-benchmark, measurement-honesty lens) of the M127 vector pilot: the ann-benchmarks
`BaseANN` adapter, the byte-identical regression module, the driver, 9 unit tests, and the measured GloVe artifact.

## Consolidated findings

| # | Severity | Finding | Resolution |
|---|---|---|---|
| 1 | LOW | The driver computes id-overlap recall but never calls `recall.py`'s distance-thresholded `recall_at_k` (whose docstring warns id-overlap "diverges under ties") — two coexisting recall defs, silently choosing one. Negligible on float32 GloVe (no ties) and id-overlap IS the ann-benchmarks default `k-nn` metric. | **FIXED** — one-line note in `_recall_per_query` documenting the deliberate ann-benchmarks-faithful choice; doc states "id-overlap … the ann-benchmarks `k-nn` metric". |
| 2 | LOW | The determinism A/B ran at ef=400 where recall is already 1.0 → near-trivially byte-identical; a low-ef A/B exercises more path-dependent traversal (stronger probe). | **FIXED** — A/B now runs at the LOWEST ef (ef=10, recall 0.73); re-measured: still `identical=True, diverged=0` — a stronger regression result. Artifact records `ab_ef_search`. |
| 3 | LOW | Two box-label phrasings (md "DO droplet" vs JSON "self-hosted NOT canonical c6a"). | **FIXED** — md header aligned: "self-hosted DO droplet — NOT the canonical AWS c6a.4xlarge, so QPS is not leaderboard-comparable". |

No BLOCKER, no HIGH.

## What the review verified (measured, not supposed)

- **Real Pareto, not degenerate:** ground-truth is an exact O(N·Q) brute force **independent of the `theodb_hnsw`
  index** (not circular); recall demonstrably drops (0.73 at ef=10 → 1.0 at ef≥200 — proof the metric is not stuck
  and the index is not returning all ids); QPS is wall-clock (`time.perf_counter`) over real single-thread queries.
- **BaseANN faithful + id-alignment sound:** `fit`/`query`/`set_query_arguments` signature-shaped; `load_vectors`
  id==corpus-position holds, so returned ids align with brute-force GT ids.
- **Wrap layer genuine:** `assert_byte_identical` is order-sensitive per-query (would FAIL on a reorder; test proves
  it) and fail-closes on qid-set mismatch (`QidMismatchError`); the significance p=1.0 on identical passes is honest
  and framed as a run1-vs-run2 wired-test demonstration, never a two-engine claim.
- **Scope honestly under-claimed:** self-hosted box (not c6a), n=5000 subsample, "not leaderboard-comparable", any
  ScaNN/AlloyDB magnitude cites m73 — respects `public-copy.md § 4`.
- **Clean bill:** 9/9 unit tests pass; no secret/API-key literals; typed/fail-closed error handling
  (`DBUnavailableError`, `QidMismatchError`, `UNBENCHMARKED` clean-exit on missing dataset).

## Post-fix measured result

recall@10 × QPS Pareto (ef 10→400: 0.73/186 → 1.0/29), byte-identical A/B at ef=10 `identical=True (diverged=0)`,
significance deterministic (p=1.0). `docs/benchmarks/m127-ann-benchmarks-vector.{md,json}`.

## Verdict

**READY_TO_MERGE.** council-benchmark: "M127 mediu" — real GloVe, independent exact GT, wall-clock QPS,
order-sensitive regression, scope honestly labeled. The 3 LOW polish findings are fixed. The adopt-and-wrap pattern
(ADR-0050) is proven end-to-end for the vector pillar; the reusable wrap layer (significance + byte-identical
regression) is ready for M128–M130.
