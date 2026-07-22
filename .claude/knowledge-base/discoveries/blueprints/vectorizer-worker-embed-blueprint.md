# Blueprint — #132 does NOT reproduce: the vectorizer bgworker embeds correctly; what actually failed was diagnosability

> Discover executed 2026-07-21 by live reproduction on the self-hosted droplet (measurement-first). Feeds M132.
> **Headline: the reported symptom is NOT reproducible on the current build.** A clean end-to-end run embeds 5/5
> rows through the background worker with an empty queue and zero failures. The real, durable defects found are
> (a) the error path discards the underlying cause, and (b) a zero-row batch is counted as success.

## Context

#132 reported that on a self-hosted TheoDB the async vectorizer background worker **dead-letters every embed job**
(`state='failed'`, `attempts=5`, `last_error='embed/upsert failed'`), leaving the embedding NULL, while
`theodb.embed(...)` succeeds in a normal session. Build under test in the issue: `develop` sha `11c4efa`
(v0.111.0). M132 was opened to fix it because it blocks the dogfood anchor's *freshness* half.

## Empirical result — the symptom does not reproduce

Environment: droplet 165.227.121.20, PG17.10 (pgrx-install, `/tmp/pgabdata`), `theodb_rs` 1.0.0 built from current
`develop` (v0.117.0-era), `shared_preload_libraries='theodb_rs'`, worker process live
(`postgres: theodb vectorizer worker`). Embedding GUCs set via `ALTER SYSTEM` and visible in session
(endpoint `https://api.openai.com/v1/embeddings`, model `text-embedding-3-small`, api-key length 164).

**Clean end-to-end (the DoD-3 shape):**

| Step | Observed |
|---|---|
| `DELETE FROM theodb.vectorizer_queue WHERE state='failed'` (5 stale 2026-07-20 rows) | queue empty |
| `INSERT` 5 fresh rows into `df_docs` | `pending 5` (trigger + enqueue work) |
| wait 90 s (worker poll) | **queue EMPTY — 0 rows, 0 `failed`** |
| `SELECT count(*), count(embedding) FROM df_docs_chunks WHERE source_pk IN (7001..7005)` | **5 chunks, 5 with embedding** |
| server log | no worker error (only this investigation's own bad-column probes) |

An earlier single-row probe behaved identically: a fresh insert was consumed by the worker and its chunk embedded,
with no manual step.

**Why the source column looked empty (and is not a defect):** every vectorizer on `df_docs` has
`chunk_strategy='fixed'`, and chunk-mode writes embeddings to the **chunk table** `public.df_docs_chunks`
(`source_pk, chunk_index, chunk_text, embedding`), NOT to `df_docs.embedding`. `df_docs.embedding IS NULL` is the
expected shape for a chunked vectorizer. The 5 rows that DO carry a column embedding are the manual backfill
recorded in the 2026-07-20 evidence file.

## Most probable cause of the 2026-07-20 failure (honest, not proven)

The issue's own repro requires `ALTER SYSTEM SET theodb.embedding_*` **followed by a restart so the worker boots
with them**. A worker that boots without the embedding GUCs fails every embed while a later session — which reads
the same values after a config reload — succeeds. That is exactly the observed discriminator.

This is not idle speculation: during this very investigation `pg_ctl restart` was issued with a wrong `-D` and
**failed silently**, leaving the old binary/config live and producing a misleading "confirmed" result until the
postmaster start time was checked. The same silent-restart failure on 2026-07-20 explains the original report
without requiring any code defect. It is recorded as *probable*, not proven — the historical log was rotated away.

## Coverage Corner 1 — Integration tests

The regression risk is not "does embed work" (it does) but "**can we tell why it failed when it fails**". A test
that asserts a failing job records a *specific* cause (not the generic wrapper) is the durable guard. The clean
end-to-end above is the anchor-level integration check and is reproducible via `benchmarks/dogfood_anchor_smoke.sh`.

## Coverage Corner 2 — Dependencies

None new. Everything lives in `theodb_rs/src/vectorizer.rs` + `embed.rs` with pgrx/`pg_sys` already declared.

## Coverage Corner 3 — Tools

`psql` for the end-to-end, the server log for uncaught errors, `ps` to confirm the worker process. No new tooling.
Note for future diagnosis: PostgreSQL's `errfinish` calls `EmitErrorReport()` **before** the longjmp, so an ERROR
caught by `PG_TRY`/`PgTryBuilder` still reaches the **server log** even though the Rust handler discards it — the
server log is the fallback channel when `last_error` is uninformative.

## Coverage Corner 4 — Techniques

Two real defects found by reading the worker while chasing the symptom:

1. **The error path throws away the cause.** `vectorizer.rs::in_subtxn` catches with
   `PgTryBuilder::catch_others(|_| None)` — the `CaughtError` (and its message) is discarded — and the failure mark
   is a hardcoded literal: `_vectorizer_mark_failed(job, owner, 'embed/upsert failed', …)`. Every failure, from a
   401 to a missing GUC to a malformed response, collapses to the same eight words. **This is what made #132 cost a
   day**: with no cause in `last_error` and no worker startup log, the only way to distinguish "worker cannot see
   the GUCs" from "endpoint returned 5xx" was a debugger.
2. **A zero-row batch counts as success.** In the group loop,
   `match batch_done { Some(n) => processed += n, None => …per-job fallback… }` — `Some(0)` takes the success arm.
   A batch call that runs cleanly but processes **nothing** is counted as processed and never falls back to the
   per-job path, so the jobs are consumed without work being done and without any failure signal.

## ADRs

### ADR-1 — M132 ships diagnosability + the two real defects; it does NOT ship a fix for a symptom that is absent

**Decision:** re-scope M132 from "make the worker embed" (already true, proven end-to-end) to: (a) make the failure
cause observable (`last_error` carries the real message; worker logs GUC presence + api-key *length* at startup),
(b) fix the `Some(0)`-as-success arm, (c) close #132 with the non-reproduction evidence.

**Rationale (Rule 3):** fabricating a fix for a non-reproducing symptom would be dishonest and untestable — there is
no red test to turn green. The durable value is that the *next* occurrence is diagnosable in one log line instead of
a day. The two defects found are real, independently verifiable by code reading, and directly caused the original
mystery.

**Alternatives rejected:** (i) close #132 as "works for me" with no change — REJECTED: leaves the exact blindness
that made it expensive, guaranteeing a repeat. (ii) Invent a speculative GUC fix (register `theodb.embedding_*` as
custom GUCs) — REJECTED: that was the *hypothesis*, never confirmed; changing operator-visible configuration on an
unproven theory is a workaround, and the startup log will confirm or refute it cheaply if it recurs.

## Verdict

**Honest-negative on the reported symptom, with two real defects harvested.** The worker embeds 5/5 end-to-end; the
dogfood anchor's freshness half is demonstrably working today. M132 ships observability + the zero-row-batch fix so
the next failure is diagnosable, and records the probable original cause (a silent restart leaving the worker
without the GUCs) without claiming it as proven.
