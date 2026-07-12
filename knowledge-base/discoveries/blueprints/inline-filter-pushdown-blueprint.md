# Blueprint — M90 inline filter pushdown (label-in-index scan-key, IVF-AQ-native)

**Date:** 2026-07-12 · **Milestone:** M90 · **Source:** council-index-storage deep research (code+web-grounded, R0) reading the real pgvectorscale (permissive) + AlloyDB published design + Postgres AM docs.

## The architectural fork (resolved by evidence)

Filtered vector search "inline" (filtro DENTRO da travessia) has two implementations:

| | Approach A — scan-key / label-in-index | Approach B — Custom Scan Provider |
|---|---|---|
| Who uses it | **pgvectorscale** (permissive, Rust+pgrx like us) | **AlloyDB** (Custom Scan "vector scan" + Bitmap Index Scan) |
| Filters on | ONE declared label column (`smallint[]`), operator `&&` | ARBITRARY `WHERE` via existing btrees/GIN → TIDBitmap |
| Mechanism | planner pushes `labels && '{…}'` as a `ScanKey` to `amrescan`; scan evaluates it during traversal | `set_rel_pathlist_hook` + `CustomScanMethods`; bitmap membership woven into the vector scan |
| Effort | moderate (opclass + 2nd index column + scan-key parse) | heavy (planner hook + custom plan node + bitmap plumbing) |
| DDL cost | user declares the label column in the index; categories → smallint IDs | none (any existing column) |
| Recall trick | Filtered-DiskANN label-aware edges at build (graph) | bitmap membership only |

**AlloyDB honest note (web-fetched):** *"Inline filtering is supported ONLY when you use the ScaNN algorithm. Inline filtering is NOT compatible with IVF, IVFFlat, or HNSW"* (cloud.google.com/alloydb/docs/ai/adaptive-filtering). It needs a scan that computes distances candidate-by-candidate with a cheap membership test woven in — **which is EXACTLY our storage-separated IVF-AQ Stage-1 (code-only prune) → Stage-2 (f32/SQ8 rerank)**. So our v5/v6 layout is a *better* inline host than AlloyDB's own IVF.

## Decision — M90 = Approach A (parsimony-correct); Custom Scan (B) = M91

Rationale vs M90's DoD ("recall@10 under ~1% selective filter, inline > M87 post-filter, measured"):
- The DoD is a **single selective filter** (a category/label). Approach A delivers exactly that, is **proven in a permissive Rust+pgrx extension** (pgvectorscale), and is the SMALLEST change producing a measurable inline > post result.
- Approach B (Custom Scan, arbitrary `WHERE`) is **YAGNI for M90** (parsimony ladder rung 1) — high effort the DoD does not require. It becomes essential for arbitrary-WHERE-without-DDL → **M91** (where AlloyDB's "adaptive" inline⇄pre-filter naturally lives, because adaptive is bitmap-driven).
- Our `amrescan(scan, _keys, …)` **already receives the scan keys** (`scan.rs:99`) — we just ignore them. Minimal delta.

## Minimal delta for Approach A (own code — Rule 9; pgvectorscale is study-only-of-design even though permissive)

1. **`am/mod.rs`** — `amcanmulticol = true`; register a label opclass: `OPERATOR 1 &&` for `smallint[]` (mirror the mechanism of pgvectorscale `mod.rs:243-262`, OWN code: a `smallint_array_overlap(smallint[], smallint[]) -> bool`). This is what makes the planner push `labels && '{…}'` as an Index Cond (not a post-filter).
2. **`am/build.rs`** — `ambuild`/`aminsert` read `*values.add(1)` when the index has a 2nd column; store a sorted-deduped label set per vector. **Store the label in the CODE pages** (v5/v6 already have a code-only page stream) so Stage-1 AH-prune drops non-matching candidates BEFORE the f32/SQ8 rerank random-read (keeps the O(probes) partial-read + crash-safety invariants). Widening the code entry = **format bump: new IVF magic v7 + REINDEX story + CHANGELOG `Changed`**.
3. **`am/scan.rs`** — stop ignoring `_keys`; parse `keys[i].sk_argument` as the query label set; thread it into `scan_ivf_aq*` so the Stage-1 candidate loop skips non-overlapping labels (the inline skip). Set `xs_recheck = true` when a label key is present (correctness even if lossy — the executor rechecks against the heap). Interacts with M87's iterative cursor: the inline skip *replaces* the M87 grow-probes post-filter for the label predicate (grow-probes still recovers recall when a probed list has few matches).

## Recall consideration (honest)

pgvectorscale keeps label-aware edges at build so the label-restricted subgraph stays navigable (Filtered-DiskANN, dl.acm.org/doi/10.1145/3543507.3583552 — `UNBENCHMARKED` on the paper's internal numbers, cited via pgvectorscale README attribution). For IVF (lists, not a graph) this is LESS acute — the lists ARE the partition; inline-skip within probed lists + M87 grow-probes recovers recall when a probed list has few label matches. The gate MEASURES whether this holds at ~1% selectivity.

## Coverage corners

- **Techniques:** pgvectorscale label scan-key (`labels/mod.rs:181-237`, `scan.rs:189,220-224,336-364`, `mod.rs:56,243-262`); Postgres scan-key contract (`index_key operator constant`, ANDed, recheck — postgresql.org/docs/current/index-scanning.html); Filtered-DiskANN edges (paper).
- **Dependencies:** none new at runtime — scan keys + opclass are core Postgres via `pg_sys`. pgvectorscale (PostgreSQL License) study-only.
- **Tools:** droplet recall-under-filter benchmark (SIFT1M + synthetic label column, ~1% selectivity), inline vs M87 post-filter.
- **Integration tests:** filtered-scan recall == exact seqscan-filtered top-k on the label predicate; EXPLAIN shows the label as Index Cond; crash-safety (build→restart→scan-identical) on the v7 format.

## ADRs

**ADR M90-1 — Approach A (label scan-key) over Custom Scan Provider (B) for M90.** Alternatives: (B) Custom Scan — REJECTED for M90: YAGNI (arbitrary WHERE not in the DoD), heavy; deferred to M91. (C) keep M87 post-filter — REJECTED: doesn't close the medium-selectivity recall gap. Chosen: A — proven, IVF-AQ-native (Stage-1 prune), minimal delta. Consequence: format bump v7 + REINDEX; filters limited to the declared label column + `&&` (documented honest boundary).

**ADR M90-2 — label stored in the CODE pages (Stage-1), not a separate stream.** So the inline skip happens in Stage-1 before the Stage-2 rerank random-read (preserves O(probes) I/O). Alternative: separate label page stream — REJECTED: an extra random-read per candidate defeats the point.

## Honest scope boundary (what M90 does NOT do)

- Only the declared label column, only `&&`, label as `smallint[]`. `WHERE price < 100` on a regular column still post-filters (that's M91/Approach B).
- NOT a QPS-superiority claim (paradigm ceiling M73/M82 stands). It's a **recall-stable-under-selective-label-filter** claim, measured.

## Citations

- pgvectorscale: `knowledge-base/references/pgvectorscale/pgvectorscale/src/access_method/{labels/mod.rs:181-237, scan.rs:189-364, mod.rs:56-317, graph/mod.rs:299-456}`; `README.md:183-235`.
- Our code: `theodb_rs/src/am/scan.rs:99` (`_keys` ignored), `am/mod.rs:82,253-285` (amcanmulticol/opclass), `am/build.rs:285,308` (values read), `am/page.rs` (code-page layout).
- Postgres: index-scanning.html (scan key), index-api.html (amcanmulticol), custom-scan.html (B, for M91).
- AlloyDB (B + adaptive, ScaNN-only): cloud.google.com/alloydb/docs/ai/{filtered-vector-search-overview,adaptive-filtering}.
- Filtered-DiskANN: dl.acm.org/doi/10.1145/3543507.3583552 (`UNBENCHMARKED` internal numbers).
