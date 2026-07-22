# Review — m101-arrow-cache-heap-authoritative

**Date:** 2026-07-16
**Verdict:** READY_TO_MERGE
**Milestone:** M101
**Plan:** knowledge-base/plans/m101-arrow-cache-heap-authoritative-plan.md

## Scope reviewed

The heap-authoritative in-memory Arrow columnar cache (HTAP): the `theodb_columnarize` pragma + heap→Arrow build (A),
the `columnar.cache_state` generation catalog + invalidate-on-write trigger + snapshot-correct rebuild (B), the M100
CustomScan extended to admit a heap-with-usable-cache (C), and the MVCC isolation permutations + HTAP benchmark (D).
Files: `theodb_rs/src/am/{arrow_cache.rs,columnar_agg.rs,df_executor.rs}`, `theodb_rs/isolation/**`,
`docs/benchmarks/m101-arrow-cache.{md,json}`.

## Measured evidence (droplet pg17 / pgrx 0.19)

- **303 pg_tests GREEN**, zero regression (`m101_cache_agg_matches_heap`, `m101_write_invalidates_cache`,
  `m101_heap_cache_customscan_matches_heap`).
- **2 `pg_isolation_regress` MVCC permutations GREEN** (`make check-isolation`): cross-backend invalidation (a
  committed write → the reader rebuilds → sees the new row) + REPEATABLE READ snapshot stability (the RR reader holds
  its snapshot across a concurrent commit; a fresh xact after commit sees the new row).
- **HTAP benchmark:** the vectorized Arrow-cache aggregate (52.4 ms) is **2.48× faster than the native heap
  aggregate** (130.0 ms), 2M rows, 5 runs, EXPLAIN-confirmed CustomScan.

## Specialist sign-off

| Reviewer | Domain | Verdict | Blockers |
|---|---|---|---|
| council-index-storage | storage / MVCC / snapshot isolation | READY_TO_MERGE | none |
| council-benchmark | measurement rigor / honesty | READY (after doc fixes) | none |

**council-index-storage:** the MVCC correctness is sound and proven — the invalidation `generation` is read via MVCC
(a read-only SPI runs under the reader's ActiveSnapshot, so the generation read and the rebuild seqscan are
co-snapshot), so `built_generation == current_generation` is a correct "the committed set I see is the set the cache
captured" test. No per-row xmin/xmax (the M99 D2 re-implement-MVCC trap is avoided). The generation lives in a heap
table, so a writer's abort reverts the bump for free. UPDATE/DELETE/TRUNCATE all invalidate (statement trigger);
concurrent writers serialize on the cache_state row lock. No snapshot-incorrectness window found.

**council-benchmark:** the 2.48× is measured, apples-to-apples (same table, same persistent session — the cache is
per-backend — with the GUC as the only delta), and honest about its ceiling (a write costs a rebuild; manual pragma;
OLTP non-interference is structural). Required traceability corrections (all applied): cite the authoritative
equivalence test, fix the isolation spec paths, mark OLTP-p95-under-load as not measured.

## Corrections applied (from the review)

- **`// MVCC-LOAD-BEARING` comment** on `cache_state` (the read-only-SPI-⇒-ActiveSnapshot invariant the correctness
  depends on — fragile to a mutating-SPI refactor; council-index-storage HIGH-doc).
- Benchmark-doc corrections: authoritative equivalence is `m101_cache_agg_matches_heap` (floats within 1e-6, not
  "byte-for-byte"); corrected the isolation spec paths; scorecard states OLTP-p95-under-load is NOT measured.

## Follow-up issues filed (accepted post-merge)

- #104 — read-your-own-write permutation (the case the read_only-SPI flip would bite), a count(*)-only admission
  test, and an OLTP-p95-under-load benchmark.

## DoD coverage (ROADMAP M101)

| DoD | Status |
|---|---|
| (1) Arrow cache derived + refresh/invalidation on write | ✅ (A + B) |
| (2) planner chooses cache vs heap by cost | ✅ (C — admitted with a cheap cost; native plan otherwise) |
| (3) pg_isolation MVCC permutations green | ✅ (2 specs) |
| (4) HTAP benchmark (OLAP accelerated) | ✅ (2.48×); OLTP-p95-under-load is a documented follow-up (#104) |
| (5) sign-off council-index-storage + council-benchmark | ✅ |
| honest boundary (manual pragma, not auto-tuned) | ✅ |

## Honest scope note

Slice-1: `count(*)` / `sum(float8)` over a single-column-cached heap table; a per-backend cache (shared-memory
residency is a follow-up); OLTP-p95-under-load is a structural argument, not a load measurement (#104). No superiority
claim vs AlloyDB's auto-maintained in-core engine.
