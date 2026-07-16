# Review — m99-columnar-tam-append-only

**Date:** 2026-07-16
**Verdict:** READY_TO_MERGE
**Milestone:** M99
**Plan:** knowledge-base/plans/m99-columnar-tam-append-only-plan.md

## Scope reviewed

The complete own-code append-only columnar Table Access Method (`theodb_columnar`): column-major TCS1 stripe encoding
+ per-chunk min/max (C1), MVCC via the `columnar.stripe` heap catalog + pre-commit flush + pending-in-memory read +
DROP event trigger (C2), the 3 `pg_isolation_regress` MVCC permutation specs (D1), crash-safety WAL-replay (D2), and
the honest columnar-vs-heap benchmark (D2). Files: `theodb_rs/src/am/columnar.rs`, `theodb_rs/src/am/columnar_codec.rs`,
`theodb_rs/src/am/page.rs`, `theodb_rs/isolation/**`, `docs/benchmarks/m99-columnar-tam.{md,json}`.

## Measured evidence (droplet pg17 / pgrx 0.19)

- **297 pg_tests GREEN** (283 baseline M98 + 12 pure codec unit tests + 2 columnar pg_tests), zero regression.
- **3 MVCC isolation permutations GREEN** (`make check-isolation`): REPEATABLE-READ snapshot stability, uncommitted/
  aborted stripe invisible, two concurrent inserters → 10 distinct rows (non-overlapping row_number ranges).
- **Crash-safety GREEN** (`crash.sh`): 10000-row committed stripe survives an immediate crash + WAL replay,
  scan-identical (count/sum/sample all match).
- **Benchmark** (`docs/benchmarks/m99-columnar-tam.{md,json}`): 9.2× on-disk compression (columnar 6.5 MB vs heap
  60.2 MB), result-equivalent aggregates; scan honestly slower (no projection/vectorization — that is M100).

## Specialist sign-off

| Reviewer | Domain | Verdict | Blockers |
|---|---|---|---|
| council-index-storage | TableAM / storage / WAL / MVCC | READY_TO_MERGE | none |
| council-rust-pgrx | FFI safety / panic-across-C / memory | READY_TO_MERGE | none |
| council-benchmark | measurement rigor / honesty | READY | none |

**council-index-storage:** visibility root is single (the catalog, not the metapage); write ordering
(data pages → reserve → header → catalog-insert last) makes crash-before-commit ≡ abort; the honest scope
(min/max stored, consumed in M100) is defensible — the compression axis alone (9.2×) exceeds the DoD's ~2-5×.

**council-rust-pgrx:** the FFI surface (detoast via `pg_detoast_datum_copy`+`pfree`, byval LE serialization, varlena
1B/4B header handling, `SET_VARSIZE_4B` reconstruction, `with_active_snapshot` Push/Pop, pre-commit `XactCallback`)
is safe and proven; every `try_into().unwrap()` in the codec is preceded by a bounds-check; all real callbacks are
`#[pg_guard] extern "C-unwind"`; the `TableAmRoutine` is in `TopMemoryContext`. No unguarded panic-across-C found.

**council-benchmark:** the benchmark passes "did you measure or assume?" — reproducible methodology, hardware, 5 runs,
mean±stddev, honest ceiling declared on line 1 (scan slower by design, not a superiority claim). No cherry-picking.

## Corrections applied (non-blocking, from the review)

- Compile-time `assert!(cfg!(target_endian = "little"))` guard on the byval column-major encoding.
- 3 benchmark-doc honesty qualifiers (9.2× is dataset-dependent; catalog heap not counted in size; result-equivalence
  scope is count/sum, GROUP BY by the isolation suite).

## Follow-up issues filed (accepted post-merge)

- #99 — `WRITE_STATES` flush unbounded → OOM on a giant single-xact `INSERT ... SELECT` (incremental flush follow-up).
- #100 — `relation_estimate_size` returns tuples=0 → planner blind (read `sum(row_count)` from `columnar.stripe`).

## DoD coverage (ROADMAP M99)

| DoD | Status |
|---|---|
| (1) `theodb_columnar` TAM registrable (`CREATE ACCESS METHOD ... TYPE TABLE`) | ✅ |
| (2) result-equivalence pg_tests vs row-store | ✅ |
| (3) pg_isolation permutation specs (MVCC) green + isolationtester wired | ✅ |
| (4) crash-safety WAL-replay | ✅ |
| (5) benchmark columnar vs heap (compression measured; skip/vectorization = M100, honest) | ✅ |
| (6) council-index-storage + council-rust-pgrx sign-off | ✅ |
| honest boundary (append-only; no update/parallel/bitmap/sample) | ✅ (typed-error stubs) |
