# M122 — Async embed releases the xmin horizon (measured + source-proven)

**Date:** 2026-07-20 · **Box:** DO droplet (32 GB), pgrx-managed PG17, `theodb_rs` installed, mock embedding
endpoint sleeping 8s (`benchmarks/mock_slow_embed.py`). **Verdict:** the 3-phase split RELEASES `backend_xmin`
during the embed; the pre-M122 single-txn path PINS it. This is a **real fix**, not a no-op.

## The claim

The vectorizer worker embeds a batch by calling an HTTP endpoint (≤ ~90s under a hung endpoint). Pre-M122 the
embed ran INSIDE the worker's process transaction, so the transaction's active snapshot pinned `backend_xmin`
for the whole HTTP round-trip — delaying local autovacuum. M122 splits the work into three top-level
transactions (read+lease → **embed with no open txn** → write+mark), so the embed holds no snapshot.

## Evidence 1 — Source proof (definitive: the mechanism itself)

pgrx's `BackgroundWorker::transaction` holds an **active snapshot for the entire closure body**
(`pgrx-0.19.0/src/bgworkers.rs:335-343`, the canonical `worker_spi.c` idiom):

```rust
StartTransactionCommand();
PushActiveSnapshot(GetTransactionSnapshot());     // ← active snapshot pushed BEFORE the body
let result = PgTryBuilder::new(transaction_body).execute();   // ← the body runs here, snapshot ACTIVE
PopActiveSnapshot();
CommitTransactionCommand();                        // ← snapshot released only here
```

- **Pre-M122 (single-txn):** the embed is part of `transaction_body` (`_vectorizer_process_upsert_batch` runs
  inside `BackgroundWorker::transaction(|| in_subtxn(|| ...))`). The active snapshot is held across the 8s HTTP
  → `backend_xmin` PINNED for the whole embed.
- **M122 (3-phase):** the embed (`embed::run_batch_resolved`) runs BETWEEN two `BackgroundWorker::transaction`
  calls — there is no active snapshot and no XID in that gap → `backend_xmin` is invalid → the horizon advances.

Per the PostgreSQL docs, `pg_stat_activity.backend_xmin` is "the current backend's xmin horizon"; VACUUM cannot
reclaim rows newer than the oldest such horizon. So an 8s snapshot held across the embed = an 8s floor on the
horizon; releasing it lets local autovacuum proceed during the embed.

## Evidence 2 — Measurement (the 3-phase worker holds no xmin during a real embed)

Sampled `pg_stat_activity.backend_xmin` of the `theodb vectorizer worker` backend every 0.5s while it processed
a real 1→1 vectorizer job against the 8s mock (the embed actually ran — a vector was written, the mock logged
the POST):

| Measurement | Samples | `backend_xmin` held | `backend_xmin` free |
|---|---|---|---|
| In-place worker (M122 3-phase), during an 8s embed | 28 (~14s @ 0.5s) | **0** | **28** |

The worker held NO snapshot at any of the 28 samples spanning the 8s embed. Vector written = the 3-phase path
ran end-to-end.

## Evidence 3 — Positive control (the sampler DOES detect a held xmin)

A concurrent `BEGIN ISOLATION LEVEL REPEATABLE READ; SELECT txid_current(); pg_sleep(...)` session (which holds
one snapshot for its whole life) was sampled the same way:

| Control | `backend_xmin` held | age(backend_xmin) |
|---|---|---|
| Held REPEATABLE READ session | **t (yes)** | 48 |

So "0 held" for the worker is a real negative, not a blind sampler — the sampler reports a held xmin as held.

## Conclusion

Evidence 1 (the pgrx snapshot mechanism) proves the pre-M122 single-txn embed pins `backend_xmin`; Evidence 2
proves the M122 3-phase worker releases it during a real embed; Evidence 3 proves the measurement can detect a
held xmin. Together: **M122 releases the xmin horizon for the embed's duration — a real fix, measured.**

## Reproduction

- `benchmarks/mock_slow_embed.py` (8s stub) + a `shared_preload_libraries='theodb_rs'` cluster with
  `theodb.embedding_endpoint` pointed at it.
- Same-worker A/B apparatus: the `theodb.vectorizer_single_txn` GUC (default off = shipped 3-phase; on = force the
  pre-M122 single-txn path on the same worker) — an operator kill-switch that also reproduces the A/B by sampling
  the worker's `backend_xmin` with the GUC off vs on. Source proof needs no run.

## Honest caveats

- The absolute VACUUM-delay reduction (seconds of horizon released) is bounded by the embed latency and the churn
  on the source table; it is not a throughput number and is not claimed as one (public-copy.md §4).
- Chunk-table mode (M66) keeps the single-txn path this milestone (documented drawback) — it still pins xmin for
  chunk vectorizers; the 1→1 in-place mode (the common case) is fixed.
