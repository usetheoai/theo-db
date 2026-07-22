---
slug: m104-system-design-hardening
milestone_id: M104
created_at: 2026-07-16
goal: Close the /loop-system-design audit's critical + high findings (bounded-memory columnar write/scan, HTTP circuit breaker, cache/queue bounds, boundary/deletion hygiene, North-Star governance) so a re-run of /loop-system-design scores ≥4.9/5 with the critical + all high findings resolved and no MVCC/crash-safety regression.
---

# M104 — system-design hardening (health 4.2 → ≥4.9/5)

## Goal

Resolve the load-bearing findings of the `/loop-system-design` Staff audit (2026-07-16, overall 4.2/5) — the CRITICAL
unbounded-RAM columnar write (#99), the 4 HIGH scaling/resilience gaps, the boundary/deletion/data-flow mediums, and
the sole rationale-invalid governance trade-off — reusing the bounded-memory patterns that already exist in-tree
(M89/M96 spill, M100 GreedyMemoryPool, M99 catalog-MVCC), with no new invention. Metric: **a re-run of
`/loop-system-design --mode=full` scores ≥4.9/5 overall, the CRITICAL + all HIGH findings are resolved, and the crash
proofs (`make -C theodb_rs/isolation check-crash`) + isolation permutations stay GREEN (no MVCC/crash-safety regression).**

## Context

The audit surfaced (report: `system-design-output/final_report.md`, 26 findings): CRITICAL columnar WRITE_STATES
buffers a whole transaction in RAM (#99 → OOM); HIGH columnar seq-scan full-materializes, VACUUM fold O(N), Arrow
cache unbounded, AI HTTP no circuit breaker; the inert `rabitq/vendor/` zombie tree; the `vec/ah.rs→am::aq` layering
inversion + `vindex→am::columnar` internals leak; vectorizer backpressure + dead-letter growth; and the North-Star
governance debt (ADR-0002 LOCKED mandate invalidated by measured ADR-0035/0036; repositioning ADR-0033 unsigned). The
deep-research blueprint (`knowledge-base/discoveries/blueprints/m104-hardening-blueprint.md`, this cycle) grounds the 3
research-worthy items (Q1 write, Q2 scan, Q3 breaker) in SOTA (Parquet/DuckDB/ClickHouse/Lance row-group flush; Arrow
RecordBatch streaming; Nygard/MS/resilience4j circuit breaker + ureq pool) + in-tree precedents.

## Baseline Context

### Files that will be touched

| File | LoC | Role | Change |
|---|---|---|---|
| `theodb_rs/src/am/columnar.rs` | 1451 | columnar TAM (WRITE_STATES, flush_pending, materialize_rows, scan state) | Q1 incremental threshold-flush + Q2 lazy chunk-group scan cursor |
| `theodb_rs/src/am/build_stream.rs` | ~300 | M96 streaming build (spill/free discipline) | reuse the `maintenance_work_mem` threshold + accumulate-flush-free pattern |
| `theodb_rs/src/am/arrow_cache.rs` | ~450 | M101 HTAP Arrow cache (per-backend HashMap) | Q-cache: size/entry-bounded eviction (LRU) |
| `theodb_rs/src/http.rs` | ~120 | outbound HTTP client (retry/backoff/SSRF) | Q3 per-backend circuit breaker + `ureq::Agent` connection pool (replace minreq) |
| `theodb_rs/src/chat.rs` / `embed.rs` | ~250 / ~90 | batched AI (generate_batch/if_batch/embed_batch) | Q3 batch-size cap (chunking) |
| `theodb_rs/src/am/aq.rs` → `theodb_rs/src/vec/aq.rs` (MOVE) | 601 | AqQuantizer (pure domain, misplaced in am/) | relocate to the vec/domain layer (fixes vec/ah.rs→am::aq inversion) |
| `theodb_rs/src/vindex.rs` | ~330 | M103 vindex (reaches am::columnar internals) | consume a typed `columnar` projection accessor (kills the leak) |
| `theodb_rs/src/vectorizer.rs` | 1014 | M54 embed worker + queue | producer backpressure + dead-letter retention/purge bound |
| `theodb_rs/src/am/build.rs` / `page.rs` | 1476 / 1982 | build format dispatch + legacy paths | `#[deprecated]`/WARN on blob/v4 legacy + flip the v4 OOM-default |
| `theodb_rs/src/rabitq/` (DELETE) | 5651 | inert vendored tree (not compiled) | delete OR `#[cfg(feature)]`-gate + fix VENDORED.md (ADR) |
| `docs/adr/0002-*.md`, `0033-*.md` (+ new 0047-0052) | — | North-Star governance + M104 design ADRs | supersede note on 0002 / accept 0033; write Q1/Q2/Q3 + rabitq ADRs |
| `theodb_rs/isolation/crash_columnar_incremental.sh` (NEW) | — | crash permutation for Q1 (H3) | crash between incremental flushes → zero visible rows |
| `docs/benchmarks/m104-*.{md,json}` | — | measured evidence | write-RAM envelope, scan time-to-first-row/RSS, breaker fail-fast |

### Current callers / prior art (reuse, not greenfield)

- `am/columnar.rs`: `WRITE_STATES` (:39), `accumulate_row` (:833), `flush_pending` (:906 — pages→dir→header→catalog row LAST), `read_visible_stripes` (:109), `materialize_rows` (:626), `ColumnarScanState` (:425 — already a cursor), `decode_stripe` (:582 — loops chunk-groups), `columnar_xact_flush` ABORT branch (:183).
- `am/build_stream.rs`: `should_stream` (:126, `maintenance_work_mem`), `assign_callback` (:176, accumulate→spill→`pfree`→free — the O(mwm) discipline).
- `am/df_executor.rs`: `GreedyMemoryPool(work_mem)` (:166 — bounded-memory precedent, DO NOT touch).
- `http.rs`: `post_json` (:41, the send loop), `is_recoverable_status`, `backoff`, `with_max_redirects(0)` (SSRF guard — a no-regress invariant).
- `am/fold.rs` + `isolation/crash_fold.sh`: the #46/#47 crash-safety invariant Q1 must NOT regress.
- In-tree peer OSS: `.claude/knowledge-base/references/paradedb/pg_search/.../fast_fields/columnar.rs` (Q2 streaming-exec precedent).

### Glossary

- **Incremental stripe flush** — flush a columnar stripe when pending bytes exceed `maintenance_work_mem`, mid-transaction; each stripe's catalog row carries the xact xid so all stripes commit/abort together.
- **Per-backend circuit breaker** — a thread_local closed/open/half-open state machine (Postgres = process-per-backend; no shared breaker without shm — a documented non-goal).
- **Lazy chunk-group scan** — `getnextslot` decodes one chunk-group (≤10 000 rows) at a time via a resumable cursor, not the whole table up front.

### Architecture boundaries

Per `rules/architecture.md`: relocating `AqQuantizer` am/→vec fixes the layering inversion (the SIMD/domain layer must not import the storage AM). No panic across C (Rule 8). Q1 preserves the M99 MVCC-via-heap-catalog invariant + the #46/#47 crash-safety (Rule: no regression, re-run the proofs).

## Prior Art & Related Work

- **Blueprint (this cycle):** `knowledge-base/discoveries/blueprints/m104-hardening-blueprint.md` — Q1/Q2/Q3 with web-cited SOTA (Parquet/DuckDB/ClickHouse/Lance row-group flush; Arrow RecordBatch stream; MS/Nygard/resilience4j circuit breaker; ureq Agent pool) + in-tree mapping.
- **Audit:** `system-design-output/final_report.md` + `adrs/{0045-northstar-governance,0046-rabitq-disposition}.md` (drafts).
- **In-tree:** M89/M96 streaming build, M99 columnar TAM, M100 df_executor, M48/#46/#47 crash-safety.

## ADRs

### D1 — Incremental threshold-flush for the columnar write (Q1, CRITICAL #99)

**Decision:** flush a stripe when pending bytes > `maintenance_work_mem` (mid-transaction), reusing `flush_pending`;
each stripe's catalog row carries the xact xid → all stripes of one INSERT commit/abort atomically. Peak RAM becomes
O(maintenance_work_mem), N-independent. **Alternatives:** (a) status-quo whole-txn buffer — REJECTED (OOM #99); (b)
spill pending rows to a temp file then one stripe — REJECTED (reinvents tuplesort, extra copy). **Rationale:** SOTA
(DuckDB row-group-at-a-time, ClickHouse one-part-per-INSERT generalized) + the in-tree `flush_pending` is already the
"pages→catalog-row-LAST" atomic unit; ~15-line change. **Crash-safety preserved BY CONSTRUCTION** (visibility still
gated on the single xact commit) — but gated on H1 (SPI/snapshot re-entrancy) + H3 (crash permutation).

### D2 — Lazy chunk-group streaming scan (Q2, HIGH)

**Decision:** `columnar_scan_begin` resolves the visible-stripe SET once (MVCC-correct), then `getnextslot` decodes
ONE chunk-group at a time via a resumable cursor (`stripe_idx/cg_idx/row_in_cg`), draining WRITE_STATES pending rows
as the final batch. Peak RAM O(one chunk-group). **Alternatives:** (a) eager materialize — REJECTED (O(N) RAM, status
quo); (b) push the seq-scan into DataFusion — REJECTED (over-scoped; seq-scan ≠ the M100 aggregate path). **Rationale:**
Arrow RecordBatch streaming + DuckDB vector-at-a-time + the ParadeDB in-tree peer; reuses the entire decode path.

### D3 — Per-backend circuit breaker + ureq connection pool (Q3, HIGH + MEDIUM)

**Decision:** a thread_local closed/open/half-open breaker (K=5 consecutive failures → open 30s, GUC-tunable →
half-open one probe) wrapping `post_json`'s send, keyed by endpoint; replace `minreq` with `ureq::Agent`
(MIT/Apache-2.0, per-Agent keep-alive pool — Rule 9) held per-backend; a `MAX_BATCH` GUC chunks the batched AI.
**Alternatives:** (a) no breaker — REJECTED (per-row surface pays K×timeout PER ROW on a dead endpoint); (b) shared-shm
cross-backend breaker — REJECTED for M104 (accidental complexity: shm+LWLock+ABI-fragile for a marginal cross-backend
gain; the per-row catastrophe runs in ONE backend so per-backend fully solves the finding; shared-shm is a documented
non-goal until a measured multi-backend workload demands it — anti-sunk-cost/YAGNI). **Rationale:** Nygard/MS/resilience4j
canonical state machine; ureq is the maintained permissive pool primitive minreq lacks. **Security no-regress:** the
ureq swap MUST preserve redirect=0 (SSRF), api-key-only-in-header, SQLSTATE 38000 — a behavior-preserving refactor
asserted by the existing oracles.

### D4 — Delete the inert rabitq/vendor tree; relocate AqQuantizer; typed columnar accessor

**Decision:** `git rm` the 5651-LoC `rabitq/vendor/` (not compiled, VENDORED.md overclaims; git preserves it) + an
ADR recording the disposition; relocate `AqQuantizer` am/→vec (fixes the layering inversion); add a typed
`columnar::decode_projection()` accessor so `vindex.rs` stops hand-decoding raw bytes. **Alternatives:** keep rabitq
`#[cfg(feature)]`-gated — acceptable fallback if the owner wants the study retained compilable. **Rationale:** the
audit's HIGH zombie + boundary findings; Rule 9 (delete dead vendored code, git is the archive).

### D5 — North-Star governance reconciliation (owner)

**Decision:** add a supersede note to the LOCKED ADR-0002 pointing at the measured verdicts ADR-0035/0036, and accept
ADR-0033 (the repositioning) — closing the sole rationale-invalid trade-off. **Alternatives:** leave 0002 LOCKED as-is
— REJECTED (the mandate-of-record contradicts the team's own measurements; the audit's only rationale_valid=0).
**Rationale:** the user's M104 goal explicitly authorizes the governance sign-off; non-code.

## Dependency Graph

```
Phase A (Q1 write, CRITICAL) ─ gates ─▶ Phase B (Q2 scan + fold cap + cache LRU)   [both touch columnar]
Phase C (Q3 HTTP breaker+pool+cap)  ── independent (http/chat) ──▶
Phase D (rabitq delete + AqQuantizer relocate + decode accessor + deprecation)  ── independent ──▶
Phase E (vectorizer backpressure + dead-letter bound)  ── independent ──▶
Phase F (governance ADR)  ── independent, non-code ──▶
Final (re-audit /loop-system-design ≥4.9 + crash proofs GREEN)  ── gates on ALL ──▶
```

## Phase A — Q1: bounded-memory columnar write (CRITICAL #99)

### Task A1 — incremental threshold-flush + H1 (self-referential INSERT) + H3 (crash permutation)

#### Why this step
The CRITICAL: make the columnar write O(maintenance_work_mem), not O(rows-in-xact). Reuse `flush_pending` at a byte
threshold. The two gates (blueprint): H1 SPI/snapshot re-entrancy of a mid-executor catalog insert; H3 crash between
incremental flushes → zero visible rows (must not regress #46/#47).

#### Files to edit
- `theodb_rs/src/am/columnar.rs` — `WRITE_STATES` carries `pending_bytes`; `accumulate_row`/`columnar_multi_insert`
  call `flush_pending(rel)` when `pending_bytes > maintenance_work_mem*1024`; a row-count floor for narrow rows.
- `theodb_rs/isolation/crash_columnar_incremental.sh` (NEW) — crash (SIGABRT via a test GUC or immediate-stop)
  between the 2nd and 3rd incremental flush of a big INSERT → after recovery, `count(*) == 0` (xact aborted).

#### TDD
- RED: `test_incremental_flush_bounds_memory` — a large `INSERT...SELECT` produces MULTIPLE stripes
  (`count(*) FROM columnar.stripe WHERE relid=...` > 1) and the committed `count(*)` is correct; a memory-envelope
  assertion (peak pending rows ≤ threshold). Fails before the threshold-flush.
- RED: `test_self_referential_insert_semantics` (H1) — `INSERT INTO c SELECT ... FROM c` does not see its own
  incrementally-flushed stripes within the statement (INSERT snapshot semantics preserved).
- GREEN: the byte counter + threshold check; verify `with_active_snapshot` is a no-op under the executor snapshot.
- REFACTOR: share the threshold read with `build_stream::should_stream`.

#### Concurrency tests
`#### Concurrency tests` — (none — single-backend write path; thread_local WRITE_STATES). The crash permutation (H3)
is the durability proof, not a race.

#### Failure scenarios
`## Failure scenarios` — a crash/abort mid-multi-stripe INSERT → all stripes invisible (catalog rows uncommitted),
`count(*)==0` after recovery (H3, crash_columnar_incremental.sh); orphan data pages are an accepted VACUUM follow-up
(H4, filed, not silently regressed).

#### Acceptance criteria
- Peak write RAM bounded by `maintenance_work_mem` (measured); a big INSERT produces many stripes, correct count;
  self-referential INSERT semantics preserved; crash-between-flushes → 0 visible rows; existing `check-crash` +
  isolation permutations still GREEN.

#### DoD
- `cargo pgrx test pg17 m104_incremental_flush` GREEN + `bash crash_columnar_incremental.sh` = OK + a measured
  `docs/benchmarks/m104-write-envelope.{md,json}` (peak RSS bounded vs the old O(N)).

## Phase B — Q2 streaming scan + fold cap + Arrow cache LRU (HIGH ×3)

### Task B1 — lazy chunk-group streaming `getnextslot`

#### Why this step
HIGH: the columnar seq-scan full-materializes the table before row 0. Make `getnextslot` decode one chunk-group at a
time (blueprint Q2), O(one chunk-group) RAM, O(1) time-to-first-row.

#### Files to edit
- `theodb_rs/src/am/columnar.rs` — new `ColumnarScanState { stripes, stripe_idx, cg_idx, row_in_cg, current_batch,
  pending_tail }`; `columnar_scan_begin` resolves the visible-stripe set only; refactor `decode_stripe`'s inner body
  into `decode_one_chunk_group(...)`; `getnextslot` pulls lazily; `rescan` resets ALL cursors.

#### TDD
- RED: `test_streaming_scan_matches_eager` — a `SELECT *` over a multi-stripe columnar table returns the identical
  row set/order as before (byte-identical), and `LIMIT 1` decodes only one chunk-group (a decode-counter or RSS
  proxy). Fails before the lazy cursor.
- GREEN: the cursor state machine + `decode_one_chunk_group`.
- REFACTOR: `rescan` resets all four cursors (R3 risk).

#### Concurrency tests
`#### Concurrency tests` — (none — single-backend scan; the stripe set is snapshot-fixed at begin).

#### Failure scenarios
`## Failure scenarios` — no buffer pin held across a row-emission yield (decode copies to owned Vec — verify); an
empty/all-pending table streams the pending tail correctly.

#### Acceptance criteria
- Streaming scan result byte-identical to eager; `LIMIT 1` decodes ONE chunk-group (measured time-to-first-row + RSS);
  `rescan` correct.

#### DoD
- `cargo pgrx test pg17 m104_streaming_scan` GREEN + `docs/benchmarks/m104-scan-ttfr.{md,json}` (time-to-first-row + peak RSS A/B).

### Task B2 — VACUUM fold memory cap + Arrow cache LRU eviction

#### Why this step
HIGH: the VACUUM fold is O(N)-in-RAM (documented M55 window); the M101 Arrow cache is unbounded per-backend. Bound
both (or a documented+benchmarked cap for the fold; an LRU size/entry cap for the cache).

#### Files to edit
- `theodb_rs/src/am/arrow_cache.rs` — cap `CACHE` by total bytes/entries with LRU eviction (a GUC `theodb.arrow_cache_max_mb`).
- `theodb_rs/src/am/build.rs`/`fold.rs` — the fold: either bound the materialization (batch the enumerate) OR a
  documented+GUC'd row cap with a WARN + honest ADR note (M55 is the full fix; M104 bounds the blast radius).

#### TDD
- RED: `test_arrow_cache_evicts_over_cap` — inserting > cap columnarized tables evicts the LRU entry (cache size stays ≤ cap).
- GREEN: the LRU bound.
- REFACTOR: reuse the generation-invalidation path.

#### Failure scenarios
`## Failure scenarios` — eviction of an in-use batch mid-query must not corrupt an active scan (evict only on build, not mid-read).

#### Acceptance criteria
- Arrow cache bounded by the GUC (measured); fold memory bounded or a documented cap with WARN.

#### DoD
- `cargo pgrx test pg17 m104_cache_lru` GREEN; fold cap documented in an ADR + `docs/benchmarks/m104-cache.{md,json}`.

## Phase C — Q3: AI HTTP circuit breaker + connection pool + batch cap (HIGH + MEDIUM)

### Task C1 — per-backend circuit breaker + ureq Agent pool + MAX_BATCH

#### Why this step
HIGH: outbound HTTP has no circuit breaker (a dead endpoint costs K×timeout per row on a per-row surface) and no
connection reuse. Add a per-backend breaker + `ureq::Agent` keep-alive pool + a batch cap (blueprint Q3), preserving
the SSRF/redirect=0/38000 invariants.

#### Files to edit
- `theodb_rs/src/http.rs` — a thread_local breaker (Closed/Open{until}/HalfOpen, K=5, open_ms GUC) wrapping the send
  loop; replace `minreq` with a per-backend `ureq::Agent` (OnceLock/thread_local); preserve redirect=0 + api-key-in-header + 38000.
- `theodb_rs/Cargo.toml` — swap `minreq` → `ureq` (deps-audit: MIT/Apache-2.0, CVE check).
- `theodb_rs/src/chat.rs`/`embed.rs` — a `MAX_BATCH` GUC chunks oversized batches.

#### TDD
- RED: `test_breaker_opens_and_fails_fast` — with a test hook simulating K consecutive endpoint failures, the (K+1)th
  call fails FAST (typed 38000 "circuit open", no TCP attempt); after open_ms → half-open probe → closed on success.
- RED: `test_ssrf_and_sqlstate_preserved` — the ureq path still rejects redirects and emits 38000 (the security oracle).
- GREEN: the breaker state machine + ureq Agent + batch chunking.
- REFACTOR: share the breaker key (endpoint) resolution.

#### Concurrency tests
`#### Concurrency tests` — (none — per-backend thread_local breaker; process-per-backend model, no shared state).

#### Failure scenarios
`## Failure scenarios` — endpoint down (breaker opens, fail-fast); a redirect (rejected, SSRF); a huge batch (chunked, not one giant request); half-open probe fails (re-open).

#### Acceptance criteria
- Breaker opens after K failures + fails fast (measured: a dead-endpoint per-row surface costs ~K probes not N×timeout);
  connection reused (keep-alive); batch capped; SSRF/38000 preserved.

#### DoD
- `cargo pgrx test pg17 m104_breaker` GREEN + deps-audit clean on ureq + `docs/benchmarks/m104-breaker.{md,json}` (fail-fast latency vs the old N×timeout).

## Phase D — boundary/deletion hygiene

### Task D1 — delete rabitq/vendor; relocate AqQuantizer; typed columnar accessor; deprecation markers

#### Why this step
HIGH zombie + boundary findings: the inert rabitq tree, the vec→am::aq layering inversion, the vindex→columnar
internals leak, unmarked legacy paths + the v4 OOM-default.

#### Files to edit
- `theodb_rs/src/rabitq/` — `git rm` (or `#[cfg(feature="rabitq_study")]`-gate + fix VENDORED.md) + ADR.
- `theodb_rs/src/am/aq.rs` → `theodb_rs/src/vec/aq.rs` — relocate `AqQuantizer` (update imports).
- `theodb_rs/src/am/columnar.rs` + `vindex.rs` — a typed `decode_projection()` accessor; vindex stops hand-decoding.
- `theodb_rs/src/am/build.rs` — `#[deprecated]`/WARN on the v4/blob legacy build path + flip the default off the v4-OOM path.

#### TDD
- RED: build compiles after the move (the existing 312-test suite is the regression oracle — no behavior change);
  a `test_v4_default_flipped` asserting the default build path is the streaming v5 (not v4-OOM).
- GREEN: the move + accessor + default flip.
- REFACTOR: minimize the public surface of the new `vec::aq`.

#### Failure scenarios
`## Failure scenarios` — `WITH (pq_subspaces=M)` without `separate_storage` now warns/defaults-safe instead of the OOM v4 path.

#### Acceptance criteria
- rabitq gone/gated; no `vec→am::aq` import; vindex uses the typed accessor; v4 default flipped; suite GREEN.

#### DoD
- `cargo pgrx test pg17` GREEN (no regression); `code-quality` clean (no new dead code); grep shows no `rabitq::` refs.

## Phase E — vectorizer backpressure + dead-letter bound

### Task E1 — producer backpressure + dead-letter retention

#### Why this step
MEDIUM data-flow: the vectorizer enqueue is uncapped (a bulk backfill floods one worker) and dead-letter rows grow
unbounded on-disk.

#### Files to edit
- `theodb_rs/src/vectorizer.rs` — a queue-depth admission signal / coalescing on enqueue; a dead-letter
  retention/purge (age or count bound, GUC).

#### TDD
- RED: `test_dead_letter_purge` — dead-lettered rows beyond the retention bound are purged/archived (queue table stays bounded).
- GREEN: the retention bound + backpressure.

#### Concurrency tests
`#### Concurrency tests` — the queue uses SKIP LOCKED + owner-uuid fencing (existing); the purge must not race a claim
— assert the purge only touches `state=failed` rows past retention (a permutation or a fencing check).

#### Failure scenarios
`## Failure scenarios` — a poison row dead-letters and is purged after N; a bulk backfill applies backpressure (no unbounded lag signal).

#### Acceptance criteria
- Dead-letter table bounded; backpressure/coalescing on enqueue (measured queue depth stays bounded).

#### DoD
- `cargo pgrx test pg17 m104_vectorizer_bound` GREEN.

## Phase F — North-Star governance (owner)

### Task F1 — supersede note on ADR-0002 + accept ADR-0033 + M104 design ADRs

#### Why this step
The sole rationale-invalid trade-off (tradeoffs dimension). Non-code governance the user's goal authorizes.

#### Files to edit
- `docs/adr/0002-*.md` — a supersede note pointing at the measured verdicts 0035/0036 (does NOT rewrite the LOCKED
  body; adds the honest "measured-invalidated axis" note per the golden-rule change protocol + owner authorization).
- `docs/adr/0033-*.md` — Status PROPOSED → Accepted (owner-authorized via the M104 goal).
- `docs/adr/00NN` — the Q1/Q2/Q3/rabitq design ADRs with alternatives.

#### TDD
- RED: `scripts/check_xrefs.py` passes (ADR cross-refs resolve).
- GREEN: the ADR edits.

#### Acceptance criteria
- ADR-0002 carries the supersede note; 0033 accepted; the M104 design ADRs present with rejected alternatives;
  `check_xrefs.py` + `test_e2e_smoke.py` PASS.

#### DoD
- `python3 scripts/check_xrefs.py` PASS; the governance debt closed in the re-audit's tradeoffs dimension.

## Coverage Matrix

| Requirement (M104 DoD) | Task(s) |
|---|---|
| (1) CRITICAL #99 bounded-memory columnar write | A1 |
| (2a) columnar streaming scan | B1 |
| (2b) VACUUM fold bounded/capped | B2 |
| (2c) Arrow cache eviction | B2 |
| (2d) AI HTTP circuit breaker + conn reuse + batch cap | C1 |
| (3) rabitq delete/gate + AqQuantizer relocate + typed decode accessor + deprecation + v4 default flip | D1 |
| (4) vectorizer backpressure + dead-letter bound | E1 |
| (5) governance: ADR-0033 sign-off / ADR-0002 supersede | F1 |
| (6) verified re-audit ≥4.9/5 + crash proofs GREEN | Final Phase |

## Drawbacks & Risks

| Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| Incremental flush breaks MVCC/crash-safety (H1/H3) | HIGH | H1 self-referential INSERT test + H3 crash permutation; visibility still gated on single xact commit (by construction); re-run check-crash + isolation | impl |
| ureq swap regresses SSRF/38000 security invariants | HIGH | behavior-preserving refactor; existing oracles assert redirect=0 + SQLSTATE 38000; deps-audit ureq (MIT/Apache) | impl |
| Scope creep (5 dimensions, one milestone) | MEDIUM | DoD is an independent checklist; the re-audit ≥4.9 is the single accept gate; medium/low may be deferred with a note | plan |
| Deleting rabitq loses the vendored study | LOW | git preserves it + an ADR records the disposition (or cfg-gate fallback) | impl |
| Orphan pages on aborted multi-stripe INSERT (H4) | MEDIUM | accepted VACUUM follow-up (same class as today's single-flush abort), filed, not silently regressed | impl |

## Unresolved Questions

- **Shared-shm cross-backend circuit breaker** — a deliberate M104 non-goal (per-backend solves the per-row finding);
  promote only if a measured multi-backend workload shows the aggregate probe cost matters (resolved at C1 by scope).
- **Full-atomic crash-safe VACUUM reclaim (M55)** — the fold's fail-loud window; M104 bounds the memory, M55 closes
  the REINDEX window (out of scope, documented).

## Failure scenarios

- **Crash between incremental stripe flushes** (A1) → all stripes invisible, `count(*)==0` after recovery (H3).
- **Self-referential INSERT** (A1) → INSERT snapshot semantics preserved, no self-visible mid-flush stripes (H1).
- **Dead HTTP endpoint** (C1) → breaker opens, fail-fast 38000, no N×timeout; redirect rejected (SSRF).
- **Arrow cache over cap** (B2) → LRU eviction, no unbounded growth.
- **Bulk vectorizer backfill / poison row** (E1) → backpressure + dead-letter purge, queue bounded.

## Global DoD

- Phase A–F tasks `cargo pgrx test pg17` GREEN on the droplet; `bash isolation/crash_columnar_incremental.sh` OK +
  existing `make check-crash` + isolation permutations still GREEN (no #46/#47 regression).
- Measured artifacts: `docs/benchmarks/m104-{write-envelope,scan-ttfr,breaker,cache}.{md,json}` with real numbers,
  methodology, honest ceilings (mean±stddev where timing applies).
- No callback panics across C; SSRF/38000 preserved; ureq deps-audit clean.
- CHANGELOG `[Unreleased]` updated; no commits to main; no Co-Authored-By trailer; the M104 design ADRs + governance
  ADRs written; `check_xrefs.py` + `test_e2e_smoke.py` PASS.
- **VERIFIED: a re-run of `/loop-system-design --mode=full` scores ≥4.9/5 overall with the CRITICAL + all HIGH resolved.**
- Sign-off: council-index-storage (Q1/Q2 storage/MVCC), council-security (Q3 HTTP/SSRF), council-benchmark (the measured artifacts).

## Final Phase — Integration Validation

- Full `cargo pgrx test pg17` suite GREEN (no regression on M17–M103 + the new M104 tests).
- The crash proofs (#46/#47 + the new incremental-flush permutation) GREEN; isolation permutations GREEN.
- All measured artifacts reproducible; honest ceilings stated.
- **Re-audit `/loop-system-design --mode=full` ≥4.9/5** (the acceptance gate) — the CRITICAL + all HIGH findings resolved.
- council-index-storage + council-security + council-benchmark = READY_TO_MERGE before `/release`.
