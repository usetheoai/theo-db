# Blueprint — Persisted-CSR index-AM (build 1× + incremental maintenance, crash-safe) (M108)

**Cycle:** discover · **Milestone:** M108 (graph pillar Phase 1) · **Date:** 2026-07-16

The M107 spike proved native CSR+BFS traversal beats recursive-CTE 106–738× — but with a caveat: **on-the-fly CSR build dominates at 1M** (31 ms build vs 1.4 ms traverse → end-to-end win collapses to ~7×). M108 closes that gap: **persist the CSR once (at `ambuild`) + maintain it incrementally (pending-region + VACUUM-fold) + crash-safe (WAL)** so per-query cost is load+traverse, not rebuild. Target: the operative number becomes the traverse-only 106–738×, not the rebuild-per-query ~7×.

## Context & the Rule-9 reuse insight

The core M108 problem — a **compact structure (CSR) + an incremental delta without full rebuild + crash-safe durability** — is *exactly* the problem TheoDB's vector index-AM already solved: build a compact structure at `ambuild`, append new tuples to a **pending region** at `aminsert`, **fold** the pending region into the compact structure at `amvacuumcleanup`, all **WAL-logged via GenericXLog** (M26/M48/M89, `am/{build,page,fold}.rs`). This is the same shape as the SOTA "delta/changeset over CSR" (LLAMA multi-versioning). M108 therefore **reuses that machinery** rather than inventing a graph-specific store.

## Coverage Corner 1 — Integration Tests

- **Build-once, query-many** — `CREATE INDEX ... USING theodb_graph` builds the CSR once; then N `graph_expand` queries each pay only load+traverse (no rebuild). The benchmark asserts per-query cost ≈ load+traverse and independent of query count.
- **Correctness across persistence** — traversal over the *persisted* CSR returns the SAME reachable-set as the M107 in-memory BFS and the recursive-CTE (set-hash oracle) on identical graphs.
- **Incremental maintenance** — after `INSERT`ing new edges (→ pending region), a traversal reflects the new edges (pending + CSR merged); after `VACUUM` (fold), the CSR contains them and the pending region is empty; results identical before/after fold.
- **Crash-safety** — (a) abort mid-`ambuild` → index is unbuilt/absent (no half-CSR); (b) committed build → survives crash + WAL replay byte-identical; (c) crash after `aminsert` (pending) → committed pending edges survive replay. (The M48/M99 isolation-harness pattern: SIGABRT injection + WAL replay.)

## Coverage Corner 2 — Dependencies

- **No new dependency** (Rule 9): reuse `am/page.rs` (WAL-logged page write/read: `write_blob`/`read_blob`/`write_chunks`/`read_chunked`), `am/build.rs` (ambuild scaffold + pending fold), `am/fold.rs` (crash-safe generation pivot), `am/mod.rs` (AM registration), `am/lock.rs` (fold exclusion).
- **Postgres index-AM API** (inherited): `ambuild` (scan edge table → CSR), `aminsert` (append to pending), `amvacuumcleanup` (fold), `ambuildempty` (unlogged path). Crash-safety = logged index + GenericXLog (NOT unlogged/ambuildempty — that discards on crash).
- **Rejected:** PCSR/PMA (packed-memory-array in-place inserts) — a genuinely different, more complex structure; the pending+fold pattern is what TheoDB already owns and crash-proved. PCSR is an ADR alternative to revisit only if fold amortization is measured insufficient.

## Coverage Corner 3 — Tools

- **Benchmark harness** — extend `benchmarks/m107_graph_spike/` (or a pgrx-side bench): (a) build the index once, (b) run K traversal queries, measure per-query load+traverse, (c) compare end-to-end vs the M107 rebuild-per-query numbers AND vs recursive-CTE. The GATE artifact: `docs/benchmarks/m108-persisted-csr.{md,json}`, ≥3 runs mean±std, ZERO fabricated numbers.
- **Crash harness** — reuse `theodb_rs/isolation/` (SIGABRT via `theodb.test_crash_*` GUC + WAL replay + result diff), as M48/M99/M104 did.
- **Differential oracle** — set-hash (`bit_xor(hashint8(node))`, the M107-review upgrade over count+sum) comparing persisted-CSR traversal vs in-memory BFS vs recursive-CTE.

## Coverage Corner 4 — Techniques

- **Persisted CSR layout** — two arrays (vertex-offset + edge-dst[+weight]) serialized into WAL-logged index pages at `ambuild`, exactly the M26 blob/chunk layout (`page::write_chunks`); read back with `read_chunked`. The build scans the edge table via `table_index_build_scan` (Postgres API), counts degrees, prefix-sums offsets, fills the edge array — the same algorithm as the M107 spike's in-Rust CSR build, now writing pages.
- **Pending-region incremental maintenance** — new edges from `aminsert` append to a pending region (uncompacted), read-merged with the CSR at traversal time; folded into the CSR at `amvacuumcleanup` when the pending region exceeds a threshold (GUC `theodb_graph.fold_threshold`, mirroring `vacuum_pending_threshold`). This is the LLAMA delta-snapshot / TheoDB pending-fold pattern.
- **Crash-safe fold** — the M48/M99 generation-pivot: pack the new CSR generation at a fresh base, write inert pages, pivot the meta page atomically (GenericXLog), reclaim the old region. Position-independent so readers need no change.
- **Traversal over persisted CSR** — a minimal `theodb.graph_expand(seeds, max_hops)` reading the CSR pages + draining the pending region, running the frontier BFS. (M108 ships the minimal read-path to prove the gate; the full vectorized MS-BFS operator + rich surface is M109/M110.)

## ADRs

### ADR-1 — Pending-region + VACUUM-fold (reuse M48/M89) over PCSR/PMA
**Decision:** persist CSR + maintain via the existing pending-region + crash-safe fold machinery. **Alternatives:** PCSR/PMA in-place inserts (rejected: new complex structure; the pending+fold pattern is proven + crash-safe in-tree — Rule 9); array-of-arrays streaming store (rejected: not a compact CSR; loses the sequential-scan-neighbors win). **Rationale:** the vector AM already crash-proved "compact + pending + fold"; graph adjacency is the same shape. Revisit PCSR only if measured fold cost is prohibitive.

### ADR-2 — Logged index + GenericXLog (NOT unlogged/ambuildempty)
**Decision:** the graph index is crash-safe (durable CSR). **Alternative:** unlogged index via `ambuildempty` (rejected: contents discarded on crash — a persisted-CSR that vanishes on crash defeats the purpose; and the traversal must survive restart). **Rationale:** durability is the whole point of persisting; reuse the vector AM's GenericXLog path.

### ADR-3 — M108 ships the minimal traversal read-path; the vectorized MS-BFS operator is M109
**Decision:** M108 includes just enough traversal (`graph_expand` reading persisted CSR + pending) to run the gate benchmark + oracle; the SIMD MS-BFS operator + multi-source throughput is M109. **Rationale:** M108's gate is "persisted CSR preserves the win without per-query build + crash-safe" — that needs a read-path, not the full operator. Bounding scope avoids conflating two milestones.

## Prior Art & Related Work

- Incremental CSR: [Packed CSR (PCSR)](https://itshelenxu.github.io/files/papers/pcsr.pdf), [Batch-Parallel CSR](https://brianwheatman.com/papers/batch_pcsr.pdf), [LLAMA multi-versioning delta over CSR](https://db.in.tum.de/teaching/ws1718/seminarHauptspeicherdbs/paper/valiyev.pdf), [RisGraph sub-ms per-update (arXiv 2004.00803)](https://arxiv.org/pdf/2004.00803).
- Postgres index-AM: [Index Access Method Functions (PG17 docs)](https://www.postgresql.org/docs/current/index-functions.html) — `ambuild`/`aminsert`/`amvacuumcleanup`/`ambuildempty`, WAL via GenericXLog.
- Internal (the reuse targets): `theodb_rs/src/am/{mod,build,page/,fold,lock}.rs` (index-AM + WAL + crash-safe fold + pending-region, M26/M48/M89/M99), `benchmarks/m107_graph_spike/` (the CSR+BFS reference + baseline), ADR-0048 (the pillar GO), the M107 blueprint + benchmark.
- USPTO 11,093,459 — parallel main-memory CSR-based graph index in an RDBMS (incremental maintenance prior art).

## Drawbacks & Risks

- **Fold cost vs insert rate** — if edges churn fast, the fold may run often; mitigation: the pending threshold GUC + amortized-cost measurement (the M89 lesson: measure the build/fold envelope).
- **Traversal-over-(CSR+pending) correctness** — reads must merge the compact CSR with the uncompacted pending region; mitigation: the set-hash oracle asserts identical results pre/post fold.
- **Crash-safety regression** — this touches WAL/fold, MVCC-load-bearing code; mitigation: reuse the proven fold + re-run the isolation/crash harness (no new crash-safety invention).
- **Scope creep into M109** — the temptation to build the full MS-BFS operator here; mitigation: ADR-3 bounds M108 to the minimal read-path for the gate.

## Unresolved Questions

- Fold threshold default (pending-region size) — tuned by the amortized-cost benchmark.
- Undirected representation in CSR — store each edge in both endpoints' adjacency (2× edge array) as the M107 spike did, or store directed + traverse both — a build-space vs traverse-branch trade decided by the benchmark.
- Whether `graph_expand` reads pending inline or requires a fold before first query — decided by the maintenance-correctness test.
