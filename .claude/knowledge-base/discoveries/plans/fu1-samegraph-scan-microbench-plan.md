# Discovery Plan: FU-1 — same-graph, box-noise-immune micro-benchmark for the HNSW scan allocation change

- **slug:** fu1-samegraph-scan-microbench
- **version:** 0.3
- **owner:** eng
- **created_at:** 2026-07-05

## Context

M46 shipped a recall-neutral allocation change to the on-disk HNSW scan (`theodb_rs/src/am/hnsw_page.rs::traverse`
— pre-size the three per-query structures + reuse one neighbor scratch). Its QPS benefit could **not** be
measured (`docs/benchmarks/m46-highrecall-qps.md`): the two-container A/B was confounded twice — (a) dev-box
contention (the pgvector control drifted +122% between runs) and (b) the M44 parallel build races
(`ann/hnsw.rs:34`), so the two containers built *different graphs*. `/review` logged FU-1
(`knowledge-base/implementations/m46-hnsw-highrecall-qps-followups.md`): the clean measurement needs a
**same-graph, box-noise-immune** harness. This discovery finds the right design from the Rust ANN prior art
(pgvectorscale + vectorchord both criterion-bench their internals) before we build it.

## Objective

Produce a blueprint that specifies **exactly** how to build a criterion micro-benchmark that isolates the M46
allocation change (pre-size vs `::new()`) over a **byte-identical fixed graph**, exercising **production**
traversal code (not a divergent re-implementation), so the latency delta is attributable solely to the
allocation strategy — immune to box noise (same-process interleaved measurement) and to build nondeterminism
(one fixed graph). Success = the blueprint answers all 8 research questions with ≥2 primary sources per
technique claim and cites only paths that resolve on disk.

## In-Scope

**Reference projects in scope:**
- `knowledge-base/references/pgvectorscale/` — in scope: `pgvectorscale/benches/`, `pgvectorscale/Cargo.toml`,
  `pgvectorscale/src/access_method/` (the search + storage boundary).
- `knowledge-base/references/vectorchord/` — in scope: `crates/simd/benches/`, `crates/simd/Cargo.toml`,
  `crates/vchordrq/` (index-accessor abstraction).

**Our own code (read-only context, not a reference project):** `theodb_rs/src/am/hnsw_page.rs::traverse`,
`theodb_rs/src/ann/hnsw.rs` (the in-memory `HnswIndex` with `node_neighbors`/`node_vector`/`entry`).

### Out-of-scope

- pgvectorscale: `pgvectorscale_derive/`, `pgvectorscale/src/util/`, all docs.
- vectorchord: `crates/rabitq/`, `crates/xtask/`, all docs.
- pgvector (C-only): excluded for the bench-technique corner — it uses PG C `palloc` + `pg_bench`, not portable
  to a Rust criterion decision (ADR-D1).

## ADRs

### D1 — Prioritize pgvectorscale (same stack) over vectorchord
pgvectorscale is a pgrx/Rust extension with the SAME toolchain as theodb (`criterion = "0.5.1"`,
`[[bench]] harness = false`); its `benches/lsr.rs` benches the list-search directly. It is the PRIMARY source.
vectorchord (also Rust, modular crates) is the SECONDARY source for the DIP storage-accessor boundary. Skipping
a C-only reference (pgvector) for the bench-technique corner is justified: pgvector uses PG's C `palloc` +
`pg_bench`, not portable to our Rust bench decision. Alternative rejected: benching via SQL only (the M46 path)
— it cannot isolate allocation from I/O + build-race, which is the entire reason FU-1 exists.

### D2 — Resolve "divergent copy vs real-code bench"
pgvectorscale's `lsr.rs` benches a **re-implemented** `ListSearchResult` (a copy of
`src/access_method/graph/mod.rs`). That is a known divergence risk (the bench drifts from production). The
blueprint MUST decide whether theodb extracts the REAL ground-loop behind a `NeighborSource` trait (DIP — bench
exercises production code) or accepts a copy. Default bet: DIP (no divergence). Alternative rejected: unguarded
copy (the bench could pass while production regresses — the exact failure the equivalence guard prevents).

### D3 — Fallback if DIP extraction is infeasible (EC-4)
If Q2/EC-1 finds the ground-loop genuinely cannot be extracted without a large refactor (e.g. the SIMD distance
is inseparable from the pinned-page scope), the documented fallback is pgvectorscale's own pattern: a bench copy
GUARDED by an equivalence test (Q7 — the benched candidate structure must produce the same visit order as
production `traverse`, reusing M46's recall-neutral oracle). A naked copy without the equivalence guard is
rejected. Alternative rejected: abandoning the micro-bench and re-trying the two-container A/B — it stays
confounded by the build race (the whole reason FU-1 exists).

## Research Questions

| Q | Question | Corner | Fase A — read/grep method | Fase B — expected answer shape |
|---|---|---|---|---|
| Q1 | How does pgvectorscale's `benches/lsr.rs` isolate the list-search from PG storage — re-implement or trait? | techniques | Read `references/pgvectorscale/pgvectorscale/benches/lsr.rs`; grep real `ListSearchResult` in `src/access_method/graph/mod.rs` | isolation boundary (copy vs trait) + divergence risk it carries |
| Q2 | What storage-access trait does the real search depend on, AND (EC-1) can OUR `traverse` ground-loop be extracted over it? | techniques | Read `references/pgvectorscale/pgvectorscale/src/access_method/storage.rs` + `graph/neighbor_store.rs`; THEN read `theodb_rs/src/am/hnsw_page.rs` traverse+load+neighbors_into | reference trait sig + OUR concrete `NeighborSource` extraction shape (addrs vs addrs+vector-bytes) |
| Q3 | How is a FIXED graph produced for a deterministic bench, at what N/dim, avoiding build nondeterminism? | techniques | Read `references/pgvectorscale/pgvectorscale/benches/lsr.rs` fixture; grep `rand`/`seed` | fixture strategy (seeded vs snapshot) + scale justified against `ef*m0` (EC-3) |
| Q4 | How is criterion wired in a pgrx crate WITHOUT a running Postgres — `Cargo.toml` shape? | tools | Read `references/pgvectorscale/pgvectorscale/Cargo.toml` + `references/vectorchord/crates/simd/Cargo.toml` | exact `[[bench]] harness=false` + dev-dep + required-features stanza to copy |
| Q5 | How does the bench avoid linking pg_sys / the cdylib entrypoints? | tools | Grep `pg_sys`/`pgrx::pg_extern`/`cfg(feature` in `references/pgvectorscale/pgvectorscale/benches/lsr.rs` + its imports | module/feature boundary the ground-loop extraction must satisfy (no pg_sys once behind the trait) |
| Q6 | What exact criterion version + bench-only dev-deps to pin — is dev-dep the minimal footprint? | deps | Read `references/pgvectorscale/pgvectorscale/Cargo.toml` `[dev-dependencies]`; check `theodb_rs/Cargo.toml` | version to pin (`criterion 0.5.x`) + confirmation dev-only (zero cdylib impact, parsimony rung 4) |
| Q7 | How does a reference guard the bench against measuring a DIFFERENT path than production? | tests | Grep `ListSearchResult`/`assert` in `references/pgvectorscale/pgvectorscale/src/access_method/`; look for a shared module | equivalence mechanism (shared module vs unguarded copy) → the guard theodb adds (bench path == `traverse`) |
| Q8 | At what statistical rigor do the references report criterion results (CI, sample size, baseline compare)? | tests | Read `references/pgvectorscale/pgvectorscale/benches/lsr.rs` + `benches/distance.rs` Criterion config | criterion config theodb adopts so the pre-size-vs-new delta clears the noise threshold with a reported CI |

## Coverage Matrix

| # | Question | Corner | Method (Fase A) | Path pre-validated |
|---|---|---|---|---|
| Q1 | search isolation in lsr.rs | techniques | Read lsr.rs + grep graph/mod.rs | ✓ `benches/lsr.rs`, `src/access_method/graph/mod.rs` |
| Q2 | trait seam + OUR extraction | techniques | Read storage.rs + neighbor_store.rs + our traverse | ✓ `src/access_method/storage.rs`, `graph/neighbor_store.rs`, `theodb_rs/src/am/hnsw_page.rs` |
| Q3 | fixed-graph fixture | techniques | Read lsr.rs fixture | ✓ `benches/lsr.rs` |
| Q4 | criterion wiring | tools | Read both Cargo.toml | ✓ `Cargo.toml` (both) |
| Q5 | avoid pg_sys linking | tools | grep bench imports | ✓ `benches/lsr.rs` |
| Q6 | criterion version dev-only | deps | Read Cargo.toml dev-deps | ✓ `Cargo.toml` |
| Q7 | guard vs divergent copy | tests | grep src/access_method | ✓ `src/access_method/` |
| Q8 | criterion rigor | tests | Read lsr.rs + distance.rs | ✓ `benches/{lsr,distance}.rs` |

**Coverage: 8/8 questions mapped; all 4 corners populated (techniques 3, tools 2, deps 1, tests 2). 100%.**

## Halt-loop Checkpoints

- A research question is DONE when: (a) the cited file was read/grepped, (b) the answer is written with a
  verbatim quote + `file:line` citation, (c) for technique claims, ≥2 independent sources are cited
  (`discover-phd-rigor.md` R2). BLOCKED when a cited path does not resolve (fail-fast, no fabrication).
- **EC-2 (I/O caveat):** before the blueprint verdict is DONE, it MUST state a no-page-I/O micro-bench magnifies
  the allocation share → the criterion delta is an UPPER bound on the production QPS benefit (the allocation cost
  removed), not the production number. Honesty per `public-copy.md`.
- **EC-3 (fixture scale):** before Q3 is DONE, confirm the fixture N is in the regime where the M46 effect exists
  (ef≥200 with `ef*m0` large enough that pre-sizing/rehashing is a measurable share). A 30-node toy graph shows
  ~zero delta; the blueprint must justify its N against `ef*m0`.

## Acceptance Criteria

- Every Q1–Q8 answered with a `knowledge-base/references/` citation that resolves on disk.
- The blueprint's Techniques corner names the DIP decision (extract real ground-loop behind `NeighborSource` vs
  copy) with pgvectorscale + vectorchord evidence AND the SOTA anchor (how the field benches ANN scans), and
  states OUR concrete extraction shape (EC-1).
- The blueprint specifies: the `Cargo.toml` bench stanza, the criterion config (sample size/CI), the fixed-graph
  fixture (seeded `HnswIndex::build(seed=42)` at an EC-3-justified N), the equivalence guard (bench path ==
  production `traverse`), and the honest EC-2 caveat.
- ≥ 4 coverage corners; ≥ 2 primary sources per technique claim; no fabricated citation.

## Global Definition of Done

Blueprint scored by `/discover-confidence` ≥ `SHIPPABLE_WITH_CAVEATS` (per `discover-blueprint-golden-rule.md`);
all four coverage corners populated; no fabricated citation; the DIP-vs-copy decision resolved with evidence.
Cites `discover-phd-rigor.md` (R1 SOTA-anchoring, R2 ≥2 sources), `architecture.md` (DIP boundary),
`testing.md` (equivalence guard), `parsimony-ladder.md` (criterion as dev-only dep, rung 4).
