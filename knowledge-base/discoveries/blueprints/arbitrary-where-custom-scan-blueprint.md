# Blueprint — M92 arbitrary-WHERE filtered vector search via Custom Scan Provider

**Date:** 2026-07-13 · **Milestone:** M92 · **Source:** 3-front discovery (council-rust-pgrx code-grounded on pgrx bindings + refs; council-index-storage code-grounded on our AM + postgres source; general-purpose web/SOTA sweep, R0 web-evidence). All findings cited below.

## The load-bearing finding (shapes the whole design)

**The Custom Scan node is REQUIRED for correctness — not just elegance.** M90's `xs_recheck` mechanism does NOT generalize to arbitrary `WHERE`:

- `nodeIndexscan.c:141` rechecks `node->indexqualorig` — the **index** quals (our pushed label/vector ScanKeys) — NOT an arbitrary `WHERE` on other columns. M90 is correct *only because* its predicate (`labels && '{…}'`) IS a pushed ScanKey on the indexed label column.
- For M92 the filter is on OTHER columns (the ones the bitmap sub-plan indexed), which are not in our `indexqualorig`. So any over-admitted candidate (lossy bitmap page) or pending row would be a **false positive that no recheck removes** → wrong results.
- **Fix (the AlloyDB-parity shape):** the arbitrary-`WHERE` recheck must be owned by a **Custom Scan node** that re-evaluates the original bitmap qual (`ExecQual`) on each emitted heap tuple under the executor snapshot — exactly as `nodeBitmapHeapscan.c:317-327` does with `bitmapqualorig`. The AM change stays tiny; the correctness lives in the Custom Scan node.

This is corroborated by the core-hackers thread (§ Prior art): the bitmap from `amgetbitmap` is UNORDERED, at odds with a distance-ordered NN scan, so the bitmap must be built **above** the AM and used as a membership oracle inside our ordered vector scan — it cannot be pushed into the AM.

## The design (measurement-gateable, M90-consistent)

1. **Planner hook** (`set_rel_pathlist_hook`, chain the previous) adds a `CustomPath` when the rel has both a vector `ORDER BY <-> LIMIT k` and scalar `WHERE` preds with a usable index → cost it against plain post-filter.
2. **Custom Scan node** (`BeginCustomScan`): run the native bitmap sub-plan (`BitmapAnd`/`BitmapOr` over existing B-tree/GIN → `TIDBitmap` via `tbm_*`), **materialize it once** into `HashSet<i64>` of exact TIDs (encode via `tid::encode`, `tid.rs:7`) + `HashSet<u32>` of lossy blocks (`tbm_iterate` returns `ntuples==-1` for lossy pages → offsets are gone, `tidbitmap.h:44`). Call a setter into the AM's `ScanState`.
3. **AM Stage-1** (`scan_ivf_aq_split_v7`, `scan.rs:632`): replace/augment the `v7_label_overlaps` skip with `if filtering_tid && !(exact.contains(&tid) || lossy_blocks.contains(&block)) { continue; }` — same shape as M90, O(1) per candidate, preserves the O(probes) partial-read invariant.
4. **Custom Scan node** (`ExecCustomScan`): pull ordered TIDs from `amgettuple`, fetch the heap tuple, and **re-run `ExecQual(original_where)`** — this is the recheck the AM's `xs_recheck` cannot do. Emit survivors.
5. **Strategy by bitmap cardinality** (reuse M91 axis): ultra-selective → PRE (fetch the small TID set + exact rerank); medium → INLINE (this membership skip); loose → POST/adaptive-probes. Measurement decides which regimes to build (M91 taught: build the estimator, measure, add PRE only if needed).

**No page-format change** → no magic bump, no REINDEX (the bitmap is a runtime input, not persisted).

## Feasibility (pgrx 0.16.1 / pg17)

CONFIRMED present in `pgrx-pg-sys-0.16.1`: `set_rel_pathlist_hook`, `RegisterCustomScanMethods`, `CustomScanMethods`/`CustomExecMethods`/`CustomPathMethods`, `CustomScan`/`CustomPath`/`CustomScanState` + NodeTags, `TIDBitmap`, `tbm_create`/`tbm_add_tuples`/`tbm_begin_iterate`/`tbm_iterate`, `ExtensibleNodeMethods`, `PgBox::alloc_node` (node-alloc primitive, `pgbox.rs:298`).
ABSENT (hand-roll): `create_customscan_path`, `make_custom_scan` — reconstruct via `PgBox::<CustomPath/CustomScan>::alloc_node(T_*)` + manual field population + `add_path`. **NO local pgrx exemplar** — paradedb uses `amgetbitmap` (opposite direction: PRODUCES a bitmap for a native BitmapHeapScan, `paradedb/pg_search/src/postgres/scan.rs:388-418`); citus is C (shows the C-API shape to mirror, not a pgrx pattern). This is the #1 risk (a mis-populated node field = planner crash or silently-wrong plan).

## Prior art (Rule 9 — study, not copy)

- **AlloyDB inline filtering** (the target): a Custom Scan "vector scan" consumes a Bitmap Index Scan, computing distances only for TID-matching rows; adaptive (inline⇄pre) by runtime selectivity, exposed in `EXPLAIN` `Execution Strategy`. **ScaNN-ONLY — NOT IVF/IVFFlat/HNSW** (cloud.google.com/blog/…/inline-filtering; docs.cloud.google.com/alloydb/docs/ai/adaptive-filtering). ⇒ an IVF extension doing this is novel territory.
- **The sanctioned extension path** (the honest ceiling): pgsql-hackers thread Zhou (TensorChord)/van de Meent (core) — *"create a planner hook plus a custom executor node that does this … but it won't be able to use much of the features inside PostgreSQL"*; core does NOT let an AM push a bitmap filter down into itself; AlloyDB's adaptivity is a **core optimizer change**. (postgresql.org/message-id/CAEze2Wh…). So: extension matches the *mechanism*, NOT the *core-optimizer mid-query adaptivity* (tier ④).
- **No permissive OSS peer does bitmap-fed inline filtering on IVF:** pgvector = post-filter/iterative (`amgetbitmap=NULL`); pgvectorscale = streaming post-filter + label-in-index (Filtered-DiskANN, `amgetbitmap=None`) — our M90 axis; VectorChord = bitmap prefilter but **AGPL (study-only)**. TheoDB M92 would be a differentiated permissive-OSS position.
- **Academic analog:** ACORN-γ's predicate-agnostic bitset-guided traversal (arXiv:2403.04871) — filtering DURING search over an existing index, closest to the bitmap-membership idea (vs the filter-aware-index school: Filtered-DiskANN, NHQ). Filtered-ANN taxonomy: arXiv:2509.07789. PG-specific: arXiv:2603.23710 (filter-agnostic vector search on Postgres).

## Coverage corners

- **Techniques:** planner-hook + Custom Scan node feeding a bitmap membership set into the ordered IVF Stage-1; qual recheck in the node (MVCC); strategy-by-cardinality (reuse M91). Academic: ACORN-γ bitset traversal.
- **Dependencies:** no new dep; reuse `TIDBitmap`/`tbm_*` (native), the M90 Stage-1 skip, the M91 adaptive axis, `tid::encode`. NO page-format change.
- **Tools:** SIFT1M harness with a scalar `WHERE` on a non-label column (real neighbors — M91 tie-density lesson); `EXPLAIN` shows the Custom Scan node.
- **Integration tests:** membership skip correct == exact; lossy-page over-admit + recheck removes false positives; pending rows rechecked; MVCC (concurrent update) via EPQ recheck.

## ADRs

**ADR M92-1 — Custom Scan node REQUIRED (not AM-only).** Alternatives: (AM-only setter + xs_recheck) REJECTED — `nodeIndexscan.c:141` only rechecks index quals, so an arbitrary `WHERE` on other columns leaks over-admitted/pending false positives; the recheck must live in a node that holds the original qual. Chosen: Custom Scan node owns bitmap build + lossy handling + `ExecQual` recheck; the AM gets a tiny membership setter + contains-check.

**ADR M92-2 — materialize bitmap to HashSet at rescan (not bitmap-drives-scan).** Alternatives: (bitmap drives the scan by heap-block order) REJECTED — inverts our centroid-ordered O(probes) partial-read access pattern (M31/M35 invariant) and breaks the AH-LUT batched scoring. Chosen: materialize once (exact-TID set + lossy-block set), O(1) membership at the M90 skip point.

**ADR M92-3 — SPIKE-first (de-risk before the full feature).** The node-construction hand-roll has NO local pgrx exemplar and real UB risk; the MVCC recheck is subtle. Build a minimal spike (register a Custom Scan, intercept the pattern, feed a bitmap membership set, prove EXPLAIN + a correct filtered result + the recheck on a lossy/pending case) BEFORE the full pre/inline/post strategy matrix. Honest-negative (the node hand-roll is infeasible/too risky in pgrx) is a valid terminal that re-scopes to "AM-only inline label + documented arbitrary-WHERE-is-post-filter".

> **SPIKE v0 RESULT (2026-07-13, commit `7224ae0`) — GO. Unknown #1 RETIRED.** A hand-rolled pass-through Custom Scan Provider in pgrx 0.16.1 (pg17) works at RUNTIME: `set_rel_pathlist_hook` + `RegisterCustomScanMethods` + hand-rolled `CustomPath`/`CustomScan` via `PgBox::alloc_node` (replacing the absent `create_customscan_path`/`make_custom_scan`) + the full plan→exec lifecycle. `EXPLAIN` shows `Custom Scan (theodb_vecfilter) on cs`; the pass-through result is byte-identical to the un-hooked plan. Gated behind `theodb.enable_vecfilter` (default OFF); 257 pg_tests GREEN (255+2), zero regression. **4 gotchas surfaced + fixed (institutional knowledge for v1):** (1) method tables hold `*const c_char` → not `Sync` → wrap in a newtype with `unsafe impl Sync`; (2) `cheapest_total_path` is NULL at `set_rel_pathlist_hook` time (`set_cheapest` runs AFTER the hook) → pick the child from `rel->pathlist`; (3) `add_path` uses `compare_path_costs_fuzzily` with a 1% fuzz factor → an epsilon cost delta reads as a tie and the new path is rejected → the cost must be meaningfully lower (spike halves it; the real feature costs the filtered scan honestly); (4) `plan.qual` must hold BARE expression nodes → convert the `PlanCustomPath` `clauses` (RestrictInfo) via `extract_actual_clauses`, else "unrecognized node type: T_RestrictInfo". **Spike v1 (next): the bitmap membership set + MVCC recheck (ADR M92-1) — the Custom Scan node runs the bitmap sub-plan, materializes exact-TID + lossy-block sets, feeds them into the AM Stage-1 via a setter, and re-runs `ExecQual` on the heap tuple.** Code: `theodb_rs/src/am/customscan.rs`.

## Honest boundary

- **Tier ②→③ (pre-filter membership + correct recheck), NOT tier ④** (mid-query cross-index re-plan / core-optimizer adaptivity — needs core changes, not reachable by a permissive extension).
- **Correlation-blind:** M91 adaptive probing reacts to matched-candidate count, not filter↔vector-distribution correlation; an anti-correlated filter still probes many lists (bounded/correct per ADR M91-3, not optimal).
- **Lossy-page over-scan:** under memory pressure the bitmap lossifies → page-granular admission + heap recheck (safe, more heap reads than an exact in-index filter).
- **NOT a QPS-superiority claim vs ScaNN/AlloyDB** (paradigm ceiling M73/M82). It's a **capability** parity (efficient arbitrary-`WHERE` filtered vector search), measured.

## Effort (honest)

HIGH — multi-week spike-gated. ~6 hand-rolled `extern "C-unwind"` callbacks + 2 from-scratch node-construction helpers (no exemplar) + cost-model tuning (the planner must CHOOSE the path) + the MVCC recheck. This is essential complexity (the problem demands it), welcome per "Esforço ≠ Complexidade" — but it is NOT a quick milestone like M91. The spike (ADR M92-3) is the gate.

## Citations

- Our code: `theodb_rs/src/am/scan.rs` (M90 skip `:632`, `v7_label_overlaps` `:688-702`, `xs_recheck` `:846-850`, label ScanKey parse `:128-142`, Stage-1 `:588-639`), `theodb_rs/src/am/tid.rs:7-21`.
- Postgres source: `references/postgres/src/include/nodes/tidbitmap.h:44-73`, `.../nodes/tidbitmap.c:232-235,403-432`, `.../executor/nodeBitmapHeapscan.c:317-327`, `.../executor/nodeIndexscan.c:138-147`, `.../access/index/indexam.c:272,301`.
- pgrx: `pgrx-pg-sys-0.16.1/src/include/pg17.rs:43538` (`set_rel_pathlist_hook`); `pgrx-0.16.1/src/pgbox.rs:298` (`alloc_node`); grep-confirmed presence/absence of the Custom Scan surface.
- Peers: `references/pgvector/src/{ivfflat,hnsw}.c` (`amgetbitmap=NULL`), `references/pgvectorscale/.../access_method/{mod.rs:81,scan.rs:364}`, `references/paradedb/pg_search/src/postgres/scan.rs:388-418`.
- Web (R0): AlloyDB inline/adaptive filtering docs+blog; pgsql-hackers CAEze2Wh… thread; ACORN arXiv:2403.04871; Filtered-ANN arXiv:2509.07789; PG-specific arXiv:2603.23710; Postgres custom-scan docs ch.60.
