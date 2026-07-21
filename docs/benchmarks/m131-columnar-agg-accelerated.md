# M131 — #135 fixed: columnar-aggregate pushdown unblocked + measured accelerated ClickBench

> Measured 2026-07-21 against self-hosted TheoDB PG17 (`theodb_rs` rebuilt with the M131 fix) on a shared
> DigitalOcean droplet (165.227.121.20, **NOT canonical hardware**). Root cause established by a **live gdb
> backtrace** — see `knowledge-base/discoveries/blueprints/columnar-agg-planner-hang-blueprint.md`.
> Artifacts: [`m131-explain-sweep.json`](./m131-explain-sweep.json) (produced by `scripts/m131_sweep.sh`),
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
Because the node is inserted **post-`set_plan_refs`**, `set_customscan_references` never re-processes it, which is
why the defect stayed invisible until a query ordered by an aggregate. Note `custom_scan_tlist` is **not** merely
deparse metadata: for `scanrelid = 0` it also becomes the node's **runtime scan `TupleDesc`**
(`nodeCustom.c: ExecTypeFromTL(cscan->custom_scan_tlist)`). Execution stayed byte-identical only because the
replacement list is **descriptor-equal** to the one it replaces — same length, and same `exprType`/`exprTypmod`/
`exprCollation`/`resname`/`resjunk` per entry, which holds by construction (a group key is admitted only as a bare
base-rel `Var`, and an aggregate entry is a copy of the same `Aggref`). That invariant is documented at the fix and
is why 43/43 came out identical by construction rather than by luck.

**The fix** (`theodb_rs/src/am/columnar_agg.rs::deparse_safe_tlist`): `custom_scan_tlist` now carries **non-special**
expressions — group keys become base-rel `Var`s, aggregates keep their `Aggref` with argument `Var`s rebuilt against
the base rel (the post-setrefs argument is `OUTER_VAR` into the dropped child subtree). `plan.targetlist` is
unchanged, so the executed output — and every query result — is untouched.

## Measured: EXPLAIN sweep over all 43 ClickBench queries (pushdown ON)

Artifact `m131-explain-sweep.json`, produced by the committed `scripts/m131_sweep.sh` (which exits non-zero if any
query hangs — a standing #135 regression gate).

| | before the fix | after the fix (committed artifact) |
|---|---|---|
| **Queries that hang** | **2** (Q16, Q33) | **0** |
| Max EXPLAIN time | ∞ (uninterruptible; needed a server restart) | **60 ms** |
| Q16 (`GROUP BY UserID ORDER BY COUNT(*) DESC LIMIT 10`) | hang | **31 ms**, CustomScan engaged |
| Q33 (`GROUP BY WatchID, ClientIP ORDER BY c DESC`) | hang | **30 ms**, CustomScan engaged |
| Queries engaging the columnar-agg CustomScan | — (see note) | **6** |

**Provenance of the "before" column (honesty):** `hung = 2` is sourced — it is the pre-fix sweep recorded in the
discovery blueprint (`columnar-agg-planner-hang-blueprint.md`, reproduced on the pre-fix build with the gdb
backtrace). The pre-fix *CustomScan-engaged* count is deliberately **left blank**: it was observed as 4 during
diagnosis but was **never archived as an artifact**, and an un-backed number does not belong in an evidence table.

`statement_timeout` never fired on the hang because it does not apply during plan printing — the only recovery was a
server restart. During diagnosis, six stuck backends were found spinning in this loop; the capture:

```
$ ps -eo pid,etimes,pcpu,comm,args --sort=-pcpu | grep '[p]ostgres'
1889678    2264 79.1 postgres  postgres: postgres postgres [local] EXPLAIN
1892348    2244 78.6 postgres  postgres: postgres postgres [local] EXPLAIN
1900870    2178 76.5 postgres  postgres: postgres postgres [local] EXPLAIN
2163946     212 72.7 postgres  postgres: postgres postgres [local] EXPLAIN
2166766     191 71.5 postgres  postgres: postgres postgres [local] EXPLAIN
2173291     142 66.5 postgres  postgres: postgres postgres [local] EXPLAIN
```

They also blocked a `pg_ctl -m fast` shutdown until SIGKILLed. That is the "zombie backends saturating columnar
tables" symptom noted during M128 — now **root-caused to this defect**.

## Measured: accelerated vs storage-path ClickBench (same box, n = 100 000)

Both runs use the identical harness and dataset on the same box, minutes apart — the only difference is
`theodb.enable_columnar_agg`. The byte-identical A/B oracle (columnar vs heap) is preserved in both.

| Run | hot geomean (43 queries) | CustomScan engaged | result A/B |
|---|---|---|---|
| **Accelerated** (`--agg`) | **0.8962 s** | 6 | **byte-identical 43/43** (diverged 0) |
| Storage path (control) | 1.6998 s | 0 | byte-identical 43/43 (diverged 0) |

**Full-suite hot geomean: 1.90×** with the pushdown engaged. Per query, on the six the pushdown touches:

| Query | accelerated | storage path | ratio | mechanism | A/B |
|---|---|---|---|---|---|
| q0 | 0.0125 s | 1.5361 s | 122.9× | vectorized aggregate pushdown | byte-identical |
| q2 | 0.0214 s | 1.5146 s | 70.8× | vectorized aggregate pushdown | byte-identical |
| q3 | 0.0130 s | 1.5432 s | 118.7× | vectorized aggregate pushdown | byte-identical |
| q6 | 0.0015 s | 1.6160 s | 1077.3× | **zone-map directory fast-path**, not the vectorized pushdown — see note | byte-identical |
| **q15 (= Q16 — previously hung)** | 0.0144 s | 1.5242 s | 105.8× | vectorized aggregate pushdown | byte-identical |
| **q32 (= Q33 — previously hung)** | 0.3220 s | 1.8208 s | 5.7× | vectorized aggregate pushdown | byte-identical |
| **subset total** | **0.385 s** | **9.555 s** | **24.8×** | — | 6/6 byte-identical |

**Note on q6 (the 1077× outlier):** q6 is `SELECT MIN(EventDate), MAX(EventDate) FROM hits`. That is answered by the
**zone-map directory fast-path** (shipped v0.105.0, independently measured ~1300–1400×) — the min/max come from
directory metadata without decoding a chunk. It is *unblocked* by this fix (the query previously could not be planned
with the pushdown on) but the 1077× is **not** attributable to the vectorized aggregate pushdown. Excluding q6, the
remaining five accelerated queries total 0.383 s vs 7.939 s = **20.7×**.

The two queries that previously hung are now among the accelerated ones — the fix does not merely stop the hang, it
unlocks the pushdown on exactly the shapes that were blocked.

### Noise floor and dispersion (what the geomean does NOT show)

The other **37 queries engage no CustomScan in either run**, so no code path differs between the two configurations.
Their measured ratio is therefore the box's noise floor:

- geomean **1.008×**, σ **0.094**, min **0.79×**.
- **17 of the 37 are measurably *slower* with the pushdown ON** (worst 0.79×, i.e. −21%). Since no code path
  differs for them, this is run-to-run variance on a shared box, not a regression — but it is disclosed rather than
  omitted, because "which queries got worse" is exactly what selective reporting hides.

**Run count (honest limitation):** the harness protocol is 3 runs *per query* (cold = 1st, hot = min of the 2 hot
runs), but only **one suite run per configuration** was executed — there is no mean ± σ across repeated suite runs,
so the point estimates above are single-suite. The measured effect (1.90× suite, 24.8× subset) is roughly an order of
magnitude larger than the ±21 % noise floor, so the conclusion survives; a multi-suite repetition would tighten the
estimate and is the honest follow-up.

## Scope & caveats (honest framing)

- **Self-hosted shared droplet — NOT canonical hardware**, and `hits` is a **100 000-row subsample** (ClickBench's
  canonical run is 100 M rows on `c6a.4xlarge`). These numbers are an **internal A/B of TheoDB against itself**
  (pushdown ON vs OFF on the same box), **not** a competitive claim and **not** a leaderboard result. No
  "faster than <another database>" is asserted (`rules/public-copy.md § 4`).
- The ratios measure the **vectorized aggregate pushdown (plus, for q6, the zone-map fast-path) vs PostgreSQL's
  native executor over columnar storage** — both are TheoDB paths.
- **Correctness is gated, not assumed:** every one of the 43 queries is byte-identical to the heap result in both
  runs — the oracle ClickBench itself lacks (its `check` is a `SELECT 1`).
- The regression test asserts the **real trigger** (EXPLAIN + `ORDER BY <aggregate>`, Q16/Q33 shapes + result parity
  vs heap), not table width — a width-based test would pass while the actual defect regressed.

## Reproduction

```bash
export PGHOST=localhost PGPORT=28900 PGUSER=postgres PGDATABASE=postgres PGPASSWORD=postgres

# 43-query EXPLAIN sweep with the pushdown ON — exits non-zero if any query hangs (#135 regression gate)
PSQL_BIN=/path/to/psql QUERIES=benchmarks/clickbench/theodb/queries.sql \
OUT=docs/benchmarks/m131-explain-sweep.json bash scripts/m131_sweep.sh

# accelerated vs storage-path ClickBench, byte-identical A/B preserved in both
python3 benchmarks/run_m128_clickbench.py --agg --n 100000 --out docs/benchmarks/m131-clickbench-agg-on.json
python3 benchmarks/run_m128_clickbench.py        --n 100000 --out docs/benchmarks/m131-clickbench-agg-off.json
```

Regression test: `theodb_rs/src/am/columnar_agg.rs::test_m131_explain_orderby_aggregate_deparses`.

## Verdict

**#135 FIXED and MEASURED.** The EXPLAIN hang is gone (43/43 plan, max 60 ms, was 2 hangs), the columnar-aggregate
pushdown is usable on the real wide ClickBench table, and the accelerated run is **byte-identical 43/43** while
measuring **1.90× on the full-suite hot geomean** and **24.8× across the six queries it touches** (20.7× excluding
the zone-map-served q6) — against a storage-path control on the same box, with the ±21 % noise floor disclosed. The
issue's "planner hang / O(cols²) / wide-table" diagnosis is corrected in the record: the defect was an
EXPLAIN-deparse infinite recursion triggered by `ORDER BY <aggregate>`. Numbers are a single-suite, self-hosted
internal A/B on non-canonical hardware, **not** a competitive claim.
