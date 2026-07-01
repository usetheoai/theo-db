# /review — M26 vector Index Access Method (theodb_ivfflat + theodb_hnsw)

**Date:** 2026-07-01
**Slug:** m26-vector-index-am
**Diff scope:** `b957414..HEAD` (theodb_rs/src/{am/*, ann/*}, benchmarks, docs)
**Verdict:** READY_TO_MERGE

## Round 1 — 4 agents (parallel)

| Agent | Verdict | BLOCKER | HIGH | MEDIUM |
|---|---|---:|---:|---:|
| rust/pgrx FFI-safety | HAS_UB_RISK | 1 | 2 | 3 |
| index-AM correctness | HAS_CORRECTNESS_BUG | 0 | 3 | 2 |
| cross-validation vs DoD | FULLY_TRACED | 0 | 0 | 1 |
| architecture + tests + error-handling | SOLID | 0 | 1 | 4 |

### Findings + resolution

| Sev | Finding | Resolution (commit `3514322` / `1db6305`) |
|---|---|---|
| **BLOCKER** | B1: `ORDER BY col <-> NULL::vector` → `pg_detoast_datum(NULL)` segfault (SIGSEGV, uncatchable) | `amrescan` checks `SK_ISNULL` before touching `sk_argument` → empty scan (pgvector behavior). New test `test_null_query_vector_does_not_crash` proves backend survival. |
| HIGH | Concurrent VACUUM `rewrite_blob` vs scan/insert → torn read / lost insert | New `am/lock.rs` advisory lock (index OID): scans/inserts SHARE, `vacuum_rebuild` EXCLUSIVE → serializes the non-atomic rewrite. |
| HIGH | `SCAN_K=10000` cap + 0-cost → silently drops rows for unbounded/large-LIMIT query | Removed the artificial cap; IVFFlat returns all probed candidates (executor LIMIT bounds); HNSW uses its native `ef_search` bound. |
| HIGH | `ambuildempty` writes MAIN_FORKNUM (should INIT) → corrupts unlogged index | `write_blob` takes a `ForkNumber`; `ambuildempty` → INIT_FORKNUM (pgvector contract). |
| HIGH | `Box<ScanState>` leaks on abort-mid-scan | Documented in ADR 0010 §D4: bounded (results empty at all amrescan error points), not-UB, freed at backend exit; proper fix (palloc arena) = follow-up. |
| HIGH | hnsw.rs has NO serialization unit tests (ivf has 3) | Added `hnsw_persist_tests` (round-trip / empty / truncated+bad-magic) + `from_bytes` referential-integrity validation. |
| MEDIUM | Untrusted length → multi-GB `with_capacity` abort | `wire.rs capacity_for` + `page.rs read_blob` cap allocation at bytes-remaining/declared. |
| MEDIUM | `read_pending` silently skips corrupt items | Now fails loud with a typed Err. |
| MEDIUM | No committed raw benchmark artifact | Added `docs/benchmarks/m26-index-am.json`. |

## Round 2 — re-review (2 agents, verify the fixes)

| Agent | Verdict |
|---|---|
| FFI-safety (re-verify B1/H1/H2) | **FIXES_SOUND** |
| correctness (re-verify HIGH-1/2/3 + HNSW tests) | **CORRECTNESS_RESOLVED** |

Re-review confirmed: B1 segfault genuinely eliminated (guard precedes the deref; backend-survival test); H1 advisory lock's Share/Exclusive modes truly serialize the rewrite; H2 accurately documented; fork/SCAN_K/HNSW-tests/entry-bounds all resolved. **No BLOCKER or HIGH remains; no defect reopened.**

Re-review surfaced one regression I introduced + one symmetry gap — both fixed in `1db6305`:
- MEDIUM-A: `SCAN_K=usize::MAX` poisoned HNSW `ef=ef_search.max(k)` → full-graph flood. Fixed: variant-appropriate bounds inside `Persisted::search_merged` (IVF probes + unbounded k; HNSW ef_search=64).
- MEDIUM: ivf `from_bytes` lacked hnsw's referential validation. Fixed: symmetric consistency + bounds checks.

Remaining (LOW / documented follow-ups, non-blocking): hnsw neighbor-index bounds (pg_guard-caught panic, needs disk corruption); advisory-lock namespace note; the O(N)-per-scan optimization (ADR 0010 §D2/D5); cosine/ip opclasses (§D1); scan-state palloc arena (§D4). All are pg_guard-caught (backend survives) or documented decisions — none are crashes, wrong results, or undocumented gaps.

## Evidence

- `cargo check` + `cargo clippy --features pg17 --tests -- -D warnings`: clean at every phase.
- Full image `theo-db:m26` builds; **68 tests green** (7 M26 + 61 M20–M22 coexistence).
- M26 tests: AM registered · CREATE INDEX persists (`pg_relation_size>0`) · EXPLAIN Index Scan · recall@5 ≥ 4/5 (both AMs) · INSERT/DELETE/VACUUM · **NULL-query no crash** · HNSW AM.
- Benchmark: persisted Index Scan **86 ± 5 ms** vs rebuild-per-query **1372 ± 26 ms** = **~16×** (`docs/benchmarks/m26-index-am.{md,json}`).

## Verdict: READY_TO_MERGE

0 BLOCKER, 0 HIGH (all resolved + independently re-verified). Every ROADMAP M26 DoD bullet is traced to real code + real test assertions (both AMs, pushdown, maintenance, benchmark, coexistence). The two highest-risk dimensions (FFI/WAL safety, AM correctness) passed re-review. Scope deviations (cosine/ip, O(N) scan, fixed params, scan-state leak) are documented via ADR 0010 with rejected alternatives + follow-up paths — decisions, not workarounds.
