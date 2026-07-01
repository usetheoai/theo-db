# Blueprint: M31 — Index AM query-latency optimization (partial-page reads)

> **Discovery verdict:** SHIPPABLE_WITH_CAVEATS — grounded in the M26 code (own) + pgvector/pgvectorscale peer
> scan I/O (cloned references). Reuses the m21/m26 blueprints. The decision below is the design contract for the
> M31 plan.

**Slug:** `m31-am-latency` · **Owner:** paulohenriquevn · **Created:** 2026-07-01

## Context

M26 shipped persisted index AMs (`theodb_ivfflat`/`theodb_hnsw`) at recall parity, but `amrescan` deserializes
the **whole** index blob from pages on every query (O(N)) — 86 ms vs pgvector's ~1.5 ms seq scan on 5k×128
(`docs/benchmarks/m26-index-am.md`; ADR 0010 §D2/D5). M31 (P0 CTO GOTO — `memory: goto-p0-vector-superiority`)
closes that gap so the persisted AM's query latency **≤ pgvector** and scales to M32's 1M+ target.

## Coverage Corner 1 — Integration Tests

How the peers prove scan latency + how M31 reuses `theodb_bench`:
- pgvector's regression + ANN-Benchmarks harness measures recall@k + QPS/p50/p95 with `enable_seqscan=off`
  forcing the index; M31 reuses `benchmarks/theodb_bench/recall.py` + a new latency harness comparing
  `theodb_ivfflat` Index Scan vs pgvector `ivfflat` on the SAME dataset (n≥100k, dim≥128), `EXPLAIN (ANALYZE)`.

## Coverage Corner 2 — Dependencies

No new dependency (pgrx + std). Reuses M26's `am/page.rs` buffer/GenericXLog primitives + `ann/wire.rs` codec.

## Coverage Corner 3 — Tools

pgrx `pg_sys` page/buffer APIs already used in M26 (`ReadBufferExtended`, `BufferGetPage`, `PageGetItem`).

## Coverage Corner 4 — Techniques

**Peer scan I/O (the crux, cited):**
- **pgvector IVFFlat is page-structured** — `ivfflat.h:47` `IVFFLAT_METAPAGE_BLKNO 0`, `:48` `IVFFLAT_HEAD_BLKNO 1`
  (first list page); `ivfscan.c:124 GetScanItems` walks ONLY the probed lists' page chains
  (`:145 ReadBufferExtended(...searchPage...)`), reading `probes·list_size` items, NOT the whole index. Bounded
  memory (relies on shared_buffers/OS page cache for repeat reads).
- **pgvectorscale** caches the meta-page pointer (`meta_page.rs`) and reads node pages on demand — same principle:
  never deserialize the whole index per scan.

**Design decision for M31 (partial-page reads, NOT a per-backend cache):**
- A relation-scoped deserialized cache would amortize the O(N) deserialize but holds the whole index (O(N)) in
  **backend-local** memory — prohibitive at M32's 1M+ (512 MB+/backend) and cross-backend invalidation-heavy.
  **Rejected** (would be re-work at scale — violates the "sem re-trabalho" mandate).
- **Chosen:** add a **per-list directory** to the meta page — for each IVFFlat centroid, the `(first_block, count)`
  of its list's entries stored in dedicated **list pages** (each list entry = `[tid i64, vector f32×dim]`).
  `amrescan` reads the meta page (centroids + directory), computes the `probes` nearest centroids, and reads ONLY
  those lists' pages — deserializing `probes·list_size` vectors, not N. Per-query I/O ∝ probes. Scales to 1M+;
  reuses M26's page/buffer/WAL infra; keeps recall identical (same IVFFlat math, same vectors).

## Cross-cutting Comparison

| Aspect | M26 (blob) | pgvector | M31 (chosen) |
|---|---|---|---|
| Per-scan I/O | O(N) whole blob | O(probes·list) | O(probes·list) |
| Memory/backend | O(N) transient | bounded (buffers) | bounded (only probed pages) |
| Scales to 1M+ | no | yes | yes |
| Reuses M26 infra | — | n/a | yes (page/buffer/WAL) |

## ADRs

### D1 — Partial-page reads via a per-list page directory (not a per-backend cache)
Chosen for scalability to M32 (1M+) + no-rework; cache rejected (O(N)/backend). See Corner 4.

### D2 — IVFFlat first; HNSW page-structuring is a separate milestone
IVFFlat → pages is natural (lists = pages). HNSW-on-pages (graph traversal reading node pages on demand) is
materially harder (pgvector's hnsw is a large C effort) — scope M31 to the DEFAULT `theodb_ivfflat` and defer
`theodb_hnsw` partial-page reads to a follow-up (documented), keeping HNSW on the M26 blob path meanwhile.

### D3 — Measurement-first
No latency claim without the reproducible harness vs pgvector on n≥100k (DoD). Honest verdict per knob.

## Recommendations

1. Restructure `theodb_ivfflat` persistence: meta page (centroids + per-list directory) + list pages (M31 T1).
2. `amrescan` reads only probed lists' pages (M31 T2).
3. Latency harness `theodb_ivfflat` vs pgvector `ivfflat`, n≥100k dim≥128, p50/p95 + recall (M31 T3).
4. HNSW deferred via ADR (D2); coexistence with M26 maintenance (aminsert pending / VACUUM fold) preserved.
