# Review — m103-vector-columnar-unified-substrate

**Date:** 2026-07-16
**Verdict:** READY_TO_MERGE
**Milestone:** M103
**Plan:** knowledge-base/plans/m103-vector-columnar-unified-substrate-plan.md

## Scope reviewed

The Lance-inspired co-residence: the IVF vector index (`part_id` + raw `vec` bytea) stored AS columns co-resident
with the scalar `label` + analytical columns in a `theodb_columnar` table, a scalar-prefiltered filtered top-k
(`theodb.vindex_knn_columnar`) that reads only the 4 index columns (column pruning), proven byte-identical in recall
to the exact filtered search. Files: `theodb_rs/src/vindex.rs` (new), `theodb_rs/src/am/columnar.rs` (visibility
widening), `theodb_rs/src/lib.rs`, `theodb_rs/isolation/bench_m103.sh`, `docs/benchmarks/m103-vector-columnar.{md,json}`,
`docs/adr/0044-m103-vector-columnar-coresidence.md`.

## Measured evidence (droplet pg17 / pgrx 0.19)

- **312 pg_tests GREEN**, zero regression (+5 M103): the byte-identity GATE
  (`m103_full_probe_byte_identical_to_exact_filtered`), the scalar-prefilter mask, reduced-probe ordering,
  empty-mask handling, and the end-to-end columnar co-residence + composition
  (`m103_columnar_coresident_filtered_topk_matches_exact_and_composes`).
- **Byte-identity GATE (DoD 3):** at full probe the co-resident columnar filtered top-k is a byte-identical
  `(tid, dist)` sequence to the exact filtered brute-force — sharing the M90-M95 `Scored` tie-break
  (`d.total_cmp(o.d).then(tid.cmp(o.tid))`) + the exact `vec::l2_dist_from_bytes` kernel. Recall EQUAL by construction.
- **Column pruning (DoD 4), isolated control:** decoding only the 4 index columns (49.57 ms ± 0.29) vs ALL columns
  (219.81 ms ± 1.78) on the wide index = **77.4 % of decode time saved** (above the noise floor); the end-to-end knn
  latency is invariant to analytical width (ratio 1.009, within a stddev); composed filter-knn + aggregation in one
  plan (225.41 ms ± 1.02). 5 runs + warmup discarded, mean ± stddev.

## Specialist sign-off

| Reviewer | Domain | Verdict | Blockers |
|---|---|---|---|
| council-vector-ann | ANN / byte-identity gate / recall honesty | READY_TO_MERGE | none |
| council-index-storage | storage / FFI / relation lifecycle / MVCC | READY_TO_MERGE | none |
| council-benchmark | measurement rigor / honesty | READY_TO_MERGE (after benchmark control + doc-integrity fixes) | none |

**council-vector-ann:** the byte-identity gate is airtight — `topk` uses exactly the M90-M95 `Scored` comparator,
both the columnar path and the oracle rerank with `vec::l2_dist_from_bytes`, and at full probe the candidate set is
genuinely all masked rows (= exact brute-force). Honest ceiling correct (recall equal-by-construction, no QPS-vs-ScaNN).
Non-blocking: the `probes` knob is currently a no-op (documented in the COMMENT); the round-trip byte-identity is
by-construction, not asserted (#108).

**council-index-storage:** relation lifecycle correct (`AccessShareLock`, single open/close); the v8/v4 byte parsing
is the exact inverse of M99's little-endian column-major encoding; column pruning genuinely skips the unprojected
columns' zstd; MVCC honored (decode reads under the caller's snapshot); fail-closed (typed errors, no panic across C).
Non-blocking: error-longjmp past `relation_close` is reclaimed by xact-abort cleanup (safe; #108); visibility widening
justified by co-residence (#108).

**council-benchmark:** initially NEEDS_FIXES — the full-probe L2 rerank dominated the end-to-end knn latency, so the
narrow/wide invariance proved pruning happens but not its magnitude. Fixed: added `theodb.vindex_decode_bytes` to
isolate the decode cost (the 77.4 % control), all timings now mean ± stddev with warmup discarded, headline reframed,
on-disk-≠-decode-cost stated, scale caveat first-class. Then two doc-integrity corrections (stale ADR bullet + .md/.json
reconciled to one run) applied → READY_TO_MERGE.

## DoD coverage (ROADMAP M103)

| DoD | Status |
|---|---|
| (1) vector index as Arrow columns in the columnar substrate | ✅ `part_id` (real IVF via `vindex_assign`), `vec` bytea, `label` co-resident with analytical cols; end-to-end test |
| (2) `WHERE scalar + ORDER BY vec` + aggregation in one vectorized plan | ✅ `vindex_knn_columnar` (prefilter + IVF + rerank in one column-pruned scan) + composed aggregation over the top-k |
| (3) result-equivalence of recall vs the exact filtered search (byte-identical) — THE GATE | ✅ `m103_full_probe_byte_identical_to_exact_filtered` GREEN |
| (4) benchmark cost/scale (column pruning), honest | ✅ isolated decode control: 77.4 % decode saved; honest scale ceiling |
| (5) sign-off council-vector-ann + council-index-storage + council-benchmark | ✅ all three READY_TO_MERGE |
| honest boundary (cost/scale/composability, NOT recall, NOT QPS-vs-ScaNN) | ✅ ADR-0044 D4, benchmark, COMMENTs |

## Honest scope note

Slice-1: a static materialized co-resident index; the columnar query path runs full-probe (the GATE); reduced-probe
over columnar-stored centroids, a native DataFusion `ORDER BY vec LIMIT k` planner node, and incremental index
maintenance are documented follow-ups (#108). Cost/scale/composability win only — recall equal-by-construction, no
QPS-vs-ScaNN (the M73/M74 paradigm ceiling is untouched). Follow-up #108.
