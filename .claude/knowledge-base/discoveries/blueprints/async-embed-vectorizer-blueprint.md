# Blueprint — M122 fully-async embedding (never pin xmin during the HTTP embed)

Date: 2026-07-20 · Source: `/roadmap-feature async-embed-vectorizer` discover (council-research-adr, web-evidenced).

## Bottom line

The fix is sound and ~80% wired. TheoDB's vectorizer queue is a **committed-lease** queue (not a
transactional-delete queue), so the crash-recovery machinery the split needs — lease expiry → re-claim,
`attempts` cap, `reap_orphans` — **already exists**. The only defect: the *process* step reads+embeds+writes
inside one `BackgroundWorker::transaction`, so the active snapshot pins `backend_xmin` for the whole HTTP
round-trip. Splitting into 3 top-level-txn phases is a small, well-supported change.

## Honest premise correction (SOTA does NOT embed outside a txn)

pgai's Vectorizer (Timescale, external Python worker) **deliberately holds the txn open across the embed**
(`_do_batch`: `async with conn.transaction(): _fetch_work(); _embed_and_write()`). It chose transactional
dequeue (atomic crash-safety via rollback) and *accepts* the long-held snapshot, engineering only around the
*lock* problem (queue table has no unique key), NOT the *xmin-horizon* problem. TheoDB's 3-phase split is a
**deliberate divergence** — correct precisely because our worker is **in-process on the same postmaster**
(ADR-0016), so its `backend_xmin` gates *local* autovacuum directly. → Record the divergence in a new ADR
(rejected alternative: pgai-style atomic txn, rejected because it pins local autovacuum).

## The mechanism (why it matters) — cited

- `pg_stat_activity.backend_xmin` = "the current backend's xmin horizon" (PG monitoring-stats docs).
- Routine-vacuuming docs: long open txns with large `age(backend_xmin)` must be ended — VACUUM can only remove
  row versions older than the oldest published horizon. A 90s embed = a 90s floor on the horizon = 90s of
  un-reclaimable dead tuples on the hottest tables.
- `worker_spi.c` idiom: `backend_xmin` is held between `PushActiveSnapshot`/`GetTransactionSnapshot` and
  `CommitTransactionCommand`. In the gap *between* two `BackgroundWorker::transaction` blocks there is no active
  snapshot and no XID → the worker pins nothing. That gap is exactly where the embed HTTP must run.

## Recommended 3-phase design (per batch group)

Replace the single `_vectorizer_process_upsert_batch` (vectorizer.rs:459) with three worker-orchestrated steps:

- **Phase A — READ + LEASE** (one `BackgroundWorker::transaction`, commits): the claim already committed
  `state='processing'`/lease/`attempts++`. Add: fetch `content` per pk **and** resolve `Config` (endpoint/model/
  api_key/target col+table/chunk strategy), return them as **owned Rust values** (`String`, owned Config).
  Commit → snapshot released.
- **Phase B — EMBED** (NO `BackgroundWorker::transaction`, NO SPI): call a new `run_batch_resolved(items,
  endpoint, model, api_key)` — pure HTTP (`post_json`), no `guc()`/SPI. `backend_xmin` invalid throughout →
  autovacuum can advance. Check `sigterm_received()` on return.
- **Phase C — WRITE + MARK** (one `BackgroundWorker::transaction`, commits): `UPDATE target SET col=$vec WHERE
  pk=$1` (idempotent overwrite) + owner-guarded `mark_done`; on B failure → `mark_failed` in this fresh txn.
  Then `bump_stats`.

Crash recovery is unchanged + already present: die before C-commit → lease expires → re-claim → re-embed
(ADR-0008: no cache); attempt cap → `reap_orphans` dead-letters. Chunk-table mode (M66, vectorizer.rs:478) fans
out per doc — same split (read docs in A, embed in B, `upsert_chunks` writes in C).

## Critical pitfalls (encode in the plan)

1. **No SPI in phase B.** Content + resolved Config must be pulled into owned Rust values in phase A and moved
   into B. pgrx returns copied `String`/`Vec` → safe if no `Datum`/PG pointer crosses the commit.
2. **`in_subtxn` does NOT help.** A subtxn runs under the parent's active snapshot; the split MUST be at the
   top-level txn boundary (two separate `BackgroundWorker::transaction` calls), not a subtxn.
3. **Error path is now two-sited.** An embed failure in B has no txn to roll back → route to `mark_failed` in a
   fresh phase-C txn. The B1 `PgTryBuilder` longjmp guard still matters for pgrx calls; pure-Rust HTTP won't
   longjmp.
4. **Lease clock** keeps running during B (no txn to lose a lease to its own snapshot, but the deadline ticks) —
   `WORKER_LEASE_SECS=120` ≥ max embed wall-time; `renew_lease` covers long batches.
5. **Signals**: check `sigterm_received()` before phase C; HTTP client honors the ~90s timeout so a hung
   endpoint in B doesn't wedge shutdown.

## Crash-safety (RQ4) — at-least-once + idempotent write

Failure window = process dies after B (HTTP 200) but before C commits. Recovery = **re-embed** (not cache):
job still `state='processing'` → lease expires → re-claim → re-embed. Write is overwrite-by-pk (idempotent);
`mark_done` is owner-guarded so a stale worker cannot mark a re-claimed job. Honest bounded cost: a crash can
waste one re-embed (double API cost for that batch) — the accepted SOTA trade-off (pgai re-embeds too);
exactly-once would need a content-hash cache (out of scope, ADR-0008).

## Benchmark obligation (DoD)

The xmin-horizon-release win is **UNBENCHMARKED** until shown reproducibly: a long-embed stub + `backend_xmin`
(from `pg_stat_activity`) sampled during the embed — pinned BEFORE the split, invalid/advancing AFTER. Artifact
in `docs/benchmarks/`. No perf claim without it (public-copy.md §4, PRD D3).

## Primary sources (all resolve)

- pgai vectorizer worker: https://github.com/timescale/pgai/blob/main/projects/pgai/pgai/vectorizer/vectorizer.py
- Timescale resilient-embedding design: https://www.timescale.com/blog/how-we-designed-a-resilient-vector-embedding-creation-system-for-postgresql-data/
- PG routine vacuuming (backend_xmin / long txns): https://www.postgresql.org/docs/current/routine-vacuuming.html
- PG monitoring-stats (backend_xmin = xmin horizon): https://www.postgresql.org/docs/current/monitoring-stats.html
- PG hot standby (hot_standby_feedback): https://www.postgresql.org/docs/current/hot-standby.html
- Transactional outbox: https://microservices.io/patterns/data/transactional-outbox.html
- Idempotent consumer: https://microservices.io/patterns/communication-style/idempotent-consumer.html
- worker_spi.c (txn/snapshot idiom): https://github.com/postgres/postgres/blob/master/src/test/modules/worker_spi/worker_spi.c
- PG bgworker: https://www.postgresql.org/docs/current/bgworker.html · SPI: https://www.postgresql.org/docs/current/spi.html

## Local anchors

- `theodb_rs/src/vectorizer.rs` — split target `_vectorizer_process_upsert_batch` :459; single-txn call to fix
  :729-745; lease primitives `_vectorizer_claim_batch` :201, `_vectorizer_mark_done` :254,
  `_vectorizer_mark_failed` :276, `_vectorizer_renew_lease` :300, `_vectorizer_reap_orphans` :524; constants :570-571.
- `theodb_rs/src/embed.rs` — `resolve_cfg` :129 (uses `guc()` = SPI, needs txn), `run_batch` :55, `post_json` (pure HTTP).
- `theodb_rs/src/pg.rs:50` — `guc()` = `Spi::current_setting` (SPI → requires txn; the reason cfg must resolve in phase A).
- `docs/adr/0016-m54-vectorizer-worker-mechanism.md` — names risk H2 (lock crossing HTTP → 3-phase); M122 completes it.
- `docs/adr/0008-no-embedding-chat-cache.md` — mandates re-embed (not cache) as the crash-recovery answer.
