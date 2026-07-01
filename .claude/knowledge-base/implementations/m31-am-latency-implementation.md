# M31 index AM latency — implementation summary

**Slug:** m31-am-latency · **milestone_id:** M31 · **Date:** 2026-07-01
**Plan:** `.claude/knowledge-base/plans/m31-am-latency-plan.md` (SHIPPABLE 99.2)
**Re-scope:** `docs/adr/0011-m31-rescope-simd-followup.md` (CTO) · **Verdict:** READY_TO_MERGE

Closed the M26 O(N)-per-scan gap for `theodb_ivfflat` via structured partial-page reads. Part of the P0
vector-superiority track (`memory: goto-p0-vector-superiority`).

## Commits

| SHA | Summary |
|---|---|
| `da0e571` | structured IVFFlat partial-page reads (meta+centroids+list pages; probed-only scan; scratch-buffer scoring; format-aware maintenance) |
| `0054944` | re-scope (ADR 0011) — O(N) closed now, SIMD parity → M31b + benchmark evidence |
| `d50c7eb` | /review fix — BLOCKER (empty-built-index pending drop) + LOW nits |

## Result (measured, n=100k dim=128)

- theodb_ivfflat Index Scan p50: **~38 ms** (M26 O(N) blob: ~1700 ms → **~45× faster**); pgvector: ~14 ms (**~2.7× behind**).
- The O(N) **algorithmic** gap is CLOSED (reads ∝ probes, ~same pages as pgvector); the residual is the
  **constant factor** (scalar/SSE2 vs pgvector AVX-SIMD C) → **M31b** (SIMD distance), sequenced before M32.
- Correctness 100%: recall preserved; INSERT (pending) / DELETE (MVCC) / VACUUM (structured fold) intact.

## Re-scoped DoD (ADR 0011) — MET

- [x] Scan reads only probed lists' pages (structured layout) — not the whole blob.
- [x] Benchmark: p50 far below the O(N) regime + within a documented band of pgvector; recall preserved (`test_index_am_latency.py`, `docs/benchmarks/m31-am-latency.{md,json}`).
- [x] No regression: `test_index_am.py` (incl. empty-built-index regression) + M20–M22 coexistence green (51 tests).
- [x] ADR 0010 §D2/D5 updated (O(N) closed for IVFFlat, with number; SIMD-parity → M31b).

## Honesty (Rule 3)

The original DoD ("p50 ≤ pgvector") was NOT met by evidence (2.7× behind). Rather than fake the benchmark or grind
SIMD unscoped, the CTO re-scoped M31 to its proven achievement and created M31b for the SIMD parity — the real
number (2.7×) is recorded, not hidden. This is the measurement-first P0 discipline: structural + algorithmic
parity reached; latency-superiority is the next measured slice.
