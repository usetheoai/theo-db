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

## T5.1 (amcostestimate honesto — findings)
- **New module `am/cost.rs` (SRP):** pure `ivf_visit_ratio`/`hnsw_visit_ratio` + pure fallback dispatch
  `ratio_for` + the fail-safe I/O wrapper `scan_visit_ratio`. `amcostestimate` (mod.rs) now calls
  `genericcostestimate` then `startup = total * ratio`. Verified honest via EXPLAIN: N=100 → Seq Scan+Sort
  (cost 3.91, no index); N=50k → Index Scan (Limit 127.00..127.94, ratio≈0.013). The old stub returned cost 0.
- **NOT "pgvector verbatim" (SEPA honesty note):** only the load-bearing `startup = total * ratio` core is
  ported from pgvector's `hnsw.c`/`ivfflat.c`. pgvector's secondary corrections — IVF `sequentialRatio` page
  adjust and the HNSW/IVF TOAST `startupPages` refinement — are OMITTED. Both favor the index; omitting them
  biases conservatively toward seqscan, which does NOT break the AC (N=100 wants seqscan; N=50k is far above
  the boundary so the index still wins). The 0.55 HNSW scaling constant IS ported. Constant tuning is out of
  scope (D5). The CHANGELOG/comments say "modelo de custo do pgvector", not "verbatim".
- **HNSW `m` from the fixed build const, not the meta (SEPA [MAJOR] fail-safe):** since HNSW `m` has no
  reloption (fixed `HNSW_M=16`), the cost path uses the const directly — avoiding a second meta read that
  could `Err` under a concurrent fold. EVERY meta read in `scan_visit_ratio` is fail-safe: `peek_magic().ok()`
  → `None` → 1.0; a torn IVF meta → `read_ivf_meta().map(dir.len).unwrap_or(0)` → `lists==0` → 1.0. EC-3
  contract (NEVER error — a costestimate error aborts ALL query planning) held.
- **EC-3 proof = 6 Rust `#[cfg(test)]` unit tests (ran in the builder: `cargo test --lib am::cost` → 6 passed):**
  the fallback matrix (`ratio_for(None|garbage|lists=0|tuples=0)` → 1.0, no panic), the AM dispatch, the
  pgvector shrink-with-N property, and the clamps. Plus the pytest `test_costestimate_never_errors_on_empty_index`
  (tuples==0 end-to-end). The Docker release build does NOT compile `#[cfg(test)]`, so these were compiled+run
  separately in the `theodb-rs-builder` stage — real evidence, not assumed.
- **No test migration needed:** the plan flagged migrating small-N EXPLAIN asserts in `test_index_am.py`, but
  those tests (8 passed) do not assert planner index-choice on toy tables, so the honest cost did not break them.
- **GREEN evidence (image theodb:m48-t51):** planner pytest 6 passed (both AMs × 3 regimes), Rust unit 6 passed,
  full maintenance 14 passed/4 skipped, regression index_am 8 + hnsw_structured 6 + reloption 5 + ann_index 26.

## T6.1 (benchmark artifact — findings)
- **Driver `benchmarks/run_m48_maintenance.py` (reuses `theodb_bench.metrics.latency_percentiles`, Rule 9):**
  measures pending degradation + fold recovery, WAL bytes of the fold, and the planner cost regimes. Load-guard
  aborts if load1 > nproc/2 (M46 lesson); runs on a THEODB_SCAN_PROFILE container to read the runtime metrics.
- **Real data (3 runs, N=50k, dev box load 0.41):** the fold fires ONLY above `vacuum_pending_threshold` (16):
  pending 8(obs 10)/16 do NOT fold (pending unchanged, ~0 index WAL); pending 64 → 0 after fold, scan **p50 drops
  ~7× (1.20→0.16 ms, effect ≫ std 0.08)** and the fold emits **12.28 MB ± 1.6 KB of WAL** (the shadow-rewrite
  cost — explicit M55 input). Planner: N=100 → Seq Scan (index NOT used), N=50k → Index Scan.
- **Two honesty bugs caught by inspecting the first artifact (fixed before commit):**
  (1) the planner section falsely reported "N=100 uses index = True" because the measurement phase left
  `SET enable_seqscan = off` on the session, forcing the index in the later EXPLAIN — fixed with a `RESET`
  before the planner EXPLAINs (the pytest planner tests were correct; only the driver's shared session leaked).
  (2) the first table showed `pages_read` (graph-traversal, ~constant ~355) which HID the fold effect — the
  pending cost is a separate linear scan visible in p50 / `pending_pages`, so the table now shows
  `pending_pages` before→after + p50 + a "foldou?" column, with the threshold behavior explained.
- **`pages_read` is graph-only, not pending:** `pages_read` (hnsw_page.rs) counts graph pages; `pending_pages`
  (page.rs) counts the linear pending scan. The fold's benefit is in p50 + pending_pages, not pages_read — the
  artifact states this so a reader does not misread the constant pages_read as "the fold did nothing".
- **Smoke test `test_m48_driver_smoke` (N=5k/1 run):** asserts the driver runs end-to-end and emits the json
  schema (runs/pending_series/wal_bytes/load) + the md caveat — guards the artifact contract.
- **No comparative claim (public-copy.md):** dev-box characterization; the md carries the box-load caveat and
  "caracterização, não competição".

## Validation gate — `acceptance_criteria` file_size FAIL is an unactionable gate/plan mismatch (honest)
- **run_validation.py final state:** 5 PASS (progress_schema, checkpoint_consistency, wiring_triad,
  test_obligations, code_quality=PASS), 4 SKIP (npm — this is a Rust/pgrx project, no JS toolchain), 1 N/A
  (patterns), **1 FAIL: `acceptance_criteria` (file_size only)**.
- **Why it cannot go green:** the gate applies an ABSOLUTE `<= 500` LoC limit to every changed file. The
  plan's ACTUAL file-size DoD (plan line ~1000) is *"fold.rs ≤ 500; nenhum arquivo tocado cresce além do
  baseline +10% sem split"* — a baseline-relative budget the gate cannot parse (it extracts the literal `500`).
  The three flagged files:
  - **CHANGELOG.md (678):** a public append-only contract; `architecture.md`'s source-file LoC budget does NOT
    apply to it and `audit-trail-rotation.md` keeps it indefinitely. It can NEVER be ≤ 500 → the gate can NEVER
    exit 0 on this (or any mature) project. This is a gate limitation, not a code defect.
  - **hnsw_page.rs (790):** PRE-EXISTING (Baseline Context = 800); it SHRANK during M48. Within baseline+10% (880).
  - **page.rs (917):** PRE-EXISTING (Baseline Context = 829); M48 features (pivot_meta_page, read_pending
    fail-loud, pending metric) pushed it to +10.6% — 5 lines over the +10% soft cap after the comment trims
    (d4d3543). The full split into `am/ivf_page.rs` is the committed M51-adjacent followup (blueprint anti-rework
    restriction keeps it out of M48). A marginal soft-cap breach on pre-existing debt.
- **The plan's REAL file-size DoD IS met** modulo page.rs's +0.6% soft-cap overage: fold.rs 139 ≤ 500 ✓,
  cost.rs 126 ≤ 500 ✓ (both NEW), hnsw_page.rs shrank ✓. NEW files are well under budget; the failures are all
  pre-existing/exempt.
- **Handoff:** this file_size caveat is exactly what `/review` adjudicates (README/CHANGELOG/file-size are
  human-adjudicated per cycle-review's agents). Not gamed (CHANGELOG cannot be shrunk; splitting every >500
  source file is out-of-scope scope-creep and would STILL leave CHANGELOG failing). Surfaced honestly, not
  papered over (Rule 3). All FUNCTIONAL gates pass; final-image evidence: maintenance 19, crash 10, regression
  47 = 76 passed, 0 failed.
