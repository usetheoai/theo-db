# Blueprint: theodb_hnsw scan QPS optimization (M41)

**Slug:** `m41-hnsw-qps`
**milestone_id:** M41
**Created:** 2026-07-03
**Rigor:** measurement-first; the oracle is the end-to-end A/B benchmark (QPS at matched recall), NOT an internal
profiler (M36 lesson: internal instrumentation misleads). Baseline = M40 (`docs/benchmarks/m40-carrier.md`).

## Context

M40 measured `theodb_hnsw` at **3–5× lower QPS than `theodb_ivfflat` at matched recall** (e.g. hnsw ef=100:
0.809 recall @ 329 QPS vs ivf probes=100: 0.902 @ 643 QPS on n=50k). A graph index should beat probing — this is
optimization headroom. This blueprint identifies the bottleneck from the code and decides the minimal fix.

## Coverage Corner 1 — Integration Tests

The `theodb_hnsw` behavior is covered by `benchmarks/tests/test_index_am.py` + `theodb_rs/src/am/hnsw_page.rs`
`#[pg_test]` traversal tests. The optimization MUST keep top-k **byte-identical** (recall unchanged) — proven by
re-running those tests. This is the M36 pattern (optimize the scan, prove recall identical by construction).

## Coverage Corner 2 — Dependencies

None new. The fix reuses the existing M31b SIMD `crate::vec::l2_dist_from_bytes` (already scores off a `&[u8]`
byte slice — no `Vec<f32>` needed) and PG's buffer manager. Zero new dependency (parsimony rung 4).

## Coverage Corner 3 — Tools

A/B benchmark: re-run `benchmarks/run_m40_carrier.py` on the optimized build (`theo-db:m41`) and compare
`theodb_hnsw` QPS at matched recall vs the M40 baseline (`theo-db:m39`). ≥3 runs mean±std (analysis-golden-rule
§A1). The benchmark is the honest oracle.

## Coverage Corner 4 — Techniques

### The bottleneck (from `theodb_rs/src/am/hnsw_page.rs` + `page.rs`)

The on-demand `traverse` (`hnsw_page.rs:472`) reads a node's element tuple (`load`, `hnsw_page.rs:432`) and its
neighbor tuple (`neighbors_of`, `hnsw_page.rs:456`) — **2 reads per visited node**. Each read calls
`page::read_page_item_at` (`page.rs:685`), which per call does:

1. `RelationGetNumberOfBlocksInFork` (`page.rs:690`) — a bounds check on EVERY read.
2. `ReadBufferExtended` + `LockBuffer(SHARE)` — pin + lock.
3. **`to_vec()` (`page.rs:711`) — heap-allocate + memcpy the item bytes into a fresh `Vec`.**
4. `UnlockReleaseBuffer` — unlock + unpin.

At ef=200 (~200 nodes), that is ~400 reads/query × {bounds-check + pin + lock + alloc + memcpy + unpin}. The
element's `to_vec` copies `dim·4` bytes (256 B at dim=64); `decode_element` then borrows that copy only to score
it once via SIMD — **the copy is pure waste**: the SIMD `l2_dist_from_bytes` can score directly off the pinned
page pointer.

**Why hnsw is slower than ivfflat:** `theodb_ivfflat`'s scan (M31b/M36) reads a page ONCE and scores ALL vectors
on it with SIMD (amortizes pin/lock over the whole page, zero per-vector alloc). `theodb_hnsw` pays the full
per-read fixed cost (bounds-check + pin + lock + alloc + memcpy + unpin) PER VECTOR (1 vector per node read). The
per-node fixed cost — dominated by the `to_vec` alloc + the redundant `nblocks` call — is the QPS gap.

### The fix (parsimony rung 6 — minimum that works)

1. **Eliminate the element-load `to_vec` copy.** Add a pinned-scope read that decodes + scores off the page
   pointer inside the pin (no alloc), returning the owned `Cand`. The SIMD `l2_dist_from_bytes` already takes a
   `&[u8]` — no `Vec<f32>` decode needed.
2. **Cache `nblocks` once per traverse** — pass the block count in (or read once), removing the per-read
   `RelationGetNumberOfBlocksInFork`.

Deferred (measure first): replacing the `HashSet<(u32,u16)>` visited set with a bitmap — only if the A/B
benchmark shows the copy/nblocks fix insufficient (avoid speculative complexity, YAGNI).

## Cross-cutting Comparison

| Aspect | theodb_ivfflat (fast) | theodb_hnsw (slow, today) | M41 fix |
|---|---|---|---|
| pin/lock granularity | per PAGE (many vectors) | per NODE (1 vector) | keep per-node (graph is random-access) but drop the per-node alloc |
| per-vector alloc | none (SIMD off page bytes) | `to_vec` (256 B) per node | eliminate — score off the pinned page |
| bounds check | once | `nblocks` per read | cache once per traverse |

## ADRs

### D1 — Score off the pinned page; do NOT change the graph layout or the traversal algorithm

**Decision:** Optimize only the per-node read cost (copy elimination + nblocks caching). Keep the M35 page layout,
the traversal order, and the top-k selection byte-identical.

**Rationale:** the layout + algorithm are correct (recall is fine at full ef); the gap is per-node overhead. A
byte-identical top-k means recall is unchanged by construction — the only measured variable is QPS (clean A/B).
Changing the algorithm would confound the measurement (M36 discipline).

**Alternatives considered:** (a) rewrite the traversal (rejected — confounds recall); (b) bitmap visited set
(deferred — measure the copy fix first, YAGNI); (c) batch neighbor+element into one read (rejected — they are
separate tuples on possibly different pages; complex, uncertain win).

**Consequences:** minimal diff, clean A/B; recall unchanged by construction; if QPS does not improve, revert
honestly (the bottleneck was elsewhere — anti-sunk-cost).

### D2 — Benchmark-gated merge (measurement-first)

**Decision:** the optimization merges only if the A/B benchmark shows `theodb_hnsw` QPS improved meaningfully at
matched recall (effect > variance, ≥3 runs). If not, revert; the discovery (bottleneck elsewhere) is the outcome.

## Recommendations for the project

1. Implement the pinned-scope scored read + nblocks caching in `traverse`.
2. Keep the hnsw `#[pg_test]` traversal tests green (recall byte-identical).
3. A/B benchmark `theo-db:m41` vs `theo-db:m39` on `run_m40_carrier.py`; record the QPS delta honestly.

## Blocked questions (if any)

(none — the bottleneck is identifiable from the code; the A/B benchmark is the oracle.)

## Related

- Baseline: `docs/benchmarks/m40-carrier.md` (hnsw 3–5× slower than ivfflat)
- Hot path: `theodb_rs/src/am/hnsw_page.rs:432,456,472`; `theodb_rs/src/am/page.rs:685`
- SIMD kernel reused: `theodb_rs/src/vec.rs:167` (`l2_dist_from_bytes`)
