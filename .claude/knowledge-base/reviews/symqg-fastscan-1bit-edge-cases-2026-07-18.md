# Edge Case Review — symqg-fastscan-1bit

Date: 2026-07-18
Tasks analyzed: 4 (T1.1, T2.1, T2.2, T3.1)
Cases found: 6 (EDGE: 2, NEGATIVE: 4 | MUST FIX: 3, SHOULD TEST: 3, DOCUMENT: 0)

The plan reuses `vec/ah.rs::ah_score_block` — a LUT16-pshufb kernel with **hard structural limits** (int16 accumulator, 32-wide block, 4-bit nibble grouping). The real risks are all **user-configurable inputs that exceed those limits**: `theodb` is a vector DB, and the most common real-world embedding dims (1536 = OpenAI, 768 = BERT) plus the `degree_bound` reloption (allowed up to 512) can silently break the kernel. All three MUST-FIX share ONE clean fix: a **FastScan eligibility dispatch** with a scalar fallback.

## MUST FIX

### EC-1: High-dim vectors overflow the int16 FastScan accumulator (1536-dim OpenAI embeddings)
- **Affected task:** T1.1, T2.2
- **Kind:** NEGATIVE (input past the kernel's numeric limit)
- **Family:** Input / Format
- **Scenario:** `ah_score_block32` accumulates `m` int8 partials into int16; `ah.rs:25` states this is safe only for `m ≤ 258`. For the sign kernel `m = dim/4`. A user does `CREATE INDEX … USING theodb_symqg` on a `vector(1536)` column (OpenAI `text-embedding-3-small`/`ada-002` — extremely common) → `m = 384 > 258` → the i16 accumulator **overflows silently** → wrong estimates → corrupted recall with NO error. 768-dim (BERT, `m=192`) is safe; 1536 is not.
- **Impact:** Silent recall corruption on the single most common production embedding size. No crash, no typed error — the worst failure mode (Rule 8).
- **Suggested fix:** Dispatch in T2.2: use FastScan only when `dim/4 ≤ 258`; else fall back to the scalar `estimate_sign` path. Add `debug_assert!(m <= 258)` in `build_sign_lut16` as the backstop. (One `if` at the scan dispatch + one assert.)

### EC-2: `dim` not a multiple of 4 breaks the 4-dim group packing
- **Affected task:** T1.1, T2.1
- **Kind:** NEGATIVE (invalid-for-kernel input)
- **Family:** Input / Format
- **Scenario:** `build_sign_lut16` groups 4 sign-dims per LUT entry (`m = dim/4`). A user indexes a `vector(130)` (or any `dim % 4 != 0`) → the last 2 dims are silently dropped from the group loop → wrong dot → wrong estimate. The plan's T2.1 deep-dive *mentions* a guard but no task/test enforces it, and T1.1's `build_sign_lut16` has no such guard.
- **Impact:** Silent recall degradation on any dim not divisible by 4 (e.g. 130, 300, 1000).
- **Suggested fix:** Same dispatch as EC-1: FastScan only when `dim % 4 == 0`; else scalar fallback. Add a `sign_lut_dim_not_mult4_falls_back` test.

### EC-3: `degree_bound > 32` exceeds the single FastScan block (reloption allows up to 512)
- **Affected task:** T2.1, T2.2
- **Kind:** NEGATIVE (input past the block width)
- **Family:** Boundary
- **Scenario:** `ah_score_block` asserts `n ≤ 32` (`ah.rs:292`). `options.rs` allows `degree_bound` up to `MAX=512` (rounded to a multiple of 32). A user does `WITH (degree_bound=64)` → a vertex has up to 64 neighbours → one 32-wide block cannot hold them → the assert **panics** (or, if the layout only writes 32, half the neighbours are dropped → recall loss). The plan's Q3 declares this "out of scope" but the reloption does NOT reject it.
- **Impact:** `CREATE INDEX … WITH (degree_bound=64)` panics the scan (assert) or silently drops neighbours — a crash/corruption on a legal reloption value.
- **Suggested fix:** In T2.2 loop `⌈degree/32⌉` block32 chunks (degree is a multiple of 32 → clean); the T2.1 layout writes `⌈degree/32⌉` consecutive block32 regions. (Alternatively, and simpler for this measured slice: restrict FastScan to `degree ≤ 32` and scalar-fall-back otherwise — but chunking is the honest fix since the reloption permits >32.)

## SHOULD TEST

### EC-4: Mixed-sign `q_r` → negative LUT partials through the affine requant
- **Affected task:** T1.1
- **Kind:** EDGE (extreme of valid: the sign-sum partials span negative..positive, unlike AQ's always-≥0 squared-L2 partials)
- **Suggested test:** `test_sign_fastscan_matches_estimate_sign_mixed_sign_qr` — with a `q_r` of mixed signs (so `lo < 0`), assert the dequantized dot matches the exact `⟨q_r,u⟩` within one requant step. The affine map `code=round((v-lo)/scale)` handles `lo<0` (since `v-lo≥0`) and `dequantize` re-adds `m·bias` — but this is the subtle correctness point vs `build_lut16` (whose partials are never negative), so it MUST be asserted, not assumed.

### EC-5: All-sentinel neighbour block (`n_real = 0`) and partial blocks
- **Affected task:** T2.2
- **Kind:** NEGATIVE (empty-but-valid)
- **Suggested test:** `test_symqg_scan_all_sentinel_block_no_admit` — a vertex whose neighbours are all `SENTINEL_ORD` yields 0 admits and no panic (call `ah_score_block` with `n = n_real` where real neighbours occupy lanes `0..n_real` and sentinels are padded last per `pack_row`; assert `n_real=0` skips the call cleanly). Already listed as T2.2 RED `symqg_scan_empty_neighbours_ok` — keep it and assert the lane-ordering invariant (real neighbours first, sentinels last) explicitly.

### EC-6: `q_r` all-zero (query == popped centre) → degenerate LUT range
- **Affected task:** T1.1
- **Kind:** EDGE (boundary: `max == min`)
- **Suggested test:** `test_sign_lut_degenerate_range_all_zero_qr` — `build_sign_lut16(&[0.0; 128])` produces a valid `Lut16` (all partials equal → `range.max(MIN_POSITIVE)` guard, all codes map to 0) and `sign_estimate_block` returns finite estimates equal to `qc2 + nr²` for every neighbour. Reuses `build_lut16`'s degenerate guard — assert it transfers.

## Summary

| Task | EDGE | NEGATIVE | MUST FIX | SHOULD TEST | DOCUMENT |
|------|------|----------|----------|-------------|----------|
| T1.1 | 1 (EC-6) | 2 (EC-1, EC-2) | 2 (EC-1, EC-2) | 2 (EC-4, EC-6) | 0 |
| T2.1 | 0 | 2 (EC-2, EC-3) | 2 (EC-2, EC-3) | 0 | 0 |
| T2.2 | 1 (EC-5) | 2 (EC-1, EC-3) | 2 (EC-1, EC-3) | 1 (EC-5) | 0 |
| T3.1 | 0 | 0 | 0 | 0 | 0 |

**Coverage check:** every task touching an input boundary (T1.1 dims, T2.1 layout, T2.2 dispatch) has both an EDGE and a NEGATIVE case. T3.1 is a pure measurement task (its RED is the recall gate) — no input boundary.

**Verdict:** PLAN NEEDS ADJUSTMENT

The three MUST-FIX (EC-1 high-dim overflow, EC-2 dim%4, EC-3 degree>32) are the same class: **the FastScan kernel has structural input limits, and `theodb`'s public surface (arbitrary `vector(N)` dims + the `degree_bound` reloption) can exceed them on common, legal inputs** (1536-dim embeddings, `degree_bound=64`). They collapse into ONE plan change: add a **FastScan eligibility dispatch** — FastScan only when `dim % 4 == 0 && dim/4 ≤ 258 && degree ≤ 32` (or chunk for degree), else the scalar `estimate_sign` path (reconstructed per-neighbour `u` from the block32 codes). This is a new ADR (D5) + a sub-task in T2.2 (dispatch) + a guard/assert in T1.1. Without it, the slice ships a silent-corruption bug on OpenAI-dim vectors — exactly the failure Rule 8 exists to prevent.
