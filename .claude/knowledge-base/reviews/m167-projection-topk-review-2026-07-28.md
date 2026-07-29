# Review: m167-projection-topk

**Date:** 2026-07-28
**Reviewers:** 3 agents in parallel — `council-rust-pgrx` (FFI/memory safety), `implement-m167-…-sepa`
(line-by-line plan↔implementation cross-validation), `council-benchmark` (measurement rigor).
**Verdict:** `NEEDS_FIXES`

Every finding below was independently re-verified by the orchestrator before being accepted. Two were **rejected**
after checking (see § Rejected). The rest are real.

## BLOCKER

### F1 — the published before/after inflates three of four ratios ~2×
- **Found by:** council-benchmark · **Re-verified:** yes
- `docs/benchmarks/m166-clickbench-agg.json` — same box, same `--n 1000000 --sample systematic --agg`, one day
  earlier, late-mat off — records q24 **3.0751 s**, q25 **2.7126 s**, q26 **2.9036 s** against the M167 "before" of
  5.9088 / 6.0517 / 5.9517. q23 agrees (22.628 vs 21.515, 0.95×).
- It is **not** a slower box: geomean m166 0.40934 vs M167-before 0.36917. Only the three narrow Sort-bound shapes
  doubled.
- **Consequence:** 42.51× / 62.07× / 41.88× are unsupported as published; against the repo's own artifact they are
  ≈22× / 28× / 20×. The 6.51× for q23 survives.
- **Action:** re-measure with a **paired same-binary A/B** (`enable_columnar_late_mat` off/on, alternating, ≥5 pairs,
  one session), which removes build/cluster/session/thermal drift by construction. Publish whatever it says.

### F2 — the ADR-4 decode guard is inert on any table that has not been ANALYZEd
- **Found by:** council-benchmark + SEPA + council-rust-pgrx (independently) · **Re-verified:** yes, empirically
- `pg_class.relpages` is written only by ANALYZE/VACUUM. Measured on a fresh 200k-row columnar table: `relpages = 0`
  immediately after load, and the guard **did not fire even at `work_mem = 64kB`**.
- council-rust-pgrx established why it is permanent, not transient: `grep -rn "pgstat" theodb_rs/src/` returns
  **zero hits**, so `changes_since_analyze` never moves and autoanalyze never fires; `relation_vacuum`
  (`columnar.rs:1851`) is an error stub. `run_m128_clickbench.py` contains no `ANALYZE`.
- **This is the same failure mode ADR-4 diagnosed for `plan_rows`, relocated** — the guard's own demonstration
  (`est_bytes=228253696`) only worked because the orchestrator had run `ANALYZE` by hand while investigating,
  without recording that as a precondition.
- **Fixed:** fall back to the relation's true current size via `RelationGetNumberOfBlocksInFork(MAIN_FORKNUM)` when
  the statistic is absent — the same source the TAM's own `columnar_relation_estimate_size` (`columnar.rs:1762-1768`)
  uses. Verified after the fix: `relpages = 0` table now declines at 64kB and routes at 1GB.

### F3 — `datlocprovider` ignored: an ICU database with `datcollate = 'C'` gets a silently wrong top-k
- **Found by:** council-rust-pgrx · **Re-verified:** yes, by creating the database
- `CREATE DATABASE d LOCALE_PROVIDER icu ICU_LOCALE 'en-US' LOCALE 'C'` stores `datcollate = 'C'` while the DEFAULT
  collation orders by ICU (`pg_locale.c` dispatches on `datlocprovider`; `dbcommands.c` writes the two fields
  independently). The M167 predicate read `datcollate` alone → returned `true` → admitted a text sort key whose
  DataFusion byte order disagrees with PG.
- **This is the exact wrong-rows class the M158 guard existed to prevent, reintroduced by the new path**, and made
  reachable with no session `SET` by the default flip.
- **Fixed:** require `datlocprovider = 'c'` (libc) before trusting `datcollate`; ICU/builtin decline. Verified: the
  ICU database now declines the text sort key.

## HIGH

### F4 — routing is evidenced by a metric the harness documents as a known false-green
- **Found by:** council-benchmark + SEPA · **Re-verified:** yes
- The verdict cites `columnar_customscan = 37`. That field is `"theodb_columnar_agg" in plan or "Custom Scan" in plan`
  (`run_m128_clickbench.py:271`) and its own docstring calls it "broad and ~always True … CANNOT tell an agg pushdown
  from a declined agg over a projection scan". Proof it is vacuous here: m166 (late-mat **off**) also reports
  `columnar_customscan = True` for q23–q26 with `columnar_agg_routed = False`.
- **Action:** quote `columnar_agg_routed` / `classify_ab` per query from both JSONs. The data is already collected;
  no new run needed.

### F5 — the verdict's results table re-conflates the two oracles its own § 2 separates
- An `A/B: identical` cell on the four top-k rows, sourced from the LIMIT-stripped storage oracle. § 2 explains the
  distinction correctly, but the table is what a downstream reader quotes.
- **Action:** relabel to `n/a (LIMIT stripped)` or delete the column on those rows.

### F6 — no oracle covers the measured relation
- `m158_ec_harness.sql` runs on `t_col`/`t_cc`/`t_dc` (20k rows, ≤6 columns); the suite A/B never exercises the top-k
  path. So the 1M-row / 105-column output that produced every published number is verified by neither.
- **Action:** add one LIMIT-preserving symmetric-EXCEPT block against `hits`/`hits_heap` with a tie-free key.

### F7 — two declared T4.1 RED tests were never written
- `test_multikey_declines_when_any_key_fails_a_guard` (bpchar as a *sort key*) and `test_multikey_over_bound_declines`
  (> `TOPK_MAX_SORT_KEYS`). Both appear in T4.1's Acceptance Criteria **and** the Global DoD. No such case exists in
  `benchmarks/`.

### F8 — multi-key × text is untested, and it is the only place the two new mechanisms intersect
- `M167-D` is `ORDER BY v, wid` (int, int); `columnar_type_ab.py`'s `topk_multikey` is `ORDER BY c2, c4` (int, int).
  ClickBench q26 is timestamp + text. The interesting defect — a per-key loop that checks key 0's collation and not
  key 1's — is not covered by any test.

### F9 — `EXPLAIN (VERBOSE)` and GUC-honored have no committed artifact
- Both are Global DoD items. EC-3 (the M131 `resolve_special_varno` recursion, uninterruptible by
  `statement_timeout`) was the reason VERBOSE was mandated. It was run interactively and reported (0 s), but no
  transcript is committed.

### F10 — no raw benchmark JSON committed
- M151…M166 all committed `docs/benchmarks/mNNN-artifacts/*.json`; `public-copy.md` § 4 requires the artifact for a
  comparative claim. Without it none of `42/43`, `columnar_customscan 37`, `diverged = 0`, `hot_geomean` is checkable.
  (It was committing the m166 artifact that let the auditor catch F1 at all.)

## MEDIUM

### F11 — the "safe direction" argument in the guard comment is inverted
- The comment claimed under-estimation "is the safe direction for a ceiling". For an OOM bound, under-estimating
  causes false **admits** — the failure the guard exists to prevent. **Fixed** in the comment.

### F12 — ADR-1 and ADR-4 partially cancel on a stock cluster
- At PostgreSQL's default `work_mem = 4MB` the budget is 32 MB, below the 228 MB `hits`, so q23–q26 decline. The
  headline "route by default, no session SET" holds on this cluster's `work_mem = 64MB`, not on a stock one. Must be
  stated in the GUC doc comment and the verdict.

### F13 — the default flip widens `swap_walk` to every planned statement
- `columnar_agg.rs:1400`: the disjunct `|| ENABLE_COLUMNAR_LATE_MAT.get()` is now always true, so the post-planning
  walk runs on every statement in every database, including those with no columnar table. Also makes
  `admit_trace`'s `std::env::var` call per-Sort-node on the default path (hoist to a `OnceLock`).

### F14 — the geomean over-attributes
- Predicted after-geomean if only q23–q26 changed: 0.26777; observed 0.25108. ~16.7% of the log-gain is a run-level
  shift across the other 38 queries, not the top-k.

### F15 — the regression dismissal uses a weaker statistic than the one it judges
- `hot` is `min-of-2` (`run_m128_clickbench.py:262`); the amplitude test used the range of 5 raw single runs, whose
  dispersion is strictly larger. The test is systematically lenient and cannot fail. The conclusion may still be
  right; the method does not establish it. A paired off/on test in one session is the correct instrument.

### F16 — T2.2's task body still describes the superseded mechanism
- ADR-4 was corrected to `relpages`; T2.2's body, REFACTOR criterion and GREEN assertion still say
  `plan_rows × plan_width`, which makes the declared GREEN unsatisfiable (the guard is per-relation, so at
  `work_mem = 4MB` q23 declines too). Updating the rationale while leaving the acceptance surface stale is the
  goalpost-fitting pattern even when the mechanism change is right.

### F17 — the halt-loop's own gates never ran
- `.progress-*.json` recorded 4 tasks as `pending` with no SHAs while six commits carried task IDs;
  `check_checkpoint_consistency` FAILed. Zero Step-4.7 phase-boundary mini reviews exist for a 5-phase plan. The
  wiring re-verification the cycle mandates never ran (the three new symbols do have real callers — verified by hand
  — so there is no fabricated evidence, but the gate that would prove it did not run).
- **Partially fixed:** the checkpoint was reconstructed from git with real SHAs.

## LOW

- **F18** — `relation_physical_bytes` released the syscache tuple before reading the datum; safe only because
  `relpages` is `attbyval`. **Fixed** (read first, release after).
- **F19** — the `AtomicU8` cache latches a *failure* permanently, converting a transient syscache miss into a
  backend-lifetime decline. Direction is safe; worth revisiting.
- **F20** — the top-k decoder walks `j` forward without bounding it against the list length, while the next read does
  bound-check. Defense-in-depth only (encoder is the sole producer, wire never crosses a binary boundary).
- **F21** — arrow-rs orders floats by IEEE `total_cmp` (negative NaN below `-inf`); PG treats all NaN as equal and
  greater than everything. Pre-existing to M158; the default flip makes it reachable without opting in.
- **F22** — `m158_ec_harness.sql` Q7's `\echo` still asserts "MUST show Sort" on a premise (en_US database) that is
  false on the C cluster M167 measured on.
- **F23** — the baseline artifact still prescribes reading a `lc_collate` GUC, which implementation proved does not
  exist in PG 18.

## Rejected after re-verification

- **"`enable_sort = off` suppresses the Sort node, so decline cases pass vacuously"** (SEPA initial brief, C3;
  repeated in plan ADR-5). **Measured false:** with `enable_sort = off` the swap still fires
  (`swap_topk_admitted`) — it is a cost penalty, not a prohibition, and the Sort node still forms. ADR-5's decision
  survives on its other two facts; the ADR text needs correcting.
- **"The columnar data lives outside the main fork, so `RelationGetNumberOfBlocksInFork` is the wrong source"**
  (orchestrator's own hypothesis mid-review). **Measured false:** the TAM's own
  `columnar_relation_estimate_size` uses `smgrnblocks(MAIN_FORKNUM)`; the 104 kB reading came from querying the
  2000-row type fixture, not ClickBench.

## Orchestrator error disclosed

Running `benchmarks/columnar_type_ab.py` against the same database **DROPped and recreated `hits`** with its own
2000-row synthetic schema, destroying the ClickBench 1M data. Two measurements taken afterwards ("q23–q26 all
decline", "relpages = 0 / 104 kB for hits") were reading the type fixture and are void. The data has been reloaded
and every affected claim must be re-measured before release.

## Handoff

`NEEDS_FIXES`. F1, F2, F3 are blockers; F2 and F3 are fixed in code and re-verified, F1 requires the paired
re-measurement. `/release` MUST NOT run until F1 is re-measured, the verdict is corrected, and F4/F5/F10 are
addressed — the published numbers are the deliverable of this milestone, and three of the four are currently wrong.
