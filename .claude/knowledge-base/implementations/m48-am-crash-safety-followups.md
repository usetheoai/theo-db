# M48 followups (halt-loop log)

## T2.1 divergences (logged per SEPA, plan NOT edited)
- **Format version:** plan D2 says "meta v2 (v1→v2)"; reality is IVF structured **v2→v3** (new `gen_base`
  field) and HNSW stays relocatable with NO format bump (elem_first/nbr_first already are pointers). The
  CHANGELOG describes the reality (v3/gen_base). The plan's DoD grep-oracle `grep "meta v2"` is stale — the
  honest entry says "format v3"; not gaming the literal grep (SEPA MAJOR).
- **EC-1 / Coverage #8 OPEN (followup, NOT ticked):** the v2→v3 auto-migrate READ half is implemented +
  covered by construction (read_ivf_meta accepts v2 with implicit gen_base=1; fold always writes v3). The
  declared `test_fold_auto_migrates_v1_index` full round-trip needs a CROSS-BINARY fixture (old binary
  creates a v2 on-disk index, new binary reads+folds→v3) — the new binary writes v3 from creation, so it
  cannot produce a v2 fixture in-process. Followup: a cross-binary integration test (or a hand-crafted v2
  page fixture Rust unit). Honest gap, left open — not faked GREEN.
- **Legacy M26 blob VACUUM path:** still uses in-place `rewrite_blob` (not routed through the crash-safe
  fold). Blob indexes are pre-M31 (no current index uses blob; new HNSW/IVF are structured). REINDEX is the
  documented upgrade. Followup: route blob→fold OR document as REINDEX-gated. #47 is closed for all
  CURRENTLY-created formats (structured HNSW + structured IVF).
- **page.rs size budget:** page.rs is 876 LoC — over both the architecture.md 500 budget (pre-existing:
  it was already 829 pre-M48) and the plan's optimistic 850 figure. T2.1 net delta is +47 (v3 gen_base
  handling + pivot_meta_page + ivf_structured_items, minus the removed rewrite_ivf_structured). This is
  pre-existing god-module debt (SRP), not introduced here. Followup: split page.rs (e.g. extract the IVF
  structured layout into am/ivf_page.rs, mirroring am/hnsw_page.rs). Not gamed to fit an arbitrary number.
- **Plan Files-to-edit omission (T2.1):** the fold-preserves/empty-corpus pytest tests are T2.1's RED/GREEN
  evidence but the plan's T2.1 Files-to-edit didn't name a test file (test_am_maintenance.py is listed under
  T3.1). They are maintenance/correctness tests (not crash → not test_am_crash.py; need a real VACUUM → not
  pure Rust), so test_am_maintenance.py is the correct home. TDD (test ships with code) overrides the
  file-ownership heuristic. Committed with T2.1; test_am_maintenance.py grows with T3.1/T5.1 later.
- **AC not ticked (honest):** page.rs ≤ 850 (876, over — followup split) and Coverage #8 EC-1 auto-migrate
  round-trip (cross-binary followup) are LEFT OPEN, not ticked GREEN.

## T2.2 (SEPA-flagged, honest)
- **"reclaim crash-safe" NOT ticked** — T2.2's RED (size-stability fold2<=fold1) proves REUSE, not the
  fail-loud-on-crash. The crash-mid-reclaim proof is T2.3 (crash-injection). Explicit dependency T2.2→T2.3.
- **ADR 0014 written** — names the FSM→contiguous-region swap + the M55 residual window (SEPA required this
  in docs/adr/, not just followups.md; /review cross-validation will cite it).
- **page.rs 894 LoC** — grew +18 (ivf_gen_base) on top of the pre-existing 876; split still the followup remedy.

## T2.3 (crash-injection gate — findings)
- **#47 core (silent corruption) CLOSED** — proven by 3 crash-injection points; every point is fail-loud
  (consistent OR typed REINDEX), never silently wrong. The crash tests CAUGHT that the pre-pivot-extend and
  post-pivot crashes leave orphan/un-reclaimed pages in the OLD/NEW pending range; read_pending now validates
  exact item length and fails loud (REINDEX). REINDEX heals; a plain re-VACUUM does NOT (it reads the same
  polluted pending). Full clean-on-crash-without-REINDEX is M55 (ADR 0014, both windows documented).
- **Suset NOT enforced by pgrx 0.16.1 for custom GUCs** — a NOSUPERUSER role can SET theodb.test_crash_*.
  Load-bearing safety is default=0 + the hook only aborts the CALLER's own backend (no privilege escalation:
  a non-super can already crash their own session). The superuser-only test was intentionally NOT shipped
  (would fake-pass). Followup: an explicit is_superuser() guard in the hook if defense-in-depth is wanted.
- **wait_ready hardened** — requires 2 consecutive stable query round-trips (an abort() restarts the whole
  postmaster in place; a single connect can race the tail of crash recovery). Makes the crash suite robust
  back-to-back.

## T3.1 (pending fold — findings)
- **PG14+ skips index cleanup on an insert-only VACUUM** — so amvacuumcleanup (and thus the pending fold)
  does NOT run on a plain `VACUUM tbl` with zero dead tuples. The fold triggers on `VACUUM (INDEX_CLEANUP ON)`
  OR when dead tuples are present (normal autovacuum). This is correct PG behaviour, documented in the
  CHANGELOG as a usage note (not a bug). Followup: verify insert-threshold autovacuum
  (autovacuum_vacuum_insert_threshold) triggers index cleanup in practice (it may also skip it).
- **pending fold requires INDEX_CLEANUP** — the tests use `VACUUM (INDEX_CLEANUP ON)`; the runtime-metric
  proof (pending_pages N→0) is the wiring pillar (c) observable, logged on every scan (including 0, so a
  folded index is not mis-read from a stale log line).
- **amvacuumcleanup is fail-safe** — pending_page_count swallows an unreadable-meta Err to 0 (skip), so a
  routine VACUUM never aborts (test_vacuum_cleanup_never_errors_across_states).

## T4.1 (cancelabilidade — findings)
- **Purity AC nuance:** the plan's `grep -c pg_sys ann/hnsw.rs == 0` counts the `#[pgrx::pg_test]` TEST
  attributes (pre-existing, lines 386-457) — those are test-harness, not production pg_sys. The honest,
  enforceable gate is `ann/hnsw_parallel.rs` production code == 0 pg_sys (verified; the 1 grep match is a
  DOC COMMENT). ann/hnsw.rs production code (build/build_cancellable) stays pure — the seam is a callback.
- **Build restructured to batched scopes** — the M44 build was one thread::scope over all N nodes; T4.1
  batches it (4096/batch) so check_interrupt runs between batches with all workers JOINED (longjmp-safe).
  This is the D4 design (the plan's pseudo showed batched), not a plan defect. Recall unaffected (within-
  batch races preserved, covered by the recall gate).
- **Cancel test needs a >3s build** — 200k×16 builds in <3s on the quiet dev box (EC-8 skip); bumped to
  500k×32 (~6s) so the cancel is actually exercised (30s test, passed), not skipped.
- **closure body must be a block** — `&|| pgrx::check_for_interrupts!()` failed ("attributes on expressions
  experimental"); `&|| { ...; }` (statement position) compiles.
- **D4 `vacuum_delay_point()` per fold page (SEPA pre-commit MAJOR — was missing):** the seam made the
  parallel BUILD cancellable, but the fold's page-write loop had no per-page interrupt/throttle point, so a
  VACUUM of a huge index only responded to cancel at rebuild-batch boundaries. Added `pg_sys::vacuum_delay_point()`
  per body page in `fold::fold` (am/fold.rs). Safe because every `fold` caller reaches it via `vacuum_rebuild`,
  called ONLY from `ambulkdelete`/`amvacuumcleanup` (verified) — always a VACUUM context. This also applies the
  cost-based delay (VACUUM I/O throttle) for free. Closes EC-4 (cancel-mid-fold) responsiveness.
- **`cargo bench --no-run` (AC) — honestly NOT run in this env:** no PGRX_HOME on the host and the Docker build
  does not compile benches. The T4.1 change is to `build_parallel`'s signature; the FU-1 bench (`benches/scan_hot_path.rs`)
  links `scan_core` (not `build_parallel`), so it is unaffected by this change. Marked as an honest gap — a
  bench-link check belongs in CI with the pgrx toolchain, not faked green here.

## VALIDATION CORRECTION (stale-container discovery — honesty, Rule 3)
- **Root cause:** `test_am_maintenance.py` / `test_am_crash.py` resolve the DB via `PGPORT`/`PGPASSWORD` env
  (defaults 55448/`theodb`), NOT `THEODB_TEST_DSN`. Earlier this session I exported `THEODB_TEST_DSN` (ignored),
  so every pytest run silently hit a stale `theodb-m48-verify` container (OLD image, port 55448) — the prior
  "42 passed" evidence did NOT exercise the current code. Fixed by pointing `PGHOST/PGPORT/PGUSER/PGPASSWORD`
  at a fresh container built from the current image and removing all stale containers.
- **Re-validated against the CURRENT code (image theodb:m48-t41v, `vacuum_delay_point` included):**
  test_am_maintenance 12 passed / 0 skipped (with a THEODB_SCAN_PROFILE container → the 4 pending_pages metric
  tests run, not skip); test_am_crash 10 passed; regression test_index_am 8, test_hnsw_structured 6,
  test_reloption 5, test_index_am_latency 2, test_ann_index 26 = 47. Total **69 passed, 0 failed, 0 skipped**.
- **`pending_pages` runtime metric (wiring pillar c) real evidence:** container log shows `pending_pages=`
  102, 23, 6, 2, 1 (non-zero, pending region present) then 0 (after the fold eliminates it) — observed, not asserted-in-vacuum.
- **VACUUM fold WAL volume (M55 input, real):** one fold of a 400-row/dim-8 HNSW index = 26 WAL records,
  13 full-page images, 86569 bytes (VACUUM VERBOSE), and grows the relation 122880→212992 (15→26 pages,
  shadow-write not in-place — the #47 structural oracle).

## Pre-existing test bug fixed (T2.1/T2.2/T3.1 fold tests never exercised the fold)
- **`test_fold_preserves` / `test_fold_reclaims_pages` / `test_fold_empty_corpus` used plain `VACUUM {table}`**
  which hits PG's index-vacuum bypass (`vacuumlazy.c` BYPASS_THRESHOLD_PAGES) on the tiny (<32-page) fixtures →
  `ambulkdelete` never ran → the fold never ran → the `size_post > size_pre` oracle silently failed on ALL
  images (t2/t22/t31/t41), i.e. these were committed RED-never-GREEN. Fixed: they now force `VACUUM
  (INDEX_CLEANUP ON) {table}` (same mechanism T3.1 already used for pending). A real large table triggers
  `ambulkdelete` without the flag; the flag only forces the path deterministically on the small fixture.
  The EC-3 safe-path test (`test_vacuum_cleanup_never_errors_across_states`) KEEPS plain `VACUUM` on purpose —
  it proves a routine VACUUM never aborts (folds via the amvacuumcleanup pending path, which is flag-independent).
