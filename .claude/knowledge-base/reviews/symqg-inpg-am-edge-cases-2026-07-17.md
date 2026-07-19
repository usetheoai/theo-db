# Edge Case Review — theodb_symqg in-PG AM (symqg-inpg-am)

Date: 2026-07-17
Tasks analyzed: 5 (T1.1 page layout, T2.1 build, T3.1 scan, T4.1 reloptions/vacuum, T5.1 benchmark)
Cases found: 11 (EDGE: 6, NEGATIVE: 5 | MUST FIX: 3, SHOULD TEST: 5, DOCUMENT: 3)

Grounding: `theodb_hnsw` treats the graph as IMMUTABLE between VACUUM rebuilds (INSERT→pending, DELETE→rebuild — `hnsw_page.rs:13`); the SymphonyQG AM must mirror this, not invent incremental-insert machinery.

## MUST FIX

### EC-1: Long build loop cannot be cancelled (CREATE INDEX hangs on Ctrl-C)
- **Affected task:** T2.1
- **Kind:** NEGATIVE (failure — unresponsive to cancel)
- **Family:** Timing / Resource
- **Scenario:** The per-parent sign-encode loop is ~N·R iterations (32M at 1M/R=32), single-thread, ~720s. A plain `for` loop never checks for interrupts, so `pg_cancel_backend` / Ctrl-C during `CREATE INDEX` is ignored until the loop ends. **This is the EXACT bug hit in E1** — the k-means loop ignored `pg_terminate_backend` (see the E1 session: had to kill the whole postmaster).
- **Impact:** an operator cannot cancel a runaway/slow build; only a postmaster kill works (drops all sessions).
- **Suggested fix:** call `pgrx::check_for_interrupts!()` every ~4096 vertices inside the encode loop in `ambuild_symqg` (mirror the `check_interrupt` seam `ann/hnsw.rs:44` already uses for the HNSW build).

### EC-2: Per-vertex row exceeds one 8 KB page at high dim × high degree
- **Affected task:** T1.1
- **Kind:** EDGE (extreme of a valid config)
- **Family:** Boundary / Format
- **Scenario:** row = R·i64 ids + R·⌈dim/8⌉ sign bytes + R·2·f32 factors. At dim=768, R=128: sign bytes = 96 B × 128 = 12 288 B > 8 KB — a single vertex row does NOT fit in one page. SIFT (dim=128, R=32) fits (~4 KB), so a naive single-page-item layout passes SIFT but corrupts/errs on a real high-dim workload.
- **Impact:** silent truncation or a `write_item`-too-big error on high-dim indexes; the format is wrong, not just slow.
- **Suggested fix:** write rows via the chunked writer + a per-vertex offset directory (the v5/v6 `dir` pattern in `page/ivf.rs`), NOT a single `write_item`; OR (KISS v1) assert `row_bytes ≤ BLCKSZ` at build and `error!` with a clear "reduce degree_bound for this dim" message. Pick the directory approach if dim>128 is in scope; otherwise the guard + documented limit.

### EC-3: Query vector dimension mismatch panics / reads OOB
- **Affected task:** T3.1
- **Kind:** NEGATIVE (invalid input)
- **Family:** Input / Format
- **Scenario:** `SELECT … ORDER BY e <-> '[…wrong dim…]'::vector` — a query whose length ≠ the index `meta.dim`. The rotate/estimate loops index `q_r[i]` for `i in 0..dim`; a shorter query reads OOB, a longer one silently ignores tail dims.
- **Impact:** panic across the C boundary (crash the backend) OR silent wrong results.
- **Suggested fix:** at the top of `scan_symqg_structured`, `if query.len() != meta.dim as usize { pg_sys::error!("theodb_symqg: query dim {} != index dim {}", query.len(), meta.dim); }` (mirror `ah::build_lut16`'s dim guard, Rule 8 validate-at-boundary).

## SHOULD TEST

### EC-4: Empty index (n=0) and single-row index (n=1, no neighbors)
- **Affected task:** T2.1 / T3.1
- **Kind:** EDGE
- **Suggested test:** `symqg_ambuild_empty_then_scan_returns_empty()` — build on an empty table, scan returns 0 rows, no panic; `symqg_scan_single_row()` — n=1 (entry is its own only candidate) returns that 1 row.

### EC-5: Isolated / zero-neighbor vertex (padding all-sentinel)
- **Affected task:** T1.1 / T3.1
- **Kind:** EDGE
- **Suggested test:** `symqg_row_all_sentinel_neighbors_round_trips()` — a vertex whose R slots are all padding sentinel ids round-trips and, at scan, every sentinel slot is skipped (never pushed as a real tid).

### EC-6: Degenerate residual nr=0 (duplicate vector, x == parent)
- **Affected task:** T1.1
- **Kind:** EDGE
- **Suggested test:** `symqg_encode_sign_zero_residual()` — a neighbor identical to its parent yields `nr=0, w=0`; `estimate_sign` returns exactly `qc2` (no div-by-zero). `encode_sign` already guards this (`symqg_spike.rs:60`) — pin it with a test so a refactor cannot regress it.

### EC-7: Corrupt / truncated page on decode
- **Affected task:** T1.1 / T3.1
- **Kind:** NEGATIVE
- **Suggested test:** `symqg_decode_truncated_row_errs()` — feed `decode_symqg_row` a byte slice shorter than the declared row; assert a **typed `Err`**, not a panic / OOB (the E1 lesson: `page/mod.rs` decoders return `Result`, never `unwrap` on attacker-length bytes).

### EC-8: ef_search < k must clamp
- **Affected task:** T3.1
- **Kind:** EDGE
- **Suggested test:** `symqg_scan_ef_below_k_clamps()` — with `ef_search=1, LIMIT 10`, the beam is clamped to ≥ k so the scan still returns k ordered rows (mirror the beam/ef handling in `scan_hnsw_structured`).

## DOCUMENT

### EC-9: INSERT/DELETE semantics — pending region + rebuild-on-VACUUM
- **Kind:** NEGATIVE (state)
- **Accepted risk:** Resolves the plan's Q3. Mirror `theodb_hnsw` (`hnsw_page.rs:13`): post-build INSERT → a pending region scored EXACT at scan (already in the plan's scan pseudo-code); DELETE → tombstone-at-scan + full rebuild at `amvacuumcleanup` (T4.1). The graph is immutable between rebuilds — NO incremental co-located-code insertion. Document this explicitly in T2.1/T4.1 so a reader does not expect live graph mutation.

### EC-10: Build memory ceiling (replicated codes, O(N·R))
- **Kind:** EDGE (resource)
- **Accepted risk:** at 1M/R=32 the sign codes are ~4.5 GB resident during build (measured in the spike); billion-scale would exceed commodity RAM. Fast-JL O(D log D) rotation + streaming encode are the levers, explicitly OUT OF SCOPE (blueprint § 3). Document the build-memory ceiling in Drawbacks so it is a known limit, not a surprise OOM.

### EC-11: Benchmark GT-subset trap
- **Kind:** NEGATIVE (measurement)
- **Accepted risk:** T5.1 MUST index the FULL 1M base — the SIFT groundtruth is over all 1M, so any subset (the spike's N=200k) yields a false ~0.25 recall ceiling. Add a one-line note in the `e2_symqg_inpg.py` harness + T5.1 acceptance criterion: "N must equal the GT base size (1,000,000)".

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 3 | 1 | 1 (EC-2) | 3 (EC-5,6,7) | 0 |
| T2.1 | 1 | 1 | 1 (EC-1) | 1 (EC-4) | 1 (EC-10) |
| T3.1 | 2 | 2 | 1 (EC-3) | 1 (EC-8) | 1 (EC-9) |
| T4.1 | 0 | 0 | 0 | 0 | 0 (covered by plan's crash/vacuum tests) |
| T5.1 | 0 | 1 | 0 | 0 | 1 (EC-11) |

**Coverage check:** every task touching an input boundary has ≥1 EDGE and ≥1 NEGATIVE case considered. T4.1's edges (reloption range, degree-not-multiple-of-32, vacuum-to-empty) are already enumerated in the plan's T4.1 Deep Dives + TDD — no new case surfaced beyond them.

**Verdict:** PLAN NEEDS ADJUSTMENT — 3 MUST FIX (EC-1 build-cancel, EC-2 row-spans-page, EC-3 query-dim guard) should be absorbed into T2.1/T1.1/T3.1 as sub-tasks before `/implement`. EC-1 and EC-3 are one-liners; EC-2 is a layout decision (chunked-row directory vs a documented degree×dim guard) worth resolving now. The 5 SHOULD-TEST cases fold into existing TDD blocks; the 3 DOCUMENT items are plan-note additions (EC-9 also resolves Q3).
