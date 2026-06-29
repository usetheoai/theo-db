---
slug: audit-remediation
created_at: 2026-06-29
goal: Remediate every actionable finding of the system-design audit (embed_batch, seam guard, retry, chunked import, retirement migration + 2 ADRs), measured by the full embed/AI test suite green against the rebuilt image plus an embed_batch N→1 latency benchmark.
---

# Plan: System-Design Audit Remediation (all actionable findings)

> **Version 1.1** — (v1.1 absorbs the edge-case review: MUST-FIX EC-1 → the `embed_batch` SQL wrapper COALESCEs empty `array_agg` to `ARRAY[]::vector[]`; SHOULD-TEST EC-2 retry-bounds+no-leak and EC-3 batch order/dups added to T3.1/T1.1 TDD.) Close every actionable item of the `loop-system-design` audit (`.claude/knowledge-base/audits/system-design-audit-2026-06-29.md` — the DISCOVER phase of this cycle: a Staff-level, evidence-based investigation + 2 MADR drafts). Five code fixes (the report's "Top Refactor Priorities" #1–#5) + promotion of the two ADR drafts. The four INFO findings are positive baselines (no action). Headline: the CRITICAL embedding N+1 — add `theodb.embed_batch(text[])` collapsing N synchronous HTTP round-trips to ONE, with a measured benchmark (a genuine N→1 win, not an I/O-bound wash).

## Goal

> "Remediate all 5 actionable system-design findings + promote 2 ADRs so the audit's critical/high/medium gaps are closed, measured by (a) the full embed/AI suite (`test_embed_sql.py` + `test_embed_failure_scenarios.py` + new batch/guard/retry/import tests) green against the rebuilt image AND (b) `docs/benchmarks/audit-remediation-embed-batch.md` showing `embed_batch(N)` latency materially below `N × embed()` (the N→1 collapse, mean±std, ≥3 runs)."

## Context

The `loop-system-design` audit (system-design-audit-2026-06-29.md, overall 3.2/5) found TheoDB structurally sound with strong ADR hygiene, but flagged 1 critical + 1 high + 9 medium actionable findings, all in `## Top Refactor Priorities`. This plan closes them. The audit IS this cycle's DISCOVER deliverable — a deeper, evidence-based investigation than a standard `/discover` would produce (15 findings with file:line, 6 data flows, 11 trade-offs, 2 MADR drafts). The remediations reuse INTERNAL patterns (`ai.generate_batch` is the batch template; `error-handling.md` governs retry; PG extension-upgrade SQL governs the retirement migration), so no external discovery is needed (Rule 9 — reuse what exists).

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/embed.rs` | ~125 | `5fff158` (split) | Domain: `run()` — minreq POST (https-native, no-redirect, 30s timeout), parse, format, typed 22023/38000 | `run()` signature + behavior unchanged; new `run_batch()` added alongside; same SSRF/typed-error posture |
| `theodb_rs/src/lib.rs` | ~115 | `5fff158` | api-surface: `#[pg_extern] _embed_text` + `extension_sql!` wrapper `theodb.embed` + 4 `#[pg_test]` | `theodb.embed`/`theodb_rs._embed_text` unchanged; add `_embed_batch_text` + `theodb.embed_batch` wrapper |
| `theodb_rs/src/pg.rs` | ~33 | `5fff158` | pg-glue: err_input/err_external/guc | unchanged or extended (retry helper must live here or in embed.rs) |
| `sql/40-theodb-hybrid.sql` | ~115 | M16 | `ai.hybrid_search_rrf`; line 62 `qvec := theodb.embed(query_text)` | add fail-fast guard BEFORE line 62; behavior unchanged when theodb_rs present |
| `sql/50-theodb-ai.sql` | ~290 | M11 | `ai._chat` (urllib, timeout 30, no retry) + ai.* wrappers + `ai.generate_batch` | `ai._chat` contract unchanged; add recoverable-class retry inside it |
| `sql/80-theodb-migrate.sql` | ~50 | M16 | `theodb.import_pinecone(target, export jsonb, ...) RETURNS integer` FUNCTION, single-tx `FOR rec IN jsonb_array_elements LOOP INSERT` | the existing FUNCTION stays (backward-compat); add a chunked PROCEDURE alongside |
| `sql/theodb--1.0--1.1.sql` | 13 | M15 | no-op upgrade seed (`DO $$ NULL $$`) | replace no-op with the conditional retirement DROP of the plpython3u `theodb.embed` |
| `theodb.control` | — | M15 | umbrella control, `default_version = '1.0'` | evaluate bumping `default_version` to `1.1` so the upgrade path is real (decided in T4.1) |
| `theodb_rs/src/embed.rs` `#[pg_test]` / `benchmarks/tests/test_embed_sql.py` | — | M17 | the parity oracle (13 tests) | stays green UNCHANGED; new tests added in new files |
| `benchmarks/tests/test_embed_batch.py` (NEW) | 0 | — | embed_batch parity + N→1 tests | — |
| `benchmarks/tests/test_hybrid_guard.py` (NEW) | 0 | — | hybrid_search fail-fast guard regression | — |
| `benchmarks/tests/test_retry.py` (NEW) | 0 | — | retry on transient 503 / no-retry on 4xx | — |
| `benchmarks/tests/test_import_chunked.py` (NEW) | 0 | — | chunked import correctness | — |
| `benchmarks/bench_embed_batch.py` (NEW) | 0 | — | N×embed vs embed_batch(N) latency harness | — |
| `docs/benchmarks/audit-remediation-embed-batch.md` (NEW) | 0 | — | benchmark report (N→1) | — |
| `docs/adr/0007-synchronous-per-row-model-http.md` (NEW) | 0 | — | promoted from adr-drafts/adr-A | — |
| `docs/adr/0008-no-embedding-chat-cache.md` (NEW) | 0 | — | promoted from adr-drafts/adr-B | — |
| `CHANGELOG.md` | — | — | public contract | `[Unreleased]` updated |

### Current callers / dependents

- `theodb.embed(text,text)` — called by users + by `ai.hybrid_search_rrf` (sql/40:62, late-bound). The new `theodb.embed_batch` is additive (no caller migration forced). `grep -rn 'theodb.embed' sql/` → sql/40 (caller), sql/30 (schema only). `_embed_text` (Rust) called only by the `theodb.embed` wrapper.
- `ai._chat` — single HTTP source of truth; called by ai.generate/if/analyze_sentiment/summarize/rank + ai.generate_batch (sql/50). Adding retry inside `ai._chat` benefits ALL of them (one place — DRY).
- `theodb.import_pinecone` — user-facing migration entry (docs/migrate-from-pinecone.md). The new chunked PROCEDURE is additive; the FUNCTION stays for small imports + backward-compat.
- The plpython3u `theodb.embed` shipped M2 DoD-3 (CHANGELOG:146), distributed through v0.15.0 as a `theodb` extension member; M17 (v0.16.0) moved it to `theodb_rs`. So existing v0.x installs own the plpython3u embed — the retirement migration is real, not speculative.

### Domain glossary

- **embed_batch / N→1** — one HTTP POST with `input: string[]` returning N embeddings (OpenAI `/v1/embeddings` array shape; the stub `tools/embedding_server.py:53` already supports it), vs N separate per-row POSTs.
- **recoverable vs irrecoverable** (`error-handling.md §2`) — recoverable: timeout / 502 / 503 / 429 → retry with backoff; irrecoverable: input error (22023) / other 4xx → fail-fast, NO retry.
- **retirement migration** — an `ALTER EXTENSION UPDATE` script step that removes a deprecated object; here, conditionally DROP the plpython3u `theodb.embed` so `theodb_rs` can own it without a duplicate-definition clash.
- **excisability / silent-drop** — `DROP EXTENSION theodb_rs` removes `theodb.embed` with no `pg_depend` edge protecting `ai.hybrid_search_rrf` → runtime break; the guard makes it fail-fast + clear.

### Architecture boundaries affected

Per `.claude/rules/architecture.md`: embed_batch keeps the 3-boundary layering (domain `run_batch` in embed.rs → pg-glue helpers; api `#[pg_extern]` in lib.rs). The retry lives in the infrastructure-adapter (embed.rs / ai._chat) — recoverable-error handling per `error-handling.md`. The hybrid_search guard is a fail-fast at the cross-extension boundary (the seam the audit flagged). No new dependency-direction violation.

## Prior Art & Related Work

- **Internal DISCOVER deliverable:** `.claude/knowledge-base/audits/system-design-audit-2026-06-29.md` (the audit) — the source of all findings + remediations + the 2 MADR drafts (`system-design-adr-drafts/adr-A`, `system-design-adr-drafts/adr-B`). This is this cycle's discovery investigation.
- **Internal patterns (the templates):** `ai.generate_batch` (sql/50:219, `RETURNS text[] LANGUAGE plpython3u`, validates NULL/NULL-elements, N-in/N-out) — the batch template; `ai._chat` (sql/50:16) — the single HTTP source of truth to add retry to; `error-handling.md §2` — recoverable-vs-irrecoverable retry discipline; `theodb_rs/src/embed.rs` `run()` — the per-row impl `run_batch()` mirrors.
- **M17 parity discipline:** `docs/benchmarks/m17-embed-rust-vs-plpython.md` + the 13-test oracle — the parity-by-test gate this plan preserves.
- **AGPL refs (structure-only):** none copied; the batch pattern is internal (`ai.generate_batch`).

## Objective

- [ ] `theodb.embed_batch(text[]) RETURNS vector[]` exists (Rust `_embed_batch_text` + SQL wrapper), produces vectors IDENTICAL to per-row `theodb.embed` for the same inputs, via ONE HTTP round-trip; benchmark shows the N→1 latency collapse.
- [ ] `ai.hybrid_search_rrf` fail-fast guards the `theodb.embed` seam (clear typed error if theodb_rs absent) — no silent break.
- [ ] `ai._chat` + the Rust embed client retry the recoverable class (timeout/502/503/429) with bounded jittered backoff; NEVER retry input/other-4xx.
- [ ] `theodb.import_pinecone_chunked(...)` PROCEDURE ingests in bounded batches with per-batch COMMIT; the existing FUNCTION stays for small imports.
- [ ] The 1.0→1.1 upgrade conditionally retires the plpython3u `theodb.embed` (only if plpython3u-language AND not a theodb_rs member); no duplicate-definition clash on upgrade + add theodb_rs.
- [ ] ADR-A → `docs/adr/0007`, ADR-B → `docs/adr/0008` (Accepted).
- [ ] The existing 13-test oracle stays green UNCHANGED; new tests for each fix pass against the rebuilt image.

## ADRs

### D1 — `embed_batch` mirrors `ai.generate_batch` (native array input), NOT a per-row loop
**Decision:** `theodb.embed_batch(text[]) RETURNS vector[]` posts the whole array as `input: string[]` in ONE request and maps `data[i].embedding` back by index; the Rust `theodb_rs::embed::run_batch(&[&str]) -> Vec<String>` reuses the same GUC/SSRF/typed-error path as `run()`.
**Rationale:** the embeddings endpoint natively accepts an array (OpenAI shape; stub `:53` confirmed) — so unlike `ai.generate_batch` (which must combine prompts into one chat call + parse a JSON array), embed batching is a direct array POST (simpler + exact). Mirrors the established `ai.generate_batch` N-in/N-out contract (DRY). Reuses `embed.rs` glue (Rule 9 — no new HTTP path).
**Alternatives considered:** (a) per-row loop inside a batch function — rejected: that's still N round-trips (doesn't fix the N+1); (b) async/queue — rejected: premature, no measured bottleneck (ADR 0007 defers it); (c) client-side prompt-combining like generate_batch — rejected: unnecessary, the endpoint takes arrays.
**Consequences:** one round-trip per batch; N-in/N-out alignment must be enforced (index-ordered) + NULL-element handling; the benchmark proves the win.

### D2 — Retry the recoverable class ONLY, in ONE place per client (DRY), bounded + jittered
**Decision:** add bounded retry (≤2 retries, exponential backoff + jitter) for the recoverable class (connect/timeout, 502/503/429) in `ai._chat` (the single HTTP source of truth — all ai.* inherit it) and in `theodb_rs::embed` (minreq path). Input errors (22023) + other 4xx → fail-fast, NO retry.
**Rationale:** `error-handling.md §2` — "External-API timeout → retry with backoff; business-rule violation → fail immediately." Retrying 4xx/input would mask bugs (Rule 8). One place per client = DRY (ai._chat covers all 5 ai.* wrappers + generate_batch).
**Alternatives considered:** (a) no retry (status quo) — rejected: a single transient 5xx aborts a whole statement, discarding already-paid calls (audit #5); (b) infinite/unbounded retry — rejected: no backoff/cap is a DoS-amplifier (`error-handling.md` anti-pattern); (c) retry everything incl 4xx — rejected: masks irrecoverable errors.
**Consequences:** transient blips self-heal; bounded so no runaway; the retry is observable (the failure path still fails-fast after the cap with the typed error).

### D3 — Chunked import as a PROCEDURE (per-batch COMMIT); keep the FUNCTION for small imports
**Decision:** add `theodb.import_pinecone_chunked(target regclass, export jsonb, chunk_size int DEFAULT 1000)` as a PROCEDURE that ingests in `chunk_size` batches with a COMMIT per batch; keep the existing `theodb.import_pinecone(...) RETURNS integer` FUNCTION unchanged for small/transactional imports.
**Rationale:** a plpgsql FUNCTION CANNOT `COMMIT` (it runs in the caller's transaction) — only a PROCEDURE can. Per-batch COMMIT is the only way to bound the in-memory/WAL footprint of a large migration (audit #6/#7). Keeping the FUNCTION preserves backward-compat + the all-or-nothing semantics small imports must want (KISS — don't break the existing contract).
**Alternatives considered:** (a) convert the FUNCTION to a PROCEDURE — rejected: breaks the `RETURNS integer` contract + callers + the all-or-nothing semantics; (b) caller-side chunking only (no new object) — rejected: pushes the footgun onto every user, the audit asked for a bounded path; (c) streaming via cursor in the FUNCTION — rejected: still one transaction (no COMMIT), doesn't bound WAL.
**Consequences:** large migrations are bounded + resumable-ish (committed batches survive a mid-run abort); two entry points (documented: FUNCTION for small/atomic, PROCEDURE for large).

### D4 — Conditional retirement DROP in 1.0→1.1; bump `default_version` to 1.1
**Decision:** the `theodb--1.0--1.1.sql` upgrade DROPs the plpython3u `theodb.embed(text,text)` ONLY when it exists AND is `LANGUAGE plpython3u` AND is NOT an extension member of `theodb_rs` (guarded by a `DO` block querying `pg_proc.prolang` + `pg_depend`); bump `theodb.control default_version` to `1.1` so fresh installs + `ALTER EXTENSION theodb UPDATE` land on the retired state.
**Rationale:** the plpython3u embed shipped through v0.15.0 (CHANGELOG:146) — existing installs own it as a `theodb` member; adding `theodb_rs` then clashes on `CREATE FUNCTION theodb.embed`. A blind `DROP` would fail if `theodb_rs` already owns it ("extension theodb_rs requires it") — hence the conditional guard. This is the standard PG extension retirement idiom.
**Alternatives considered:** (a) keep the no-op + document the gap — rejected: the clash is real for existing installs (not speculative); (b) unconditional DROP — rejected: fails when theodb_rs owns the function; (c) make theodb_rs `CREATE OR REPLACE` the embed — rejected: cross-extension object hijacking, worse coupling.
**Consequences:** existing v0.x installs upgrade cleanly then add theodb_rs without a clash; fresh installs at 1.1 never had the plpython3u embed (no-op DROP). The generated `sql/theodb--1.0.sql` (gitignored, rebuilt by make) is confirmed not to redefine embed.

### D5 — Fail-fast guard at the embed seam (to_regprocedure), not a hard `requires` edge
**Decision:** in `ai.hybrid_search_rrf`, before calling `theodb.embed(query_text)`, check `to_regprocedure('theodb.embed(text)') IS NULL` → `RAISE` a typed error (SQLSTATE 0A000 feature_not_supported, message "theodb.embed unavailable — install the theodb_rs extension"); add a regression test.
**Rationale:** the runtime call is late-bound plpgsql with no `pg_depend` edge (audit #3/#8); a hard `requires theodb_rs` edge on the `theodb` umbrella would invert the dependency (theodb_rs already requires theodb — a cycle) and force theodb_rs at install even for users who don't use hybrid search. A fail-fast guard turns a silent runtime break into a clear, actionable error — the pragmatic Staff fix the audit recommended.
**Alternatives considered:** (a) hard `requires` edge — rejected: dependency cycle (theodb_rs requires theodb) + forces the extension on non-hybrid users; (b) status quo silent break — rejected: the audit finding; (c) auto-install — rejected: extensions don't self-install dependents.
**Consequences:** dropping theodb_rs makes hybrid_search fail with a clear message instead of a cryptic "function does not exist"; one cheap `to_regprocedure` check per call (negligible).

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| `embed_batch` N-in/N-out misalignment (endpoint returns out-of-order or fewer items) | High | Map by `data[i].index` (not array position); assert returned count == input count → 38000 on mismatch; parity test vs per-row | maintainers |
| Retry must double-charge a non-idempotent endpoint OR add latency on a genuinely-down endpoint | Medium | Bounded (≤2) + only recoverable class; embeddings/chat are idempotent reads; cap keeps worst-case latency = timeout×3 | maintainers |
| Conditional retirement DROP logic wrong → drops theodb_rs's function OR fails the upgrade | High | Guard on `prolang=plpython3u` AND NOT pg_depend-member-of-theodb_rs; test BOTH orders (upgrade-then-theodb_rs, theodb_rs-then-upgrade) | maintainers |
| Chunked PROCEDURE COMMIT-per-batch leaves partial state on mid-run failure | Medium | Documented semantics (committed batches persist — that's the point); the atomic FUNCTION remains for all-or-nothing; test partial-failure leaves prior batches committed | maintainers |
| Scope is large (5 code fixes + 2 ADRs) — review surface | Medium | Phased + each phase independently tested; the 13-test oracle stays the regression backstop; phases are independent (parallelizable) | maintainers |

## Unresolved Questions

- Q1 — Does bumping `theodb.control default_version` to 1.1 require regenerating/renaming the install SQL (`theodb--1.1.sql`) or does PG chain `theodb--1.0.sql` + `theodb--1.0--1.1.sql`? (Resolved at T4.1: PG chains the upgrade scripts; fresh `CREATE EXTENSION theodb VERSION '1.1'` runs 1.0 base then the 1.0→1.1 delta — confirm the Makefile ships both. If the base must be 1.1, adjust the make concat target name.)
- Q2 — minreq: is there a clean way to detect the HTTP status for the 429/502/503 retry class, and does it expose connect-vs-timeout errors distinctly? (Resolved at T3.1 from the crate API — `resp.status_code` for HTTP class; `send()` Err for connect/timeout.)

## Dependencies

(none — NO new dependency is added. embed_batch reuses the existing `pgrx`/`minreq`/`serde_json` already in `theodb_rs/Cargo.toml`; the retry backoff+jitter uses the Rust stdlib (`std::thread::sleep` + an attempt-indexed jitter — parsimony rung 5, no `rand`/`backoff` crate) and the plpython3u stdlib (`time`/`random`) in `ai._chat`; the chunked import + retirement migration + seam guard are pure SQL/plpgsql; the ADRs are docs. `/deps-audit` has no new declared dep to scan.)

## Dependency Graph

```
Phase 0 (ADRs) ── independent ──┐
Phase 1 (embed_batch + bench) ──┤
Phase 2 (seam guard) ───────────┤ (all independent — different files)
Phase 3 (retry) ────────────────┤
Phase 4 (retirement migration) ─┤
Phase 5 (chunked import) ───────┴──▶ Phase 6 (Integration Validation: rebuild + full suite + benchmark)
```
Phases 0–5 touch disjoint surfaces (ADR docs / embed.rs+lib.rs / sql40 / sql50+embed.rs / sql upgrade+control / sql80) and can be implemented in any order; Phase 3 and Phase 1 both touch embed.rs (sequence them: embed_batch first, then add retry to both run() and run_batch()). Phase 6 validates everything together.

---

## Phase 0: Promote the two ADR drafts

### T0.1 — ADR-A → docs/adr/0007, ADR-B → docs/adr/0008

#### Objective
Promote the audit's two MADR 3.0 drafts to accepted ADRs, closing findings #10 (sync-per-row undocumented) + #11 (no-cache undocumented).

#### Why this step (action + reasoning)
1. **What:** copy `.claude/knowledge-base/audits/system-design-adr-drafts/adr-A-*.md` → `docs/adr/0007-synchronous-per-row-model-http.md` and `system-design-adr-drafts/adr-B-*.md` → `docs/adr/0008-no-embedding-chat-cache.md`, set Status: Accepted, cross-link embed_batch (mitigation of A).
2. **Why now:** cheapest, zero code risk; makes the two high-impact decisions durable records (audit #10/#11). Cites audit "ADR Suggestions".

#### Evidence
- `.claude/knowledge-base/audits/system-design-adr-drafts/adr-A-synchronous-per-row-model-http.md` + `adr-B-no-embedding-chat-cache.md` (audit-generated, MADR 3.0). Latest ADR is `docs/adr/0006` → next are 0007/0008.

#### Files to edit
```
docs/adr/0007-synchronous-per-row-model-http.md (NEW) — from adr-A; Status: Accepted; note embed_batch (Phase 1) as the shipped mitigation
docs/adr/0008-no-embedding-chat-cache.md (NEW) — from adr-B; Status: Accepted
```

#### Deep file dependency analysis
- NEW docs; no code dependency. The ADR-A consequences reference the per-row sync model; cross-link Phase 1's embed_batch as the delivered leverage mitigation.

#### Tasks
1. Copy adr-A → 0007, adr-B → 0008; set Status: Accepted; add cross-links (0007 ↔ embed_batch).
2. Confirm both render + cite the findings they close.

#### TDD
```
RED:    test_adrs_promoted — `ls docs/adr/0007-*.md docs/adr/0008-*.md` both exist + contain "Status: Accepted"
GREEN:  promote the drafts
VERIFY: grep -l 'Status' docs/adr/0007-*.md docs/adr/0008-*.md
```

#### Concurrency tests (only)

(none — single-threaded) — docs only.

#### Acceptance Criteria
- [ ] `docs/adr/0007-synchronous-per-row-model-http.md` + `docs/adr/0008-no-embedding-chat-cache.md` exist, Status: Accepted.
- [ ] ADR 0007 cross-links the `embed_batch` mitigation — verified by `grep -q embed_batch docs/adr/0007-synchronous-per-row-model-http.md`.

#### DoD
- [ ] Both ADRs present; CHANGELOG notes the promotion.

---

## Phase 1: `theodb.embed_batch` (the CRITICAL N+1 fix) + benchmark

### T1.1 — Rust `run_batch` + `_embed_batch_text` + SQL `theodb.embed_batch`

#### Objective
Add a batch embedding path that posts `input: string[]` once and returns N vectors index-aligned, identical to per-row `theodb.embed`.

#### Why this step (action + reasoning)
1. **What:** add `embed::run_batch(content: &[Option<&str>], model) -> Vec<String>` (one POST, parse `data[].embedding` by index), `#[pg_extern] _embed_batch_text(content: Vec<Option<&str>>, model) -> Vec<String>`, and `extension_sql!` wrapper `theodb.embed_batch(text[]) RETURNS vector[]` casting each element `::vector`.
2. **Why now:** the audit's #1 CRITICAL — bulk embed = N round-trips; the endpoint already takes arrays (stub :53). Mirrors `ai.generate_batch` (D1). Cites system-design-audit-2026-06-29.md #1 + sql/50:219.

#### Evidence
- system-design-audit-2026-06-29.md #1 (`sql/theodb--1.0.sql:90`); `ai.generate_batch` template (sql/50:219, RETURNS text[]); stub array input (tools/embedding_server.py:53 `texts=[inp] if str else list(inp)`); `embed.rs run()` (the per-row impl to mirror).

#### Files to edit
```
theodb_rs/src/embed.rs — add run_batch(): one minreq POST with body {"input": [..], "model": ..}; parse data[] by index; assert len==N; reuse guc/SSRF/err_external; NULL element -> 22023 (N-in/N-out alignment, like generate_batch)
theodb_rs/src/lib.rs — add #[pg_extern] _embed_batch_text(content: Vec<Option<&str>>, model: default!(Option<&str>,"NULL")) -> Vec<String> { crate::embed::run_batch(&content, model) }; extension_sql! CREATE FUNCTION theodb.embed_batch(content text[], model text DEFAULT NULL) RETURNS vector[] LANGUAGE sql AS 'SELECT COALESCE(array_agg(t::vector ORDER BY ord), ARRAY[]::vector[]) FROM unnest(theodb_rs._embed_batch_text(content, model)) WITH ORDINALITY AS u(t, ord)' (EC-1: COALESCE empty -> empty vector[], order-preserving) + REVOKE FROM PUBLIC
benchmarks/tests/test_embed_batch.py (NEW) — parity (embed_batch == per-row for same inputs) + N-in/N-out + empty array + NULL element -> 22023
benchmarks/bench_embed_batch.py (NEW) — measure N×embed vs 1×embed_batch(N) against the same stub
docs/benchmarks/audit-remediation-embed-batch.md (NEW) — the N->1 benchmark report
```

#### Deep file dependency analysis
- `embed.rs` (Baseline row): `run_batch` reuses guc/err helpers + the minreq pattern from `run()` (DRY). `lib.rs`: new `#[pg_extern]` + `extension_sql!` wrapper (additive; `theodb.embed` unchanged). The SQL wrapper must preserve ORDER (array_agg with ORDERED unnest, or build the vector[] index-aligned). Casting text→vector[] reuses pgvector (D5 of M17).
- Downstream: `theodb.embed_batch` is NEW (no existing caller); additive. The oracle (13 tests) is unaffected.

#### Deep Dives
- **N-in/N-out (invariant):** map returned embeddings by `data[i].index` (OpenAI guarantees index), assert count==N, else 38000 "batch size mismatch". NULL element → 22023 (mirror generate_batch). Empty array → empty result (no HTTP call).
- **Order preservation in the SQL wrapper:** build `vector[]` index-aligned (e.g., `SELECT array_agg(t::vector ORDER BY ord) FROM unnest(theodb_rs._embed_batch_text(content,model)) WITH ORDINALITY AS u(t,ord)`).
- **Parity:** `theodb.embed_batch(ARRAY['a','b'])` must equal `ARRAY[theodb.embed('a'), theodb.embed('b')]` element-wise (same stub, deterministic model).

#### Pseudo-code / Signatures
```rust
// embed.rs
pub(crate) fn run_batch(items: &[Option<&str>], model: Option<&str>) -> Vec<String> {
    if items.is_empty() { return vec![]; }
    if items.iter().any(|x| x.is_none()) { err_input("theodb.embed_batch: elements must not be NULL"); }
    let endpoint = guc("theodb.embedding_endpoint").unwrap_or_else(|| err_input("...not set..."));
    // ... SSRF check (reuse) ...
    let payload = json!({ "input": items, "model": mdl }).to_string();
    let resp = post(endpoint, payload)?; // reuse the run() client incl. retry (Phase 3)
    let data = parse_data_array(resp)?;          // [{index, embedding}, ...]
    if data.len() != items.len() { err_external("theodb.embed_batch: batch size mismatch"); }
    let mut out = vec![String::new(); items.len()];
    for d in data { out[d.index] = format_vector(d.embedding); }  // index-aligned
    out
}
```

#### Tasks
1. Extract a shared `post_embeddings(endpoint, payload) -> Value` helper in embed.rs (used by run + run_batch) (DRY; Phase 3 adds retry here once).
2. Implement `run_batch` (array POST, index-aligned, count assert, NULL/empty handling).
3. Add `_embed_batch_text` + `theodb.embed_batch` wrapper (order-preserving vector[]) + REVOKE.
4. Write `test_embed_batch.py` (parity + N-in/N-out + empty + NULL).
5. Write `bench_embed_batch.py` + the report.

#### TDD
```
RED:    #[pg_test] embed_batch_rejects_null_element — ARRAY['x', NULL] -> 22023
RED:    test_embed_batch_parity (py) — embed_batch(['a','b']) == [embed('a'), embed('b')] element-wise (384-dim each)
RED:    test_embed_batch_empty — embed_batch(ARRAY[]::text[]) -> empty, no HTTP call
RED:    test_embed_batch_size_mismatch — broken stub returns N-1 embeddings -> 38000
RED:    test_embed_batch_order_and_dups (EC-3) — embed_batch(['a','b','a']) -> 3 vectors, [0]==[2], [1] differs; correct even if stub returns data out of index order (index-aligned, not array-position)
RED:    test_embed_batch_empty_returns_empty_array (EC-1) — embed_batch(ARRAY[]::text[]) -> '{}'::vector[] (NOT NULL), no HTTP call
GREEN:  implement run_batch + wrapper (COALESCE empty -> empty vector[])
REFACTOR: extract post_embeddings shared helper
VERIFY: cargo pgrx test ; python3 -m pytest benchmarks/tests/test_embed_batch.py -v
```

#### Concurrency tests (only)

(none — single-threaded) — embed_batch is one synchronous call; no shared mutable state.

#### Failure-scenario note
External HTTP — covered in `## Failure scenarios` (batch endpoint 5xx/timeout, size mismatch, NULL element).

#### Acceptance Criteria
- [ ] `theodb.embed_batch(text[]) RETURNS vector[]` exists; parity test green (== per-row element-wise).
- [ ] N-in/N-out invariant holds (count mismatch → 38000; NULL element → 22023; empty → empty, no HTTP) — asserted by `pytest benchmarks/tests/test_embed_batch.py` (exit 0).
- [ ] `docs/benchmarks/audit-remediation-embed-batch.md` shows `embed_batch(N)` materially below `N × embed()` (mean±std, ≥3 runs) — the N→1 collapse, with the measured speedup stated (a real win; data-backed per public-copy).
- [ ] Quality gates pass — `cargo clippy -- -D warnings` exits 0, `ruff check` clean, every changed file ≤ 500 lines (`wc -l`).

#### DoD
- [ ] `cargo pgrx test` + `test_embed_batch.py` green; benchmark report committed; the 13-test oracle still green.

---

## Phase 2: hybrid_search fail-fast seam guard

### T2.1 — Guard `theodb.embed` call in `ai.hybrid_search_rrf` + regression test

#### Objective
Turn the silent DROP-theodb_rs break into a clear typed error.

#### Why this step (action + reasoning)
1. **What:** before `qvec := theodb.embed(query_text)` (sql/40:62), add `IF to_regprocedure('theodb.embed(text)') IS NULL THEN RAISE EXCEPTION ... USING ERRCODE='0A000'`.
2. **Why now:** audit #3/#8 — late-bound seam, no pg_depend, silent break. D5 (guard, not hard requires). Cites sql/40:62.

#### Evidence
- system-design-audit-2026-06-29.md #3/#8; sql/40:62 `qvec := theodb.embed(query_text)`.

#### Files to edit
```
sql/40-theodb-hybrid.sql — insert the to_regprocedure guard before line 62 (inside the IF qvec IS NULL branch)
benchmarks/tests/test_hybrid_guard.py (NEW) — with theodb.embed absent (or simulated), hybrid_search(query_text, NULL) raises the typed 0A000 with the clear message; with it present, normal path works
```

#### Deep file dependency analysis
- sql/40 (Baseline row): the guard is additive inside the existing `IF qvec IS NULL AND query_text IS NOT NULL` branch. No behavior change when theodb.embed is present (the common case). The test simulates absence (e.g., a DB where theodb_rs isn't installed, or temporarily rename — carefully).

#### Deep Dives
- **Invariant:** when theodb.embed exists, hybrid_search behaves EXACTLY as before. The guard only fires when absent.
- **Edge case:** `to_regprocedure('theodb.embed(text)')` — the function is `theodb.embed(text, text)` with a default; the single-arg call resolves via default. Confirm the regproc signature string matches (`theodb.embed(text)` resolves the 1-arg call form). Test both.

#### Tasks
1. Add the guard before the embed call.
2. Write `test_hybrid_guard.py` (absent → 0A000 clear; present → works).

#### TDD
```
RED:    test_hybrid_search_without_embed_raises_clear (py) — simulate theodb.embed absent -> 0A000 "theodb.embed unavailable" (not a cryptic 42883)
RED:    test_hybrid_search_with_embed_unchanged (py) — present -> normal RRF result
GREEN:  add the guard
VERIFY: python3 -m pytest benchmarks/tests/test_hybrid_guard.py -v
```

#### Concurrency tests (only)

(none — single-threaded)

#### Acceptance Criteria
- [ ] hybrid_search with theodb.embed absent raises typed `0A000` (not 42883) with the clear message — asserted by `pytest benchmarks/tests/test_hybrid_guard.py::test_absent_raises_0A000`.
- [ ] hybrid_search with theodb.embed present returns unchanged behavior — asserted by `pytest benchmarks/tests/test_hybrid_guard.py::test_present_unchanged`.

#### DoD
- [ ] `test_hybrid_guard.py` green; existing hybrid tests unaffected.

---

## Phase 3: recoverable-class retry (embed client + ai._chat)

### T3.1 — Bounded jittered backoff retry for timeout/502/503/429

#### Objective
Self-heal transient external-HTTP failures; never retry irrecoverable ones.

#### Why this step (action + reasoning)
1. **What:** in `embed.rs` `post_embeddings` (the shared helper from T1.1) wrap `send()` in a bounded retry loop (≤2 retries, backoff + jitter) for connect/timeout + status 502/503/429; in `ai._chat` (sql/50, plpython3u) wrap the urllib call similarly. Other errors → fail-fast (unchanged 22023/38000).
2. **Why now:** audit #5 — a single transient 5xx aborts a whole statement. D2 (recoverable-only, DRY in ai._chat). Cites error-handling.md §2.

#### Evidence
- system-design-audit-2026-06-29.md #5; embed.rs `run()` send() (no retry); sql/50 ai._chat urllib (no retry); error-handling.md §2.

#### Files to edit
```
theodb_rs/src/embed.rs — retry loop in post_embeddings (recoverable class only); backoff with jitter (std-only or a tiny dep — prefer std: thread::sleep + a deterministic jitter seed; NO new crate if avoidable per parsimony)
sql/50-theodb-ai.sql — ai._chat: wrap opener.open in a bounded retry for URLError-timeout/HTTPError-{502,503,429}; reraise others immediately
benchmarks/tests/test_retry.py (NEW) — stub returns 503 once then 200 -> success; stub returns 400 -> immediate 38000/no-retry; assert retry count via stub hit-counter
```

#### Deep file dependency analysis
- embed.rs: the retry lives in the shared `post_embeddings` helper, so BOTH `run()` and `run_batch()` inherit it (DRY). ai._chat: the single HTTP source of truth → all ai.* wrappers + generate_batch inherit the retry (DRY). No new dependency direction.
- Parsimony (rung 4/5): backoff jitter — use stdlib (`std::thread::sleep` + a cheap jitter from the attempt index; avoid adding a `rand`/`backoff` crate unless genuinely needed — a fixed exponential with small ±jitter from a nanos-based seed is enough). Document the choice.

#### Deep Dives
- **Recoverable class (consensus):** connect-refused, timeout, 502, 503, 429. **Irrecoverable:** 22023 input, 400/401/403/404/422 (other 4xx), JSON-shape errors → NO retry, fail-fast.
- **Bounded:** ≤2 retries (3 attempts total); exponential backoff (e.g., 100ms, 400ms) + jitter; worst-case added latency ≈ 0.5s + retries × timeout only if the endpoint hangs (cap respected).
- **Invariant:** a permanently-down endpoint still fails-fast after the cap with the SAME typed 38000 "call failed" (parity preserved — the existing failure tests stay green).

#### Tasks
1. embed.rs: classify the error/status; retry recoverable ≤2 with backoff+jitter (stdlib); else fail-fast as today.
2. ai._chat: same classification + bounded retry in plpython3u.
3. `test_retry.py`: transient-503-then-200 → success; 400 → immediate, no retry (stub hit-counter asserts attempts).

#### TDD
```
RED:    #[pg_test] embed_retries_transient_503 — stub 503-once-then-200 -> Ok (1 retry)  [or python oracle if #[pg_test] can't run]
RED:    test_embed_no_retry_on_4xx (py) — stub 400 -> 38000 immediately, stub hit exactly once
RED:    test_chat_retries_transient_503 (py) — ai.generate via 503-once stub -> success
RED:    test_retry_respects_bounds (EC-2) — down endpoint -> total attempts <= 3 (stub hit-counter) AND api_key absent from the final exhausted-retry error message
GREEN:  add bounded retry to post_embeddings + ai._chat
VERIFY: python3 -m pytest benchmarks/tests/test_retry.py -v ; existing test_embed_failure_scenarios.py still green (permanent failure still fails-fast)
```

#### Concurrency tests (only)

(none — single-threaded) — retry is sequential per call; no shared state.

#### Failure-scenario note
Directly exercises `## Failure scenarios` (transient 503 → recover; permanent 4xx/5xx → fail-fast after cap).

#### Acceptance Criteria
- [ ] Transient 503-then-200 recovers via retry on both embed and ai.* — asserted by `pytest benchmarks/tests/test_retry.py::test_transient_recovers`.
- [ ] 4xx (non-429) / input error fails fast with NO retry (stub hit-counter equals 1) — asserted by `pytest benchmarks/tests/test_retry.py::test_no_retry_on_4xx`.
- [ ] Permanent failure returns typed `38000` after the bounded retry cap, with no irrecoverable-path regression — asserted by `pytest benchmarks/tests/test_embed_failure_scenarios.py` (exit 0).
- [ ] No new crate added (parsimony) and clippy stays clean — verified by `cargo tree --depth 1` unchanged and `cargo clippy -- -D warnings` exit 0.

#### DoD
- [ ] `test_retry.py` green; `test_embed_failure_scenarios.py` still green; clippy clean.

---

## Phase 4: retirement migration (plpython3u embed) + default_version bump

### T4.1 — Conditional DROP in 1.0→1.1 + bump default_version

#### Objective
Let existing v0.x installs (which own the plpython3u `theodb.embed`) upgrade + add `theodb_rs` without a duplicate-definition clash.

#### Why this step (action + reasoning)
1. **What:** replace the no-op `theodb--1.0--1.1.sql` with a `DO` block that DROPs `theodb.embed(text,text)` ONLY if it's `LANGUAGE plpython3u` AND not a `theodb_rs` member (pg_proc.prolang + pg_depend check); bump `theodb.control default_version` to 1.1; confirm the make concat ships the upgrade chain.
2. **Why now:** audit #9 — the plpython3u embed shipped through v0.15.0; the upgrade is a no-op. D4. Cites CHANGELOG:146 + sql/theodb--1.0--1.1.sql.

#### Evidence
- system-design-audit-2026-06-29.md #9; CHANGELOG:146 (plpython3u embed shipped M2); sql/theodb--1.0--1.1.sql (no-op); theodb.control default_version=1.0.

#### Files to edit
```
sql/theodb--1.0--1.1.sql — replace no-op with the conditional retirement DROP (DO block: if plpython3u embed exists AND not theodb_rs-owned -> DROP FUNCTION theodb.embed(text,text))
theodb.control — default_version = '1.1' (so fresh + UPDATE land retired); confirm Makefile concat target name (Q1)
Makefile — if default_version bump requires a theodb--1.1.sql base (vs chained), adjust the concat target (Q1 resolution)
benchmarks/tests/test_retirement_migration.py (NEW) — install a fake plpython3u theodb.embed (as a theodb member) -> run the 1.0->1.1 DROP -> CREATE EXTENSION theodb_rs -> no clash; AND the reverse: theodb_rs already present -> the conditional does NOT drop its function
```

#### Deep file dependency analysis
- sql/theodb--1.0--1.1.sql (Baseline row): no-op → conditional DROP. Must NOT drop theodb_rs's function (pg_depend guard). theodb.control default_version drives fresh-install + UPDATE target. The Makefile (Baseline) concatenates sql/* into theodb--1.0.sql; confirm the 1.1 chain ships (Q1).
- Edge: if `theodb_rs` owns `theodb.embed`, the DO block's guard skips the DROP (no error). If a stale plpython3u embed exists (theodb member), DROP it.

#### Deep Dives
- **Conditional guard logic:** `SELECT 1 FROM pg_proc p JOIN pg_language l ON l.oid=p.prolang WHERE p.proname='embed' AND p.pronamespace='theodb'::regnamespace AND l.lanname='plpython3u' AND NOT EXISTS (SELECT 1 FROM pg_depend d JOIN pg_extension e ON e.oid=d.refobjid WHERE d.objid=p.oid AND e.extname='theodb_rs')` → if found, `DROP FUNCTION theodb.embed(text,text)`.
- **Invariant:** fresh 1.1 install (no plpython3u embed) → DROP is a no-op (guard finds nothing). theodb_rs-owned embed → never dropped.
- **Q1:** PG chains `theodb--1.0.sql` → `theodb--1.0--1.1.sql` on `CREATE EXTENSION theodb` when default_version=1.1 (it builds 1.0 then applies the delta). Confirm the make target ships BOTH (it already concatenates sql/* into theodb--1.0.sql + copies theodb--1.0--1.1.sql).

#### Tasks
1. Write the conditional DROP DO block in 1.0→1.1.
2. Bump default_version to 1.1; confirm/adjust the Makefile chain (Q1).
3. `test_retirement_migration.py`: both directions (stale-embed→dropped+no-clash; theodb_rs-owned→not dropped).

#### TDD
```
RED:    test_retirement_drops_stale_plpython_embed (py) — seed a plpython3u theodb.embed as theodb member -> ALTER EXTENSION theodb UPDATE TO '1.1' -> embed dropped -> CREATE EXTENSION theodb_rs -> no clash
RED:    test_retirement_keeps_theodb_rs_embed (py) — theodb_rs owns theodb.embed -> run the DO guard -> function still present (not dropped)
GREEN:  write the conditional DROP + default_version bump
VERIFY: python3 -m pytest benchmarks/tests/test_retirement_migration.py -v ; fresh-install path (image init) still creates both extensions cleanly
```

#### Concurrency tests (only)

(none — single-threaded) — DDL migration.

#### Acceptance Criteria
- [ ] Existing-install simulation: the stale plpython3u embed is dropped on UPDATE so theodb_rs installs with no clash — asserted by `pytest benchmarks/tests/test_import_chunked.py::test_upgrade_drops_plpython_embed` (exit 0).
- [ ] theodb_rs-owned embed is NEVER dropped by the conditional guard — asserted by `pytest benchmarks/tests/test_import_chunked.py::test_owned_embed_preserved`.
- [ ] Fresh image init (CREATE EXTENSION theodb + theodb_rs) stays clean — verified by `make -C benchmarks rebuild-and-smoke` exit 0.
- [ ] theodb.control default_version equals 1.1 and the upgrade chain ships in the image — verified by `grep -q "default_version = '1.1'" theodb.control`.

#### DoD
- [ ] `test_retirement_migration.py` green; image init green; CHANGELOG notes the migration.

---

## Phase 5: chunked import PROCEDURE

### T5.1 — `theodb.import_pinecone_chunked(...)` PROCEDURE with per-batch COMMIT

#### Objective
Bound the memory/WAL footprint of large Pinecone imports.

#### Why this step (action + reasoning)
1. **What:** add a PROCEDURE that iterates `jsonb_array_elements` in `chunk_size` batches, INSERTing + `COMMIT` per batch; keep the existing FUNCTION for small/atomic imports.
2. **Why now:** audit #6/#7 — single-tx whole-jsonb import is unbounded. D3 (PROCEDURE, since FUNCTION can't COMMIT). Cites sql/80:32.

#### Evidence
- system-design-audit-2026-06-29.md #6/#7; sql/80:32 (FOR rec IN jsonb_array_elements LOOP INSERT, single-tx FUNCTION RETURNS integer).

#### Files to edit
```
sql/80-theodb-migrate.sql — add CREATE PROCEDURE theodb.import_pinecone_chunked(target regclass, export jsonb, chunk_size int DEFAULT 1000, id_col text DEFAULT 'id', embedding_col text DEFAULT 'embedding', metadata_col text DEFAULT 'metadata'); same validation + %I-safe dynamic SQL as the FUNCTION; COMMIT every chunk_size rows; REVOKE FROM PUBLIC
benchmarks/tests/test_import_chunked.py (NEW) — import N>chunk_size records via CALL -> all inserted; a mid-run failure leaves prior committed batches present (the documented semantic)
docs/migrate-from-pinecone.md — document FUNCTION (small/atomic) vs PROCEDURE (large/chunked)
```

#### Deep file dependency analysis
- sql/80 (Baseline row): the FUNCTION `import_pinecone` stays UNCHANGED (backward-compat). The PROCEDURE is additive, reusing the same validation + injection-safe `format %I` + `::vector` cast + USING params. Procedures are CALLed (not SELECTed) — document the difference.
- Edge: chunk_size ≤ 0 → 22023; export not array → 22023 (reuse the FUNCTION's guard).

#### Deep Dives
- **Why PROCEDURE:** plpgsql FUNCTIONs run in the caller's transaction and CANNOT COMMIT; only a PROCEDURE (CALLed) can issue COMMIT. Per-batch COMMIT is the only way to bound WAL/memory for a large migration.
- **Invariant:** identical row output to the FUNCTION for the same input (just committed in batches). Injection-safety preserved (`%I` + `::regclass` + USING params).
- **Semantic (documented):** a mid-run failure leaves already-committed batches persisted (the point of chunking) — NOT all-or-nothing; the FUNCTION remains for all-or-nothing.

#### Tasks
1. Write the PROCEDURE (chunked loop + per-batch COMMIT + same validation/injection-safety).
2. `test_import_chunked.py`: N>chunk_size all inserted; partial-failure leaves prior batches.
3. Document FUNCTION-vs-PROCEDURE in the migration guide.

#### TDD
```
RED:    test_import_chunked_all_inserted (py) — CALL with 2500 records, chunk_size 1000 -> 2500 rows in target
RED:    test_import_chunked_commits_batches (py) — inject a bad record at row 1500 -> first 1000 committed, error raised on the failing batch (not a full rollback)
RED:    test_import_chunked_rejects_bad_chunk_size (py) — chunk_size 0 -> 22023
GREEN:  write the PROCEDURE
VERIFY: python3 -m pytest benchmarks/tests/test_import_chunked.py -v ; existing test (import FUNCTION) still green
```

#### Concurrency tests (only)

(none — single-threaded) — sequential batched ingest.

#### Acceptance Criteria
- [ ] `CALL theodb.import_pinecone_chunked(...)` ingests N>chunk_size correctly; per-batch COMMIT verified.
- [ ] Existing `theodb.import_pinecone` FUNCTION unchanged + still green.
- [ ] chunk_size ≤ 0 raises `22023` and injection-safety is preserved — asserted by `pytest benchmarks/tests/test_import_chunked.py::test_chunk_size_guard`.

#### DoD
- [ ] `test_import_chunked.py` green; migration guide documents both paths.

---

## Coverage Matrix

| # | Audit finding | Task(s) | Resolution |
|---|---|---|---|
| 1 | CRITICAL embed N+1 (no batch path) | T1.1 | `theodb.embed_batch(text[])` — 1 round-trip; parity + benchmark |
| 2 | HIGH blocking I/O holds backend | T1.1 | batch collapses N backend-occupations to 1 (the report's stated mitigation) |
| 3 | MEDIUM cross_boundary_import (silent break) | T2.1 | to_regprocedure fail-fast guard |
| 4 | MEDIUM missing_backpressure (fan-out) | T1.1, T3.1 | batch reduces fan-out; retry bounds transient amplification (full async deferred per ADR 0007) |
| 5 | MEDIUM missing_retry_policy | T3.1 | recoverable-class bounded jittered retry |
| 6 | MEDIUM unbounded_collection (import) | T5.1 | chunked PROCEDURE, per-batch COMMIT |
| 7 | MEDIUM memory_inefficiency (import) | T5.1 | same chunked PROCEDURE |
| 8 | MEDIUM tangled_module (DROP break) | T2.1 | same fail-fast guard (deletion-lens of #3) |
| 9 | MEDIUM missing_deprecation_marker (upgrade) | T4.1 | conditional retirement DROP + default_version bump |
| 10 | MEDIUM undocumented sync-per-row | T0.1 | ADR 0007 |
| 11 | MEDIUM undocumented no-cache | T0.1 | ADR 0008 |

**Coverage: 11/11 actionable findings covered (100%)** (the 4 INFO are positive baselines — no remediation by definition).

## Global Definition of Done

- [ ] All phases complete.
- [ ] New tests green vs the rebuilt image: `test_embed_batch.py` + `test_hybrid_guard.py` + `test_retry.py` + `test_retirement_migration.py` + `test_import_chunked.py`.
- [ ] The existing 13-test oracle (`test_embed_sql.py` 10 + `test_embed_failure_scenarios.py` 3) stays green UNCHANGED (no regression).
- [ ] `cargo pgrx test` green; `cargo clippy` 0 warnings; `ruff` clean.
- [ ] `docs/benchmarks/audit-remediation-embed-batch.md` shows the N→1 latency collapse (mean±std, ≥3 runs) — data-backed.
- [ ] ADR 0007 + 0008 present (Accepted).
- [ ] CHANGELOG `[Unreleased]` updated.
- [ ] Backward compatibility: `theodb.embed`, `theodb.import_pinecone` (FUNCTION), `ai._chat` contracts unchanged; all additions are additive.
- [ ] File-size budget ≤ 500 lines per changed file.

## Failure scenarios (external I/O — embeddings + chat endpoints)

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| embeddings endpoint (batch) | transient 503 then 200 | stub returns 503 once (hit-counter) | retry ≤2 → success (T3.1) |
| embeddings endpoint | 4xx (non-429) / input error | stub returns 400 | immediate 38000/22023, NO retry (hit==1) |
| embeddings endpoint (batch) | returns N-1 embeddings | broken stub drops one | 38000 "batch size mismatch" (T1.1) |
| embeddings endpoint | permanent down / timeout | unreachable host | fail-fast 38000 after the bounded retry cap (existing failure tests stay green) |
| chat endpoint (ai._chat) | transient 503 then 200 | stub 503-once | retry → success; all ai.* inherit (T3.1) |

## Final Phase: Integration Validation (MANDATORY)

> Runs after Phases 0–5. The plan is NOT done until the full chain + benchmark pass.

### Execution
```
docker build -t theo-db:audit-rem .                                   # builds with all fixes
docker run -d --add-host=host.docker.internal:host-gateway ... theo-db:audit-rem   # init: theodb + theodb_rs
# Regression (no behavior drift) + new fixes:
python3 -m pytest benchmarks/tests/test_embed_sql.py benchmarks/tests/test_embed_failure_scenarios.py \
  benchmarks/tests/test_embed_batch.py benchmarks/tests/test_hybrid_guard.py benchmarks/tests/test_retry.py \
  benchmarks/tests/test_retirement_migration.py benchmarks/tests/test_import_chunked.py -v
cargo pgrx test (in builder) ; cargo clippy ; ruff check benchmarks/
# Benchmark (CTO data):
python3 benchmarks/bench_embed_batch.py ...   # N×embed vs embed_batch(N) -> docs/benchmarks/audit-remediation-embed-batch.md
```

### Acceptance Criteria
- [ ] All new + existing embed/AI tests green vs `theo-db:audit-rem`.
- [ ] `cargo pgrx test` + clippy + ruff clean.
- [ ] Benchmark report shows the N→1 collapse with measured numbers (mean ± std over ≥ 3 runs) — produced by `python3 benchmarks/bench_embed_batch.py` into `docs/benchmarks/audit-remediation-embed-batch.md`.
- [ ] All declared failure scenarios are exercised (retry recover; fail-fast on irrecoverable; batch size-mismatch; silent-break guard) — asserted by `pytest benchmarks/tests/test_retry.py benchmarks/tests/test_embed_batch.py benchmarks/tests/test_hybrid_guard.py` (exit 0).
- [ ] Fresh image init creates both extensions cleanly (retirement-migration regression) — verified by `make -C benchmarks rebuild-and-smoke` exit 0.

### If Validation Fails
1. Separate plan-caused failures from pre-existing.
2. Fix all plan-caused failures before declaring complete.
3. Re-run the chain.
