# Edge Case Review — m21-own-ann-index (implementation plan)

Date: 2026-06-30
Tasks analyzed: 5 (T1.1, T1.2, T2.1, T2.2, T3.1)
Cases found: 9 (EDGE: 4, NEGATIVE: 5 | MUST FIX: 2, SHOULD TEST: 4, DOCUMENT: 3)

The plan already covers a strong baseline: empty corpus, k>N clamp, ef_search<k clamp, lists>N clamp, probes>lists
clamp (T1.1/T1.2), and a Failure-scenarios table (missing column, inconsistent dim, query-dim mismatch, empty
table, identifier injection → 22023). The cases below are the ones NOT yet foreseen.

## MUST FIX

### EC-1: NULL vectors in the indexed column
- **Affected task:** T2.1 (`spi_read`)
- **Kind:** NEGATIVE (invalid/unexpected row)
- **Family:** Input / Format
- **Scenario:** `SELECT id, embed_col::real[] FROM src_table` returns rows where `embed_col IS NULL` (a real, common table state). A NULL → `Option<Vec<f32>>`; unwrapping panics or a NULL row enters the corpus with a bogus vector.
- **Impact:** panic (crash) or corrupted graph/lists — and pgvector's own index simply skips NULL rows, so including them also breaks the recall comparison.
- **Suggested fix:** in `spi_read`, skip rows where the vector is NULL (match pgvector index semantics); add a one-line note in T2.1 Deep Dives. (≤3 lines: `if v.is_none() { continue; }`.)

### EC-2: Unbounded build/search parameters → memory blow-up (resource DoS)
- **Affected task:** T2.1 (validation, ADR D4)
- **Kind:** NEGATIVE (out-of-range)
- **Family:** Resource
- **Scenario:** the plan validates `ef_construction≥1`, `ef_search≥k`, `lists≥1`, `probes≥1`, `m≥2` but sets NO upper bound. A caller passes `ef_construction=2_000_000_000` or `lists=2_000_000_000`; the build allocates unbounded candidate lists / centroids → OOM crash of the backend.
- **Impact:** backend OOM / denial of service (the functions are REVOKEd from PUBLIC but can be GRANTed).
- **Suggested fix:** cap to pgvector's documented ranges in the boundary validation (22023 if exceeded): `m≤100`, `ef_construction≤1000`, `ef_search≤1000`, `lists≤32768`, `probes≤32768` (`pgvector/src/hnsw.h:50-58`, `ivfflat.h:51-54`). Add these upper bounds to ADR D4 + T2.1 validation list.

## SHOULD TEST

### EC-3: Cosine distance on a zero-norm vector
- **Affected task:** T1.1 / T1.2 (metric='cosine')
- **Kind:** EDGE (extreme valid — a zero vector is a valid `vector`)
- **Suggested test:** `test_cosine_zero_norm_does_not_panic` — build a corpus containing `[0,0,…]` with metric=cosine; assert search returns without panic and the zero vector sorts last (its distance is NaN/inf via `vec::cosine_distance`'s 0/0). EDGE → assert correct ordering at the boundary (NaN handled, not a crash).

### EC-4: Single-element corpus (N=1) with HNSW
- **Affected task:** T1.1
- **Kind:** EDGE (smallest non-empty)
- **Suggested test:** `test_hnsw_single_element` — corpus of 1, k=10 → returns exactly that 1 element with its distance; entry point = the sole node, no infinite loop. EDGE → correct result at the lower boundary.

### EC-5: Empty `queries` array
- **Affected task:** T2.1
- **Kind:** EDGE (empty-but-valid)
- **Suggested test:** `test_knn_empty_queries_returns_zero_rows` — `theodb.hnsw_knn('t','e', ARRAY[]::vector[], 10)` → 0 rows, no error, no build attempted (early-return before Spi read). EDGE → correct empty result.

### EC-6: `id_col` not integer-typed
- **Affected task:** T2.1 (`spi_read` casts id to i64)
- **Kind:** NEGATIVE (wrong type)
- **Suggested test:** `test_knn_non_integer_id_col_raises_22023` — table with `id uuid`/`id text`; call with `id_col='id'` → typed 22023 ("id_col must be an integer column"), not a raw Spi cast panic. NEGATIVE → assert the specific 22023 + message.

## DOCUMENT

### EC-7: Many duplicate / identical vectors
- **Kind:** EDGE
- **Accepted risk:** corpora with many identical vectors create distance-0 ties; the HNSW heap ordering is stable-enough and IVF assigns them to the same list. No crash; recall is unaffected for ties. Documented as accepted — no special handling needed (the heaps tolerate equal distances).

### EC-8: `metric='ip'` is supported in the API but gated only on l2 + cosine
- **Kind:** EDGE
- **Accepted risk:** inner product is non-metric (no triangle inequality); `theodb_bench`'s ground truth supports l2/cosine. Per plan Q3, `ip` is callable but the recall parity gate runs on l2 + cosine only. Document in T3.1 + the API doc so a caller does not expect a gated ip recall number.

### EC-9: metric ↔ pgvector opclass must match in the benchmark
- **Kind:** NEGATIVE (benchmark correctness)
- **Accepted risk (test-time discipline):** the pgvector arm MUST pair the operator with the right opclass (`<->`+`vector_l2_ops`, `<=>`+`vector_cosine_ops`); mismatching them measures the wrong ground truth. Documented as a benchmark invariant in T3.1 (not a product bug). A wrong pairing would make the parity comparison meaningless — call it out in the bench code comment.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 2 (EC-3,EC-4) | 0 | 0 | 2 | 0 |
| T1.2 | 1 (EC-3) | 0 | 0 | (shared) | 0 |
| T2.1 | 1 (EC-5) | 3 (EC-1,EC-2,EC-6) | 2 (EC-1,EC-2) | 2 (EC-5,EC-6) | 0 |
| T2.2 | 0 | 0 | 0 | 0 | 0 |
| T3.1 | 0 | 1 (EC-9) | 0 | 0 | 2 (EC-8,EC-9) |
| (algo) | — | — | — | — | 1 (EC-7) |

**Coverage check:** every boundary task (T2.1 Spi read, T1.x build/search) now has both an EDGE and a NEGATIVE
case considered. T2.2/T3.1 are test/benchmark tasks whose negatives are the assertions themselves.

**Verdict:** PLAN NEEDS ADJUSTMENT (2 MUST FIX — EC-1 NULL rows, EC-2 param caps; absorbed into plan v1.1 + key SHOULD TEST added to TDD)
