---
slug: async-embed-vectorizer
milestone_id: M122
created_at: 2026-07-20
goal: Split the vectorizer embed into 3 top-level-txn phases so the worker's backend_xmin is released during the HTTP embed
---

# Plan — M122 Embed totalmente assíncrono no vectorizer (fecha o xmin-pin)

## Goal

Split the vectorizer batch-embed into 3 top-level-transaction phases (read+lease → embed-no-txn → write+mark) so the background worker's `backend_xmin` is **released** during the HTTP embed — verified by a measured `pg_stat_activity.backend_xmin` sample that is **invalid/advancing** while a ≥8s stub embed is in flight (it is pinned on the pre-split baseline).

**Single metric:** the worker backend's `backend_xmin` is `NULL`/absent (no held snapshot) during the stub embed, measured on the droplet — pinned before, released after.

## Context

Consumes the discovery blueprint `.claude/knowledge-base/discoveries/blueprints/async-embed-vectorizer-blueprint.md`. The vectorizer background worker (ADR-0016, `docs/adr/0016-m54-vectorizer-worker-mechanism.md`) is a committed-lease job queue: `_vectorizer_claim_batch` commits `state='processing'`/lease/`attempts++` in its own txn, then the worker processes each batch group. The defect: `_vectorizer_process_upsert_batch` reads content + calls `embed::run_batch` (HTTP) + writes the vector all inside ONE `BackgroundWorker::transaction`, so the active snapshot pins `backend_xmin` for the whole ~90s-bounded HTTP round-trip, delaying local autovacuum. ADR-0016 already anticipated this as risk **H2** (lock crossing HTTP → 3-phase design); M122 completes it.

## Baseline Context

Repo state: git sha `b9c6a77`, branch `develop`.

### Files that will be touched

| File | LoC | Role today | Change |
|---|---|---|---|
| `theodb_rs/src/embed.rs` | 189 | `run_batch` (`:55`) = validate + `resolve_cfg` (`:129`, GUC via SPI) + HTTP+parse (`:72-124`) | Extract `run_batch_resolved` (no-GUC HTTP+parse); `run_batch` delegates. |
| `theodb_rs/src/vectorizer.rs` | 1153 | `_vectorizer_process_upsert_batch` (`:459`) = read+embed+write in one txn; worker single-txn call (`:729-745`) | Split into `_vectorizer_read_batch` + `_vectorizer_write_batch`; worker orchestrates A/B/C. |
| `docs/adr/` | — | ADRs 0001-0048 | New ADR: 3-phase lease split (divergence from pgai). |

### Current callers / dependents (verified `file:line`)

- `theodb_rs/src/embed.rs:55` `run_batch` — callers: `theodb_rs/src/api.rs:32` (`ai.embed_batch` SQL wrapper), `theodb_rs/src/graph_rag.rs:243,337`, `theodb_rs/src/vectorizer.rs:459` (the batch path being split).
- `theodb_rs/src/embed.rs:129` `resolve_cfg` → `theodb_rs/src/pg.rs:50` `guc()` = `Spi::get_one("SELECT current_setting(...)")` → **requires a txn snapshot** (the reason cfg MUST resolve in phase A).
- `theodb_rs/src/vectorizer.rs:729-745` — the single `BackgroundWorker::transaction(|| in_subtxn(|| process_upsert_batch(...)))` that pins xmin across the embed.
- Lease primitives (unchanged, reused): `_vectorizer_claim_batch:201`, `_vectorizer_mark_done:254` (owner-guarded), `_vectorizer_mark_failed:276`, `_vectorizer_renew_lease:300`, `_vectorizer_reap_orphans:524`. Constants `WORKER_LEASE_SECS=120`/`WORKER_MAX_ATTEMPTS=5` `:570-571`.
- `lookup_config:332` (→ `VecCfg`), `build_sql:389`, `upsert_chunks:362` (M66 chunk mode).

### Domain glossary

- **backend_xmin** — a backend's published xmin horizon (`pg_stat_activity.backend_xmin`); VACUUM cannot reclaim row versions newer than the oldest such horizon.
- **committed-lease queue** — dequeue = mark in-flight with owner+deadline and COMMIT; external work runs with no txn; crash → lease expiry → re-claim (at-least-once).
- **3-phase split** — A: read+lease (txn, commits); B: embed (no txn, no SPI); C: write+mark (txn).

### Architecture boundaries affected

The worker is in-process (`shared_preload_libraries`), so its `backend_xmin` gates local autovacuum directly. `guc()`/`Spi::*` require a txn → phase B must carry only owned Rust values (`String`, owned cfg tuple). Per `rules/architecture.md`, the HTTP adapter (`post_json`, pure network) stays free of PG state.

## Prior Art & Related Work

- Blueprint `.claude/knowledge-base/discoveries/blueprints/async-embed-vectorizer-blueprint.md` (web-evidenced): pgai Vectorizer **deliberately holds the txn across the embed** (transactional dequeue) — the opposite choice — because it is an external worker that tolerates the held snapshot. TheoDB's split is a deliberate divergence justified by being in-process. `worker_spi.c` confirms `backend_xmin` is held only between `PushActiveSnapshot`/`CommitTransactionCommand` → the gap between two `BackgroundWorker::transaction` blocks pins nothing.
- `docs/adr/0016-m54-vectorizer-worker-mechanism.md` (risk H2), `docs/adr/0008-no-embedding-chat-cache.md` (mandates re-embed, not cache, on crash).

## ADRs

### ADR M122-1 — 3-phase lease split (embed HTTP between two top-level transactions)

**Decision:** the worker orchestrates each upsert batch as A (read content + resolve cfg, txn commits) → B (`run_batch_resolved` HTTP, NO txn/SPI) → C (write vector + `mark_done`, txn). The embed runs with no active snapshot, so `backend_xmin` is released.

**Rationale (cites `rules/architecture.md` DIP + blueprint):** `guc()`/`Spi` require a txn, so cfg+content resolve in A and move into B as owned values; `post_json` is pure network. The split is at the **top-level** txn boundary — `in_subtxn` does NOT release the snapshot (a subtxn runs under the parent's active snapshot).

**Alternatives rejected:**
- **pgai-style atomic dequeue-embed-write in one txn** (the SOTA reference) — REJECTED: it pins `backend_xmin` across the embed, which is exactly the local-autovacuum stall M122 exists to fix (acceptable for pgai's external worker, not for our in-process bgworker).
- **Cache the in-flight embedding by content-hash for exactly-once** — REJECTED: out of scope per ADR-0008 (no embedding cache in v1); at-least-once + idempotent overwrite is correct and matches SOTA (pgai also re-embeds on crash).

### ADR M122-2 — crash recovery = re-embed via lease expiry (at-least-once)

**Decision:** a crash after B (HTTP 200) but before C commit leaves the job `state='processing'`; the lease expires → `_vectorizer_claim_batch` re-claims → re-embed. The write is overwrite-by-pk (idempotent); `mark_done` is owner-guarded so a stale worker cannot mark a re-claimed job.

**Alternatives rejected:** transactional rollback (pgai) — not available once we split the txn; and it re-embeds anyway. Bounded cost: a crash wastes one re-embed (double API cost for that batch) — accepted, documented.

## Dependencies

No new crate. Reuses stdlib + pgrx + the existing `post_json`/`serde_json`. Parsimony rung 4 (reuse installed): `embed::run_batch` internals + the lease primitives already exist. `## Dependencies` section: **none added** — verified against `theodb_rs/Cargo.toml` (no new entry).

## Coverage Matrix

| Goal claim | Task |
|---|---|
| Embed HTTP runs with NO open txn (backend_xmin released) | T1 (embed split), T3 (worker A/B/C), T4 (xmin benchmark) |
| cfg resolves in phase A (no GUC/SPI in phase B) | T1 (`run_batch_resolved` no-GUC), T2 (`_vectorizer_read_batch`) |
| write is idempotent + owner-guarded; crash → re-embed | T2 (`_vectorizer_write_batch`), T4 (crash-safety test) |
| L2 synchronous `ai.embed`/`ai.embed_batch` path unchanged | T1 (`run_batch` delegates, order preserved), T5 (regression) |
| MEASURED evidence (xmin released) | T4 (droplet benchmark) |

## Phase 1 — embed.rs no-GUC split

### T1.1 — Extract `run_batch_resolved` (pure HTTP+parse, no GUC/SPI)

#### Why this step
The embed must run in phase B with no txn; `resolve_cfg`→`guc()` uses SPI (needs a txn). Extracting the HTTP+parse tail into a `run_batch_resolved(items, endpoint, model, api_key)` that never reads a GUC lets phase B call it after phase A resolved the cfg. Reasoning: DRY — one HTTP+parse implementation shared by the standalone `run_batch` (which resolves then delegates) and the vectorizer's phase B.

#### Files to edit
- `theodb_rs/src/embed.rs` — add `pub(crate) fn run_batch_resolved(items: &[Option<&str>], endpoint: &str, model: &str, api_key: Option<&str>) -> Vec<String>` (empty-check + NULL-check + `post_json` + index-mapped parse, the current `:72-124` body). `run_batch` keeps its NULL-before-GUC validation, calls `resolve_cfg`, then delegates the HTTP+parse to the shared inner. Preserve error messages verbatim.

#### TDD
- RED: `run_batch_resolved_maps_indices_and_preserves_errors` — given a stub `endpoint` (a `#[cfg(test)]` in-process mock is out of reach for a network call; instead assert the pure parse invariants by unit-testing the extracted index-mapping/format on a synthetic response). Concretely: `test_run_batch_resolved_null_element_rejected` asserts a `None` element raises the same 22023 typed error as `run_batch` does today (behavior parity), and `test_run_batch_resolved_empty_input_no_http` returns `vec![]`.
- GREEN: extract the tail; `run_batch` delegates.
- REFACTOR: ensure a single HTTP+parse path (no duplication).

#### Concurrency tests
(none — single-threaded) — pure function, no shared state.

#### Acceptance criteria
- `run_batch_resolved` contains ZERO `guc(`/`resolve_cfg`/`Spi` references (grep-asserted).
- `run_batch` output byte-identical to pre-split for the same inputs+GUCs (the `ai.embed_batch` oracle test still passes).

#### DoD
- `grep -n 'guc(\|Spi::\|resolve_cfg' theodb_rs/src/embed.rs` shows those only inside `run_batch`/`resolve_cfg`, never inside `run_batch_resolved`.
- Existing embed tests green.

## Phase 2 — vectorizer read/write split

### T2.1 — `_vectorizer_read_batch` + `_vectorizer_write_batch` (replace `_vectorizer_process_upsert_batch`)

#### Why this step
Phase A must return owned content + resolved cfg (so phase B needs no SPI); phase C must write + mark in a fresh txn. Splitting the single SQL-callable function into a read half and a write half lets the worker put the commit boundary between them. Reasoning: the read half calls `lookup_config` + the content fetch (SPI, txn A); the write half calls the `UPDATE` + `mark_done` (SPI, txn C); neither embeds.

#### Files to edit
- `theodb_rs/src/vectorizer.rs` — add `_vectorizer_read_batch(vectorizer_id, source_pks) -> (resolved cfg fields + Vec<Option<String>> contents)` (the `:459-490` read half + `resolve_cfg`-equivalent via `lookup_config` + `guc` for endpoint/model/api_key). Add `_vectorizer_write_batch(vectorizer_id, job_ids, source_pks, vecs, owner) -> i64` (the `:491-500` UPDATE + `mark_done` half). Keep `_vectorizer_process_upsert` (per-job fallback) and the chunk path intact for now (chunk mode reads in A, writes in C in T3; if not fully splittable this milestone, it keeps the single-txn path with an explicit note — honest scope).

#### TDD
- RED: `read_batch_returns_content_and_cfg_without_embedding` (SPI test: seed a row, assert read returns its content + the resolved model, and does NOT touch the target column); `write_batch_upserts_and_marks_done_idempotent` (call twice with the same vec → same target, job `done`, no duplicate).
- GREEN: implement both halves.
- REFACTOR: share `build_sql` fetch/update query construction.

#### Concurrency tests
`write_batch_is_owner_guarded` — a **race** between a stale worker and the re-claimer: a `mark_done` with a non-owner token is a no-op (the job stays claimable), proving a stale worker whose lease expired cannot mark a **concurrently** re-claimed job (the at-least-once lease-boundary invariant). This is the **concurrent test** for the lease/owner fencing (race detector for the stale-worker vs re-claimer schedule).

#### Acceptance criteria
- `_vectorizer_read_batch` performs no `UPDATE`/write to the target; `_vectorizer_write_batch` performs no embed/HTTP.
- Idempotent write: running `write_batch` twice yields one row, `state='done'`.

#### DoD
- SPI tests green; `grep` confirms no `run_batch`/`post_json` inside `_vectorizer_read_batch`/`_vectorizer_write_batch`.

## Phase 3 — worker orchestration (A → B → C)

### T3.1 — Replace the single-txn batch call with 3 top-level phases

#### Why this step
This is where the xmin release actually happens: the worker runs phase A (`BackgroundWorker::transaction`), then phase B (`run_batch_resolved`, NO `BackgroundWorker::transaction`), then phase C (`BackgroundWorker::transaction`). Reasoning: only a top-level commit between A and B releases the snapshot; B holds no txn → `backend_xmin` invalid → autovacuum advances.

#### Files to edit
- `theodb_rs/src/vectorizer.rs:721-758` — rewrite the `for (vid, group)` loop: (A) `let (endpoint, model, key, contents) = BackgroundWorker::transaction(|| _vectorizer_read_batch(...))`; (B) `let vecs = <PgTryBuilder-guarded> run_batch_resolved(&contents, &endpoint, &model, key.as_deref())` with NO surrounding `BackgroundWorker::transaction`; check `sigterm_received()`; (C) `BackgroundWorker::transaction(|| _vectorizer_write_batch(vid, job_ids, pks, vecs, owner))`. On B failure → per-job fallback + `mark_failed` in a fresh txn (existing path). Keep `renew_lease` for long batches.

#### TDD
- RED: this is the ONLY non-CI-testable piece (needs `shared_preload_libraries`) — the RED is the droplet integration test T4 (xmin sample) + an SPI-level test that `read_batch`→(rust embed stub)→`write_batch` composes to the same end-state as the old single call. `worker_three_phase_endstate_matches_singletxn` seeds jobs, runs read+ (mock vecs) +write, asserts the target column + job states match the pre-split behavior.
- GREEN: rewrite the loop.
- REFACTOR: keep the delete + chunk paths coherent.

#### Concurrency tests
`sigterm_between_embed_and_write_leaves_job_reclaimable` — a **cancellation** race: simulate SIGTERM after phase B before phase C (cancellation propagation mid-batch), assert the job stays `processing`, its lease expires, and it is re-claimed by a concurrent worker (no orphan, no double-write beyond the accepted at-least-once re-embed). Cancellation-safety of the top-level-txn boundary.

#### Acceptance criteria
- The embed (`run_batch_resolved`) is NOT inside any `BackgroundWorker::transaction` (source-grep + review).
- End-state parity with the pre-split worker for the happy path.

#### DoD
- SPI composition test green; T4 droplet benchmark shows xmin released.

## Phase 4 — benchmark + crash-safety (the measured evidence)

### T4.1 — Prove backend_xmin is released during a slow embed (droplet)

#### Why this step
The DoD's single metric: `backend_xmin` invalid/advancing during a ≥8s stub embed. Reasoning: a mock endpoint that sleeps 8s + sampling `pg_stat_activity.backend_xmin` of the worker backend distinguishes the pinned (before) from the released (after) state — the honest measurement (public-copy.md §4).

#### Files to edit
- `docs/benchmarks/m122-async-embed-xmin.md` (NEW) — methodology + the measured before/after.
- `benchmarks/mock_slow_embed.py` (NEW) — the sleeping OpenAI-shaped stub (already drafted in scratchpad).

#### TDD
- RED: the measurement is the test — with the split, sample `SELECT backend_xmin FROM pg_stat_activity WHERE application_name LIKE '%vectorizer%'` (or the worker's `backend_type`) while a job embeds against the 8s stub; assert it is NULL (no held snapshot) during the sleep. On the pre-split binary the same sample is a non-null pinned xid.
- GREEN: N/A (measurement); the split from T3 is what makes it pass.

#### Concurrency tests
`crash_before_write_reclaims_and_reembeds` — a **cancellation propagation** test (concurrent test): kill the worker after phase B (HTTP 200) before phase C commits; a **concurrent** re-claim after lease expiry must re-embed and converge (idempotent overwrite), never leave an orphan. Race-aware crash-safety at the embed→write boundary.

#### Failure scenarios (external I/O — the embed HTTP)
- **Hung/slow endpoint (8s+):** phase B blocks on the HTTP but holds NO txn → xmin released (the whole point). Bounded by the HTTP timeout; on timeout → `mark_failed` in phase C, lease frees the job.
- **5xx / malformed response:** `run_batch_resolved` raises the existing typed `err_external` (38000) → caught by the worker's `PgTryBuilder` → per-job fallback → `mark_failed`.
- **Crash after HTTP-200 before phase-C commit:** lease expiry → re-claim → re-embed (at-least-once; idempotent overwrite). Test: `crash_before_write_reclaims_and_reembeds`.

#### Acceptance criteria
- Measured `backend_xmin` NULL during the stub embed on the split binary; documented with the exact sampling command.
- Crash-safety test green.

#### DoD
- `docs/benchmarks/m122-async-embed-xmin.md` shows the before (pinned) vs after (released) sample with reproduction commands.

## Phase 5 — Integration Validation (final gate)

### T5.1 — Full regression + ADR + CHANGELOG

#### Why this step
"Eat your own cooking": the standalone `ai.embed`/`ai.embed_batch` path must be unchanged, the ADR recorded, CHANGELOG updated.

#### Files to edit
- `docs/adr/0049-m122-three-phase-async-embed.md` (NEW) — ADR M122-1/2 (references ADR-0016 H2, ADR-0008).
- `CHANGELOG.md` `[Unreleased] § Fixed` — the xmin-pin fix.

#### TDD
- RED: `ai_embed_batch_output_unchanged_after_split` — a regression test asserting `ai.embed_batch` returns byte-identical vectors for a fixed input+GUC set as the pre-split baseline (the oracle); it MUST fail if `run_batch` no longer preserves the NULL-before-GUC order or the index-mapping.
- GREEN: the T1 delegation (validate→resolve→`run_batch_resolved`) preserves the contract → the oracle passes.
- REFACTOR: ensure the standalone path and the vectorizer path share exactly one HTTP+parse implementation (no drift).

#### Concurrency tests
(none — single-threaded) — regression only: re-runs existing suites + writes ADR/CHANGELOG; no new concurrent path.

#### Acceptance criteria / DoD
- `ai.embed_batch` byte-identical output (oracle test green).
- ADR + CHANGELOG present. `/code-quality` ∉ {FAIL_HARD, INVALID}.

## Failure scenarios

External I/O = the embedding HTTP endpoint. Per-dependency scenarios covered in T4.1 (hung/slow → xmin released + timeout→mark_failed; 5xx/malformed → typed error → mark_failed; crash mid-flight → re-embed via lease). No other external I/O is touched.

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| At-least-once re-embed on crash: a crash after HTTP-200 before phase-C commit wastes one re-embed (double API cost for that batch) | MEDIUM | Idempotent overwrite + owner-guarded mark; accepted SOTA trade-off (ADR-0008 forbids the cache that would give exactly-once) | implementer |
| Chunk-table mode (M66) split completeness: the `upsert_chunks` fan-out per doc may not split cleanly this milestone → chunk-mode vectorizers keep the single-txn path (still pin xmin) | MEDIUM | Explicit honest scope note + follow-up; the common 1→1 in-place mode (backlog target) is fixed | implementer |
| Worker is not CI-testable: the bgworker needs `shared_preload_libraries`, so the xmin evidence is a droplet measurement, not a CI unit test | LOW | SPI-level composition tests cover the read/write halves; the droplet benchmark is the integration proof | implementer |

## Unresolved Questions

- Should chunk-mode be split in this milestone or deferred? Resolved at plan time: **deferred with an explicit note** if the fan-out makes the 3-phase non-trivial — the 1→1 in-place batch is the committed target (backlog scope). The plan does not block on it.
- (none other — every decision is resolved at plan time.)

## Global DoD

- `backend_xmin` released during a ≥8s stub embed — MEASURED on the droplet (`docs/benchmarks/m122-async-embed-xmin.md`).
- `run_batch_resolved` has no GUC/SPI; the embed runs outside any `BackgroundWorker::transaction`.
- Idempotent + owner-guarded write; crash → re-embed (test green).
- `ai.embed_batch` output unchanged (oracle green).
- ADR-0049 + CHANGELOG `[Unreleased]`. `/code-quality` ∉ {FAIL_HARD, INVALID}. Lint clean. No new dependency.
