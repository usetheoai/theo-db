# Edge Case Review — m22-own-quantization (implementation plan)

Date: 2026-06-30
Tasks analyzed: 3 (T1.1, T2.1, T2.2, T3.1)
Cases found: 6 (EDGE: 3, NEGATIVE: 1 | MUST FIX: 1, SHOULD TEST: 2, DOCUMENT: 3)

The plan is strong (reuses M20+M21, zero deps, bits/over_fetch caps, NULL skip, dim mismatch, injection already
covered). Below are the unforeseen cases.

## MUST FIX

### EC-1: Memory gate "own bytes/vector ≤ pgvectorscale" is parity-by-construction — must not be framed as a memory WIN
- **Affected task:** T3.1 (recall+memory gate), ADR D3/D4
- **Kind:** EDGE (honesty / measurement validity)
- **Family:** Format
- **Scenario:** at the same `bits_per_dim`, own SBQ bytes/vector `ceil(dim·bits/8)` is IDENTICAL to pgvectorscale's
  `quantized_size_bytes` (same formula, blueprint Q4). So "own ≤ pgvectorscale" is trivially true (equal) — it
  proves memory **parity by construction**, NOT a win over pgvectorscale.
- **Impact:** if the doc/CHANGELOG framed it as "less memory than pgvectorscale", that would be a false claim
  (`rules/public-copy.md` honesty).
- **Suggested fix:** the benchmark doc + gate state honestly: **memory = parity with pgvectorscale (identical
  formula) AND ~Nx reduction vs f32 (4·dim)**; the **recall@k with rerank** is the substantive differentiator the
  gate tests. Add this framing to T3.1 + the benchmark-doc template + ADR D4.

## SHOULD TEST

### EC-2: over_fetch larger than the candidate pool
- **Affected task:** T2.1 (`sbq::knn`)
- **Kind:** EDGE (extreme of valid)
- **Suggested test:** `sbq_overfetch_exceeds_pool` — probes return fewer than `k·over_fetch` candidates; assert the
  rerank uses all available and returns `min(k, available)` rows, no panic, no out-of-bounds.

### EC-3: n-bit (bits ≥ 2) quantize round-trip
- **Affected task:** T1.1 (n-bit encode)
- **Kind:** EDGE (the multi-bit path is more complex than 1-bit)
- **Suggested test:** `sbq_quantize_2bit_monotonic` — for bits=2, a larger value encodes ≥ as many set bits as a
  smaller value along a dimension (the unary z-score property), and `bytes_per_vector(dim,2) == ceil(dim·2/64)·8`.
  Guards the multi-bit packing the gate may exercise.

## DOCUMENT

### EC-4: SQL-callable search uses an f32 carrier (IVFFlat) — runtime memory win is M22b
- **Kind:** EDGE
- **Accepted risk:** the candidate-gen reuses M21 `IvfflatIndex` (f32 k-means), and rerank reads f32 — so this
  scope reads f32 at build/search time. The **storage** memory metric (bytes/vector of codes) is the real,
  honest win; the **runtime** memory win (search touching only codes) requires the on-disk AM storing codes only
  → M22b. Document in ADR D4 + the benchmark doc (already partially in Drawbacks; make explicit).

### EC-5: value exactly equal to the per-dim mean
- **Kind:** EDGE
- **Accepted risk:** `x > mean[d]` (strict) → a value equal to the mean encodes bit 0, matching pgvectorscale's
  `>` (`quantize.rs:57-62`). Consistent; no special handling. Document the strict-`>` choice in `sbq.rs`.

### EC-6: pgvectorscale `diskann` availability in the benchmark image
- **Kind:** NEGATIVE (benchmark prerequisite)
- **Accepted risk:** T3.1 compares against `CREATE INDEX … USING diskann … (storage_layout=memory_optimized)`.
  The theo-db image builds pgvectorscale (Dockerfile Stage 1), so `diskann` is present. If a future image dropped
  it, the benchmark must fail loudly (skip-with-reason), never silently compare against nothing. Document the
  precondition in `bench_sbq_index.py` (assert the `diskann` AM exists before the pgvectorscale arm).

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 1 (EC-3,EC-5) | 0 | 0 | 1 (EC-3) | 1 (EC-5) |
| T2.1 | 1 (EC-2) | 0 | 0 | 1 (EC-2) | 1 (EC-4) |
| T3.1 | 1 (EC-1) | 1 (EC-6) | 1 (EC-1) | 0 | 1 (EC-6) |

**Verdict:** PLAN NEEDS ADJUSTMENT (1 MUST FIX — EC-1 memory-honesty framing; absorbed into plan v1.1 + SHOULD TEST added to TDD)
