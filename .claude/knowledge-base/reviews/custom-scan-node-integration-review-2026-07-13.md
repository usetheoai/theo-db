# Review — M92/M93 arbitrary-WHERE Custom Scan node integration

**Date:** 2026-07-13 · **Slug:** custom-scan-node-integration · **Milestones:** M92 + M93 · **Verdict:** `READY_TO_MERGE` (after fixes)

Three specialist councils reviewed the M92/M93 Custom Scan Provider (a from-scratch hand-rolled planner+executor node in pgrx). All returned `NEEDS_FIXES` on the first pass; every BLOCKER and HIGH is now fixed and re-verified (263 tests GREEN).

## Severity matrix (first pass → disposition)

| Council | Verdict | BLOCKER | HIGH | MEDIUM/LOW |
|---|---|---:|---:|---:|
| council-rust-pgrx | NEEDS_FIXES → fixed | 1 | 3 | 2 |
| council-index-storage | NEEDS_FIXES → fixed | 1 (== rust-pgrx B1) | 0 | 1 |
| council-benchmark | NEEDS_FIXES → fixed | 1 | 2 | 3 |

## The convergent BLOCKER (both code councils) — FIXED

**Two vecfilter nodes in one plan cross-contaminated the per-backend membership → silent wrong results.** The executor runs every node's `BeginCustomScan` (Init) before any pull (Exec), and the AM reads the membership lazily at scan time, so a `UNION` / self-join / partitioned `Append` of two filtered vector queries would have the second node's membership overwrite the first → the first branch silently drops valid rows.

**Fix (commit `46a9704`):** `begin_custom_scan` now **fails loud** (`pg_sys::error!`) if a membership is already active — never a silent wrong answer (Unbreakable Rule 8). New pg_test `m93_concurrent_vecfilter_fails_loud` proves a `UNION` of two filtered vector queries errors. This is the councils' explicitly-accepted path for a GUC-gated spike; per-scan membership scoping (to *support* concurrent scans) is a documented follow-up. The concurrency limit is disclosed in the CHANGELOG + benchmark boundary.

## HIGH findings — all FIXED

| Finding | Council | Fix (commit `46a9704`) |
|---|---|---|
| Membership leak past an `error!` longjmp → next plain scan reads stale set | rust-pgrx H1 / index-storage MEDIUM-1 | an xact-ABORT callback (`RegisterXactCallback`) clears the membership |
| `rescan` swallowed a bad bitmap + didn't clear-then-set | rust-pgrx H2 | rescan clears-then-sets + `error!`s on a non-TIDBitmap |
| `vector_path` selection unvalidated (any ordered path) | rust-pgrx H3 | require `pathtype == T_IndexScan` for the vector child |
| POST's `max_scan_tuples=20000` ceiling undisclosed; POST not swept | benchmark | disclosed in the benchmark md/json; POST confirmed the strongest native alternative (its recall floor 0.59–0.67 is below INLINE's whole sweep) |
| "262 pg_tests" unsupported by the pg_test-attribute count | benchmark | corrected to "263 tests (`cargo pgrx test` — pg_tests + unit)", the real test-run total |

## MEDIUM — dispositioned

- **M2 (rust-pgrx):** tag-check `bplan` is `T_BitmapHeapScan` before `.lefttree` — **FIXED** (commit `7f67c43`).
- **M1 (rust-pgrx, targetlist shape):** tied to H3; the H3 `T_IndexScan` guard + the child-slot pass-through are sound for a same-rel scan — accepted.
- **Benchmark MEDIUM/LOW:** matched-recall→Pareto-dominance relabel, v7-is-pg_test-proven caveat, provenance measurement commit — all **FIXED**.

## Key confirmations (councils verified SOUND)

- **The MVCC recheck is correct** (index-storage): `theodb_ivfflat`'s opclass has no scalar operators, so `cat=K` can never be an index cond of the vector index — it always lands as the vector child IndexScan's qpqual Filter, applied by `ExecScan` to EVERY emitted row. Lossy over-admits + pending rows are rechecked out. No path emits a non-matching row (single-node case).
- **recall 0.95 (not 1.0) is an ANN artifact, ACCEPTABLE — not a leak** (index-storage): recall is monotone in probes (0.953→0.969), the classic IVF probes tradeoff; a correctness leak would be a fixed wrong set. The `m93_t2` pg_test proves byte-identical-to-seqscan at full probe.
- **No panic across the C boundary** (rust-pgrx): every corrupt-state path is `pg_sys::error!`, never a bare panic; the `VecFilterState` struct-embed + `palloc0` init are sound; the TIDBitmap is not double-freed.
- **No page-format change** — `page.rs` untouched (no REINDEX); GUC-off path byte-identical.

## Hard gates (cycle-review)

- Full suite **263 tests GREEN, 0 failed** (droplet `cargo pgrx test pg17`), incl. 8 M92/M93 tests + the new concurrency-guard test.
- No commits to `main`; no `Co-Authored-By`; CHANGELOG updated; on `develop`. No secrets. No format change.
- Benchmark (M92 DoD): INLINE dominates POST +0.28/+0.32 recall AND 1.4–12× QPS, MEASURED (`docs/benchmarks/m92-arbitrary-where.{md,json}`).

## Verdict

`READY_TO_MERGE`. The convergent BLOCKER is resolved by fail-loud (never silent-wrong); all HIGHs fixed; the benchmark is honest and traceable. Proceed to `/release`.
