# Review — m32-scale-benchmark

**Date:** 2026-07-02 · **Verdict:** READY_TO_MERGE · **Milestone:** M32
**Method:** 3 parallel specialist agents (harness code-quality · benchmark methodology+honesty · cross-validation) over `git diff e1da2ab..HEAD`.

## Verdict path

Agent verdicts: code-quality NEEDS_FIXES · methodology NEEDS_FIXES · cross-validation **READY_TO_MERGE**. No BLOCKER; 1 HIGH + several MEDIUM/LOW. All resolved in commit `<review-fix>`; re-validated (42 unit + 4-way integration green). Final: **READY_TO_MERGE**.

## Findings & resolutions

| # | Sev | Finding | Resolution |
|---|---|---|---|
| 1 | HIGH | base CLI `--full-train` (no `--n`) → pgvector ivfflat built with `lists=5` on 1M (unfair) | `build_config` derives ivfflat `lists` from the REAL HDF5 train size under `--full-train` (`_train_size`). The committed artifact was already correct (driver passed the real N); this closes the trap for any CLI user. |
| 2 | MED | artifact `.md`/`.json` mislabel GT as "brute-force" (run used neighbors-GT) | harness emits `gt_source` per path; committed `.md` header + `.json` methodology corrected to "neighbors-GT (ADR-2)". |
| 3 | MED | `.md` omits mean/std (DoD wants mean±std); ef=100 heavy-tail unflagged | mean±std added to the `.md` (+ future runs render mean/std columns); ef=100 outlier (std 11.29 ≫ mean 4.62) flagged as mobile-CPU thermal jitter. |
| 4 | MED | cosine branch of `neighbors_ground_truth` untested | added `test_neighbors_ground_truth_cosine_matches_brute_force`. |
| 5 | MED | `load_hdf5_full` negative paths untested | added missing-`neighbors` + k>neighbors tests. |
| 6 | LOW | `neighbor_ids` out-of-range silently wraps (numpy) → wrong GT | fail-fast bounds validation + `test_neighbors_ground_truth_rejects_out_of_range_ids`. |
| 7 | LOW | `query_cap <= 0` → bare IndexError | `query_cap >= 1` guard in build_config + harness + `test_query_cap_must_be_positive`. |
| 8 | LOW | theodb-only + cosine → silent empty benchmark (exit 0) | empty-specs guard raises + `test_theodb_only_cosine_raises_empty_specs`. |
| 9 | LOW | 1M train double-alloc | `astype(np.float32, copy=False)`. |
| 10 | LOW | hardware "12 cores" (i7-1355U is 10C/12T) | corrected + thermal-variance caveat. |
| 11 | LOW | `maintenance_work_mem` only helps pgvector — not disclosed | disclosed in the `.md` fairness note (anti-cherry-pick direction). |
| 12 | LOW | verdict row "HNSW family @ recall≈0.98" (theodb_hnsw is 0.964) | relabeled with explicit per-index recalls. |
| 13 | INFO | T2.2 (`query_cap`) added during /implement, not in the scored plan | additive, test-only fairness fix for theodb_hnsw's O(N) scan; documented — accepted. |

## Confirmed positives (independently verified by the agents)

- **DoD MET (cross-validation, all checks PASS):** n=1M in the artifact; QPS + p50/p95/p99 + recall@10 + build time + index bytes for theodb ivfflat/hnsw vs pgvector ivfflat/hnsw; runs=3 with mean+std per result in the JSON; hardware cited; `.md`+`.json` committed; honest per-knob verdict; ANN-Benchmarks distance-thresholded recall.
- **Anti-cherry-pick confirmed:** full frontier reported (incl. unflattering pgvector ivfflat probes=1 @ recall 0.37); theodb's worse QPS bolded INFERIOR; builds single-threaded (handicaps pgvector) yet theodb still reported build-INFERIOR; theodb_hnsw cap disclosed + labeled `[q=50]`.
- **Honesty (Rule 3):** the North Star vector-superiority pillar is explicitly declared NOT met on QPS at scale; root causes (fixed 100-list under-partitioning; O(N) hnsw blob scan) are technically correct, verified against source (`DEFAULT_LISTS=100`, `SCAN_PROBES=10`), and named as fixable future levers — not excuses.
- **recall semantics sound:** neighbors-GT recomputed from neighbor VECTORS (not the shipped `distances`), float32 contract matches brute force (test asserts atol=1e-4).
- **Clean:** ruff + vulture clean (no dead code); DIP preserved; commits on develop, ZERO Co-Authored-By; measurement milestone only (no Rust engine change).

## Gate results

- Unit: 42 passed (incl. new cosine/negative/query_cap/empty-specs tests); 40 harness tests no regression; ruff clean; vulture clean.
- Integration: `test_4way_scale_harness_runs_on_real_sift` passed (real SIFT n=20k, all 4 AMs via their own index).
- ≥1M artifact: `docs/benchmarks/m32-scale-sift1m.{md,json}` — n=1M, 3 runs, mean±std, hardware, peak RSS ~3.3GB, honest per-knob verdict, reproduction command.
