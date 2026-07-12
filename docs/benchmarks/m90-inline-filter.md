# M90 — inline label filter (v7): recall@10 1.00 vs 0.52 (M87 post-filter) @ ~1% selectivity, MEASURED

**Date:** 2026-07-12 · **Host:** DO c-8 (Intel Xeon Platinum 8280, 8 vCPU, 15 GB) · **Verdict:** `GO (inline > post)`

M90 pushes a label filter INTO the IVF-AQ traversal (Approach A — scan-key/label-in-index, the pgvectorscale
mechanism, own code). The gate: does the inline skip beat the M87 post-filter recall at the selectivity where the
post-filter degrades? **It does — decisively.**

## Result (500k synthetic, 200 well-separated clusters, ~1% label selectivity, probes=32, k=10, 100 queries)

| strategy | index | recall@10 | QPS | build |
|---|---|---:|---:|---:|
| **INLINE (v7)** | `theodb_ivfflat (e, lbl)` | **1.0000** | **394.7** | 177 s |
| M87 POST-filter (v5) | `theodb_ivfflat (e)` + `WHERE lbl && …` | 0.5180 | 21.0 | 176 s |
| **delta** | | **+0.4820** | **~19×** | — |

Ground truth = exact seqscan-filtered top-10 (`WHERE lbl && '{L}' ORDER BY e <-> q LIMIT 10`), same 100 queries,
same data. The label filter is ~1% selective (100 labels, one per row). Recall = |index-filtered ∩ exact-filtered|
/ |exact-filtered|, averaged.

## Why the inline wins so much at ~1% selectivity

- **M87 post-filter (v5):** the AM returns candidates in vector-distance order; the executor filters `lbl && …`
  AFTER. At ~1% selectivity, almost all of the probed lists' rerank-pool candidates fail the filter, so the
  filtered top-10 is starved (recall 0.52) and the iterative re-search grows probes across many lists chasing the
  few matches (→ 21.0 QPS).
- **INLINE (v7):** the label is co-located in the Stage-1 code pages; non-overlapping candidates are SKIPPED before
  they cost a rerank slot, so the rerank pool fills with MATCHING candidates → the filtered top-10 is complete
  (recall 1.00) and no expensive re-search is needed (→ 394.7 QPS). `xs_recheck=true` guarantees correctness.

The QPS gap (~19×) is a bonus — the DoD only required **recall inline > post**; we got +0.48 recall AND ~19× QPS.

**Honest read of the 1.00 (easy-data-favored — the delta is the load-bearing result, not the perfect number):** the
absolute recall@10 = 1.00 is favored by well-separated clusters + on-centroid queries (the queries are existing rows
sitting at their cluster centroid), so at probes=32 the filtered neighbors reliably fall in the probed lists. On
harder/overlapping data or off-centroid queries the absolute inline recall would be < 1.00. The **data-independent,
load-bearing result is the +0.48 delta vs the post-filter on the SAME data** (v5 post got 0.52 on the same easy
data) — attributable to the post-filter's structural starvation, not to data difficulty. The GO verdict rests on
the delta, not on the perfect 1.00.

## Correctness (zero regression)

**253 pg_tests GREEN** (250 + 3 v7 tests: inline filter, VACUUM no-op, pending false-positive), **0 failed**. The v7 test asserts every
index-scan filtered row satisfies the predicate (inline skip + `xs_recheck`) and filtered recall@5 ≥ 0.8 against
exact seqscan. Vector-only and label-less v5/v6 indexes are byte-identical (v7 is opt-in on the 2nd column — new
magic only when a label column is declared).

## How (Approach A — own code, Rule 9)

- **opclass:** `amcanmulticol=true` + a DEFAULT label opclass `theodb_ivfflat_label_ops` (`OPERATOR 1 &&` on
  `smallint[]`) backed by own `theodb_smallint_array_overlap` → the planner pushes `lbl && '{…}'` as an Index Cond.
- **v7 format:** the per-list code blob widens to `[ids][labels_fixed][codes]` (`LABEL_K=8` `smallint` slots +
  a count per vector, co-located so Stage-1 reads the label without a separate random-read). Streaming writer
  (reuses the M89 per-list flush). Page accounting identical to v5 (label region is inside the code pages).
- **scan:** `amrescan` parses the `&&` ScanKey → the query label set + sets `xs_recheck`; `scan_ivf_aq_split_v7`
  reads each candidate's co-located label in Stage-1 and skips non-overlapping before the rerank.

## Honest boundary (what M90 does NOT do)

- **Only the declared label column + `&&`, label as `smallint[]`.** `WHERE price < 100` on a regular heap column
  still post-filters — arbitrary-`WHERE` inline (Custom Scan Provider + bitmap, the AlloyDB Approach B) is **M91**.
- **Format v7 + REINDEX to use labels.** Existing vector-only / label-less indexes are unaffected (no REINDEX).
- **NOT a QPS-superiority claim vs ScaNN/AlloyDB** — the paradigm ceiling (M73/M82) stands. This is a
  **recall-stable-under-selective-label-filter** result (with a large QPS bonus), measured.
- **Synthetic caveat:** 200 well-separated clusters give unambiguous neighbors (recall is meaningful here, unlike
  M88's tie-dense clusters). The inline-vs-post comparison is same-data, so it is valid regardless.

## Verdict

**`GO (inline > post)`.** The inline label filter (v7) delivers **recall 1.00 vs 0.52** and **~19× QPS** vs the M87
post-filter at ~1% selectivity — the medium/high-selectivity regime where post-filtering starves. DoD met with
margin. Closes the **inline** half of the filtered-vector-search gap vs AlloyDB (the **arbitrary-WHERE + adaptive**
half is M91).

## Provenance

- Implementation: commits `20f1d97` (opclass), `a368668` (label collection), `c178baf` (v7 format + inline scan).
- Blueprint: `knowledge-base/discoveries/blueprints/inline-filter-pushdown-blueprint.md`. Plan: `knowledge-base/plans/inline-filter-pushdown-plan.md`.
- Harness: `benchmarks/m90_filter_bench.py`. Raw + hardware: `docs/benchmarks/m90-inline-filter.json`. ADR: `docs/adr/0040-m90-inline-label-filter-verdict.md`.
