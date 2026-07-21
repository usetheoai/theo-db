---
slug: columnar-agg-planner-hang-fix
milestone_id: M131
date: 2026-07-21
generated_by: roadmap-feature
status: completed
---

# Feature grill — M131 fix #135 (columnar-agg planner hang on wide mixed-type tables)

Answers synthesized from the codebase + issue #135 (which the assistant filed in M128) per the grill protocol's
"explore first, ask the user only for intent/preference" rule. User intent was explicit: "Abra o M131 fix #135".

## Q1 — What is this feature and why NOW?

Fix issue **#135**: the `theodb_columnar_agg` CustomScan **planner hangs (uninterruptible, during PLANNING not
execution)** on wide (105-col) mixed-type TEXT-heavy tables like the real ClickBench `hits`. Because it is in the
planner, `statement_timeout` cannot kill it — only a server restart clears the backend. Narrow/uniform columnar
tables do NOT reproduce; the trigger is the wide + TEXT-heavy schema hitting an apparent pathological loop in the
CustomScan path/cost creation.

**Why now:** M127–M130 (the official-benchmark adopt-and-wrap program) are all `[x]` / ROADMAP_COMPLETED. #135 is
**the single blocker to a defensible ClickBench columnar RANK** — today the columnar ClickBench run only measures the
STORAGE path (agg OFF = heap-equivalent latency); the vectorized aggregate PUSHDOWN (the actual columnar advantage)
is unusable on real wide tables. Fixing it converts "we have a ClickBench entry" into "we have columnar acceleration
worth submitting."

## Q2 — Dependencies (which milestones must be [x])

- **M128** `[x]` — the columnar ClickBench entry + the harness (`benchmarks/run_m128_clickbench.py`) that surfaced #135.
- **M100/M114/M115** `[x]` — the `theodb_columnar_agg` vectorized-aggregate CustomScan whose planner path is being fixed.

All satisfied (ROADMAP is fully `[x]`).

## Q3 — Definition of Done (verifiable)

1. `EXPLAIN SELECT UserID, COUNT(*) FROM hits GROUP BY UserID` on the **real 105-col ClickBench `hits`** with
   `theodb.enable_columnar_agg = on` plans in **< 1 s** (no hang, no server restart) — measurement-first, the exact
   repro from #135.
2. A **planner-latency guard**: when the CustomScan cannot cost the path cheaply (width/type threshold), it bails to
   the native plan instead of hanging — a pathological plan can never hang the backend again (defense in depth).
3. A **wide-table (100+ col, TEXT-heavy) GROUP BY** regression test added to the columnar planner tests.
4. **MEASURED columnar-accelerated ClickBench**: re-run the M128 harness with `enable_columnar_agg = on`; the
   aggregation queries beat the storage-path / heap on the **same box**, with **byte-identical** result A/B vs heap
   (the correctness oracle preserved). Honest: self-hosted, not canonical hardware; UNBENCHMARKED clean-exit if the
   accelerated path still cannot run.

## Q4 — Top 2 NEW risks

1. **The pathological loop may be deep in the DataFusion/planner path interaction** (not a simple O(cols²) loop) →
   harder to isolate. Mitigation: **measurement-first profiling spike** of the `plan_custom_path`/cost hook on a wide
   mixed-type table BEFORE committing an approach (discover-first).
2. **The planner-latency guard could over-trigger** and silently disable acceleration on legitimately-wide tables →
   losing the columnar benefit we are trying to unlock. Mitigation: make the guard threshold **measured/tunable** and
   assert acceleration STILL engages on the real `hits` after the fix (customscan=1, not a silent native fallback).

## Prior art (from #135)

- `theodb_rs/src/am/columnar_agg.rs` (the CustomScan planner hook — `plan_custom_path`/cost creation).
- `benchmarks/run_m128_clickbench.py` (the harness that surfaced it; `SET theodb.enable_columnar_agg = off` is the
  current workaround).
- PostgreSQL `setrefs.c::set_customscan_references`; Citus / TimescaleDB wide-table CustomScan cost patterns.
- Related: M115 composability verdict (`docs/benchmarks/columnar-groupby-verdict.md`).

## SOTA delta

None — existing references + the codebase cover the CustomScan-planner-cost domain. No new peers cloned.
