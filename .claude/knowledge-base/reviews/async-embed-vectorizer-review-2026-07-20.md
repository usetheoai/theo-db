# Review — M122 3-phase async embed (async-embed-vectorizer)

**Date:** 2026-07-20 · **Slug:** async-embed-vectorizer · **Milestone:** M122
**Branch:** develop (commit `df8a8aa` + LOW-fix follow-up) · **Verdict:** READY_TO_MERGE

## Scope

The vectorizer worker's in-place (1→1) batch embed split into 3 top-level transactions (read+lease → embed
off-txn → write+mark) so a slow/hung embedding endpoint no longer pins `backend_xmin` (delaying local autovacuum).
`embed.rs` (`run_batch_resolved` no-GUC extraction), `vectorizer.rs` (`_vectorizer_read_batch`/`_vectorizer_write_batch`
+ worker 3-phase routing), `guc.rs` (`theodb.vectorizer_single_txn` kill-switch/A/B GUC).

## Reviewers (domain-appropriate for a bgworker / txn-boundary change)

### council-rust-pgrx — Rust/pgrx FFI + txn/panic safety → **SOUND (no BLOCKER/HIGH/MEDIUM)**

Verified: (1) phase B touches no SPI/GUC — the only SPI-backed GUC reader `pg::guc()` is confined to phase A;
(2) longjmp out of phase B is safe to catch with no open txn (phase B holds zero PG resources — no LWLock/pin/SPI/
subtxn), `AssertUnwindSafe` justified (only shared immutable borrows, nothing read after unwind); (3) `BatchRead`
is fully owned — no Datum/PG pointer crosses the phase-A commit; (4) `in_subtxn` correct; the embed genuinely runs
between two top-level txns; (5) `vectorizer_single_txn()` is a process-local `GucSetting::get()` (not SPI); (6) no
uncaught unwind across the C ABI on the hung/5xx path; N-in==N-out makes phase-C indexing bounds-safe. Positive
security note: the new phase fns are `pub(crate)` non-`#[pg_extern]` → REVOKE surface unchanged.

### council-index-storage — crash-safety / lease / xmin claim → **SOUND (no BLOCKER/HIGH/MEDIUM); a REAL fix, not a no-op**

Independently verified the pgrx mechanism against source (`pgrx-0.19.0/src/bgworkers.rs:335-343`) and refuted the
"could be a no-op like M121" concern: pgrx pushes ONE active snapshot for the whole closure (not per-statement),
so the pre-M122 single-txn embed genuinely pinned `backend_xmin` across the HTTP; the 3-phase split releases it.
Timeout math checks out: worst-case embed `(MAX_RETRIES+1)×HTTP_TIMEOUT = 90s` < `WORKER_LEASE_SECS=120` (30s
margin), renewed per-group right before phase B. Crash between B and C: idempotent overwrite-by-pk + owner-guarded
`mark_done` + lease re-claim / reaper → no orphan, no lost job, at-least-once (one wasted re-embed) the only cost.
Every failure (phase A/B/C, SIGTERM) funnels to `mark_failed`/re-claim/reaper — no stuck-forever path.

## Findings (all LOW/INFO — none block merge)

| Sev | Finding | Disposition |
|---|---|---|
| LOW | Off-txn longjmp catch is safe only because phase B holds no PG resources; a future edit adding SPI/txn work to that path would break it | **FIXED** — strengthened the load-bearing invariant doc comment on `run_batch_resolved` (`embed.rs`) |
| LOW | Concurrent source UPDATE (not just crash) between phase A and C → transiently stale vector; self-heals under single-worker, widens window for the future multi-worker launcher | **FIXED** — acknowledged in ADR-0049 Consequences with the multi-worker forward caveat |
| LOW | Pre-existing: Rust destructors skipped on the `err_external` longjmp through `post_json` (~KB leak per failed embed) — unchanged by M122 | Tracked as pre-existing (`http.rs`); not an M122 regression, not fixed here |
| LOW | Chunk-mode does a redundant phase-A read (discarded before the single-txn path re-reads) | KISS-acceptable — chunk mode is the deferred drawback; early-routing adds complexity for the rare case (YAGNI). Not fixed. |
| INFO | Commit message "NULL-before-GUC order preserved" is imprecise at the orchestration level (phase A resolve_cfg now precedes phase B's NULL check) — no functional regression (both funnel to the identical per-job fallback) | Noted; no code change |

## Gate checks

- Build: `cargo build --features pg17` EXIT=0. No new dependency.
- Dead-code: all new symbols (`run_batch_resolved`, `resolve_batch_cfg`, `_vectorizer_read_batch`,
  `_vectorizer_write_batch`, `embed_resolved`, `validate_inputs`, `vectorizer_single_txn`) have callers.
- No secrets; no `Co-Authored-By`; no direct commit to `main`.
- CHANGELOG `[Unreleased] § Fixed` updated; ADR-0049 + `docs/benchmarks/m122-async-embed-xmin.md` present.
- Evidence: source proof (pgrx snapshot mechanism, independently re-verified by both reviewers) + measured
  (worker `backend_xmin` 0/28 held during a real 8s embed) + positive control (held session detected, age=48).

## Verdict

**READY_TO_MERGE** — two independent domain reviews SOUND with no BLOCKER/HIGH/MEDIUM; both confirm M122 is a
genuine, source-proven, measured xmin-horizon fix (emphatically not a no-op). The two actionable LOW findings are
fixed; the rest are pre-existing or KISS-acceptable and documented. Proceed to `/release`.
