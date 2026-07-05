# Edge Case Review — fu1-samegraph-scan-microbench (plan)

Date: 2026-07-05
Tasks analyzed: 4
Cases found: 5 (EDGE: 3, NEGATIVE: 2 | MUST FIX: 1, SHOULD TEST: 3, DOCUMENT: 1)

## MUST FIX

### EC-1: fixture "determinism" claim is wrong — N≥50k uses the NONDETERMINISTIC parallel build
- **Affected task:** T3.1 (+ ADR-2)
- **Kind:** EDGE (the fixture-build boundary)
- **Family:** State
- **Scenario:** ADR-2 claims a "seeded deterministic fixture" citing `ann/mod.rs::hnsw_deterministic_same_seed`.
  But that test covers the **sequential** build (`n < PARALLEL_BUILD_THRESHOLD = 4096`, `ann/hnsw.rs:44`). The
  plan's N≥50k triggers the **M44 parallel build** (`ann/hnsw.rs:44-53`), whose linking order **races** — the
  exact nondeterminism that broke M46's two-container A/B. So `HnswIndex::build(seed=42)` at 50k is NOT
  byte-identical across `cargo bench` runs.
- **Impact:** the ADR-2 rationale is false as written; a reader (or a cross-run `criterion --baseline` compare)
  would wrongly assume the graph is reproducible across runs. It is not.
- **Suggested fix:** correct ADR-2 + T3.1 to state the invariant that actually holds and that the measurement
  needs: **same-graph WITHIN a run** — the fixture is built ONCE per `cargo bench` invocation and the SAME
  `HnswIndex` reference is shared by both `presized` and `unsized` bench functions, so the delta is measured over
  a byte-identical graph. Cross-run reproducibility is NOT required (the plan benches both fns in one group in one
  run; it does not use `--baseline` across runs). Drop the `hnsw_deterministic_same_seed` citation (wrong build
  path); cite instead "built once, shared by reference" as the same-graph guarantee.

## SHOULD TEST

### EC-2: `ground_search` with `ef > node_count` (tiny/near-empty graph)
- **Affected task:** T1.1
- **Kind:** EDGE (extreme of valid — ef larger than the graph)
- **Suggested test:** `ground_search_ef_exceeds_node_count_returns_all` — build a 5-node graph, run
  `ground_search` at ef=200; assert it returns ≤ 5 results (not a panic, not ef padding). EDGE → assert the
  correct bounded result (`result.len() <= node_count`). The M46 loop already guards `result.len() < ef`, so this
  should pass; the test locks it.

### EC-3: `MemNeighborSource` asked for a node-id out of range → typed error, not panic
- **Affected task:** T1.1
- **Kind:** NEGATIVE (invalid input)
- **Suggested test:** `mem_neighbor_source_out_of_range_node_is_typed_err` — call
  `neighbors_into(u64::MAX, &mut v)`; assert a typed `Err("scan_core: node id out of range")`, NOT an index-out-of-
  bounds panic. NEGATIVE → assert the specific typed error + message. (In practice `ground_search` only requests
  discovered node-ids, but the trait boundary must fail-fast, not panic — `error-handling.md`.)

### EC-4: `NodeId` packing round-trip at max `(blk, off)` (production adapter)
- **Affected task:** T2.1
- **Kind:** EDGE (extreme of valid — max page address)
- **Suggested test:** `page_neighbor_source_nodeid_roundtrip` — pack `(blk=u32::MAX, off=u16::MAX)` into a u64 via
  `(blk<<16)|off` and unpack; assert exact recovery. EDGE → assert `unpack(pack(x)) == x` at the max. (u32<<16 |
  u16 = 48 bits, fits u64 — but the shift/mask must be verified, a classic off-by-shift bug.)

## DOCUMENT

### EC-5: NaN distance (zero-norm cosine vector) at the ground layer
- **Affected task:** T1.1
- **Kind:** NEGATIVE (degenerate valid input)
- **Accepted risk:** a zero-norm vector under cosine yields NaN distance. The existing `Cand`/`Scored` `Ord`
  already puts NaN LAST (worst) — `ann/mod.rs:116` documents this. `ground_search` reuses that ordering, so a NaN
  falls to the end of the heaps and does not corrupt the traversal. No new handling needed; documented so a
  reviewer does not re-flag it. The fixture uses L2 (no NaN), so the bench is unaffected.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 1 (EC-2) | 2 (EC-3, EC-5) | 0 | 2 (EC-2, EC-3) | 1 (EC-5) |
| T2.1 | 1 (EC-4) | 0 | 0 | 1 (EC-4) | 0 |
| T3.1 | 1 (EC-1) | 0 | 1 (EC-1) | 0 | 0 |
| T4.1 | 0 | 0 | 0 | 0 | 0 |

**Coverage check:** T1.1 (the core algorithm) covered on both lenses (EDGE ef>N, NEGATIVE out-of-range + NaN).
T3.1 fixture-build state boundary caught (EC-1). T2.1 packing boundary caught (EC-4). T4.1 (report) has no input
boundary — no lens applies.

**Verdict:** PLAN NEEDS ADJUSTMENT (absorb EC-1 into ADR-2/T3.1 wording; add EC-2/EC-3/EC-4 tests to T1.1/T2.1 TDD;
EC-5 as a documented note).
