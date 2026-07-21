# M131 — #135 fixed: columnar-aggregate pushdown unblocked + measured accelerated ClickBench

> Measured 2026-07-21 against self-hosted TheoDB PG17 (`theodb_rs` rebuilt with the M131 fix) on a shared
> DigitalOcean droplet (165.227.121.20, **NOT canonical hardware**). Root cause established by a **live gdb
> backtrace** — see `knowledge-base/discoveries/blueprints/columnar-agg-planner-hang-blueprint.md`.
> Artifacts: [`m131-explain-sweep.json`](./m131-explain-sweep.json),
> [`m131-clickbench-agg-on.json`](./m131-clickbench-agg-on.json) (accelerated),
> [`m131-clickbench-agg-off.json`](./m131-clickbench-agg-off.json) (storage-path control).

## The defect (and why #135's diagnosis was wrong)

#135 reported an "uninterruptible **planner** hang, O(cols²) on wide mixed-type tables". A live gdb backtrace of the
hung backend **falsifies all three claims**:

```
#0  check_stack_depth / get_tle_by_resno
#1  resolve_special_varno          (ruleutils.c:7699)
#2  resolve_special_varno          (ruleutils.c:7674)   ← recursing into itself
#3  get_variable → deparse_expression
#6  show_sort_group_keys ("Sort Key") → show_sort_keys
#8  ExplainNode → ExplainPrintPlan → ExplainOnePlan     ← EXPLAIN plan-PRINTING
```

- **Not the planner** — it is EXPLAIN **deparse** of the Sort node's keys.
- **Not execution** — the affected query executes correctly in **0.537 s**.
- **Not width/TEXT** — a plain `GROUP BY userid` on the same 105-column table plans in **27 ms**. The real trigger is
  **`ORDER BY <aggregate>` above the CustomScan** (exactly ClickBench Q16 and Q33).

**Root cause:** the M115 Agg-swap published a **self-referential `custom_scan_tlist`** — every entry was
`Var(INDEX_VAR, i)`. For a `scanrelid = 0` CustomScan, ruleutils resolves an upper node's Var through
`custom_scan_tlist`; finding another `INDEX_VAR` at the same position, `resolve_special_varno` recursed forever.
Because the node is inserted **post-`set_plan_refs`**, `set_customscan_references` never processes it — EXPLAIN
deparse is its only consumer, which is why the defect stayed invisible until a query ordered by an aggregate.

**The fix** (`theodb_rs/src/am/columnar_agg.rs::deparse_safe_tlist`): `custom_scan_tlist` now carries **non-special**
expressions — group keys become base-rel `Var`s, aggregates keep their `Aggref` with argument `Var`s rebuilt against
the base rel (the post-setrefs argument is `OUTER_VAR` into the dropped child subtree). `plan.targetlist` is
unchanged, so the executed output — and every query result — is untouched.

## Measured: EXPLAIN sweep over all 43 ClickBench queries (pushdown ON)

| | before the fix | after the fix |
|---|---|---|
| **Queries that hang** | **2** (Q16, Q33) | **0** |
| Max EXPLAIN time | ∞ (uninterruptible; needed a server restart) | **54 ms** |
| Q16 (`GROUP BY UserID ORDER BY COUNT(*) DESC LIMIT 10`) | hang | **39 ms**, CustomScan engaged |
| Q33 (`GROUP BY WatchID, ClientIP ORDER BY c DESC`) | hang | **34 ms**, CustomScan engaged |
| Queries engaging the columnar-agg CustomScan | 4 | **6** |

`statement_timeout` never fired on the hang because it does not apply during plan printing — the only recovery was a
server restart. During diagnosis, **6 stuck backends were found burning 70–79 % CPU each** in this loop; that is the
"zombie backends saturating columnar tables" symptom noted during M128, now **root-caused to this defect**.

## Measured: accelerated vs storage-path ClickBench (same box, n = 100 000)

Both runs use the identical harness and dataset on the same box, minutes apart — the only difference is
`theodb.enable_columnar_agg`. The byte-identical A/B oracle (columnar vs heap) is preserved in both.

| Run | hot geomean | CustomScan engaged | result A/B |
|---|---|---|---|
| **Accelerated** (`--agg`) | **0.8887 s** | 6 | **byte-identical 43/43** (diverged 0) |
| Storage path (control) | 1.6998 s | 0 | byte-identical 43/43 (diverged 0) |

**Full-suite hot geomean: 1.91× faster** with the pushdown engaged. On the six queries the pushdown actually
accelerates:

| Query | accelerated | storage path | speedup | A/B |
|---|---|---|---|---|
| q0 | 0.0123 s | 1.5361 s | **124.9×** | byte-identical |
| q2 | 0.0240 s | 1.5146 s | **63.1×** | byte-identical |
| q3 | 0.0105 s | 1.5432 s | **147.0×** | byte-identical |
| q6 | 0.0017 s | 1.6160 s | **950.6×** | byte-identical |
| **q15 (= Q16 — previously hung)** | 0.0135 s | 1.5242 s | **112.9×** | byte-identical |
| **q32 (= Q33 — previously hung)** | 0.3879 s | 1.8208 s | **4.7×** | byte-identical |
| **subset total** | **0.450 s** | **9.555 s** | **21.2×** | 6/6 byte-identical |

The two queries that previously hung are now among the accelerated ones — the fix does not merely stop the hang, it
unlocks the pushdown on exactly the shapes that were blocked.

## Scope & caveats (honest framing)

- **Self-hosted shared droplet — NOT canonical hardware**, and `hits` is a **100 000-row subsample** (ClickBench's
  canonical run is 100 M rows on `c6a.4xlarge`). These numbers are an **internal A/B of TheoDB against itself**
  (pushdown ON vs OFF on the same box), **not** a competitive claim and **not** a leaderboard result. No "faster
  than <another database>" is asserted (`rules/public-copy.md § 4`).
- The speedups measure the **vectorized aggregate pushdown vs PostgreSQL's native executor over columnar storage** —
  both are TheoDB paths.
- **Correctness is gated, not assumed:** every one of the 43 queries is byte-identical to the heap result in both
  runs — the oracle ClickBench itself lacks (its `check` is a `SELECT 1`).
- The regression test asserts the **real trigger** (EXPLAIN + `ORDER BY <aggregate>`, Q16/Q33 shapes + result parity
  vs heap), not table width — a width-based test would pass while the actual defect regressed.

## Reproduction

```bash
export PGHOST=localhost PGPORT=28900 PGUSER=postgres PGDATABASE=postgres PGPASSWORD=postgres

# 43-query EXPLAIN sweep with the pushdown ON (expect hung=0)
bash benchmarks/clickbench/theodb/../../../scripts/m131_sweep.sh   # or the inline sweep in this milestone's log

# accelerated vs storage-path ClickBench, byte-identical A/B preserved
python3 benchmarks/run_m128_clickbench.py --agg --n 100000 --out docs/benchmarks/m131-clickbench-agg-on.json
python3 benchmarks/run_m128_clickbench.py        --n 100000 --out docs/benchmarks/m131-clickbench-agg-off.json
```

Regression test: `theodb_rs/src/am/columnar_agg.rs::test_m131_explain_orderby_aggregate_deparses`.

## Verdict

**#135 FIXED and MEASURED.** The EXPLAIN hang is gone (43/43 plan, max 54 ms, was 2 hangs), the columnar-aggregate
pushdown is usable on the real wide ClickBench table, and the accelerated run is **byte-identical 43/43** while being
**1.91× faster on the full-suite geomean** and **21.2× on the six queries it accelerates** — measured on the same box
against the storage-path control. The issue's "planner hang / O(cols²) / wide-table" diagnosis is corrected in the
record: the defect was an EXPLAIN-deparse infinite recursion triggered by `ORDER BY <aggregate>`. Numbers are a
self-hosted internal A/B on non-canonical hardware, **not** a competitive claim.
