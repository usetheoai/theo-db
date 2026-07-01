# Architecture Review — `theodb_rs` (TheoDB Rust engine) — FAANG-level verdict

**Date:** 2026-07-01 · **Target:** `theodb_rs/src` (Rust, pgrx, 3122 LoC / 15 files) · **Mode:** 7-dimension review, measured (lizard CC) + 4 specialist agents + SOTA peer comparison (pgvector / pgvectorscale / vectorchord).

## Headline

Two very different answers, and the distinction is the whole point:

- **The CODE CRAFT is already at / above the FAANG bar.** 0 cyclic dependencies (the non-negotiable), a textbook stability gradient, a real enforced DIP boundary, exemplary typed error handling, and — notably — the author *resisted* two tempting over-abstractions (an `AnnIndex` trait, a metric-Strategy object) that would have added indirection without removing real divergence. That restraint is a senior signal. **No BLOCKER, no HIGH in any craft dimension.**
- **The ENGINE ARCHITECTURE is not yet SOTA — and this is the real source of the "not FAANG-level database" feeling.** TheoDB's HNSW/IVFFlat is a **SQL-callable, in-memory, rebuild-per-query** function. Every peer (pgvector C, pgvectorscale Rust, vectorchord Rust) is a **real Postgres index Access Method** (`IndexAmRoutine`) with persistence, planner integration, and incremental maintenance. This gap is **HIGH** — but it is a **disclosed, deliberate scope deferral** (M21b), not a code defect.

**So: the code is professional-grade; the product is a professional-grade ANN *function*, not yet a professional-grade *index engine*.** Closing that is a roadmap decision (M25), not a cleanup.

## Scorecard (0-100, weighted; threshold sources tagged)

| Dim | Dimension | Score | Verdict | Key evidence |
|---|---|---|---|---|
| 1 | Structure & cohesion | 85 | PASS (strong) | Real enforced pg-glue/domain/SPI layering; coherent `ann/` sub-package; **`lib.rs` 721 LoC** two-responsibility append-magnet (peers: pgvectorscale lib.rs=47, paradedb=192). |
| 2 | Naming | 92 | PASS (strong) | 100% Rust-idiomatic case; screaming-architecture top level; minor: `vec.rs` (=distance ops) + `ann_query.rs` (location) names. |
| 3 | SOLID / DRY / Clean | 82 | PASS w/ refactors | `nl_to_sql` CCN 19, `run_rrf` 84 NLOC, `ann_query::knn` CCN 15 = **inlined linear stages**, decompose cleanly. `rerank_dist` duplicates `Metric::dist` (real DRY, 3-line fix). OCP/DIP correct. |
| 4 | Coupling / cohesion / **cycles** | 95 | PASS | **0 cycles (ADP ✅)**. Clean hub-and-spoke: `pg` Ca=10/I=0 stable boundary; `ann` hidden behind `ann_query`; SDP satisfied. `ann_query` doubles as SPI-util (cohesion MEDIUM). |
| 5 | Design patterns | 85 | PASS_WITH_CAVEATS | pgrx `#[pg_extern]`+`extension_sql` correct; enum-match-as-strategy idiomatic; **no missing/over-engineered pattern** (AnnIndex trait rightly avoided). One DRY MEDIUM. |
| 5.5 | SOTA vs peers | 65 | SPLIT | Algorithm math = **parity** with pgvectorscale (SBQ 1-bit/n-bit z-score, HNSW SelectNeighbors, k-means++). Engine = **not an index AM** (the gap). Vector-type coexistence = correct call. |

**Composite ≈ 84/100 — "Refactor Lightly" (craft) + one strategic HIGH (engine=function-not-AM, roadmap-scoped).**

## Findings roll-up

- **BLOCKER: 0** · **HIGH (code craft): 0** · **HIGH (architecture/product): 1** (no index AM — disclosed M21b deferral).
- **MEDIUM (5):** `lib.rs` split (extern shims + DDL → move next to feature modules, peers keep lib.rs tiny); `nl_to_sql` extract L2/L4 validation stages (security boundary, currently no fast Rust test of the L2 composition); `run_rrf` extract `resolve_query_vector`; `rerank_dist` DRY (widen `Metric::dist` to `pub(crate)`, delete the copy); `ann_query` SPI-util cohesion (move `require`/`valid_ident`/`read_corpus` to a `pg`/`spi_util` home).
- **LOW/INFO:** `sbq::knn` 12-param `#[allow(too_many_arguments)]` → adopt the sibling's `Params` struct; magic numbers (`http` timeout 30, ivf Lloyd 10) → named consts; missing Rust unit tests for pure `chat`/`embed` parsers (inconsistent with `vec`/`nl`/`sbq`); doc-drift (500-LoC budget cited to `architecture.md`, actually `analysis-golden-rule.md`); `Params` fat-object with dummy fills.

## What this means for the user's question ("far from FAANG-level")

The feeling is **correct about the product, wrong about the code**:
- The **code the M17–M24 cycles produced is disciplined and above-bar** — this review found zero cyclic deps, zero god-classes-of-logic, faithful SOTA algorithms, and mature restraint. That is not "far from FAANG."
- The **database is "not yet FAANG" in exactly one architectural axis**: the vector index is an in-memory per-query function, not a persistent Postgres Access Method. That is the honest frontier, and it is already on the roadmap (M21b→M25).

## Prioritized action (Staff recommendation)

**Tier 1 — quick craft wins (behavior-preserving, one cycle):** `rerank_dist` DRY fix (3 lines), extract `nl_to_sql` L2/L4 (+ add the missing fast Rust tests of the L2 composition — it's a security boundary), `sbq::knn` `Params` struct, promote magic numbers. Low risk, closes every MEDIUM/LOW craft finding.

**Tier 2 — the `lib.rs` split:** move each feature's extern shim + `extension_sql!` next to its module; leave `lib.rs` a thin module-map. Peer-supported (pgvectorscale/paradedb), behavior-preserving, ends the append-magnet trajectory.

**Tier 3 — the strategic decision (CTO/roadmap, NOT a cleanup):** promote the in-memory ANN to a real Postgres **index Access Method** (`IndexAmRoutine`: `ambuild`/`aminsert`/`ambeginscan`/`amgettuple`/`ambulkdelete`/`amcostestimate` + planner `ORDER BY <-> ` pushdown). This is the single change that moves TheoDB from "SQL-callable ANN function" to "SOTA index engine." It is M21b/M25 scope and would surface the low-level pgrx competencies (C-unwind guards, memory contexts, page/buffer/WAL) the AM peers exercise.

## Method / honesty

Measured complexity via `lizard -l rust` (real Rust CC). Coupling/cycles via manual import-graph (Rust dep tooling `cargo-modules`/`cargo-depgraph` was absent — graph built by reading every `use crate::`/`use super::`). SOTA divergences each carry peer `file:line` evidence (no fabrication). Threshold sources tagged: cyclic-deps=0 / CC≤10 / DRY-Rule-of-3 / SDP = **consensus**; clippy-7-params = **default**; 500-LoC file / ~60-NLOC fn / "unstable" bands = **heuristic**.
