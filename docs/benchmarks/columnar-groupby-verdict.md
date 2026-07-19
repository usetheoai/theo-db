# theodb_columnar GROUP BY pushdown — verdict

**Date:** 2026-07-19 · **Module:** `theodb_rs/src/am/columnar_agg.rs` (`admit` accepts a `groupClause`, group-key +
output-layout parse, multi-row `exec` cursor), `am/df_executor.rs` (`run_columnar_grouped_aggs`,
`arrow_value_to_datum` reverse conversion, `run_df_collect` shared runtime). Own-code glue over the adopted DataFusion
hash-aggregate (Rule 9). Plan: `columnar-groupby-pushdown`.

**What is measured** (DigitalOcean **c-8** dedicated, PG 17.10 + pgrx 0.19.0, `shared_buffers=2GB`, `work_mem=256MB`):
a **1M-row** `theodb_columnar` table vs an identical heap table. For four GROUP BY shapes, the full **top-level**
grouped result set from the columnar `CustomScan` is compared row-by-row against the heap's native aggregate, and the
speedup is measured as GUC-on (CustomScan) vs GUC-off (native aggregate over the SAME columnar table). Reproduce:
`benchmarks/columnar_groupby_ab.py`. Raw: `columnar-groupby-verdict.json`.

**Goal:** byte-identical grouped result set + CustomScan engaged (EXPLAIN) + a measured speedup. **Result: MET.**

---

## The gap this closes

The M100 `CustomScan` admitted only a **scalar** aggregate (no GROUP BY — it declined any `groupClause`) and emitted
exactly **one** row. GROUP BY is the remaining large columnar capability — it turns the columnar aggregate from a
single global scalar into a real analytical `GROUP BY key, agg`, the canonical OLAP query. This slice adds it.

## How it works

1. **admit** (`columnar_agg.rs`) now accepts a `groupClause`: it walks the output target, classifying each expr as a
   bare group `Var` (a `build_arrow`-supported column type) or a supported `Aggref`, and builds the aggregates + group
   keys + an **output layout** (ADR-2) in target order.
2. The **executor** (`df_executor::run_columnar_grouped_aggs`) decodes the group + sum columns, runs DataFusion's
   `.aggregate(group_exprs, agg_exprs)`, and materializes the multi-row result — each output slot filled from the
   correct batch column per the layout, so PG's target order (even agg-before-key) is honored.
3. **Group keys** are converted back to PG Datums by `arrow_value_to_datum` (the reverse of `build_arrow`, all
   supported types incl. temporal), built in the executor's per-query memory context (ADR-3) so `text` key datums
   survive across the multi-row emit.
4. **exec** emits one row per call via a cursor (the scalar path is just the N=1 case).

## Result — byte-identical + measurably faster, every shape

`EXPLAIN` shows `Custom Scan (theodb_columnar_agg)` for every grouped aggregate. The full grouped result set from the
columnar CustomScan is **row-by-row identical** to the heap's native aggregate:

| shape | CustomScan | result identical | groups | latency CustomScan | latency native | speedup |
|---|:---:|:---:|---:|---:|---:|---:|
| `GROUP BY k` (int key) | YES | YES | 100 | 148.0 ms | 887.6 ms | **6.00×** |
| `GROUP BY k, k2` (multi-key) | YES | YES | 100 | 218.3 ms | 988.3 ms | **4.53×** |
| `GROUP BY d` (date, temporal key) | YES | YES | 365 | 67.8 ms | 661.1 ms | **9.75×** |
| `sum(x), k … GROUP BY k` (agg-before-key, ADR-2) | YES | YES | 100 | 148.8 ms | 863.9 ms | **5.81×** |

The `#[pg_test]`s additionally prove a `text` group key (ADR-3 varlena datum lifetime across the multi-row emit), a
NULL group, and the decline cases (GROUP BY + WHERE, and a grouping expression `date_trunc(...)` → native plan).

---

## Verdict (honest)

- **GOAL MET.** Top-level GROUP BY over a columnar table runs through the vectorized CustomScan, byte-identical to the
  native plan, **4.5×–9.75× faster** across int / multi-key / temporal / agg-before-key shapes. The column-order
  mapping (ADR-2) and the temporal-key support (reusing the zone-map temporal slice) both hold.
- The columnar pillar now supports vectorized **`GROUP BY key, sum/count`** — a real analytical engine, not just a
  global scalar.

## Caveats (honest)

- **Consuming a columnar-aggregate OUTPUT VALUE inside an enclosing expression** (e.g. `SELECT sum(s) FROM (SELECT
  k, sum(x) s FROM col GROUP BY k) q`, or an aggregate's `ORDER BY` over the agg value) hits a **pre-existing M100
  limitation** — the scalar path has the identical failure (`SELECT s+1 FROM (SELECT sum(x) s FROM col) q` also
  fails). It stems from the planner inlining the `Aggref` of a `scanrelid=0` CustomScan across a SubqueryScan-removal,
  so the synthetic tuple is re-evaluated (`cache lookup failed for attribute N of relation 0`). It is **orthogonal to
  GROUP BY** (not introduced here) and is tracked separately; the canonical top-level `SELECT key, agg FROM t GROUP BY
  key [ORDER BY …]` — the shape measured above — works.
- Scope: `count(*)` / `sum(float8)`, bare-column group keys of `build_arrow`-supported types, **no simultaneous
  WHERE** (GROUP BY + WHERE combined declines to the native plan — a later slice). `avg` / `sum(int)` / HAVING /
  grouping expressions / DISTINCT decline to the native plan.
- The speedup is GUC-on vs GUC-off on the same columnar table (isolates the vectorized path); warm regime.
