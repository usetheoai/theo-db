---
slug: m109-msbfs-operator
milestone_id: M109
date: 2026-07-16
cycle: review
---

# /review — M109 vectorized Multi-Source BFS operator

**Verdict:** READY_TO_MERGE

Two independent adversarial reviewers: **council-index-storage** (kernel correctness) + **council-benchmark**
(measurement honesty — critical here because the first benchmark was confounded).

## council-index-storage — correctness: NO BLOCKER / HIGH / MEDIUM

Traced the kernel end-to-end; sound on every question:
- **Bitmask/tiling (Q1):** tile-local bit `l` ↔ global lane `base+l` symmetric on seed-setup and extraction;
  fresh `visit`/`seen` per tile → no cross-tile leak; max shift `1<<63` (no overflow). SOUND.
- **Frontier-driven mid-hop `seen` mutation (Q2):** monotonic OR; `new_bits = vv & !seen[ni]` reads previous
  hop's `visit`; two lanes reaching the same vertex same-hop don't block each other (disjoint bits). SOUND.
- **Decomposition invariant (Q3):** `expand_multi([[s]],h)[0]` provably set-equals `expand(&[s],h)`. SOUND.
- **`load_cached_csr` refactor (Q4):** preserves M108 epoch-invalidation + OID-reuse safety; no borrow-across-
  SPI, no Rc hazard. SOUND.
- **Seeds/memory/pgrx boundary (Q5–7):** dedup/out-of-range/empty/`nsets==0` all safe; O(nn)-per-tile memory
  bounded; length-mismatch → typed `ereport` (not panic across C); `'static` iterators sound. SOUND.
- **LOW (test-strength, addressed):** the per-lane oracle uses XOR set-hash (hash-only). Mitigated by
  `lanes_independent` (count check) + the crossover oracle keying `set_id` into the hash. Not a production bug.

## council-benchmark — honesty: HONEST; 4 LOW findings, ALL FIXED

- **Confound-fix legitimate (Q1):** both sides count in Rust (`graph_expand_multi_card` vs `graph_expand_card`);
  the batched `_card` path even *builds* the full node Vecs then discards them → isolation is fair-to-
  conservative. 1-SPI-call amortization honestly disclosed. HONEST.
- **Oracle integrity (Q4):** lane-keyed `bit_xor(hashint8(set_id*P+node))`, run before timing at every N,
  compares batched-per-lane vs N sequential expands exactly (not a weaker count). HONEST.
- **[LOW-MED] std dev promised but not reported → FIXED:** benchmark now computes + reports mean±std per point
  (json `batched_std`/`seq_card_std`, md table). N=1 crossover now defensible (1.63±0.08 vs 2.77±0.81 = 1.70×).
- **[LOW] topology-gaming concern → FIXED with evidence:** added a **uniform-random** graph floor datapoint —
  N=64 gives ~10.2× (STRONGER than hub), refuting the hub-best-case worry. The win is topology-robust.
- **[LOW] "~19×" rounded 18.83 / led with peak → FIXED:** prose now uses the honest range "~5–8× pure";
  naive numbers shown in the curve, not led with as a headline.
- **[LOW] hardware/build-flags unspecified → FIXED:** md now states CPU (DO-Regular 4 vCPU @ 2.0GHz), pgrx/PG
  version, release build.

## Findings — accepted (tracked)

- **[LOW]** run-to-run variance on the shared DO-Regular vCPU (disclosed via ±std); the ~5–8× headline is far
  outside noise, individual points shift.
- **[LOW]** XOR set-oracle theoretical collision (same discipline accepted in M108).

## Hard gates
✅ no BLOCKER · ✅ no secrets · ✅ no main commit · ✅ commit-trailer policy honored · ✅ CHANGELOG updated ·
✅ benchmark artifact with mean±std + oracle PASS + topology floor · 337 pg_tests GREEN (+7 M109, 0 regression).

## DoD (ROADMAP M109)
(1) MS-BFS own-code ✅ (scope corrected ADR-1) · (2) bounded ≤H set-hash == theo-rag ✅ · (3) benchmark N-seeds
✅ (measured 5–8×, mean±std) · (4) M108 AM integration ✅ · GATE (oracle + throughput + N-seeds gain) ✅.
