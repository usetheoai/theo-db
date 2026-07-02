# Blueprint: M35 — theodb_hnsw page-native structured scan (partial-read, à la M31 for the graph)

> **Discovery verdict:** SHIPPABLE_WITH_CAVEATS — exact prior art in-repo (M31 ivfflat structured pages) + the
> reference implementation (pgvector hnsw) both read; the pattern is well-understood. Method: deep read of
> pgvector `src/hnsw{.h,scan.c,utils.c,build.c}` + the M31 `page.rs` pattern + the current O(N) blob path.

**Slug:** `m35-hnsw-structured-scan` · **Owner:** paulohenriquevn · **Created:** 2026-07-02

## Context

`theodb_hnsw` today serializes the WHOLE graph to a single blob and the scan deserializes ALL of it per query
(`ann/hnsw.rs:243`, `am/scan.rs:168` — "O(N)"; ~6.5 GB / ~0.6 s at 1M). M35 replaces this with a **page-native
structured** persistence (meta + element tuples + neighbor tuples) and an **on-demand traversal** that reads only
visited nodes → **O(ef·M) pages, flat in N**. This mirrors what M31 did for `theodb_ivfflat` (structured pages,
partial read) but for the HNSW graph, and mirrors pgvector's `HnswElementTupleData`/`HnswNeighborTupleData`/
`HnswMetaPageData` layout.

## The KISS scope-cut (load-bearing — makes M35 ~half the feared size)

theodb already handles **INSERT via an append-only pending region** (`page.rs:113-208`) and **DELETE via VACUUM
full-rebuild** (`build.rs:143-216`). **Keep that model.** Therefore the built graph is **IMMUTABLE between
rebuilds** — intra-graph refs are always valid, and M35 does **NOT** need pgvector's hardest machinery: on-disk
incremental neighbor rewiring (`hnswinsert.c:546`), tombstones, or the `version`-field stale-detection during
concurrent mutation (`hnswutils.c:777`). **M35 = a page codec + an on-demand read path + one GUC.** (ADR-1.)

## Coverage Corner 1 — Integration Tests

- **Codec round-trip (unit, `#[pg_test]`):** structured graph traversal == current in-memory `HnswIndex::search`
  on the same corpus/seed (recall preserved). Empty index (`entry_level=-1` → `[]`), single-node (no neighbor
  deref crash), per-layer slice correctness for every `lc ∈ 0..=level` (the `start=(level-lc)*m` math).
- **Negative (unit):** truncated element/neighbor page, bad magic, corrupt `neighbortid` → typed `Err`, never a
  panic across `C-unwind` (mirror `hnsw.rs:338` `hnsw_from_bytes_rejects_truncated_and_bad_magic`).
- **Graph integrity (integration, SQL):** entry-point fallback (entry node fewer live layers than recorded → no
  infinite loop / OOB); INSERT lands in pending + folded by scan; DELETE+VACUUM rebuilds over live TIDs only
  (deleted TIDs never returned); coexistence M20–M22 green; `theodb_ivfflat` untouched.
- **Performance (the DoD gate):** pages-read O(ef·M) NOT O(N) — a scan-profiler counter (the wiring-triad runtime
  metric, mirror `scan.rs:105-164`) asserts pages-read is flat in N (measure at N and 2N); **QPS ≥ ~50 at 1M×128,
  recall preserved** via a re-run of the M32 harness → `docs/benchmarks/`; `ef_search` GUC sweep recall↑/QPS↓
  monotone (mirror the M34 probes sweep).

## Coverage Corner 2 — Dependencies

**No new crate** (parsimony-ladder rung 4). All `pgrx` (=0.16.1) `pg_sys` FFI + `std`: buffers/WAL
(`ReadBufferExtended`, `GenericXLog*`, `PageAddItemExtended`, `LockRelationForExtension` — already used in
`page.rs`), `PageIndexTupleOverwrite` (a `pg_sys` symbol; port the wrapper like the other macros at `page.rs:632`
if pgrx doesn't export it — no external dep), GUC (`GucRegistry`/`GucSetting` — already in `guc.rs`), SIMD
distance (in-repo `vec.rs:167`). `deps-audit` is a no-op.

## Coverage Corner 3 — Tools / Primitives

**Reuse as-is from `page.rs`:** the WAL scaffold (`extend_page_with_item` `:72`, `try_add_to_page` `:145`,
`reinit_page_with_item` `:275`), the pending region (`append_pending`/`read_pending` `:113-208`, unchanged), page
macros (`page_get_item_id`/`page_get_item`/`page_get_max_offset` `:632-649`), `read_all_page_items` `:211`,
`peek_magic` `:313`, SIMD `l2_dist_from_bytes` (`vec.rs:167` — score off `&etup.data` bytes, no per-node alloc),
`tid::encode`/`set_on`, `Metric::from_tag`/`dist`.
**New primitives M35 adds:** (a) `read_page_item_at(rel, blkno, offno)` — read ONE item at an arbitrary offset
(current `read_page_item` hardcodes offset 1, `page.rs:619`) = the on-demand load-one-node primitive; (b) a
multi-tuple page writer returning the assigned `(blkno, offno)` (pgvector `HnswBuildAppendPage`,
`hnswbuild.c:122`); (c) in-place `PageIndexTupleOverwrite` by (blkno,offno) under WAL (size-preserving thanks to
fixed degree) for the pass-2 neighbor fill; (d) HNSW meta parse/write + an `HNSW_STRUCT` arm in `main_index_pages`
(`page.rs:327`) so the pending region is located; (e) `theodb_hnsw.ef_search` GUC by **cloning the `probes`
pattern** (`guc.rs:20-31`) — honest correction: there is NO ef_search GUC today (it's a fixed `SCAN_EF=64` const,
`index.rs:9`).

## Coverage Corner 4 — Techniques

**Split-tuple layout (mirror pgvector).** One **element tuple** per node (vector + heap-tid + level + pointer to
its neighbor tuple) and one **neighbor tuple** per node holding ALL layers' neighbor index-pointers, laid out top
layer → ground with **fixed M per layer** (`m` upper, `m0=2m` ground — pgvector `HnswGetLayerM` `hnsw.h:112`) so
the tuple is **fixed-size** ⇒ in-place overwrite is size-preserving. Entry point `(blkno,offno,level)` in the
**meta page** (`entry_level=-1` sentinel = empty). Per-layer slice: layer `lc` at `start=(level-lc)*m`, length
`lm=(lc==0?m0:m)` (pgvector `hnswutils.c:784`). Cap max level so a neighbor tuple always fits one page
(pgvector `hnsw.h:118`).

**On-demand traversal (mirror `HnswSearchLayer`).** Start from the meta entry stub (level only, no vector). Upper
layers `lc=entry_level..1` greedy-descend with `ef=1`; ground layer `lc=0` with `ef=ef_search`. Expanding a
candidate reads its **neighbor tuple** (1 page); each unvisited neighbor reads its **element tuple** (1 page) and
is SIMD-scored off the raw bytes. A visited-set keyed by `(blkno,offno)` loads each node ≤ once. Pages ≈
1 (meta) + O(entry_level) + O(ef·m0) → **flat in N** (pgvector `hnswscan.c:26-56`, `hnswutils.c:760-985`).

**Two-pass build (why element/neighbor tuples are separate).** Pass 1 writes every element tuple + a zeroed
placeholder neighbor tuple, assigning each node its `(blkno,offno)` and setting `etup.neighbortid`. Pass 2
overwrites the placeholders with real neighbor pointers (now that every node has an address). Neighbor TIDs can
only be filled after all nodes have addresses (pgvector `hnswbuild.c:151-298`). v1 simplification: two page ranges
(all elements, then all neighbors) — page co-location for locality is deferred (YAGNI).

## Cross-cutting Comparison

| | theodb_hnsw today (blob) | M35 structured (this) | pgvector hnsw (reference) |
|---|---|---|---|
| Persistence | 1 blob across chunk pages | meta + element tuples + neighbor tuples | same (element/neighbor/meta tuples) |
| Scan cost | O(N) — deserialize all (~6.5 GB @1M) | O(ef·M) pages, flat in N | O(ef·M) |
| INSERT | pending region (append) | pending region (unchanged) | on-disk incremental rewire |
| DELETE | VACUUM full-rebuild | VACUUM full-rebuild (unchanged) | repair + tombstone + version |
| Stale refs | n/a (immutable graph) | n/a (immutable graph) | version field + repair |

## ADRs

### ADR-1 — immutable-between-rebuilds graph; keep pending+VACUUM-rebuild (do NOT port pgvector insert/repair)
**Decision:** M35 is ONLY a page codec + on-demand read path + ef_search GUC. Keep INSERT-via-pending and
VACUUM-full-rebuild. **Rationale:** the built graph is immutable between rebuilds → intra-graph refs always valid →
pgvector's on-disk incremental insert (`hnswinsert.c`), tombstones, and version-stale-detection
(`hnswvacuum.c`/`hnswutils.c:777`) are UNNECESSARY. This halves the milestone and removes the concurrency-hazard
surface. **Rejected:** port pgvector's incremental insert (huge, concurrency-hard, and the pending region already
delivers correct INSERT semantics — YAGNI).

### ADR-2 — fixed M per layer / fixed-size neighbor tuple (not variable-degree)
**Decision:** fixed max degree per layer (m upper, m0 ground), single neighbor tuple per node, cap max level so it
fits one page. **Rationale:** pgvector confirms fixed-M is standard (`HnswGetLayerM`); fixed size makes the pass-2
overwrite size-preserving (simplest possible) and the slice math trivial. **Rejected:** variable-degree neighbor
lists (buys nothing, breaks size-preserving overwrite, complicates the codec).

### ADR-3 — new format magic + REINDEX gate for the old blob (like the M31 IVF v1→v2 gate)
**Decision:** `HNSW_STRUCT_MAGIC` ("THSS"); the old blob `HNSW_MAGIC` ("THNS") is recognized on read and rejected
with a REINDEX message (mirror the IVF v1→v2 gate `page.rs:340-344`). **Rationale:** pre-1.0 engine, a clean
format break beats a dual-read path. **Rejected:** silent dual-format read (complexity for a pre-1.0 index).

## Recommendations

1. Land the **codec + traversal FIRST**, gated by a round-trip test asserting identical results to
   `HnswIndex::search` on the same seed/corpus (recall preserved) — BEFORE any benchmark.
2. Add `read_page_item_at` + the multi-tuple page writer + `PageIndexTupleOverwrite` (vetted ports section).
3. Add `theodb_hnsw.ef_search` GUC (clone the `probes` pattern).
4. Wire the scan-profiler counter (pages-read) as the wiring-triad runtime metric; assert flat-in-N.
5. Re-run the M32 harness at 1M → `docs/benchmarks/m35-hnsw-structured-scan.{md,json}`: QPS ≥ ~50, recall
   preserved, ef_search sweep, pages-read O(ef·M). Honest verdict.

## Top 3 risks

- **R1 (SINGLE HARDEST, ~3-4× M31):** correct on-demand traversal of a pointer-chased graph (element→neighbor→
  element) across arbitrary pages with a visited-set + per-layer slice, proven bit-identical to the in-memory
  graph at 1M. *Mitigation:* codec-first round-trip gate; faithful port of `HnswSearchLayer`/`HnswLoadNeighborTids`;
  immutable graph removes race complexity; reuse M31b SIMD scorer.
- **R2:** off-by-one in the per-layer slice / level cap → silent recall loss (no crash). *Mitigation:* unit-test
  the slice for every `lc` on multi-level fixtures; assert neighbor-tuple ≤ one page at build; recall@10 vs blob
  baseline as a hard gate.
- **R3:** new multi-item page writer / `PageIndexTupleOverwrite` FFI misuse → WAL/page corruption. *Mitigation:*
  reuse the exact WAL scaffold from `extend_page_with_item`/`try_add_to_page`; port `PageIndexTupleOverwrite` in the
  vetted ports section; crash-safety test (build → WAL replay → identical scan); corrupt-page → typed `Err`.

## Minimal-correct first version (KISS/YAGNI)

Ship exactly: (1) fixed-degree split-tuple layout, two page ranges, entry point in meta; (2) on-demand read path
(`read_page_item_at` + visited-set + SIMD from-bytes scoring); (3) keep pending+VACUUM-rebuild; (4) `ef_search`
GUC. **Deferred (YAGNI):** element+neighbor page co-location, on-disk incremental insert, tombstone/version
stale-handling, parallel build, multiple heap-tids per element. None required by the M35 DoD.
