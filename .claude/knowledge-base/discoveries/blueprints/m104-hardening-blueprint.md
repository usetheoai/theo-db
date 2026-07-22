# M104 Hardening — deep-research blueprint

**Date:** 2026-07-16 · **Milestone:** M104 · **Source audit:** `system-design-output/final_report.md` · **Rigor:** discover-phd-rigor R0/R2/R3 (web-cited).

Covers the 3 research-worthy items (Q1 write, Q2 scan, Q3 breaker). The other 23 findings are mechanical (delete rabitq, sign 0033, LRU-cap a HashMap, deprecation markers) — no blueprint, straight to plan.

## Q1 — Bounded-memory columnar WRITE under MVCC (CRITICAL #99)

**SOTA (web-cited):** the row-group/part/fragment is the atomic flush unit, flushed INCREMENTALLY, made visible by ONE atomic commit action.
- Parquet: row group flushed single-pass, footer (metadata) written LAST = commit point (parquet.apache.org/docs/file-format).
- DuckDB: `DEFAULT_ROW_GROUP_SIZE = 122880` rows, row-group-at-a-time; single-file "in-place ACID" (duckdb `storage_info.hpp`).
- ClickHouse MergeTree: one part per INSERT, atomically visible once written (clickhouse.com/docs/…/mergetree).
- Lance: append new fragments + manifest version bump = ACID commit (lance.org/format).

**In-tree mapping:** our stripe IS the atomic unit (`read_visible_stripes` gates on `columnar.stripe` catalog row xmin — = ClickHouse part / Lance manifest); `flush_pending` already writes pages→dir→header→catalog-row-LAST; `CHUNK_GROUP_ROWS=10000` is the sub-boundary; M96 `build_stream.rs` is the accumulate→flush→`pfree`→repeat discipline; `should_stream`'s `maintenance_work_mem` is the ready threshold.

**Recommendation (own-code, ~15 lines):** change "one stripe per xact" → "one stripe per `maintenance_work_mem` of pending". Track `pending_bytes` in WRITE_STATES; call existing `flush_pending` when over threshold; final drain at finish_bulk_insert/pre-commit. Peak RAM O(mwm), many stripes per INSERT, all sharing the xact xid (commit/abort atomically).

**Hazards (GATES):**
- **H1 (HIGH):** mid-executor SPI catalog insert re-entrancy under the INSERT's active snapshot. `with_active_snapshot` is a no-op when a snapshot exists (reads under query snapshot). Prove with a self-referential `INSERT INTO c SELECT FROM c` — must not see own mid-flush stripes.
- **H2 (HIGH→LOW):** same-xact scan unions committed-so-far stripes + pending rows; `flush_pending` removes pending atomically → disjoint (single-threaded). Test it.
- **H3 (CRITICAL):** crash-safety NOT regressed — each threshold-flush is an independent pages→catalog-row-LAST unit; crash between flushes → uncommitted catalog rows → invisible → correct. **Preserved BY CONSTRUCTION** (visibility still gated on single xact commit). Prove with a new crash permutation (crash between incremental flushes → count(*)==0).
- **H4 (MEDIUM debt):** aborted big INSERT leaks more orphan pages (same class as today's abort). VACUUM follow-up, filed not hidden.

## Q2 — Streaming columnar SCAN (HIGH)

**SOTA (web-cited):** pull one batch at a time. Arrow: table = "a sequence of record batches" (arrow.apache.org/docs/format/Columnar). DuckDB: vector-at-a-time (2048) over 122880-row row-groups. In-tree peer: ParadeDB `pg_search` columnar exec pulls fast-fields per batch (`.claude/knowledge-base/references/paradedb/.../fast_fields/columnar.rs`).

**In-tree mapping:** `ColumnarScanState` already has cursor shape (`rows` + `cursor`); the defect is `scan_begin` calls `materialize_rows` EAGERLY. `decode_stripe`'s inner chunk-group loop body IS one RecordBatch.

**Recommendation (own-code, ~40 lines):** `scan_begin` resolves the visible-stripe SET once (MVCC-correct, snapshot-fixed); `getnextslot` decodes ONE chunk-group at a time via `(stripe_idx, cg_idx, row_in_cg)` cursors + a `current_batch` (≤10000 rows) + a pending tail. Refactor `decode_stripe` body into `decode_one_chunk_group`. Peak RAM O(one chunk-group); time-to-first-row O(1). `rescan` must reset ALL cursors (R3). Leave `decode_columns` (M100 aggregate path, bounded by GreedyMemoryPool) untouched (YAGNI).

## Q3 — Per-backend circuit breaker + connection pool (HIGH + MEDIUM)

**SOTA (web-cited):** the Nygard/MS closed→open→half-open state machine.
- MS Azure "Circuit Breaker": Closed (count failures, trip over threshold in a time window → Open + timer); Open (fail fast immediately); Half-Open (limited probes; all-success → Closed, any-fail → Open). "Resource differentiation" + "Concurrency: shouldn't block" caveats.
- resilience4j: `failureRateThreshold=50%`, `minimumNumberOfCalls=100`, `waitDurationInOpenState=60000ms`, `permittedInHalfOpen=10`, state in AtomicReference.
- ureq::Agent (MIT/Apache-2.0 — passes D1): clones share a keep-alive connection pool; "retain a single pool for the entire process". minreq lacks a pool (fresh conn per send).

**In-tree mapping:** `http.rs::post_json` already has the per-call half (timeout, bounded retry, backoff+jitter, `with_max_redirects(0)` SSRF, 38000). The `thread_local` per-backend idiom is house style (WRITE_STATES, HeldInterrupts). Postgres = process-per-backend → no shared breaker without shm.

**Recommendation (own-code ~60 lines + ureq):** a `thread_local! { BREAKERS: HashMap<endpoint, Breaker> }`, K=5 consecutive failures → Open 30s (GUC) → Half-Open one probe; check at the top of `post_json`'s loop, record success/failure. Replace minreq with a per-backend `ureq::Agent` (OnceLock/thread_local pool). `MAX_BATCH` GUC chunks batched AI. **Security no-regress:** ureq must preserve redirect=0, api-key-in-header-only, 38000 — behavior-preserving refactor with existing oracles.

**Honest risk — per-backend sufficiency:** per-backend FULLY solves the actual finding (a per-row surface over N rows runs in ONE backend → first ~5 rows trip it, rest fail fast). It does NOT coordinate across N backends (each burns ~5 probes). Shared-shm breaker = strictly more complex (shm+LWLock+ABI-fragile) for a marginal gain → **documented M104 non-goal until a measured multi-backend workload demands it** (anti-sunk-cost/YAGNI, like RaBitQ).

## Decision records to write (each with rejected alternatives)
- Q1 `incremental-stripe-flush-under-mvcc`: reject (a) whole-txn buffer [OOM], (b) temp-file spill [reinvents tuplesort]; choose incremental threshold-flush. MUST reference the #46/#47 crash-safety invariant.
- Q2 `streaming-columnar-seqscan`: reject (a) eager [O(N)], (c) push-to-DataFusion [over-scoped]; choose lazy chunk-group cursor.
- Q3 `per-backend-http-circuit-breaker`: reject (a) no breaker, (c) shared-shm [accidental complexity until measured]; choose per-backend + ureq pool.

## Citations (all fetched live)
Parquet file-format+configurations; DuckDB storage_info.hpp + lightweight-compression post; ClickHouse MergeTree docs; Lance format; Arrow Columnar format; MS Circuit Breaker pattern; resilience4j CircuitBreaker; ureq Agent docs + Cargo license. In-tree: `am/{columnar,build_stream,df_executor,fold,http}.rs` + ParadeDB peer.
