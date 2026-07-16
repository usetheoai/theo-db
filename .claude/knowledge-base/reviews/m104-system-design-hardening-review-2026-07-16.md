---
slug: m104-system-design-hardening
milestone_id: M104
date: 2026-07-16
cycle: review
---

# /review — M104 System-Design Hardening

**Verdict:** READY_TO_MERGE

Scope: commits `c96e433^..HEAD` on develop (11 commits, 38 files, +2877/−6960). Two independent specialist agents (storage/AM + architecture, and security/resilience) reviewed the M104 arc adversarially against the current source. Complemented by a fresh Staff-level `/loop-system-design` re-audit (**4.91/5**, the DoD acceptance gate) and per-phase mini-reviews during implementation.

## Severity matrix

| Severity | Count | Status |
|---|---|---|
| BLOCKER | 0 | — |
| HIGH | 1 | **FIXED** (76cb882) |
| MEDIUM | 1 | **FIXED** (76cb882) |
| LOW | 2 | accepted / documented |
| INFO | many | verified-correct |

## Findings

### HIGH — M104 GUCs advertised as configurable but never registered → FIXED

The five bounded-memory/resilience knobs (`theodb.vacuum_fold_max_mb`, `arrow_cache_max_entries`, `vectorizer_dead_letter_max`, `http_breaker_open_ms`, `ai_max_batch`) were read via `current_setting('theodb.…', true)` but never registered with `GucRegistry`, so `SET` was silently ignored and the code always used the compiled default. Defaults were safe (no OOM/correctness regression), but the "configurable bound" headline was inoperable.

**Fix (76cb882):** registered all five via `GucRegistry::define_int_guc` (Userset) in `am/guc.rs::init` with typed `GucSetting<i32>` statics + accessors; the five read sites now call the accessors. Proven by `m104_dead_letter_max_guc_is_registered_and_settable` (SET → current_setting round-trips '42').

### MEDIUM — `_vectorizer_*` internals default to PUBLIC EXECUTE → FIXED

The `theodb_rs` schema is not blanket-REVOKE'd, so the `#[pg_extern]` `_vectorizer_*` helpers (claim/mark/process/reap + the new dead-letter purge) defaulted to PUBLIC EXECUTE — a deviation from the codebase's 79-REVOKE per-function least-privilege convention. Materially mitigated (SECURITY INVOKER + no table GRANT → a non-privileged caller's DELETE fails on a default install), but the PR was the right moment to close it.

**Fix (76cb882):** dynamic `REVOKE ALL ... FROM PUBLIC` (via `::regprocedure`, `proname ~ '^_vectorizer_'`) covering the whole family (existing + new). Proven by `m104_vectorizer_internals_revoked_from_public` (`has_function_privilege('public', …)` = false for purge AND reap_orphans).

### Verified-correct (INFO) — the four storage + four security focus areas

- **Columnar bounded write** — incremental flush reuses the M99-proven atomic `flush_pending`; pages-durable-then-catalog-LAST holds per stripe → no torn/visible partial stripe on crash; pending-bytes accounting resets correctly; self-referential INSERT snapshot-safe.
- **page.rs split** — byte-identical modulo the one intended `am::aq`→`vec::aq` rename; facade completeness proven statically (every `page::SYMBOL` caller resolves); no visibility/semantic change.
- **VACUUM fold guard** — skip defers SPACE only, never correctness (the modern v4–v7 formats already no-op the fold; heap-side MVCC visibility check drops dead TIDs independent of the fold).
- **AqQuantizer relocation** — layering inversion genuinely fixed (vec imports 0 from am; zero residual `crate::am::aq`).
- **Circuit breaker** — SSRF posture intact (scheme guard upstream of the breaker; `with_max_redirects(0)` unconditional; api-key-in-header; fail-closed 38000); thread_local per-backend, no cross-tenant leak; attacker cannot toggle to bypass a check.
- **Coalescing** — no SQL injection (parameterized, `%L`-quoted pkcol); DELETE supersedes pending upsert (last-op-wins); `enqueued_at` preserved → no starvation.
- **Batch chunking** — no injection via system-prompt closure; stdlib `chunks` bounds correct (final partial chunk kept); fail-closes on wrong model count.
- **Dead-letter purge** — parameterized, `keep.max(0)` guards negative LIMIT.

### LOW (accepted)

- `amvacuumcleanup` over-reports reclaimed pages when the fold is skipped (VACUUM VERBOSE / pg_stat inaccuracy only; query correctness unaffected). Documented; not blocking.
- Prompt-injection into the `role:user` batch message is the inherent, pre-existing NL→LLM limitation (system/user role isolation maintained); out of scope for this PR (not a regression).

## Hard gates (cycle-review)

- ✅ Tests green on working branch — 321/321 pg_tests GREEN.
- ✅ No new secrets committed.
- ✅ No direct commit to main.
- ✅ No Co-Authored-By trailer.
- ✅ CHANGELOG updated for all production source changes.

## Conclusion

No BLOCKER; the sole HIGH and the sole MEDIUM are FIXED and proven with new tests. The M104 DoD acceptance gate (`/loop-system-design` ≥4.9) is met at **4.91/5** with the residual sub-5.0 caps documented as deliberate bounded designs (ADR-0047). **READY_TO_MERGE.**
