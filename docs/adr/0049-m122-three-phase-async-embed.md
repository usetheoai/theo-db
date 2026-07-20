# ADR 0049 — M122: 3-phase async embed in the vectorizer (release the xmin horizon)

**Status:** Accepted (2026-07-20) · **Milestone:** M122 · **Supersedes/completes:** the H2 risk anticipated in
ADR-0016 (vectorizer worker mechanism). **Related:** ADR-0008 (no embedding/chat cache).

## Context

The vectorizer background worker (ADR-0016) embeds a batch by calling an HTTP endpoint (≤ ~90s under a hung
endpoint). Pre-M122 the embed ran INSIDE the worker's process transaction: `_vectorizer_process_upsert_batch`
did read + `embed::run_batch` (HTTP) + write, all inside `BackgroundWorker::transaction(|| in_subtxn(|| …))`.
Because `BackgroundWorker::transaction` pushes an active snapshot for the entire closure body
(`pgrx-0.19.0/src/bgworkers.rs:335-343`, the `worker_spi.c` idiom), the transaction's snapshot pinned
`backend_xmin` for the whole HTTP round-trip — a floor on the local autovacuum horizon for the embed's duration.

## Decision

Split the in-place (1→1) batch embed into three TOP-LEVEL transactions, orchestrated by the worker:

- **Phase A** (`_vectorizer_read_batch`, one `BackgroundWorker::transaction`, commits): read the content +
  resolve the network cfg (endpoint/model/api_key) into OWNED Rust values.
- **Phase B** (`embed::run_batch_resolved`, **NO** `BackgroundWorker::transaction`, **NO** SPI/GUC): the HTTP
  embed runs with no active snapshot → `backend_xmin` is released for the whole call.
- **Phase C** (`_vectorizer_write_batch`, one `BackgroundWorker::transaction`, commits): write the vectors
  (idempotent overwrite-by-pk) + owner-guarded `mark_done`.

`embed::run_batch_resolved` is a no-GUC/no-SPI extraction of the HTTP+parse tail of `run_batch`; the cfg is
resolved in phase A because `guc()` reads GUCs via SPI (needs a txn).

## Rationale + alternatives rejected

- **pgai-style atomic dequeue-embed-write in one txn** (the SOTA reference — Timescale pgai holds the txn across
  the embed on purpose) — REJECTED: it pins `backend_xmin` across the embed, which is exactly the local-autovacuum
  stall M122 fixes. Acceptable for pgai's EXTERNAL worker; not for our IN-PROCESS bgworker whose `backend_xmin`
  gates local autovacuum directly.
- **Cache the in-flight embedding by content-hash for exactly-once** — REJECTED: out of scope per ADR-0008; the
  crash-recovery answer is re-embed (below), matching the SOTA (pgai also re-embeds on crash).

## Crash recovery (at-least-once + idempotent write)

A crash after phase B (HTTP 200) before phase C commit leaves the job `state='processing'`; the lease expires →
`_vectorizer_claim_batch` re-claims → re-embed. The write is overwrite-by-pk (idempotent); `mark_done` is
owner-guarded so a stale worker whose lease expired cannot mark a re-claimed job. Bounded cost: a crash wastes
one re-embed (double API cost for that batch) — the accepted SOTA trade-off.

## Consequences

- The xmin horizon is released for the embed's duration (measured: worker `backend_xmin` = 0/28 held during a
  real 8s embed; source-proven via the pgrx snapshot mechanism — `docs/benchmarks/m122-async-embed-xmin.md`).
- One extra commit per batch vs the single-txn path — trivial vs a multi-second HTTP.
- Chunk-table mode (M66) keeps the single-txn path this milestone (documented drawback — still pins xmin for
  chunk vectorizers); the 1→1 in-place mode is fixed.
- A `theodb.vectorizer_single_txn` GUC (default off) is retained as an operator kill-switch (revert to the
  pre-M122 single-txn path) and the same-worker A/B apparatus.
