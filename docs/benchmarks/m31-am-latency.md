# M31 — Index AM query latency: structured partial-page reads

**Milestone:** M31 (`theodb_ivfflat` partial-page reads) · **Date:** 2026-07-01 · **Image:** `theo-db:m31` · PG17 · pgrx 0.16.1
**Plan:** `.claude/knowledge-base/plans/m31-am-latency-plan.md` · **Re-scope:** `docs/adr/0011-m31-rescope-simd-followup.md`

Measurement-first (the North Star pillar — `memory: goto-p0-vector-superiority`). Numbers from `EXPLAIN (ANALYZE,
TIMING ON)` against the container, warm cache, ≥ 5 runs.

## 1. What M31 changed

M26's `amrescan` deserialized the WHOLE index blob per query (O(N)). M31 restructures `theodb_ivfflat` persistence
into a **meta page** (centroids + a per-list directory) + **list pages**; `amrescan` reads the meta + centroids
(∝ nlists) then ONLY the probed lists' pages (∝ probes), scoring entries off raw page bytes with a reused scratch
buffer (no per-entry `Vec` allocation). VACUUM fold + aminsert-pending are format-aware.

## 2. Latency — n = 100 000, dim = 128, probes = 10, LIMIT 10 (warm cache, literal query vector)

| Approach | p50 | vs pgvector |
|---|---:|---|
| M26 blob (O(N)-per-scan deserialize) | **~1700 ms** (extrapolated: 86 ms at 5k × 20) | ~120× slower |
| **M31 structured partial reads** | **~38 ms** | **~2.7× slower** |
| pgvector `ivfflat` (AVX-SIMD C) | **~14 ms** | 1× (reference) |

- **The O(N) I/O gap is CLOSED:** M31 reads ~the same number of index pages as pgvector (buffers ≈ equal) and is
  **~45× faster than the M26 O(N) path.**
- **Honest residual:** M31 is **~2.7× behind pgvector**. The algorithmic gap (O(N) → O(probes)) is closed; the
  remaining gap is the **constant factor** — theodb's scalar/SSE2 (auto-vectorized 4-wide) distance vs pgvector's
  **AVX 8-wide SIMD + runtime CPU dispatch** (hand-tuned C). Closing that is **M31b** (SIMD distance, ADR 0011) —
  NOT this slice.

The optimization within M31 that mattered: replacing the per-entry `Vec<f32>` allocation with a reused scratch
buffer took the structured scan from ~62 ms → ~38 ms.

## 3. Recall — preserved

`benchmarks/tests/test_index_am_latency.py` asserts the structured index's top-10 overlaps the brute-force top-10
(≥ 8/10); `test_index_am.py::test_index_scan_returns_correct_neighbors` asserts recall@5 ≥ 4/5. The restructure is
correctness-preserving (same IVFFlat math, same vectors, same distances).

## 4. Re-scoped DoD (CTO decision 2026-07-01) — MET

The original DoD ("p50 ≤ pgvector") is NOT met by evidence (38 > 14). Per ADR 0011 the CTO re-scoped M31 to its
proven achievement: the O(N)-per-scan gap CLOSED (structured partial reads, ~45× vs M26) with correctness +
maintenance intact and latency within a documented band of pgvector; SIMD-parity is M31b. The re-scoped gate
(`test_index_am_latency.py`) asserts: recall parity · p50 far below the O(N) regime · p50 within 4× of pgvector —
all green.

## 5. Reproduction

```bash
docker build -t theo-db:m31 .
docker run -d --name theo-db-m31 -e POSTGRES_PASSWORD=postgres -p 5436:5432 theo-db:m31
PGPORT=5436 python3 -m pytest benchmarks/tests/test_index_am_latency.py -q
# manual: seed 100k×128, CREATE INDEX ... USING theodb_ivfflat / ivfflat, EXPLAIN (ANALYZE) both with probes=10.
```
