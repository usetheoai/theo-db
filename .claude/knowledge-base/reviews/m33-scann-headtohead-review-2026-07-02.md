# Review — m33-scann-headtohead

**Date:** 2026-07-02 · **Verdict:** READY_TO_MERGE · **Milestone:** M33
**Method:** 3 parallel specialist agents (benchmark methodology+scientific-honesty · cross-validation plan↔impl↔artifact · Python code-correctness+robustness) over the M33 commit + artifact.

## Verdict path

All 3 agents returned **READY_TO_MERGE** with NO BLOCKER and NO HIGH findings. The science was independently verified honest (identical recall scoring both sides, bit-identical query set, verbatim M34 reuse, fair matched config, honestly-labeled GAP with the mandatory library-vs-database caveat, public-copy-compliant README). The MEDIUM/LOW hardening findings were all fixed in commit `f22aae7` and the full SIFT1M measurement re-run (verdict stable). Final: **READY_TO_MERGE**.

## Findings & resolutions

| # | Sev | Finding | Resolution (commit f22aae7) |
|---|---|---|---|
| 1 | MED | Hardware provenance was a hostname only — a SIMD/AVX2-bound ~25-37× speed claim is not reproducible without the CPU spec (analysis-golden-rule § 3) | `_hardware()` records real CPU model + cores + RAM + AVX2 in the artifact meta (i7-1355U / 12c / 15.3 GB / AVX2=True); rendered in the .md |
| 2 | MED | No assertion that M33 `n_queries`/`runs`/`seed` == the reused M34 meta — latent parity-misreport if an operator reran with different params | fail-fast parity guard in `main` before consolidating; refuses to emit a misleading artifact on mismatch |
| 3 | MED | Latency percentiles came from the LAST run while QPS is best-of-N — p50 and QPS described different runs (internally inconsistent) | percentiles now taken from the SAME (min-mean) best-of-N run that defines QPS |
| 4 | MED | `_load_m34_rows` mislabeled ANY non-`theodb_ivfflat` row as pgvector + `KeyError` risk on a row missing `index` | explicit key match (`theodb_ivfflat` / `ivfflat`), skip unknown index types, fail-fast on schema drift (missing required keys) |
| 5 | MED/LOW | ScaNN partition training is unseeded → recall/QPS vary run-to-run; training-sample asymmetry (250k vs ~50k) not named; SQL-round-trip framing | `reproducibility_note` field discloses all three honestly (variance dwarfed by the gap; asymmetry favors ScaNN recall, immaterial to throughput; sub-ms SQL floor shown by pgvector probes=1 p50=0.37 ms) |
| 6 | LOW | Possible `ZeroDivisionError` if ScaNN p50 rounds to 0.00; `--runs 0` / `--n-queries 0` crash | guard `scann_best["p50"] > 0`; validate `--n-queries`/`--runs` >= 1 at parse time |
| 7 | LOW | memory verdict `INDETERMINATE` not in the plan's {SUPERIOR,PARITY,GAP} set | honest by design (peak-RSS incl. corpus vs on-disk index bytes are non-comparable) — pre-registered in the plan's Unresolved Questions + covered by the caveat; accepted |
| 8 | INFO | reused M34 p95/p99 render as raw full-precision floats (cosmetic) | left as-is (verbatim reuse; not a DoD item) |

## Confirmed positives (independently verified by the agents)

- **Recall scoring honest + identical both sides:** ScaNN's quantized AH distance is discarded; exact Euclidean is recomputed from the corpus for the returned ids (`_exact_l2`, sqrt of float64 sum-of-squares) and fed to the SAME `recall_at_k` (distance-thresholded, ANN-Benchmarks) theodb uses. squared_l2→l2 handled correctly. CI test locks this (`test_exact_l2_matches_bruteforce_metric`, `test_scann_recall_matches_theodb_recall_semantics`).
- **Query-set parity bit-identical:** same SIFT1M, `load_hdf5_full(..., seed=42, k=10)`, n_queries=1000, runs=3 — the same deterministic subsample + neighbors-GT as the M34 theodb run.
- **Verbatim M34 reuse:** every theodb/pgvector number is byte-identical to `m34-ivfflat-reloption.json` — no re-derivation, no fabrication.
- **Fair matched config:** num_leaves=1000 == lists=1000; leaves_to_search {1,10,50,100} == probes sweep; {200,400} only let ScaNN reach ≥0.99 (SOTA lib, would be an unfair handicap to cap it lower); `_best_qps_at_recall` picks max-QPS-above-floor for both — neither cherry-picked.
- **Per-query latency both sides** (mirrors theodb's per-query SQL) — not batched, no unfair ScaNN advantage.
- **Verdict honesty:** GAP is the correct label (ScaNN ~25× QPS / ~26× lower p50 at recall≥0.99 in the committed run; up to ~37× across runs); recall PARITY; memory INDETERMINATE with different-measures caveat. Library-vs-database caveat prominent (json + md blockquote + README + CHANGELOG). README states superiority is NOT yet met, benchmark-linked (public-copy § 4/§ 5 satisfied).
- **DoD:** every ROADMAP M33 DoD bullet satisfied (benchmark vs ScaNN OSS + caveats; per-dimension verdict recall/QPS/latency/memory in docs/benchmarks + .json; public-copy gate). `milestone_id: M33` in plan frontmatter.
- **Clean:** ruff clean; no dead code (11/11 functions referenced); 43 pure-python tests green (incl. the 2 M33); commit on develop, ZERO Co-Authored-By. Container-dependent tests (Rust AM) fail only for lack of a running container — unrelated to M33 (validated in their own milestones).

## Gate results

- Code-quality: PASS (vulture silent, ruff F-checks clean, 11/11 functions referenced).
- plan-confidence: SHIPPABLE (98.8; coverage 100%, zero hard caps, citations resolved).
- CI test: `test_m33_scann.py` 2/2 (recall-semantics fairness guard, scann installed → ran for real).
- DoD artifact: `docs/benchmarks/m33-scann-headtohead.{md,json}` — per-dimension verdict + mandatory caveat + real hardware + repro + ScaNN v1.4.2 + arXiv:1908.10396; re-run after the review fixes, verdict stable (recall PARITY, QPS GAP, p50 GAP).
