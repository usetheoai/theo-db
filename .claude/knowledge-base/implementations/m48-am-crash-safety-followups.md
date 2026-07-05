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
