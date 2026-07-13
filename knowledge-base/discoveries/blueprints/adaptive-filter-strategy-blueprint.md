# Blueprint — M91 adaptive filter strategy (AM-local, label scan-key; NOT Custom Scan)

**Date:** 2026-07-12 · **Milestone:** M91 · **Source:** council-index-storage deep research (code-grounded; pgvectorscale checked out, AlloyDB blueprint-cited per M90's web-fetch — R0 honesty: WebSearch unavailable this session, grounded on the cloned refs + M90's fetched blueprint).

## The load-bearing finding (re-scopes M91)

**Neither pgvectorscale nor AlloyDB does adaptive strategy selection for the label case.**
- **pgvectorscale** (our permissive Rust+pgrx analog) has ONE strategy — inline — and its `amcostestimate` hardcodes `numIndexTuples/100` with a literal `//TODO need better estimate` (`references/pgvectorscale/.../cost_estimate.rs:36-37`); no pre/post fork, no selectivity read.
- **AlloyDB** adaptive is ScaNN-only + bitmap-driven (Approach B) — *"inline filtering NOT compatible with IVF/IVFFlat/HNSW"* (M90 blueprint:18).

So M91's "3 fixed strategies" the adaptive must dominate are **OUR OWN**: M87 (post), M90 (inline v7), and a possible new PRE. This is a *smaller* problem than "reimplement AlloyDB adaptive" — we already own two of the three on the same label scan-key.

## The fork (resolved)

| | Approach A — AM-local adaptive on the label scan-key | Approach B — Custom Scan Provider |
|---|---|---|
| Filters on | the declared `smallint[]` label column + `&&` (reuses M90 v7 + M87) | ARBITRARY `WHERE` (any column) via bitmap over btrees |
| Adaptive input | in-scan match-rate (fraction of the first probed list matching the label) OR `amcostestimate` selectivity | bitmap cardinality |
| Change | a selectivity branch in `amrescan`/`scan_ivf_structured` (+ maybe a PRE fn) — small, no format change | planner hook + custom plan node + custom exec + bitmap plumbing — heavy |
| Covers M91 DoD (label-selectivity sweep)? | ✅ fully | ✅ but over-scoped |

**Decision: M91 = Approach A.** The DoD sweep is a LABEL-selectivity sweep (§ DoD); A covers it entirely, reuses M90/M87, needs NO new page format / REINDEX / WAL surface, and `xs_recheck` is already correct for all branches. Arbitrary-WHERE (B, the true AlloyDB bitmap design) is **YAGNI for M91** — a future milestone (M92) gated on measured demand. Same parsimony call M90's discovery made (Custom Scan → scan-key).

## The design (measurement-first — PRE may itself be YAGNI)

The three strategies on the label scan-key:
- **INLINE (M90 v7):** `scan_ivf_aq_split_v7` — Stage-1 skips non-overlapping. Measured recall 1.00 @ ~1% (M90).
- **POST (M87):** the iterative re-search (grow probes) on a non-label index — degrades at selective (recall 0.52 @ 1%, M90).
- **PRE (new, ONLY if measured needed):** scan ALL lists' compact code pages for the tiny match set + exact rerank → recall 1.0 by construction. HONEST TENSION: a true whole-index label scan is O(N), tensioning the M31/M35 partial-read invariant — only pays off at ultra-selective (<~0.1%) where scanning ~18 B/vec labels for few matches beats starving the probed lists.

**Selectivity estimator (parsimony-correct = in-scan):** the fraction of the FIRST probed list's candidates that overlap the query label — free, data-true, no plan-time→exec-time plumbing (the awkward part of the alternative). `amcostestimate` selectivity via `clauselist_selectivity` is the "correct" source but needs plan→exec plumbing (a field in IndexScanDesc doesn't exist) — deferred unless the in-scan proxy proves insufficient.

**CRITICAL (measurement-first):** M90 already measured INLINE recall = 1.00 at ~1%. If INLINE also wins ultra-selective (0.01–0.1%), **PRE is YAGNI** and adaptive collapses to a 2-way INLINE⇄POST switch (an even smaller M91). The build: implement the in-scan estimator + INLINE⇄POST branch, **measure the sweep (0.01%→30%)**, and add PRE ONLY if the data shows a regime where INLINE loses. Do not build PRE on assumption.

## DoD measurement (the sweep)

Extend `benchmarks/m90_filter_bench.py`: build a v7 index once, vary label selectivity across {0.01%, 0.1%, 0.5%, 1%, 5%, 10%, 30%}. At each point measure recall@10 + QPS for: post-only (M87 via v5), inline-only (M90 v7), [pre-only if built], and adaptive. Ground truth = exact seqscan-filtered top-10. **DoD pass:** adaptive's recall tracks max(strategies) and its QPS is within noise of the best, at EVERY selectivity point (adaptive rides the upper envelope).

## Coverage corners

- **Techniques:** the INLINE⇄POST adaptive branch by in-scan selectivity; pgvectorscale's single-strategy (proof adaptive is our own addition, ahead of it); AlloyDB adaptive = bitmap/Approach B (deferred).
- **Dependencies:** none new — reuses M90 v7 + M87 iterative + the label already in `ScanState`. No Custom Scan, no format change.
- **Tools:** the selectivity-sweep harness (0.01%→30%).
- **Integration tests:** adaptive picks the right branch per regime; recall == exact; the sweep artifact.

## ADR

**ADR M91-1 — AM-local adaptive (A) over Custom Scan Provider (B) for M91.** Alternatives: (B) Custom Scan — REJECTED for M91: YAGNI (arbitrary-WHERE not in the DoD sweep), heavy → future milestone. Chosen: A — reuses M90/M87, no format change, in-scan estimator. Consequence: adaptive is LABEL-only; arbitrary-WHERE is the deferred B.

**ADR M91-2 — measurement-first on PRE.** Build the estimator + INLINE⇄POST first, measure the sweep, add PRE only if a regime shows INLINE losing. Alternative: build all 3 upfront — REJECTED: PRE may be YAGNI (M90 got 1.00 @ 1%; INLINE may win ultra-selective too), and PRE tensions the partial-read invariant.

## Honest boundary

- **Label-only.** `WHERE price < 100` on a regular column is not adaptive here (needs Approach B). Claim "adaptive on the declared label column, measured 0.01%→30%", never "adaptive filtered search" unqualified.
- **NOT a QPS-superiority claim vs ScaNN/AlloyDB** (paradigm ceiling M73/M82 stands).

## Citations

- Our code: `theodb_rs/src/am/scan.rs` (amrescan label parse `:127-142`, scan_ivf_aq_split_v7 `:560-659`, M87 iterative `:861-886`), `am/mod.rs` (amcostestimate `:127-161`), `am/page.rs` (v7 `:1208`, LABEL_K `:1187`).
- pgvectorscale (no adaptive): `references/pgvectorscale/.../cost_estimate.rs:36-37`, `.../scan.rs:189`.
- AlloyDB (ScaNN-only, bitmap/B — blueprint-cited): `knowledge-base/discoveries/blueprints/inline-filter-pushdown-blueprint.md:18,24`.
- M90 verdict deferring B → M91: `docs/adr/0040-m90-inline-label-filter-verdict.md`.
