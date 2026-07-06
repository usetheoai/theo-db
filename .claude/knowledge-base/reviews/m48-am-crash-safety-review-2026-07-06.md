# /review — M48 AM crash-safety & durability

Date: 2026-07-06 · Slug: `m48-am-crash-safety` · Range: `5286b44..HEAD` (develop)
Reviewers: 4 domain specialists (index-storage, rust-pgrx, benchmark, security)

## Verdict: READY_TO_MERGE (after review-fix commit `3dffcd2`)

3 reviewers returned READY_TO_MERGE; council-security returned NEEDS_FIXES (1 HIGH needs-repro, 1 MEDIUM
honesty, 1 LOW-MED). All security findings resolved: the HIGH was **refuted by repro**, the MEDIUM and LOW-MED
were **fixed**. No BLOCKER; no remaining HIGH.

## Severity matrix

| Reviewer | Verdict | BLOCKER | HIGH | MEDIUM | LOW/INFO |
|---|---|---|---|---|---|
| council-index-storage (crash-safety) | READY_TO_MERGE | 0 | 0 | 1 (fixed) | 1 |
| council-rust-pgrx (FFI/longjmp/unsafe) | READY_TO_MERGE | 0 | 0 | 0 | 4 |
| council-benchmark (honesty/rigor) | READY_TO_MERGE | 0 | 0 | 1 (fixed) | 2 |
| council-security (attack surface) | NEEDS_FIXES → resolved | 0 | 1 (refuted) | 1 (fixed) | 1 (fixed) |

## Findings + resolutions

### council-index-storage — READY_TO_MERGE
- Validated all 5 load-bearing crash-safety questions HOLD: meta-pivot atomicity (single full-image record,
  no half-written-block-0 window); `free_region`/`cur_gen_start` never overwrite live data (HNSW `elem_first=base`,
  `nbr_first=base+elem_npages` so `[1,elem_first)` is genuinely dead — property-tested); `read_pending` exact-length
  fail-loud; v2→v3 version-gated migration; `vacuum_delay_point` called with no buffer held. **#47 silent-corruption
  elimination claim HOLDS. #46 solid + tested.**
- **[MEDIUM — FIXED]** A cancelled (not only crashed) tail-fold leaves orphan pages → fail-loud REINDEX; the
  CHANGELOG/ADR framed it as a crash-only window. → Added the honest cancel caveat to the CHANGELOG cancellability
  entry + `fold.rs` `vacuum_delay_point` docstring (commit `3dffcd2`).
- **[LOW — noted]** Empty-prior-generation crash path can surface duplicate (live) tids (not garbage) — untested edge.
  Tracked as a follow-up crash test (not corruption; not a blocker).

### council-rust-pgrx — READY_TO_MERGE
- **Verified sound:** the T4.1 longjmp-safety crux — `check_interrupt()` runs at `hnsw_parallel.rs:69`, AFTER
  `thread::scope` returns (all workers joined), so `ereport(ERROR)→longjmp` can never cross a live worker (structural,
  not probabilistic). Workers are pure Rust (0 `pg_sys`). `amcostestimate` NoLock open/close balanced (no refcount
  leak); `std::mem::zeroed::<GenericCosts>()` sound (POD); `scan_visit_ratio` genuinely fail-safe. `pivot_meta_page`
  buffer lifecycle leak-free; superuser gate on crash hooks correct.
- **[LOW/INFO — noted]** manual buffer RAII in write paths (idiomatic, matches pgvectorscale); `Sync` bound
  over-constraining on check_interrupt (harmless); always-compiled fault-injection (see security). Follow-ups.

### council-benchmark — READY_TO_MERGE
- Confirmed both pre-commit honesty fixes are in the committed code (RESET enable_seqscan; pending_pages headline).
  Data honest, no cherry-picking (shows non-firing 0/8/16 rows), no data degeneracy (correlated subquery, not
  random()-hoist), effect (~7×) ≫ variance, framing respects public-copy.md (no comparative claim).
- **[MEDIUM-LOW — FIXED]** Under-disclosed hardware/version/SHA → added `## Ambiente` (i7-1355U, PG 17.10, git SHA)
  + WAL-is-cluster-wide caveat (commit `4a733ac`).
- **[LOW/INFO — noted]** "~355 pages_read" loose; smoke-test lowercase coupling. Cosmetic.

### council-security — NEEDS_FIXES → RESOLVED
- **[HIGH autovacuum DoS — REFUTED by repro 2026-07-06].** Claim: a non-super persists the crash GUC where an
  autovacuum superuser session reads it → instance abort(). Repro on the real instance: `ALTER DATABASE ... SET`
  and `ALTER ROLE ... SET` of the Suset GUC by a NOSUPERUSER BOTH fail `permission denied to set parameter` — PG
  enforces Suset for persisted settings. No non-super → autovacuum path exists. The in-session SET only hits the
  attacker's own backend (guarded). Refuted, not a live hole.
- **[MEDIUM honesty — FIXED].** The stale NOTE claiming the superuser guard was "not shipped / a followup"
  contradicted the shipped `guc.rs` guard + shipped test. Rewrote to match reality (commit `3dffcd2`).
- **[LOW-MED test rigor — FIXED].** Added a superuser positive control to `test_crash_hook_is_superuser_gated`
  (proves the hook is reachable so the gate proof is not vacuous). Crash suite 10 passed.
- **[Follow-up, not a blocker].** Move the fault-injection hooks behind a `crash_injection` cargo feature
  (fail-closed by construction) — defence-in-depth since the exploit is refuted.

## Hard gates (cycle-review)
- Failing tests on branch: NONE (76 passed on the final image + 10 crash re-run with the positive control).
- New secrets: none. Direct commit to main: none (all on `develop`). Co-Authored-By trailer: none.
- CHANGELOG updated: yes (every phase).

## Evidence
- Tests: 76 passed / 0 failed (maintenance 19, crash 10, regression 47) + 6 Rust unit (am::cost). Crash suite
  re-run with the positive control: 10 passed.
- Benchmark: `docs/benchmarks/m48-am-maintenance.{md,json}` (3 runs, i7-1355U, PG 17.10, git 3dffcd2).
- Security repro: ALTER DATABASE/ROLE SET permission-denied for NOSUPERUSER (HIGH refuted).

## Open items (follow-ups, none blocking)
1. Compile-time `crash_injection` cargo feature (defence-in-depth).
2. Empty-prior-generation crash test (duplicate-tid edge).
3. `page.rs` split into `am/ivf_page.rs` (M51-adjacent; file-size soft-cap).
4. Cross-binary v2→v3 auto-migrate integration test.
5. Full cancel/crash-without-REINDEX fold (M55, ADR 0014).
