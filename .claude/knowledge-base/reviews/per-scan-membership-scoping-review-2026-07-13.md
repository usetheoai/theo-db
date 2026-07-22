# Review — M94 per-scan membership scoping

**Date:** 2026-07-13 · **Slug:** per-scan-membership-scoping · **Milestone:** M94 · **Verdict:** `READY_TO_MERGE` (fixes applied)

council-rust-pgrx (the owning council — this resolves ITS B1 BLOCKER from the M92/M93 review) reviewed the M94 diff
against the real PG17 source (`nodeIndexscan.c`, `nodeBitmapIndexscan.c`, `execAmi.c`, `nodeCustom.c`).

## Verdict headline

**"B1 status: RESOLVED. The M92/M93 BLOCKER is genuinely fixed, not papered over."** The pull-window claim was
verified airtight against core: the lazy `index_rescan` runs INSIDE `IndexNext` (inside our exec window); no core
path pulls the vector child behind our back (`custom_ps` is only walked by EXPLAIN/tree-walkers); EPQ builds a fresh
node through our own callbacks; parallel is off (DSM callbacks None). Node-pointer registry keys cannot be wrongly
reused (begin's `registry_insert` replaces before any exec can read). Re-entrancy (SubPlan-in-Filter) traced sound.
No RefCell borrow crosses FFI.

## Findings → dispositions (all actionable items FIXED in commit `0f29c7c`)

| Finding | Sev | Disposition |
|---|---|---|
| Swap-restore not unwind-safe (relied on abort callbacks under longjmp) | MEDIUM-1 | **FIXED** — `ActiveGuard` RAII restores via `Drop` even when the pull unwinds; callbacks demoted to belt-and-braces |
| **TIDBitmap leaked per begin/rescan** — `MultiExecBitmapIndexScan` creates a FRESH bitmap per call and `ExecEndBitmapIndexScan` does NOT free it; the old "ExecEndNode frees it" comment was factually wrong; N rescans leaked N work_mem-sized bitmaps | MEDIUM-2 | **FIXED** — `tbm_free` immediately after `materialize_bitmap` in begin AND rescan (copy-out-before-release); comment corrected |
| `XACT_EVENT_PREPARE` missing from the clear list | LOW-2 | **FIXED** — added |
| Rescan lacked the exec-side fail-loud symmetry | LOW-3 | **FIXED** — added |
| Subxact-orphaned registry entries accumulate until xact end | LOW-1 | ACCEPTED (documented design; never a correctness issue; subxact-id tagging is a future option) |
| Hook can wrap a non-vector ordered btree query (plan-quality, results stay correct) | LOW-4 | ACCEPTED (pre-existing; the honest cost model is the declared follow-up) |
| Untested: LATERAL-varying bitmap qual; nested vecfilter | LOW-5 | HARDENED — the hook now requires **unparameterized** children (a parameterized LATERAL bitmap would break our `param_info = NULL` contract); such queries fall back to native plans (correct, documented boundary). Nested vecfilter traced sound by the council (Q4); test is a future nicety |

## Gates

- **265 tests GREEN, 0 failed** (droplet `cargo pgrx test pg17`, re-run AFTER the fixes) — incl. the 3 new M94 tests:
  `m94_union_two_filtered_scans_correct` (the exact B1 scenario: 2 nodes asserted in EXPLAIN, result == union of
  exact seqscans), `m94_rescan_reuses_membership_correct`, `m94_subxact_abort_clears_membership`.
- **Benchmark spot-check (plan Final Phase):** the M92 harness re-run on the M94 build — **recall byte-identical at
  every point** (1%: 0.953/0.969/0.968/0.969, POST 0.673; 5%: 0.915/0.910/0.910/0.912, POST 0.593 — exactly the
  v0.80.1 numbers), deterministic proof the swap-discipline does not perturb the scan. QPS uniformly ~35% lower on
  BOTH arms (POST included, which never touches the node) = host variance — a different droplet CPU (Xeon Platinum
  8168 vs the v0.80.1 run's Xeon Gold 6548N; the M83 lesson). The INLINE/POST ratios hold (12.8×, 1.57×). Raw:
  `docs/benchmarks/m94-spotcheck.log`.
- plan-confidence: SHIPPABLE 100.0. No page-format change; `scan.rs` untouched; GUC-off byte-identical.
- No commits to `main`; no `Co-Authored-By`; CHANGELOG updated.

## Verdict

`READY_TO_MERGE`. The capability the M93 fail-loud guard refused — filtered `UNION`/self-join/`Append` — now works,
each node scoped to its own membership, with the leak and unwind-safety findings fixed and re-verified. Proceed to
`/release`.
