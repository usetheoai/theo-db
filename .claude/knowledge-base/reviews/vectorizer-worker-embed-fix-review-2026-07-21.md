# Review — M132 (#132): vectorizer worker diagnosability

**Slug:** vectorizer-worker-embed-fix
**Milestone:** M132
**Date:** 2026-07-21
**Reviewers:** council-security (attack surface / fail-closed) + council-rust-pgrx (unsafe, FFI, node lifetime)
**Verdict:** READY_TO_MERGE

## Scope

`theodb_rs/src/vectorizer.rs` (`in_subtxn_msg`, `sanitize_error_text`, `startup_config_line`, `process_one`, the
`Some(n) if n > 0` arm, 4 new `#[pg_test]`s), plus the evidence and the dogfood anchor file.

## Findings — both reviews 0 BLOCKER, 0 HIGH; all MEDIUM/LOW resolved

| Sev | Reviewer | Finding | Resolution |
|---|---|---|---|
| MEDIUM | security | The endpoint body echo (200 chars) is now **persisted** in `last_error`; an echo/debug endpoint reflecting `Authorization` writes a token into a durable row | RESOLVED — `sanitize_error_text` redacts `Bearer …` / `sk-…` and bounds the text **at the sink**; verified live (injected token absent from every row) |
| MEDIUM | rust-pgrx | `Some(0)` → fallback lands on the lease-lost path; `process_one` counted a job the worker no longer owns (double-count with the new owner), violating the H1 fencing contract | RESOLVED — `process_one` returns the owner-guarded `mark_done` result |
| LOW | rust-pgrx | The new startup diagnostic ran SPI unprotected — a PG ERROR would crash-restart the worker | RESOLVED — subtxn-isolated |
| LOW | security | `mark_done` interpolated while the sibling bound parameters | RESOLVED — both arms bound |
| LOW | security | `truncate` enforced only at the call site | RESOLVED — moved into the sink |
| INFO | security | Exact `api_key_len` fingerprints the provider | ACCEPTED — the diagnostic value (0 vs >0, and "is the whole key loaded") outweighs a marginal fingerprint on a self-hosted log |
| INFO | rust-pgrx | Allocating a `String` inside `catch_others` | VERIFIED SOUND against pgrx 0.19 source (C `ErrorData` freed + state flushed before the panic is raised) |

Follow-ups filed by the reviews as non-blocking (not absorbed here): the duplicate-embed cost on the lease-lost
fallback path, and an ops-doc line recommending `GRANT INSERT` (not `GRANT ALL`) on the queue.

## DoD check

| DoD item | Status |
|---|---|
| The old blanket literal no longer reaches `last_error` | Met ✓ (0 uses as an argument; only a comment + the test asserting it is not used) |
| Real cause recorded | Met ✓ — `ERRCODE_EXTERNAL_ROUTINE_EXCEPTION: theodb.embed_batch: endpoint call failed: circuit open` |
| Startup log with key length, never the value | Met ✓ (test + live line) |
| `Some(0)` falls back instead of counting as success | Met ✓ |
| End-to-end drains with 0 failures and N/N embeddings | Met ✓ (5/5, queue empty — before and after the change) |
| Evidence + dogfood file + #132 closed | Met ✓ (#132 CLOSED with the non-reproduction proof) |

## Verdict

**READY_TO_MERGE.** 0 residual BLOCKER/HIGH. The milestone is an honest-negative on the reported symptom plus the
diagnosability that made it expensive, with every review finding — including two defects this milestone itself
introduced — fixed at the root and re-verified on the shipped binary.
