# M95 blueprint — honest cost model for the vecfilter node (council-index-storage)

Code-grounded discovery (council-index-storage, verified against PG17 `costsize.c` + our `cost.rs`).

## Verdict
Honest cost IS computable at hook time. One reframing (R1): term V is a NEW cost function re-deriving
effective-probes from selectivity, NOT reading the child IndexPath cost (which prices default probes, blind to
M91 adaptive). Feasible; small.

## Formula
`cost(node) = term_B + term_V`
- **term_B** = `(*(bitmap_path as *BitmapHeapPath)).bitmapqual.total_cost` — the bitmap-PRODUCING cost, exact & free,
  avoids double-counting the heap fetch (`costsize.c:1048,1116-1127`). NOT `bitmap_path.total_cost` (includes heap run_cost).
- **term_V** = f(effective_probes): `effective_probes = clamp(max(probes_default, rerank_pool/(s*avg_list_size)), 1, lists)`;
  cost = reads*random_page_cost + candidates*cpu_op + rerank*random + centroids*cpu_op. Images the M91 loop (`scan.rs:641`).
- **s** = `(*bitmap_path).rows / (*rel).tuples` (planner's own estimate, free).

## Readable at hook time (confirmed)
bitmapqual.total_cost ✅; bitmap rows ✅; lists/avg_list_size via `page::ivf_list_count` ✅ (fail-safe, EC-3);
probes/rerank_pool via guc ✅; cost globals via `get_tablespace_page_costs` ✅.

## pathkeys credit — automatic
Keep `path.pathkeys` set: BitmapHeapScan+Sort competitor pays `cost_sort` (`costsize.c:506-515`), we don't. The
plain vector IndexScan+Filter competitor also carries pathkeys → node must win on total_cost there.

## Measurement gate
Sweep s∈{0.1%..50%}, dense in [5%,25%] (the bracketed crossover from M92: 12× QPS margin at 1% → 1.4× at 5%).
At each s: chosen (EXPLAIN) vs force-INLINE vs force-POST vs force-Bitmap+Sort; assert chosen==Pareto-best-measured.
The sweep calibrates ≤2 cost constants.

## Risks
R1 (LOUD): child cost is probe-blind → term V must re-derive. R2: bad stats → wrong s (recall-safe direction).
R3: constant calibration → sweep is the oracle. R4: M87 competitor also probe-blind (cancels partially). R5: EC-3
fail-safe mandatory (never error in a planner hook — `cost.rs:65` pattern).

## Sources
`costsize.c:1013-1144` (cost_bitmap_heap_scan / cost_bitmap_tree_node), `costsize.c:506-515` (cost_sort),
`pathnodes.h` (BitmapHeapPath.bitmapqual), `theodb_rs/src/am/cost.rs` (M48 reuse), `scan.rs:641` (M91 loop).
