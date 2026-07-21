# M132 — #132: worker-failure diagnosability (measured), and the honest non-reproduction of the reported symptom

> Measured 2026-07-21 on the self-hosted droplet (165.227.121.20, PG17.10 pgrx-install, `theodb_rs` rebuilt with the
> M132 changes; **NOT canonical hardware** — but this milestone measures *behaviour*, not performance).
> Discovery: `knowledge-base/discoveries/blueprints/vectorizer-worker-embed-blueprint.md`.

## Headline

**The symptom reported in #132 does NOT reproduce.** The background worker embeds fresh rows end-to-end. What was
real — and what this milestone fixes — is that when a job *does* fail, nothing tells you why.

## 1. Non-reproduction of the reported symptom (before any code change)

Clean end-to-end on the pre-change build: stale dead-letters cleared → 5 fresh rows inserted → worker polled.

| Step | Observed |
|---|---|
| `DELETE FROM theodb.vectorizer_queue WHERE state='failed'` (5 stale rows from 2026-07-20) | queue empty |
| `INSERT` 5 rows into `df_docs` | `pending 5` |
| wait 90 s | **queue EMPTY — 0 rows, 0 `failed`** |
| `count(*), count(embedding) FROM df_docs_chunks` for the 5 pks | **5 chunks, 5 embeddings** |

The "NULL embedding column" in the original report is the **expected** shape for a chunk-mode vectorizer: chunks are
written to `df_docs_chunks (source_pk, chunk_index, chunk_text, embedding)`, not to `df_docs.embedding`.

## 2. The real defect: every failure looked identical (red → green, measured)

**Before.** `in_subtxn` caught with `PgTryBuilder::catch_others(|_| None)` — the `CaughtError` was discarded — and
the mark was a hardcoded literal: `_vectorizer_mark_failed(job, owner, 'embed/upsert failed', …)`. A 401, a missing
embedding GUC and a malformed response all produced the same eight words. That is why #132 could not be diagnosed
without a debugger.

**After.** The caught SQLSTATE + message are returned and stored. Forced failure by pointing the endpoint at a dead
port (`http://127.0.0.1:1/v1/embeddings`), restart, insert one row, wait:

```
state  | attempts | last_error
failed |    5     | ERRCODE_EXTERNAL_ROUTINE_EXCEPTION: theodb.embed_batch: endpoint call failed: circuit open
```

The same failure previously recorded only `embed/upsert failed`. The cause is stored via a **bound parameter**, not
string interpolation — an arbitrary error message concatenated into the SQL literal would be an injection vector.

## 3. Worker startup config line (measured)

The probable cause of the original report is a worker that booted **without** the `ALTER SYSTEM` embedding GUCs (a
restart that silently did not take effect). It was invisible; it is now one line, emitted at worker start:

```
LOG:  theodb vectorizer worker: embedding_endpoint=set embedding_model=set api_key_len=164
```

The api key is reported by **length only** — the value never reaches the log (asserted by
`test_m132_startup_log_never_logs_key_value`). A GUC-blind worker prints `embedding_endpoint=MISSING … api_key_len=0`.

## 4. Zero-row batch no longer counts as success

`match batch_done { Some(n) => processed += n, … }` accepted `Some(0)`: a batch that ran cleanly but embedded
**nothing** was counted as processed and its jobs consumed — no result, no failure signal. Now `Some(n) if n > 0`
takes the success arm and a zero-row batch joins `None` on the per-job fallback, whose outcome is always observable
(done, or a real cause in `last_error`).

## 5. Post-change end-to-end (no regression)

Endpoint restored to the real one, queue cleared, 5 fresh rows:

| Check | Result |
|---|---|
| queue after 90 s | **empty — 0 rows, 0 `failed`** |
| chunks / embeddings for the 5 pks | **5 / 5** |
| worker startup line present | yes (see § 3) |

## Verification notes (honest)

- **The DoD's grep criterion was refined during verification.** It originally read "`grep -c 'embed/upsert failed'`
  returns 0". The literal legitimately survives in **two non-code places**: a comment documenting what changed, and
  the test that asserts the value is *not* used. The meaningful check — the literal is never passed as an argument —
  is `grep 'embed/upsert failed' … | grep -v '^\s*//' | grep -v assert_ne | wc -l` → **0**. The substance (the
  literal no longer reaches `last_error`) is proven directly by § 2.
- **Anti-silent-restart gate.** Every verification restart asserts `postmaster_start_time > .so mtime` before
  trusting the result. This is not ceremony: earlier in this same investigation a `pg_ctl restart` with a wrong `-D`
  failed silently and produced a false "confirmed" reading. The same trap is the probable cause of #132 itself.
- The 2026-07-20 root cause is **not provable** — the server log was rotated away. It is recorded as *probable*,
  never as established fact.

## Verdict

**#132 closed as non-reproducing, with the diagnosability gap it exposed fixed and measured.** A failing job now
names its cause (SQLSTATE + message) instead of a blanket literal, the worker declares its embedding-config view at
startup without leaking the key, and a zero-row batch can no longer consume jobs silently. The end-to-end embeds
5/5 with the queue draining to zero failures, before and after the change.
