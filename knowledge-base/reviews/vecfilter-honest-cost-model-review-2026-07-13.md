# Review — M95 honest vecfilter cost model

**Date:** 2026-07-13 · **Slug:** vecfilter-honest-cost-model · **Milestone:** M95 · **Verdict:** `READY_TO_MERGE` (fixes applied)

council-index-storage (author of the M95 discovery blueprint) audited the implementation against it. First pass
`NEEDS_FIXES` (2 HIGH); both fixed and re-verified (273 tests GREEN).

## Findings → dispositions

| # | Sev | Finding | Disposition |
|---|---|---|---|
| 1 | HIGH | `term_B = bitmapqual.total_cost` double-counts the heap-fetch run cost when `bitmapqual` is a plain `IndexPath` (the common single-predicate case) — `cost_bitmap_tree_node` uses `indextotalcost` there, not `total_cost` | **FIXED** — branch on `nodeTag`: IndexPath → `indextotalcost + 0.1·cpu_operator_cost·rows`; BitmapAnd/Or → `total_cost` (mirrors costsize.c:1113-1136, Rule 9) |
| 2 | HIGH | `read_page_item_into` calls `ReadBufferExtended(RBM_NORMAL)` with no `block < nblocks` guard (its sibling `read_page_item_at` has one) — a torn/folded meta page → C `ereport(ERROR)` longjmp → aborts ALL query planning from the planner hook, defeating the EC-3 `Option`/sentinel fail-safe | **FIXED** — added the `block >= nblocks → Err` guard to `read_page_item_into` (fixes the M48 read path too); a typed Err now degrades to the fail-safe |
| 3 | LOW | The measured honest-negative (`chosen_node=False` everywhere) is a real feature limitation and must be recorded as tracked debt with the M48-probe-aware fix as the named follow-up | **RECORDED** — in the benchmark md + a blueprint § Follow-up note |
| 4 | INFO | `rerank_pool = 64·over_fetch()` reuses the `over_fetch` GUC for an IVF node — verified correct (matches scan.rs:180), cross-AM namespacing is a readability smell only | ACCEPTED (correct; no behavior change) |

## Council rulings (verified SOUND)

- **Q4 sentinel discipline airtight** — `term_V=0` cannot leak as a real cost of 0: three layers (`effective_probes` sentinel → `eff<=0 → None` guard → `vecfilter_scan_cost` re-propagation). The model neither forces nor suppresses.
- **Q3 index_open/index_close balanced** — no `return` between the NoLock open and the unconditional close (and HIGH-2's fix removes the last longjmp that could have leaked the ref).
- **Q5 honest-negative + force-GUC is a legitimate, PRE-PREDICTED milestone outcome, not a gap** — M95's scope was "price the NODE honestly," delivered; the competitor's probe-blind pricing (M48) is a separate surface, and bundling an M48 rewrite would be scope creep. `vecfilter_force` is the direct analogue of Postgres's `enable_*` knobs. The M48-probe-aware fix is the named follow-up (finding #3).
- **Q6 planner/exec-only** — no page-format / WAL / VACUUM surface; `page.rs` change is a read-path bounds guard. No crash-safety test needed.
- `effective_probes`/`vecfilter_scan_cost` faithfully image the M91 loop (scan.rs:641); 6 unit tests cover loose/selective/clamped/degenerate/monotone/sentinel.

## Gates

- **273 tests GREEN, 0 failed** (droplet, re-run after fixes) — incl. 6 cost unit tests + `m95_loose_selectivity_not_chosen` (hack gone / no over-selection) + `m95_multi_predicate_filter_correct` (term_B else-branch + fail-safe on the planning path).
- **Measurement gate (T3.1):** SIFT1M sweep artifact `docs/benchmarks/m95-cost-model.{md,json,log}` — the honest-negative on auto-selection (R4, measured) + the node's quantified value (POST recall 0.55-0.67 vs forced INLINE 0.88-0.95 across 1-25%, 13× QPS at 1%). Honest — no QPS-superiority claim vs ScaNN/AlloyDB (M73/M82 ceiling stands).
- plan-confidence SHIPPABLE 95.6. No page-format change; GUC-off byte-identical. No commits to main; no Co-Authored-By; CHANGELOG updated.

## Verdict

`READY_TO_MERGE`. The forced-selection hack is gone (the node no longer hijacks any query — the real harm closed); the node is priced honestly; both HIGH findings fixed and re-verified; the auto-selection honest-negative is measured, explained (R4), and resolved by the explicit `vecfilter_force` override with the M48-probe-aware fix documented as the follow-up. Proceed to `/release`.
