# Review — M131 fix #135 (columnar-agg CustomScan EXPLAIN deparse recursion)

**Slug:** columnar-agg-planner-hang-fix
**Milestone:** M131
**Date:** 2026-07-21
**Reviewers:** council-rust-pgrx (unsafe/FFI/node-lifetime) + council-benchmark (measurement honesty)
**Verdict:** READY_TO_MERGE

## Scope

The M131 fix for issue #135: `theodb_rs/src/am/columnar_agg.rs::deparse_safe_tlist` (new) replacing the
self-referential `custom_scan_tlist`, its regression test, `scripts/m131_sweep.sh`, the `--agg` path of
`benchmarks/run_m128_clickbench.py`, and `docs/benchmarks/m131-columnar-agg-accelerated.md` + 3 artifacts.

## Findings

Both reviews returned **0 BLOCKER, 0 HIGH**. All MEDIUM/LOW resolved:

| Sev | Reviewer | Finding | Resolution |
|---|---|---|---|
| MEDIUM | rust-pgrx | Doc claimed "EXPLAIN deparse is `custom_scan_tlist`'s only consumer" — **false**: for `scanrelid=0` it is also the runtime scan `TupleDesc` (`ExecTypeFromTL`, `nodeCustom.c`) | RESOLVED — the **descriptor-equality invariant** is now documented at the fix, in the evidence, and in the CHANGELOG; byte-identity holds by construction, not by luck |
| MEDIUM | rust-pgrx | Degradation paths were not fail-closed; a short tlist would corrupt the runtime descriptor (dropped column / read past the scan slot) | RESOLVED — every path returns NIL; `try_swap_agg` declines the swap (native plan) and adds an explicit arity guard vs `out_arity` |
| LOW | rust-pgrx | Half-defensive null check on `copyObjectImpl` | RESOLVED — null now declines |
| HIGH | benchmark | The EXPLAIN sweep had **no committed producer**; the repro path did not exist → headline unreproducible | RESOLVED — `scripts/m131_sweep.sh` committed, parameterized (env vars, no droplet paths), **verified reproducible**, and exits non-zero if any query hangs (standing regression gate) |
| MEDIUM | benchmark | Pre-fix "CustomScan engaged = 4" asserted without an artifact | RESOLVED — removed; provenance of the `hung = 2` "before" value explained (blueprint + gdb) |
| MEDIUM | benchmark | 15 queries measurably slower; noise floor undisclosed | RESOLVED — 37 untouched queries: geomean 1.008×, σ 0.094, **17 slower** (worst −21 %) disclosed |
| MEDIUM | benchmark | n=1 suite run per side, no dispersion stated | RESOLVED — stated explicitly as a limitation; effect is ~an order of magnitude above the noise floor |
| MEDIUM | benchmark | The outlier credited to the wrong mechanism | RESOLVED — q6 attributed to the **zone-map directory fast-path** (v0.105.0), not the pushdown; 20.7× reported excluding it |
| MEDIUM | benchmark | "6 stuck backends at 70–79 % CPU" unsourced | RESOLVED — the `ps` capture embedded in the evidence |
| LOW | benchmark | Harness comment still asserted the falsified "planner hang" diagnosis | RESOLVED — corrected |

council-benchmark independently **recomputed every headline number as EXACT** against the committed JSONs.

## Re-verification on the shipped (fail-closed) binary

The fail-closed changes were rebuilt and everything re-measured; all committed artifacts now come from the shipped build:

- EXPLAIN sweep (`scripts/m131_sweep.sh`): **43 queries, hung = 0, CustomScan engaged = 6, max 60 ms**, Q16 31 ms, Q33 30 ms. Exit 0.
- Accelerated ClickBench: **byte-identical 43/43** (diverged 0), geomean 0.8962 s vs 1.6998 s storage-path control = **1.90×**; subset **24.8×** (20.7× excluding the zone-map-served q6).
- No regression from the arity guard: the CustomScan still engages on 6 queries.

## DoD check (plan `columnar-agg-planner-hang-fix-plan.md`)

| DoD item | Status |
|---|---|
| `custom_scan_tlist` contains no special-varno Var | Met ✓ (fix + test; 0 hangs proves termination) |
| 43-query sweep `hung = 0` (was 2), max < 1000 ms | Met ✓ (0; max 60 ms) |
| Q16/Q33 plans contain `theodb_columnar_agg` | Met ✓ (`customscan = 1` both) |
| Regression test on the real trigger | Met ✓ (`test_m131_explain_orderby_aggregate_deparses`, Q16/Q33 shapes + heap parity) |
| Accelerated ClickBench byte-identical 43/43 + timings | Met ✓ |
| Evidence + JSON + CHANGELOG committed | Met ✓ |
| `NOT canonical hardware` present, no unqualified `faster than` | Met ✓ (grep-asserted) |

## Verdict

**READY_TO_MERGE.** 0 residual BLOCKER/HIGH. The fix is memory-safe (copy-before-mutate verified), fail-closed, and
the descriptor-equality invariant that makes it correct is now explicit rather than accidental. Every measured
number resolves to a committed artifact produced by the shipped binary, with the noise floor and single-suite
limitation disclosed.
