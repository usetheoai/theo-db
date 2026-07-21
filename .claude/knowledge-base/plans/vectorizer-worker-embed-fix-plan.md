---
slug: vectorizer-worker-embed-fix
milestone_id: M132
created_at: 2026-07-21
goal: Make a failed vectorizer job record its real cause (instead of a hardcoded eight-word wrapper) and stop counting a zero-row batch as success, so the next worker failure is diagnosable from one log line
---

# Plan — M132 (#132): worker-failure diagnosability + zero-row-batch defect

## Goal

Replace the hardcoded `'embed/upsert failed'` mark with the **real caught error message**, log the worker's
embedding-config visibility at startup (endpoint/model presence + api-key **length**, never the value), and stop
`Some(0)` from counting as a processed batch — so a failing vectorizer job is diagnosable from `last_error` + one
startup log line instead of a debugger.

**Single metric:** a job that fails because the worker cannot resolve the embedding config records a `last_error`
containing the **specific** cause (not the literal `embed/upsert failed`), asserted by a test and recorded in
`docs/benchmarks/m132-vectorizer-diagnosability.md`.

## Context

The discovery blueprint (`knowledge-base/discoveries/blueprints/vectorizer-worker-embed-blueprint.md`) proved by
clean end-to-end run that **#132 does not reproduce**: the background worker embeds 5/5 fresh rows, the queue drains
to empty with 0 failures, and the chunk table carries 5 embeddings. The "NULL embedding column" in the original
report is the *expected* shape for a chunk-mode vectorizer (chunks land in `df_docs_chunks`).

What the investigation did surface are two real defects that made the original report cost a day, and an honest
probable cause (a silent `pg_ctl restart` leaving the worker without the `ALTER SYSTEM` embedding GUCs — the exact
mistake reproduced accidentally during this very investigation). Per ADR-1 in the blueprint, M132 ships the
diagnosability and the zero-row defect rather than fabricating a fix for an absent symptom.

## Baseline Context

Repo state: git sha `abf94ef`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `theodb_rs/src/vectorizer.rs` | 900+ | `in_subtxn` discards the caught error; `mark_failed` is called with the literal `'embed/upsert failed'`; `match batch_done { Some(n) => processed += n, … }` accepts `Some(0)` | Capture the caught message and thread it into the failure mark; add the worker startup config log; treat a zero-row batch as a fallback trigger, not success |
| `docs/benchmarks/m132-vectorizer-diagnosability.md` | — | (NEW) | Evidence: the clean end-to-end non-reproduction + the before/after of `last_error` |
| `.claude/knowledge-base/dogfood/evidence/` | — | (NEW file) | Anchor evidence recording that the freshness half now passes end-to-end |

### Current callers / dependents (verified `file:line`)

- `theodb_rs/src/vectorizer.rs:658` — `in_subtxn(f)` — `PgTryBuilder::catch_others(|_| None)` discards the `CaughtError`; called from the per-job path, the chunk-batch path, and the phase-A read.
- `theodb_rs/src/vectorizer.rs:762` — `_vectorizer_mark_failed({job_id}, '{owner}', 'embed/upsert failed', …)` — the hardcoded literal every failure collapses to.
- `theodb_rs/src/vectorizer.rs:675` — `theodb_embed_worker_main` — the worker entry; `connect_worker_to_spi` then the poll loop (the startup-log insertion point).
- `theodb_rs/src/vectorizer.rs:~853` — `match batch_done { Some(n) => processed += n, None => …per-job fallback… }` — the `Some(0)`-as-success arm.
- `theodb_rs/src/embed.rs:163` — `resolve_batch_cfg` reads the `theodb.embedding_*` GUCs via `pg::guc()` (`SELECT current_setting(name, true)`); this is what a GUC-blind worker fails on.

### Domain glossary

- **`in_subtxn`** — runs a closure in an internal subtransaction; on error it rolls the subtxn back and returns `None`, isolating a poison row from the batch (council H-1).
- **dead-letter** — a queue row at `attempts >= max_attempts` left in `state='failed'` with `last_error`.
- **chunk-mode** — `chunk_strategy IS NOT NULL`; embeddings are written to the chunk table (`source_pk, chunk_index, chunk_text, embedding`), NOT to the source column.
- **phase A/B/C (M122)** — read+resolve cfg in a txn → commit → HTTP with no txn (no `backend_xmin` pin) → write+mark in a fresh txn.

### Architecture boundaries affected

Per `rules/architecture.md`: change is confined to the vectorizer worker module (`theodb_rs/src/vectorizer.rs`). No
schema change, no new SQL function signature, no API surface change. `_vectorizer_mark_failed` already accepts an
error-text argument — only the *value* passed changes.

## Prior Art & Related Work

- Blueprint (live end-to-end, 2026-07-21): `knowledge-base/discoveries/blueprints/vectorizer-worker-embed-blueprint.md` — the non-reproduction proof, the two defects, and the probable original cause.
- Issue #132 (the original report + its discriminator), `knowledge-base/dogfood/evidence/2026-07-20-anchor-failure-modes.md` (the `outcome: partial` record this milestone supersedes).
- PostgreSQL: `errfinish` calls `EmitErrorReport()` **before** the longjmp, so a `PG_TRY`-caught ERROR still reaches the server log — the fallback diagnosis channel documented in the blueprint's Tools corner.
- pgrx `PgTryBuilder::catch_others(|e| …)` exposes the `CaughtError`, whose message is what T1.1 threads through.

## ADRs

### ADR M132-1 — ship diagnosability + the zero-row defect; do NOT fabricate a fix for an absent symptom

**Decision:** M132 delivers (a) real cause in `last_error`, (b) a worker startup config log, (c) `Some(0)` no longer
counted as success, (d) #132 closed with the non-reproduction evidence. It does **not** change GUC registration or
the embed path.

**Rationale (cites the blueprint + Rule 3):** the end-to-end proves the worker embeds 5/5 with an empty queue; there
is no red test to turn green. Inventing a fix would be untestable and dishonest. The durable value is that the next
occurrence is diagnosable in one line instead of a day — which is precisely what failed here.

**Alternatives rejected:**
- **Register `theodb.embedding_*` as custom GUCs** — REJECTED: that was the unproven *hypothesis*; changing operator-visible configuration on a theory is a workaround. The startup log confirms or refutes it cheaply if it recurs.
- **Close #132 as "works for me" with no change** — REJECTED: leaves the exact blindness that made it expensive, guaranteeing a repeat.

### ADR M132-2 — a zero-row batch falls back to the per-job path instead of counting as processed

**Decision:** `batch_done == Some(0)` takes the fallback arm (per-job processing), not the success arm.

**Rationale:** a batch that runs cleanly but processes nothing has done no work; counting it as processed consumes
the jobs with no result and no failure signal. The per-job path either succeeds or records a real failure — either
outcome is observable, which the current silent-success is not.

**Alternatives rejected:** marking the group failed immediately — REJECTED: the per-job path may legitimately succeed
for some rows (poison-row isolation is the existing, proven design).

## Dependencies

`## Dependencies`: **none new**. Uses pgrx 0.19.0 (already in `theodb_rs/Cargo.toml`) — `PgTryBuilder`,
`pgrx::log!`/`warning!`, and the existing `pg::guc()`. No crate added (parsimony rung 4).

## Coverage Matrix

| Goal claim | Task |
|---|---|
| `last_error` records the real caught cause instead of the literal | T1.1 |
| Worker logs embedding-config visibility at startup (key length, never the value) | T1.2 |
| A zero-row batch falls back instead of counting as success | T2.1 |
| Evidence + dogfood anchor update + #132 closed with proof | T3.1 |

## Phase 1 — diagnosability

### T1.1 — thread the caught error message into `_vectorizer_mark_failed`

#### Why this step
Every failure currently collapses to `'embed/upsert failed'`, so a 401, a missing GUC and a malformed response are
indistinguishable — the direct cause of #132 costing a day. Reasoning: `PgTryBuilder::catch_others(|e| …)` exposes
the `CaughtError`; capture its message in `in_subtxn`, return it to the caller, and pass it (escaped, truncated) as
the `mark_failed` error text instead of the literal.

#### Files to edit
- `theodb_rs/src/vectorizer.rs`.

#### TDD
- RED: `test_m132_mark_failed_records_real_cause` — force a job whose processing raises a distinctive typed error; assert `last_error` **contains that error's text** and is **not** equal to `embed/upsert failed`.
- GREEN: change `in_subtxn` to return the caught message; thread it into the failure mark.
- REFACTOR: single truncation/escaping helper for the error text (SQL-literal safe).

#### Concurrency tests
A concurrent test with two racing worker owners contending for the same job; assert the stale owner's `mark_failed`
(now carrying an error message) is still rejected by the owner guard, so the new message argument cannot let a
losing incarnation overwrite the winner's outcome. This keeps the existing fencing contract under a real race
rather than assuming it.

#### Acceptance criteria
- `grep -c "embed/upsert failed" theodb_rs/src/vectorizer.rs` returns `0` (the hardcoded literal is gone).
- `test_m132_mark_failed_records_real_cause` asserts `last_error` contains the raised error's text and `cargo test` exits 0.

#### DoD
- `cargo build` exits 0; the test passes.

### T1.2 — worker startup log of embedding-config visibility

#### Why this step
The probable original cause (a worker booted without the `ALTER SYSTEM` GUCs after a silent restart failure) is
invisible today. Reasoning: at `theodb_embed_worker_main` startup, after `connect_worker_to_spi`, read the three
embedding GUCs and log presence for endpoint/model and the **length** of the api-key — never the value (a secret
must not reach the log).

#### Files to edit
- `theodb_rs/src/vectorizer.rs`.

#### TDD
- RED: `test_m132_startup_log_never_logs_key_value` — build the log line from a known key and assert it contains the length but **not** the key substring.
- GREEN: implement the log-line builder + emit it at worker startup.
- REFACTOR: keep the builder a pure function so it is testable without a worker.

#### Concurrency tests
(none — single-threaded)

#### Acceptance criteria
- The log-line builder returns a string containing `api_key_len=` and NOT containing the key value (asserted).
- The line names the endpoint/model presence so a GUC-blind worker is identifiable from the log alone.

#### DoD
- Test green; the line is emitted once per worker start.

## Phase 2 — the zero-row-batch defect

### T2.1 — `Some(0)` triggers the per-job fallback

#### Why this step
ADR M132-2: a clean batch that processed zero rows currently takes the success arm, consuming jobs with no work and
no signal. Reasoning: change the match so only `Some(n)` with `n > 0` counts as processed; `Some(0)` joins `None` on
the per-job fallback, whose outcome is always observable (done or a real `last_error`).

#### Files to edit
- `theodb_rs/src/vectorizer.rs`.

#### TDD
- RED: `test_m132_zero_row_batch_falls_back` — a batch call returning 0 must route to the per-job path (asserted by the resulting queue state: jobs are either done or carry a non-generic `last_error`, never silently vanished).
- GREEN: change the match arm.
- REFACTOR: none beyond the match.

#### Concurrency tests
A concurrent test over the fallback: two racing workers on the same group, assert each job is
processed exactly once (no duplicate chunk rows) — the lease renewal + owner guard must still hold when `Some(0)`
newly routes a group into the per-job path. Adds no parallelism of its own; proves the existing race contract
survives the new routing.

#### Acceptance criteria
- `Some(0)` no longer increments `processed`; the per-job fallback runs (asserted by the test).
- No regression on the `Some(n>0)` path (the end-to-end still drains 5/5).

#### DoD
- Test green; the end-to-end re-run still embeds 5/5 with an empty queue.

## Phase 3 — evidence

### T3.1 — evidence, dogfood anchor update, and closing #132

#### Why this step
The milestone's original purpose was unblocking the dogfood anchor's freshness half — which the discovery proved is
already working. Reasoning: record the non-reproduction end-to-end plus the before/after `last_error` shape, write a
dogfood evidence file for the passing anchor, and close #132 with the proof rather than leaving a phantom open bug.

#### Files to edit
- `docs/benchmarks/m132-vectorizer-diagnosability.md` (NEW); `.claude/knowledge-base/dogfood/evidence/2026-07-21-anchor-freshness-pass.md` (NEW); `CHANGELOG.md`.

#### TDD
- RED: the evidence file does not exist and `gh issue view 132` shows OPEN.
- GREEN: evidence written from the measured run; #132 closed with the evidence comment.

#### Concurrency tests
(none — single-threaded)

#### Failure scenarios
- The re-run end-to-end fails to drain (worker regression from T1/T2): the evidence records the failure honestly and the milestone does NOT claim the anchor passes — an honest BLOCKED beats a false PASS.
- The OpenAI endpoint is unavailable at re-run time: the run is recorded `UNBENCHMARKED` with the reason; no fabricated pass.

#### Acceptance criteria
- `docs/benchmarks/m132-vectorizer-diagnosability.md` records the measured end-to-end (queue drains to 0, N/N chunks embedded) and the before/after `last_error`.
- `gh issue view 132 --json state` returns `CLOSED` with a comment citing the evidence path.

#### DoD
- Evidence + dogfood file committed; CHANGELOG `[Unreleased]` updated; #132 closed.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| The captured error text could leak sensitive content (an endpoint URL with a token in the query string) into `last_error` | MEDIUM | Truncate and never include the api-key GUC; the startup log logs only the key **length**; a test asserts the key value never appears | engine |
| Changing `in_subtxn`'s return type touches three call sites (per-job, chunk-batch, phase-A read) — a mistake could break poison-row isolation | MEDIUM | Keep the `None`-on-error semantics identical; only add the message alongside. Re-run the end-to-end after the change (it exercises all three paths) | engine |
| `Some(0)` fallback could double-process a job that the batch already marked done | MEDIUM | The per-job path is owner-guarded and idempotent by design (overwrite semantics); the end-to-end re-run asserts no duplicate chunks | engine |
| The evidence depends on a live OpenAI endpoint + a valid key on the droplet | LOW | Record `UNBENCHMARKED` with the reason if unavailable; never fabricate a pass | benchmarks |

## Unresolved Questions

- Was the 2026-07-20 failure really a GUC-blind worker? Not provable — the historical log was rotated. The startup log added here answers it definitively if it ever recurs; the blueprint records it as *probable*, not proven.
- Should the 7 duplicate vectorizers on `df_docs` (accumulated across dogfood runs) be deduplicated? Out of scope here; noted for a follow-up if it distorts future anchor runs.

## Failure scenarios

- **A caught error carries no message** (panic without ereport): the mark falls back to a generic-but-labelled text (e.g. `embed/upsert failed (no message)`), still distinguishable from the old blanket literal. Reproduced by a test that raises a bare panic.
- **The worker cannot resolve the embedding GUCs**: the startup log names which of endpoint/model/api-key is missing, and the resulting `last_error` carries the resolver's typed error text — the exact scenario #132 could not diagnose.
- **The batch path returns 0 for every group**: every job routes to the per-job fallback; the queue never silently empties without work (asserted by T2.1).

## Global Definition of Done

- [ ] `grep -c "embed/upsert failed" theodb_rs/src/vectorizer.rs` returns `0`, and `test_m132_mark_failed_records_real_cause` asserts `last_error` contains the raised error text; `cargo test` exits 0.
- [ ] The startup log-line builder emits `api_key_len=<n>` and a test asserts the key **value** never appears in it.
- [ ] `Some(0)` routes to the per-job fallback (asserted by `test_m132_zero_row_batch_falls_back`); `processed` is not incremented for a zero-row batch.
- [ ] End-to-end re-run on the droplet: queue drains to **0 rows with 0 `failed`** and **N/N chunks carry embeddings**, recorded in `docs/benchmarks/m132-vectorizer-diagnosability.md`.
- [ ] A dogfood evidence file records the anchor's freshness half passing; `gh issue view 132 --json state` returns `CLOSED`.
- [ ] `CHANGELOG.md` `[Unreleased]` updated; the evidence states the honest-negative (the reported symptom did not reproduce) rather than claiming a fix.

## Final Phase — Integration Validation

- `cargo build` + `cargo test` green.
- Rebuild on the droplet, restart PG **with the postmaster start time verified** (the silent-restart trap this milestone documents), re-run the end-to-end.
- council-rust-pgrx review (error capture across the C boundary, no secret in logs) + council-security review (the `last_error` content cannot leak a credential).
