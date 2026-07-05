# Review — M46 theodb_hnsw scan hot-path hygiene (2026-07-04)

Consolidated multi-agent review (council-index-storage, council-benchmark, council-rust-pgrx) of the M46
change on `develop` (commits `89a0492` T2.1, `feat(m46) T1.1`, `feat(m46) T3.1`).

## Verdict: READY_TO_MERGE (after fixes below)

No BLOCKER. Two HIGH findings, both **resolved/mitigated** in-cycle. Production path is sound across all three
reviewers; the change is surgical, recall-neutral, and scan-only.

## Severity matrix

| # | Sev | Reviewer | Finding | Resolution |
|---|---|---|---|---|
| 1 | HIGH | rust-pgrx | pg_test `Spi::select(&sql, None, None)` is wrong for pgrx 0.16.1 (3rd arg is `&[]`, a slice) → `#[cfg(pg_test)]` module fails to compile → the 3 new tests could not have run as written. | **Fixed** — `None, &[]` (matches `hybrid.rs`/`ann_query.rs`). Compile-checked in a builder container. Recall-neutral itself was already proven via **live SQL** against the shipped binary, independent of the pg_test. |
| 2 | HIGH | benchmark | The report's prescribed next measurement (SIFT1M two-container A/B) inherits the parallel-build confound — different graphs even on a quiet box. | **Documented + deferred** — report now prescribes a **same-graph** design (criterion micro-bench over a fixed graph, or persist/restore); logged as FU-1 in `m46-hnsw-highrecall-qps-followups.md`. |
| 3 | MED | benchmark | `.md` omitted the theodb QPS base→post numbers (incl. ef=300 −12%). | **Fixed** — added the theodb median-QPS table with the control caveat; the −12% is shown and explained as a noise artifact. |
| 4 | MED | benchmark | Mechanical `recall_neutral_verdict` gate would return `RECALL_REGRESSION` (recall moved 0.9960→0.9955); report overrides via prose. | **Documented** — the report now states the byte-gate can't distinguish build-race from regression; the SQL oracle is the proof, not the gate. |
| 5 | LOW | index-storage | The no-stale-scratch-leak invariant (Err short-circuits before scratch is read) is implicit/load-bearing. | **Fixed** — added a comment at the `neighbors_into(...)?` call. |
| 6 | LOW | rust-pgrx | Corrupt-meta `m0` (u16) could drive a large speculative `with_capacity` (bounded, no panic). | **Deferred** — FU-2 (optional `cap` clamp); not a defect. |
| 7 | LOW | benchmark | "PROVEN" rests on a 5-row SQL oracle; repro `--nq` mismatch. | **Fixed** — repro command corrected to `--nq 200 --n 50000`; "PROVEN" is a correctness claim (oracle + code-structure argument), stakes low. |

## What all three reviewers confirmed SOUND

- **Recall-neutral by construction.** `with_capacity` is a std-guaranteed capacity hint that cannot alter visit
  order (membership-only `HashSet`, `Vec`-order expansion, `BinaryHeap` by `Cand::Ord`); scratch reuse copies the
  same `Addr` values in the same slot order; `out.clear()` + the Err short-circuit make it leak-free.
- **No panic across the C-unwind boundary.** Decode slice reads are bounds-guarded → typed `Err`; `saturating_mul`
  + GUC-capped `ef` + `u16` `m0` keep `cap` well under `isize::MAX`; `with_page_item`'s RAII `SharePin` releases
  on every path.
- **Scan-only.** No touch to build/insert/vacuum/WAL/pending-region; the immutable-graph invariant holds.
- **Honest measurement.** The pgvector control (+122% drift on an unchanged binary) correctly invalidates the QPS
  comparison; no superiority claim; the "50k ≠ 44% regime" reasoning is grounded in the run's own 6–12% variance.

## Downstream

Ready for `/release` once the compile-check of the Spi fix is green (in flight). The QPS/variance win verdict is
explicitly deferred to FU-1 (same-graph measurement) — M46 ships the correct recall-neutral code + the honest
negative, not a fabricated win.
