# theodb_columnar zone-map skip-pruning: SIFT-free 1M verdict

**Date:** 2026-07-18 · **Module:** `theodb_rs/src/am/zonemap.rs` (pure `chunk_can_match`),
`am/columnar_agg.rs` (`extract_zone_predicate` + `admit` widening + `custom_private` carry),
`am/columnar.rs::decode_columns` (skip guard + metric), `am/df_executor.rs` (`build_filter_expr` + Filter),
`am/guc.rs` (`theodb.columnar_zonemap_skip`). Own-code; Apache-permissive (D1).

**What is measured** (DigitalOcean **c-8** dedicated, PG 17.10 + pgrx 0.19.0, `shared_buffers=2GB`): a clustered
**1M-row** `theodb_columnar` table (sorted on `y` → tight per-chunk-group ranges, 100 chunk groups of 10 000 rows).
A ~**10%-selective** range filtered aggregate `SELECT sum(x) FROM cz WHERE y BETWEEN 450000 AND 550000` is run with
`theodb.columnar_zonemap_skip` **ON vs OFF** on the SAME table (the kill-switch isolates the effect). Reproduce:
`benchmarks/columnar_zonemap_ab.py`. Raw: `columnar-zonemap-verdict.json`.

**Goal:** byte-identical result AND the skip path decodes **≤ 25%** of the chunk groups the skip-off baseline
decodes. **Result: MET.**

---

## The gap this closes

The `theodb_columnar` TAM already **wrote** a per-`(chunk_group, column)` min/max zone-map (`compute_minmax`) but
**never read it** — the M99/M100/M103 verdicts named the missing consumer ("without WHERE", "where pruning
unproven", "skip chunk groups via the min/max directory this milestone already writes"). Projection pushdown (decode
fewer columns) existed; **predicate pushdown (skip rows)** — the defining columnar advantage on selection — did not.
This slice builds the consumer.

## How it works (correctness first — ADR D3/D5)

1. The M100 planner `CustomScan` (`admit`) now **accepts a `WHERE`** and extracts `col <op> const` predicates —
   resolving the operator via its **btree strategy number** in the column type's default opfamily, and requiring the
   const be the SAME native type as the column (**D5 same-domain-or-fallback**; a cross-type literal or any
   un-pushable qual → decline to the native plan, which applies the WHERE correctly).
2. The predicates are carried plan-time → exec in the `CustomScan`'s `custom_private` IntList.
3. A **DataFusion Filter** (`build_filter_expr`) applies the predicate on the decoded batch — the **final authority**
   over surviving rows.
4. `decode_columns` **skips** a chunk group when any predicate's min/max PROVES no row can match (`chunk_can_match`,
   proven off-PG) — an **admission filter** on top of (3), fail-safe on `has_minmax=false` / unknown column.

Because (4) only drops chunk groups that provably contribute zero matching rows and (3) filters the survivors, the
aggregate is **byte-identical** to a full decode.

## Result 1 — Correctness (D3 hard gate): byte-identical, CustomScan engaged

`EXPLAIN` shows `Custom Scan (theodb_columnar_agg)` on the filtered aggregate (the WHERE is admitted). Skip-ON vs
skip-OFF return the identical scalar across every shape:

| range | matched chunk groups | skip-on sum | skip-off sum | identical |
|---|---|---:|---:|:---:|
| `5000..15000` (partial overlap of cg1+cg2) | partial | 30008 | 30008 | ✅ |
| `25000..26000` (only cg3; skips 2/3) | 1 | 3003 | 3003 | ✅ |
| `1..30000` (all; no skip) | 3 | 90000 | 90000 | ✅ |
| `999999..1000000` (empty; skips all) | 0 | ∅ | ∅ | ✅ |

The **partial-overlap** case (`5000..15000`) is the one that proves soundness: the skip drops cg3, and the DataFusion
Filter drops the non-matching rows inside cg1/cg2 — without it the aggregate would over-count.

## Result 2 — Effectiveness (the Goal metric), 1M clustered, 10%-selective

| metric | value |
|---|---:|
| chunk groups total | 100 |
| chunk groups **skipped** | **89** |
| chunk groups decoded | 11 (**11%** ≤ 25% target) |
| latency skip-ON | **19.3 ms** |
| latency skip-OFF | 140.8 ms |
| **skip speedup** | **7.29×** |

The zone-map directory skips 89/100 chunk groups for the 10%-selective range (the metric fires under
`THEODB_SCAN_PROFILE=1`), decoding only 11% of the data for a **measured 7.29× lower latency**, byte-identical.

---

## Verdict (honest)

- **GOAL MET.** The zone-map skip-pruning consumer is **correct** (byte-identical, incl. the partial-overlap case)
  and **effective** (skips 89% of chunk groups → 7.29× lower latency on a clustered 10%-selective range).
- This **closes the columnar "where pruning unproven" gap** (M99/M100/M103): the min/max directory the TAM already
  wrote is now consumed. The columnar pillar goes from "compress + prune columns" to "compress + prune columns **and
  rows**" — the defining columnar advantage on selection.
- Unlike the E1/E2 vector slices (honest-negatives against a paradigm ceiling), this is a **real measured win** on a
  differentiated axis (the lakehouse/columnar bet D2, not the vector-QPS ceiling).

## Caveats (honest)

- The skip ratio tracks **selectivity × clustering**. On an UNSORTED column, a 10%-selective range prunes little
  (each 10k-row chunk group spans the whole domain). The **7.29× is on a clustered column** — the claim is exactly
  that, not an unconditional speedup (`public-copy.md` rule 5). Real workloads benefit when the filter column is
  correlated with insert order (time-series, monotonic ids) or after `CLUSTER`.
- Scope: `col <op> const` on native-min/max types (I2/I4/I8/F4/F8/Bool), the M100 CustomScan aggregate path. Deferred
  (separate measured slices): text/date zone-maps (`MinMaxKind` extension), the seqscan path, OR/composite predicates,
  out-of-RAM. Any un-pushable WHERE falls back to the native plan (correct, unpruned).
- Warm (in-`shared_buffers`) regime; the out-of-RAM regime (where skipping avoids disk reads) would show a larger
  win but was out of scope.
