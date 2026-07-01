# M26 vector Index Access Method — implementation summary

**Slug:** m26-vector-index-am · **milestone_id:** M26 · **Date:** 2026-07-01
**Plan:** `.claude/knowledge-base/plans/m26-vector-index-am-plan.md` (SHIPPABLE 93.2)
**Verdict:** IMPLEMENTATION_COMPLETE → READY_TO_MERGE

Promoted the in-memory rebuild-per-query ANN into two persisted Postgres Index Access Methods
(`theodb_ivfflat`, `theodb_hnsw`) on pgrx 0.16 FFI. Closes the architecture audit's single HIGH.

## Commits

| SHA | Task | Summary |
|---|---|---|
| `c123e66` | T0.1 | de-risk spike: register IndexAmRoutine + CREATE ACCESS METHOD (the ROADMAP ALTO risk, proven) |
| `ce444f6` | T1.1–T4.1 | page persistence (GenericXLog) + ambuild + scan + opclass pushdown (IVFFlat, l2) |
| `c1a1a90` | T5.1 | incremental INSERT (pending buffer) / DELETE (MVCC recheck) / VACUUM (fold) |
| `c7a98e7` | T6.1 | theodb_hnsw AM sharing the layer (Persisted enum dispatch by blob magic) |
| `e4d141f` | T7.1 | benchmark (16× vs rebuild) + coexistence + scope ADR 0010 |
| `3514322`, `1db6305` | review-fix | BLOCKER + HIGH from `/review` (NULL crash, wrong-results, concurrency lock, corruption safety, HNSW ef) |
| `58798d1` | review-fix | benchmark json + ADR D4/D5 |

## DoD coverage (ROADMAP M26)

| DoD | Evidence |
|---|---|
| IndexAmRoutine registered (all 8 hooks) | `am/mod.rs make_amroutine` — all `Some(...)`, all real |
| CREATE INDEX USING theodb_hnsw persisted | `theodb_hnsw` AM + opclass; `pg_relation_size>0`; build-once |
| Planner pushdown (EXPLAIN Index Scan) | amcanorderbyop + amcostestimate; test asserts Index Scan |
| Incremental INSERT/DELETE/VACUUM | `test_incremental_insert_delete_vacuum` green |
| Benchmark recall parity + latency | recall@5 ≥ 4/5; 86ms vs 1372ms = 16×; `docs/benchmarks/m26-index-am.{md,json}` |
| Coexistence (no M20–M22 break) | 61 passed |

## Wiring triad (per phase)

Behavior-adding feature. Callers + tests + runtime observability:
- Caller: the Postgres planner/executor invokes the AM hooks via the registered `IndexAmRoutine` (CREATE INDEX / ORDER BY <-> LIMIT). Runtime metric: `EXPLAIN (ANALYZE)` node timings + `pg_relation_size` (persistence) — the ops-visible signals used in the benchmark.
- Tests: `test_index_am.py` (7) end-to-end + `ivf_persist_tests`/`hnsw_persist_tests` unit round-trips.

## Architecture

- `am/` (infra: pg_sys FFI) — mod (registration) · build (ambuild/aminsert) · scan · page (GenericXLog persistence) · vacuum(mod) · index (Persisted dispatch) · tid (TID codec) · lock (advisory fold lock). All ≤ 500 LoC.
- `ann/` (pure domain) — unchanged algorithms + additive to_bytes/from_bytes/search_merged/entries/rebuilt_with + shared `wire.rs` codec. Zero pg_sys leak (verified).

## Deviations (documented — ADR 0010)

- D1: l2 opclass ships; cosine/ip = follow-up (pgrx 0.16 lacks get_opfamily_name for opclass→metric).
- D2/D5: blob-per-scan O(N) deserialize (16× vs rebuild, slower than seq scan at small N); partial-page-read optimization = follow-up.
- D3: fixed build params (lists=100, m=16, ef_construction=64); reloptions = follow-up.
- D4: concurrency serialized by an advisory lock; scan-state `Box` leak on abort-mid-scan (bounded, not-UB) = palloc-arena follow-up.

All are documented decisions with rejected alternatives + follow-up paths, not workarounds.
