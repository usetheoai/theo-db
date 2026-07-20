# Review — M118 filtered-ann-resume-discarded

Date: 2026-07-20 · Reviewers: `council-rust-pgrx` (safety) + `council-index-storage` (scan/MVCC correctness),
spawned in parallel on the committed delta (50eb574, 764ecaa, f874667).

## Verdict: **READY_TO_MERGE**

No BLOCKER, no HIGH, no MEDIUM. Two LOW findings (both flagged by both reviewers) — **fixed before merge**.

## Severity matrix

| Severity | Count | Findings |
|---|---|---|
| BLOCKER | 0 | — |
| HIGH | 0 | — |
| MEDIUM | 0 | — |
| LOW | 2 | (1) `approx_bytes` under-counts HashSet/heap overhead → ceiling looser than nominal; (2) resume-loop `unwrap` relied on an invariant |
| INFO | 3 | `read_meta` re-read per grow (stable under SHARE lock); cross-batch order relaxation (inherited M52/pgvector-0.8); `reads` Cell reset (cosmetic observability) |

## council-rust-pgrx — SOUND (no panic across the C boundary)

- All new error paths terminate in `pg_sys::error!` inside `#[pg_guard] extern "C-unwind"` functions (`amrescan`/`amgettuple`) — the sigsetjmp boundary is present; no raw `panic!`/`unwrap`/`expect` on data-controlled input.
- NaN-distance heap hazard defused (`Ranked::cmp` → `unwrap_or(Ordering::Equal)`, EC-5). Dim guard runs BEFORE `resumable_init` → the SIMD length assert is never hit with a mismatched query.
- `unsafe` FFI correctly scoped; per-call `PageNeighborSource` reconstruction is safe (frontier holds only owned `Copy` `Cand` — no `rel`/`q` borrow retained across `amgettuple`).
- Disjoint field borrow (`&state.query` + `state.resume.as_mut()`) sound. `Cand → pub(crate)` leaks nothing (fields private).

## council-index-storage — CORRECT on all 5 axes

1. **Rescan** — `amrescan` clears `resume=None` + heap/emitted/exhausted; per-query frontier rebuilt; no skip/dup across a self-join rescan.
2. **Pending region (the high-value question) — NO MISS.** Pending is folded UNCONDITIONALLY into the first batch (not ef-truncated); the first heap drains fully before the resume branch ever runs; both dispatch arms (Some/None) fold pending; a pending-only index takes the None path (still folds). Not-re-folding on resumed batches is correct because `emitted` dedups and all pending is already in batch 1.
3. **MVCC/tombstones** — retained `Cand` addresses stay valid: the SHARE advisory lock is txn-scoped and blocks the VACUUM fold's EXCLUSIVE for the scan's lifetime (no relocation mid-scan); tombstones navigated-through but never emitted (`emittable = !deleted` preserved in `next_batch`).
4. **Termination** — provable: `visited` monotonic + bounded by N; `cands` strictly shrinks to empty once visited saturates; plus `emitted>=cap` + memory-ceiling guards. No infinite loop on all-already-emitted batches.
5. **Fail-safe** — `resume_max_mb` overflow → clean `exhausted` return (no panic), correct-but-lower-recall, bounded by MVCC recheck. Acceptable.

## LOW fixes applied (this review pass)

- **`unwrap` → `if let`** in the `amgettuple` resume loop (`scan.rs`) — structurally cannot panic across C if a future edit clears `resume` mid-loop.
- **GUC doc caveat** on `theodb_hnsw.resume_max_mb` (`guc.rs`) — documents that `approx_bytes` is conservative-permissive (real RSS ~2-3× nominal); size with headroom. It is a soft fail-safe, correctness never depends on it.

## Correctness evidence (independent of the reviews)

- recall@10 = 1.0 vs brute-force exact kNN under a selective filter (A/B in-PG, `Index Scan using theodb_hnsw`).
- Own-path A/B: resume ON 14.33 ms vs OFF (M52 re-search) 27.94 ms @ recall 0.9967 → ~1.95× faster (matched recall).
- INV1 (union ⊇ single-ef) / INV2 (EC-1 exhaustion) / INV3 (EC-3 single-node) pass; `resume_max_mb=1` returns cleanly (EC-5 no-panic).
- pgvector-parity DoD FALSIFIED (structural page-native gap) — no such claim made (Rule 5). See `docs/benchmarks/m118-resume-discarded.md`.
