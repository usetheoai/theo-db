# Review — durability crash-recovery proofs (#46 / #47)

**Date:** 2026-07-16
**Verdict:** READY_TO_MERGE
**Scope:** off-roadmap durability verification (no milestone_id — ad-hoc)
**Commits:** 2d5a1de (proofs) + 486c2bf (F1/F2 hardening)

## Scope reviewed

The M48 fixes for #46 (UNLOGGED INIT fork WAL-logging, `am/build.rs::wal_log_init_fork`) and #47 (crash-safe VACUUM
meta-pivot fold, `am/fold.rs`) already existed in code but were `[NEEDS-REPRO]` — never proven under a real crash
(ADR-0014 flagged the proof as pending). This work builds the end-to-end crash-recovery proofs. Files:
`theodb_rs/isolation/crash_fold.sh` (#47), `theodb_rs/isolation/crash_unlogged.sh` (#46),
`theodb_rs/isolation/Makefile` (`check-crash`), `theodb_rs/src/vectorizer.rs` (rustc≥1.85 forward-compat),
`docs/adr/0014-m48-crash-safe-fold-reclaim-mechanism.md`.

## Measured evidence (droplet pg17 / pgrx 0.19, real clusters)

- **#47 (`crash_fold.sh`):** **3 real SIGABRT crashes** confirmed in the Postgres log (non-vacuous guard: ≥3
  aggregate + **exactly +1 per phase**), one per fold crash point (after-body-page / post-pivot / mid-reclaim, the
  last forced via a shrink DELETE so the fold reuses the low region and the reclaim loop runs). MEASURED: crash
  BEFORE the pivot ⇒ old generation correct (post-crash index == fresh REINDEX over the same data); crash AFTER the
  pivot / mid-reclaim ⇒ **fail-loud typed REINDEX error, never silently wrong** (the #47 worst case). `CRASH_FOLD_OK`.
- **#46 (`crash_unlogged.sh`):** standby-promotion RED/GREEN toggle — with `wal_log_init_fork` commented out, the
  promoted standby's UNLOGGED index is broken (`aminsert before build`); with it restored, INSERT + scan work
  (`SCAN_COUNT=1`). Proves the fix is load-bearing and the test detects the bug. `CRASH_UNLOGGED_OK`.
- **312 pg_tests GREEN**, zero regression (the `#[unsafe(no_mangle)]` forward-compat change).

## Specialist sign-off

| Reviewer | Domain | Verdict | Blockers |
|---|---|---|---|
| council-index-storage | AM durability / WAL / crash-recovery / relation lifecycle | READY_TO_MERGE | none |

**council-index-storage** audited both proofs end-to-end against the real code paths and confirmed they are
**rigorous and non-vacuous**: (1) the crash only fires inside `fold()` reachable only via `amvacuumcleanup` when
`pending > threshold` (set to 1), so a `signal 6` in the log proves the meta-pivot ran — the exact property earlier
vacuous iterations lacked (wrong opclass → seqscan; multi-statement `-c` → VACUUM in a txn block); (2) comparing the
post-crash answer to a fresh REINDEX correctly encodes "never silently wrong" (robust to data mutation + IVF
approximation); (3) the shrink-forced mid-reclaim is the only legitimate way to reach `CRASH_PHASE_MID_RECLAIM`;
(4) standby promotion is the correct #46 test (a single-node restart keeps OS cache and can mask the missing WAL),
and the RED/GREEN toggle is load-bearing (differential proof); (5) no overclaim — the guarantee is honestly stated as
"consistent OR fail-loud REINDEX, never silently wrong", with the reclaim window deferred to M55.

Two non-blocking hardenings recommended and **applied inline** (commit 486c2bf): F1 (per-phase +1 crash-count guard,
closing the theoretical 2+1+0 aggregate loophole) and F2 (ADR note that torn-pivot-page safety rests on
`GENERIC_XLOG_FULL_IMAGE` + `full_page_writes=on`, not an injected mid-pivot-record crash).

## Outcome

Issues #46 and #47 verified with measured crash-recovery evidence and closed on the tracker. ADR-0014's "Prova
pendente" is FECHADA. The M48 crash-safety guarantee — **never silently wrong at any crash point** — is now proven,
not just designed. No production code changed except a one-line rustc-forward-compat fix.
