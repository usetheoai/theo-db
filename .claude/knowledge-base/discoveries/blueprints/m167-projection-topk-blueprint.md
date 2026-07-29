# Blueprint — M167: columnar projection-top-k routing (`SELECT cols WHERE pred ORDER BY <col> LIMIT k`)

**Author:** council-index-storage · **Date:** 2026-07-27 · **Status:** SHIPPABLE_WITH_CAVEATS
**Scope:** ClickBench q23–q26 (`queries.sql:24–27`, harness 0-indexed q22–q25).

## TL;DR — the premise is partly falsified by measurement (honest-negative)

The framing "q23–q26 do NOT take a late-mat top-k path today" is **wrong for q23 and q24**. The M158 machinery
(`try_swap_topk`, `columnar_agg.rs:1813`) already sorts on a **stored base column**, single-key; on the live droplet
**q23 and q24 already route byte-identically** to late-mat top-k — they don't appear in the ClickBench numbers only
because the harness never turns on the gating GUC (`enable_columnar_late_mat`, default OFF, `columnar_agg.rs:33`).
q25/q26 are genuine gaps and both are **collation / multi-key honest-negatives** in the portable case.

| Query | Sort key(s) | Verdict | Routes today (GUC on)? | Blocker |
|---|---|---|---|---|
| **q23** `SELECT * … URL LIKE '%google%' ORDER BY EventTime LIMIT 10` | EventTime (timestamp) | **ROUTE** (proven byte-identical) | Yes | GUC default OFF + harness never sets it |
| **q24** `SELECT SearchPhrase … <> '' ORDER BY EventTime LIMIT 10` | EventTime (timestamp) | **ROUTE** (proven byte-identical) | Yes | same |
| **q25** `… ORDER BY SearchPhrase LIMIT 10` | SearchPhrase (text) | **HONEST-NEGATIVE** | No (declines) | text-key collation guard `columnar_agg.rs:1927–1939` |
| **q26** `… ORDER BY EventTime, SearchPhrase LIMIT 10` | EventTime, SearchPhrase | **HONEST-NEGATIVE** | No (declines) | multi-key guard `:1853–1855` + text tiebreaker collation |

Planner-side upper-path swap only — no page/WAL/VACUUM/crash surface. Columnar table append-only, read-only for this path.

## Mechanism already present (M158 + M149 + M156)

- `columnar_project.rs` (M149) = pure projection scan, `pathkeys=null` (`:351`) — never orders; a `Sort` is planned above.
- `columnar_agg.rs::try_swap_topk` (`:1813`) detects `Limit(k) → Sort([1 key]) → CustomScan(project)` and swaps the Sort for a
  late-mat top-k node (mode==2), reusing the agg CustomScan method table (`:2004`) — so EXPLAIN prints
  `Custom Scan (theodb_columnar_agg)` for a top-k (shared method table, not a bug).
- Resolves the sort key to a **base column** incl. resjunk sort-only cols (`:1905–1919`) — `ORDER BY EventTime` not in SELECT still routes.
- `run_columnar_topk` (`df_executor.rs:764`) decodes `{proj ∪ key ∪ filter}` for all N, filters, then `df.sort([key]).limit(0, k)`
  → DataFusion bounded-heap **TopK** (O(N log k)); only the ≤k survivors become PG Datums (`:807–815`) — the M148/M158
  late-mat payoff (materialize k rows, not N). For `SELECT *` (q23) the decode set is all 105 cols (saves on datum-materialization,
  not decode); q24 decodes narrow `{SearchPhrase, EventTime}`.

## Per-query verdict

- **q23 — ROUTE (already correct).** Single timestamp sort key; `URL LIKE '%google%'` pushes (`classify_text_op` → Like;
  `extract_text_predicate:469`, dangling-`\` guard keeps error-22025). Live EXPLAIN: `Limit → Custom Scan (theodb_columnar_agg)`, no Sort. A/B top-10 EventTime byte-identical.
- **q24 — ROUTE (already correct).** `SearchPhrase <> ''` pushes (Ne). EventTime carried resjunk. Live EXPLAIN routed, no Sort. A/B byte-identical.
- **q25 — HONEST-NEGATIVE (text sort key).** DataFusion memcmp ≠ PG collation order; a deterministic collation constrains
  equality not order. Guard `:1927–1939` admits text sort key only under collation OID 950(C)/951(POSIX); ClickBench default
  collation OID=100 → declines (correct-but-conservative). Live EXPLAIN still has a `Sort` node. Trap is A/B-invisible if the
  oracle strips ORDER BY/LIMIT (§5.1). *Nuance:* the droplet DB is `C.UTF-8` (byte-order), so q25 would be byte-correct there,
  but the guard keys on the collation OID (100≠950/951) and declines safely.
- **q26 — HONEST-NEGATIVE (multi-key + text tiebreaker).** Declines at `:1853–1855` (numCols!=1); even with multi-key the
  SearchPhrase tiebreaker is a text sort key → same collation trap. Live EXPLAIN has a 2-key Sort.

**Cross-cutting A/B caveat:** all four are `LIMIT 10` over a key with **ties** (many equal EventTime). The *set* of top-k key
values is deterministic; *which rows* among equal keys is unspecified — columnar vs heap may legitimately pick different
tie-rows. A `SELECT *` A/B must compare as a **multiset** or add a total-order tiebreaker. Columnar timestamp epoch-2000 µs
vs Arrow epoch-1970 µs (M157) is a **constant additive offset → order-preserving**, so top-k-by-timestamp is unaffected.

## SOTA (R0)

- **DataFusion TopK** (`references/datafusion/datafusion/physical-plan/src/topk/mod.rs`): bounded `TopKHeap` (BinaryHeap),
  `insert_batch → maybe_compact()` discards all but top-k; sort keys via Arrow `RowConverter`; payload `interleave`d only for
  kept rows — exactly what `df.sort(key).limit(0,k)` lowers to.
- **DuckDB `PhysicalTopN`** (`references/duckdb/.../physical_top_n.hpp`): adds a **selection vector** (order without moving
  payload — the late-mat idiom); `column_lifetime_analyzer.cpp` trims columns not needed above the sort (mirrors `columns_needed`).
- **Abadi et al., "Materialization Strategies in a Column-Oriented DBMS", ICDE 2007** — origin of "late materialization" (cited
  by venue/authorship; PDF not in acervo — honest R6). C-Store (`papers/cstore-stonebraker-2005.pdf`) + MonetDB/X100
  (`papers/monetdb-x100-boncz-2005.pdf`) establish the column-projection + vectorized substrate.

Consensus: decode only sort-key + filter columns, bounded-heap TopK over N, materialize payload for k survivors. TheoDB's
`run_columnar_topk` already implements this shape.

## Ranked recommendation

1. **q24 (73×) + q23 (110×) — ROUTE.** Highest leverage, zero new mechanism, proven byte-identical. M167 delta = the harness
   never sets `enable_columnar_late_mat` (default OFF). Action: measure with late-mat on; decide default-ON via ADR (needs the
   full 43-query no-regression proof) OR label the measurement. **Mandatory:** the type-coverage A/B (§5.1) MUST keep
   `ORDER BY … LIMIT` (not strip it) and compare as a **multiset** (ties), with the collation positive control.
2. **q25 (113×) — HONEST-NEGATIVE (portable build).** Text sort key under non-C/POSIX-OID collation = wrong top-k. Document
   as known non-covered (as M155).
3. **q26 (132×) — HONEST-NEGATIVE.** Needs multi-key + a byte-order text tiebreaker; text component is collation-unsafe.

**Optional (owner ADR, NOT M167):** admit a text sort key when the DB `datcollate` resolves to byte order (C/POSIX/C.UTF-8) —
unlocks q25/q26 on C.UTF-8 deployments. Risks: glibc/locale-name dependence, routing changes with cluster locale, multi-key is
new mechanism. Given the M158/M165/M166 collation-trap history + Rule 3, ship M167 as **q23/q24 + honest-negative q25/q26**;
treat the C-locale text-sort extension as a later ADR-gated slice.

## Files

`columnar_agg.rs` (`try_swap_topk:1813`, numCols guard:1853, text-collation guard:1927–1939, `topk_type_supported:1770`,
`extract_text_predicate:469`, GUC default:33, `swap_walk:2028`), `columnar_project.rs` (`pathkeys=null:351`, `columns_needed:428`),
`df_executor.rs` (`run_columnar_topk:764`, TopK:793–815), `benchmarks/clickbench/theodb/{queries.sql:24–27,create.sql}`,
`benchmarks/run_m128_clickbench.py` (sets agg only, never late_mat), `benchmarks/columnar_type_ab.py` + `.claude/rules/testing.md §5.1`.
