---
slug: m63-vector-join
milestone_id: M63
created_at: 2026-07-09
goal: Validate, measure and document the LATERAL-index-scan vector join on theodb_hnsw so it is first-class in the relational model, measured by join-recall@k ≥ pgvector single-query parity (±0.01) with EXPLAIN proving Index Scan.
---

# Plan: M63 — Vector JOIN (vetor first-class no join relacional via LATERAL-index-scan)

> **Version 1.0** — M63 does NOT build a new engine mechanism. The `CROSS JOIN LATERAL (SELECT … FROM b ORDER BY b.emb <=> a.emb LIMIT k) j` pattern is already a planner-integrated similarity join that uses the `theodb_hnsw` AM (each LATERAL iteration is a single-vector top-k that `amcanorderbyop` serves — exactly the shape M52 proved for `WHERE … ORDER BY`). This plan (1) proves by `EXPLAIN` that the inner branch is an Index Scan (not Seq Scan + Sort O(n·m)), (2) measures join-recall vs an exact brute-force ground truth, (3) benchmarks 3 arms (LATERAL-index / naive cross-join+sort / pgvector control) into `docs/benchmarks/m63-vector-join.{md,json}`, (4) ships an **optional** ergonomic helper `theodb.vector_join(...)` that codegens the LATERAL — and does an end-to-end dedup/entity-resolution case in pure SQL. A custom join executor node is rejected (Rule 9 — Postgres already provides LATERAL; no algorithmic gain per pgvector maintainer, blueprint [A1]).

## Goal

> "Enable SQL users to run a similarity JOIN (`a CROSS JOIN LATERAL … ORDER BY b.emb <=> a.emb LIMIT k`) that uses the `theodb_hnsw` index instead of an O(n·m) nested-loop, so that vector search is first-class in the relational model, measured by `vector_join_recall_matches_exact_within_tol` (join-recall@k ≥ pgvector single-query parity within ±0.01) AND `vector_join_uses_index_scan` asserting the inner branch is an `Index Scan using … theodb_hnsw`."

**Primary metric:** join-recall@k (mean over the outer rows of `a`, vs exact O(n·m) ground truth) ≥ pgvector-hnsw single-query recall parity (tolerance ±0.01), with the inner LATERAL branch proven to be an Index Scan.

## Context

The vector is already first-class in `ORDER BY` (M52 — `docs/benchmarks/m52-filtered-ann.md` + `theodb_rs/src/am/hnsw_page.rs:2283` `filtered_scan_preserves_recall_via_iterative`). ROADMAP.md § M63 asks for "similarity join com uso do índice (não nested-loop O(n²)); planner escolhe o AM vetorial; recall preservado" plus a benchmark and an end-to-end dedup case. The discovery blueprint (`.claude/knowledge-base/discoveries/blueprints/m63-vector-join-blueprint.md`) concluded — with ≥2 primary web sources per claim — that the LATERAL pattern already delivers this today: pgvector maintainer @ankane confirms a top-level column-vs-column join cannot use the index (blueprint [A1]/[A2]), but the inner body of a LATERAL (`b.emb <=> a.emb` evaluated per outer row of `a`) reduces to `b.emb <=> <constant> ORDER BY … LIMIT k` — the index-served single-vector top-k (blueprint [B1]/[C1]). A custom join node is PhD-level, duplicates LATERAL + `amcanorderbyop`, and yields no algorithmic gain (blueprint ADR-1, [A1]). M63 is therefore validate + measure + document + optional sugar, not a new node.

## Baseline Context (deep review of current state)

### Files that will be touched

| File | LoC today | Last commit (sha + date) | Why it exists today | Invariants to preserve |
|---|---|---|---|---|
| `theodb_rs/src/am/mod.rs` | 434 | `7b5bbad` (2026-07-08) | Registers the `IndexAmRoutine`; `amcanorderbyop = true` at `:78` enables the `ORDER BY <op> LIMIT k` pushdown | Must keep `amcanorderbyop = true`; the AM registration untouched |
| `theodb_rs/src/am/hnsw_page.rs` | ~3400 | `7b5bbad` (2026-07-08) | The `theodb_hnsw` scan/traverse + M52 iterative-scan tests (`:2283`, `:2313`) | Existing `#[pg_test]`s stay green; scan hot-path NOT modified by M63 |
| `theodb_rs/src/am/scan.rs` | ~520 | (M52) `7b5bbad` (2026-07-08) | `ambeginscan`/`amrescan`/`amgettuple` + M52 iterative scan state (`:52` `iterative`, `:133` armed via `max_scan_tuples`) | Read-only for M63 — cited for evidence, not edited |
| `theodb_rs/src/am/guc.rs` | ~245 | `7b5bbad` (2026-07-08) | `theodb_hnsw.ef_search` (`:25`) + `max_scan_tuples` (`:46`) knobs | Read-only for M63 — the LATERAL relies on these existing GUCs unchanged |
| `theodb_rs/src/api.rs` (helper here IF ADR D2 accepts) | ~540 | `7b5bbad` (2026-07-08) | Home of `extension_sql!` public `theodb.*` wrappers (`theodb.embed` at `:304`, `theodb.embed_batch` at `:333`) | New `extension_sql!` block appended; existing wrappers untouched; REVOKE-from-PUBLIC parity kept |
| `benchmarks/run_m63_vector_join.py` (NEW) | 0 | — | (file to be created — mirrors `benchmarks/run_m52_filtered_ann.py`) | — |
| `docs/benchmarks/m63-vector-join.md` (NEW) | 0 | — | (file to be created — honest verdict report) | — |
| `docs/benchmarks/m63-vector-join.json` (NEW) | 0 | — | (file to be created — reproducible raw numbers, emitted by the harness) | — |
| `docs/adr/0022-m63-vector-join-lateral-not-node.md` (NEW) | 0 | — | (file to be created — records D1/D2) | — |
| `CHANGELOG.md` | — | `7b5bbad` (2026-07-08) | Public contract (Unbreakable Rule 6) | `[Unreleased]` gains one entry per shipped surface |

Every file listed in any `#### Files to edit` block below appears in this table. `(NEW)` rows are expected.

### Current callers / dependents

For the only production symbol M63 *may* add (helper, conditional on ADR D2):

- **Symbol:** `theodb.vector_join(left_tbl regclass, left_col text, right_tbl regclass, right_col text, k int, metric text)` (NEW, in `theodb_rs/src/api.rs` via `extension_sql!`)
- **Callers (production):** none pre-existing — it is a new public SQL surface; its "caller" for the wiring triad is the integration test + the benchmark harness invoking it against the real container.
- **Callers (tests):** `theodb_rs/src/api.rs` `#[pg_test]` (new) + `benchmarks/run_m63_vector_join.py` (new) exercising it against `theodb:m63`.
- **External (public API consumed by other repos):** yes — it is a public `theodb.*` function; once shipped its signature is a contract (SemVer). REVOKE-from-PUBLIC parity with `theodb.embed` (`api.rs:319`).

The LATERAL pattern itself needs **no** new symbol — it is user SQL over the existing `theodb_hnsw` AM (`mod.rs:78`). Use `grep -rn 'amcanorderbyop' theodb_rs/src/am/mod.rs` and `grep -rn 'vector_join' theodb_rs/ sql/ benchmarks/` to confirm the helper name is currently unused (it is).

### Domain glossary

- **LATERAL** — a Postgres `FROM`-clause subquery re-evaluated once per outer row, with that row's columns visible inside (PostgreSQL docs §7.2.1.5, blueprint [C1]). Turns `b.emb <=> a.emb` into `b.emb <=> <constant>` per outer row of `a`.
- **`amcanorderbyop`** — `IndexAmRoutine` flag (`mod.rs:78`) that tells the planner the AM can serve `ORDER BY <distance-op> $1 LIMIT k` — the trigger for the index-served top-k.
- **join-recall@k** — for each outer row `a_i`, `recall_i = |ANN_topk(a_i) ∩ EXACT_topk(a_i)| / k`; join-recall = mean over rows of `a` (report min + mean ± std). ANN-Benchmarks per-query recall, aggregated over the outer side.
- **iterative scan / `max_scan_tuples`** — M52 mechanism (`guc.rs:46`, `scan.rs:133`) that grows `ef` under a selective filter until N tuples emitted, preserving recall. M63 inherits it unchanged.
- **`theodb_hnsw`** — the project HNSW index AM (`mod.rs:66`), the AM the LATERAL inner scan must use.

### Architecture boundaries affected

- **interface (SQL surface)** — the optional `theodb.vector_join` helper adds one public `theodb.*` function via `extension_sql!` in `api.rs` (same layer as `theodb.embed`). No new dependency inward.
- **infrastructure (index AM)** — read-only. M63 does NOT cross into the AM engine (no edit to `scan.rs`/`hnsw_page.rs` scan path); it consumes the existing `amcanorderbyop` contract. This is the load-bearing honesty of the plan: no engine change.
- **benchmark harness** — reuses `benchmarks/theodb_bench/` (`metrics.latency_percentiles`, `recall`) — no new infra (blueprint Corner 2).

## Prior Art & Related Work

- **Internal blueprint** — `.claude/knowledge-base/discoveries/blueprints/m63-vector-join-blueprint.md` § "Recomendação (ADR-1)" and § "Design do benchmark (R3)". The LATERAL-vs-custom-node decision and the 3-arm join-recall benchmark design are consumed directly.
- **Internal benchmark (M52)** — `docs/benchmarks/m52-filtered-ann.md` + `theodb_rs/src/am/hnsw_page.rs:2283` (`filtered_scan_preserves_recall_via_iterative`). The inner LATERAL body is the same `WHERE … ORDER BY emb <=> $1 LIMIT k` shape M52 already proved index-served; M63 extends it to the outer side of a join.
- **Internal harness** — `benchmarks/run_m52_filtered_ann.py` (arms dict `:36`, `_make_dataset` `:56`, `_ground_truth` `:79`, `_measure` `:89`, mean/pstdev aggregation `:148`, DoD gate `:153`) and `benchmarks/run_m51_sbq_inline.py` (`_make_dataset` `:85` gaussian-mixture, `latency_percentiles` import `:18`). `run_m63_vector_join.py` mirrors these (Rule 9 — no new harness infra).
- **Internal SQL surface pattern** — `theodb_rs/src/api.rs:304` (`theodb.embed` via `extension_sql!` + REVOKE-from-PUBLIC `:319`) is the template for the optional helper.
- **External literature** — pgvector issues #812/#713/#703/#645 + README ("Why isn't a query using an index?"); PostgreSQL docs §7.2.1.5 (LATERAL); arXiv:2402.13397 (Xling, ANN-join = per-point top-k + pruning); arXiv:1706.04266 (kNN-join). All cited in the blueprint's "Evidência web (R0)" with ≥2 sources per claim.

## Objective

- [ ] Sub-goal 1 — `EXPLAIN` proves the inner LATERAL branch is an `Index Scan using … theodb_hnsw` (not Seq Scan + Sort) — `vector_join_uses_index_scan`.
- [ ] Sub-goal 2 — join-recall@k of the LATERAL-index join matches the exact O(n·m) ground truth within the AM's recall tolerance, reported min + mean ± std — `vector_join_recall_matches_exact_within_tol`.
- [ ] Sub-goal 3 — threshold/range LATERAL (`WHERE b.emb <=> a.emb < τ`) returns exactly the pairs below τ; τ negative → typed error, not crash — `vector_join_threshold_correct` + `vector_join_negative_threshold_errors`.
- [ ] Sub-goal 4 — decide (ADR D2) whether the `theodb.vector_join` helper is worth shipping vs raw LATERAL sugar; if yes, TDD its codegen; if no, record the rejection.
- [ ] Sub-goal 5 — benchmark 3 arms (T1 LATERAL-index / T2 naive cross-join+sort / T3 pgvector control) into `docs/benchmarks/m63-vector-join.{md,json}` with an honest per-axis verdict.
- [ ] Sub-goal 6 — end-to-end dedup/entity-resolution self-join in pure SQL, measured (duplicate-detection precision/recall).
- [ ] Sub-goal 7 — existing suite does not regress; validation runs on the existing `theodb:m63` image.

## ADRs

### D1 — Adopt `CROSS JOIN LATERAL (… ORDER BY <op> LIMIT k)` as the M63 vector join; do NOT build a custom join executor node

- **Decision:** M63 delivers the similarity join via the LATERAL-index-scan pattern over the existing `theodb_hnsw` AM. The code deliverables are: `EXPLAIN` validation tests, the 3-arm benchmark, the dedup case, and the optional helper (D2). No engine node.
- **Rationale:** each LATERAL iteration is a single-vector top-k that `amcanorderbyop` (`mod.rs:78`) already serves — the exact shape M52 proved (`hnsw_page.rs:2283`). Postgres provides LATERAL; the AM provides the ordered scan; nothing relational is missing (Rule 9).
- **Alternatives considered:** **(A) Custom CustomScan/Join node that pushes the AM.** *Rejected.* PhD-level (planner hook + path generation + custom scan state + join cost model), duplicates LATERAL + `amcanorderbyop`, and the pgvector maintainer confirms it "would still need N separate index lookups" (blueprint [A1]) — no algorithmic gain, only accidental complexity (violates Rule 9 + "Esforço ≠ Complexidade"). **(B) Materialize cross-product + top-level `ORDER BY` (the naive #812).** *Rejected as product* — it is O(n·m), does not use the index; it is the T2 **baseline we measure against**, not a deliverable.
- **Consequences:** vector-join first-class today; recall = the AM's recall (inherited, preserved by construction). Risk concentrated in R1 (planner may not push the index in some LATERAL shapes) — resolved empirically by the `EXPLAIN` gate.

### D2 — Ship the `theodb.vector_join(...)` helper ONLY IF the raw LATERAL is proven ergonomically insufficient; default is raw LATERAL + documentation

- **Decision:** the helper is an `extension_sql!` set-returning SQL function that codegens the idiomatic LATERAL (`theodb_rs/src/api.rs`, modelled on `theodb.embed` `:304`). It is **optional sugar** — the raw LATERAL is the first-class surface. Parsimony rung 1: build the helper only if it earns its existence. Acceptance test for "worth it": the helper must (a) reduce a real user query to a single call AND (b) not hide the `EXPLAIN`-provable Index Scan. If it cannot preserve the Index Scan (dynamic SQL via `regclass`/`format()` may defeat the planner pushdown — this is a REAL risk, see R5), the helper is **rejected** and D2 records raw-LATERAL-only.
- **Rationale:** the LATERAL is already the idiom the pgvector ecosystem recommends (blueprint [C2], issue #645). A helper adds ergonomics but risks defeating the very pushdown it wraps (dynamic SQL). Measurement-first: the phase-2 `EXPLAIN` test on the helper output decides.
- **Alternatives considered:** **(A) Always ship the helper.** *Rejected* — YAGNI/parsimony; a helper that codegens dynamic SQL can lose the Index Scan, shipping a slower path than the raw LATERAL. **(B) Never ship, doc only.** *Considered, is the fallback* — if D2's acceptance test fails, this is the outcome; the LATERAL idiom is documented in the benchmark md.
- **Consequences:** if accepted, one new public `theodb.*` contract (SemVer surface, REVOKE-from-PUBLIC parity). If rejected, zero new production code — M63 is pure validation + measurement + docs. Either way the DoD (index-served join, recall preserved) is met by the LATERAL.

## Drawbacks & Risks

| Drawback / Risk | Severity | Mitigation | Owner |
|---|---|---|---|
| **R1 — Planner may NOT pick the Index Scan inside the LATERAL in some shapes** (esp. `WHERE b.emb <=> a.emb < τ` WITHOUT `ORDER BY … LIMIT` — the AM requires `ORDER BY <op> LIMIT`, blueprint [B1]/[A2]) | High | `EXPLAIN` test asserts Index Scan per shape; where it doesn't fire, document the fallback (`ORDER BY … LIMIT` wide + outer `< τ` filter, or `UNION ALL` alt-C) and mark the arm honestly, not masked. This is the largest technical risk. | Eng |
| **R2 — "join-recall" metric is subtle and easy to misreport** (a mean can hide rows with recall 0; M52 already made & corrected this under high variance) | High | Report min recall_i + mean ± std per-row, exact O(n·m) GT, fixed seed. Recall is the deterministic gate (independent of box load). | Eng |
| **R3 — LATERAL batch latency may be poor vs vector-DBs** (N sequential searches = N× single-query; #645/[E2] gap) | Medium | Measure and document the real cost; do NOT promise batch-superior without a number. Batch amortization is explicit follow-up (D2-seed, blueprint ADR-2), out of M63 scope. | Eng |
| **R4 — Contended box pollutes QPS** (M46/M52 lesson) | Low | median + back-to-back + buffers deterministic; recall (not QPS) is the DoD gate. Cite hardware. | Eng |
| **R5 — The helper's dynamic SQL may defeat the pushdown** (a `regclass`/`format()`-generated LATERAL can lose the planner's Index Scan choice the static LATERAL keeps) | Medium | D2 acceptance test runs `EXPLAIN` on the helper output; if the Index Scan is lost, reject the helper (D2 fallback = raw-LATERAL-only). Never ship a helper that is slower than the idiom it wraps. | Eng |

## Unresolved Questions

- Q1 — Does the planner keep the Index Scan when the LATERAL inner adds a `WHERE b.id <> a.id` (the dedup self-join filter)? (Phase 1 `EXPLAIN` test on the dedup shape answers it.)
- Q2 — At what outer cardinality does the LATERAL batch latency cross pgvector's? (Phase 3 T1-vs-T3 latency axis answers it; if T1 never wins latency at scale, that is an honest documented gap, not a blocker — the DoD is "uses index, not O(n²)", which T1 meets and T2 fails.)
- Q3 — Is the range/threshold join (`< τ` without `LIMIT`) index-servable at all, or must it always be phrased as `ORDER BY … LIMIT` + outer filter? (R1; Phase 1 threshold `EXPLAIN` test + Phase 3 doc answer it.)
- Q4 — Does the helper (D2) preserve the Index Scan through dynamic SQL, or is it doc-only? (D2 acceptance test in Phase 2 decides; genuinely unresolved until measured.)

## Dependency Graph

```
Phase 1 (validate LATERAL uses index — EXPLAIN + recall + threshold)
   │
   ├──▶ Phase 2 (OPTIONAL helper — gated by D2 acceptance test; may be a no-op)
   │
   └──▶ Phase 3 (benchmark 3 arms + dedup e2e)  ── depends on Phase 1 (needs the validated query shapes)
                    │
                    ▼
              Phase 4 (Integration Validation — suite green on theodb:m63)
```

Phase 1 is the sequential blocker (it validates the shapes Phase 3 benchmarks). Phase 2 is optional and independent of Phase 3 (the benchmark uses the raw LATERAL regardless). Phase 4 runs last.

---

## Phase 1: Validate the LATERAL uses the index (EXPLAIN + recall + threshold)

**Objective:** prove by `EXPLAIN` that the inner LATERAL branch is an Index Scan on `theodb_hnsw`, that join-recall matches exact GT within tolerance, and that the threshold/range shape is correct with typed error on bad input.

### T1.1 — `EXPLAIN` asserts Index Scan on the inner LATERAL branch

#### Objective
Assert that `SELECT … FROM a CROSS JOIN LATERAL (SELECT b.id FROM b ORDER BY b.emb <=> a.emb LIMIT k) j` plans the inner branch as `Index Scan using … theodb_hnsw`, not `Seq Scan` + `Sort`.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — adds a `#[pg_test]` that builds a small table with a `theodb_hnsw` index, runs `EXPLAIN (VERBOSE)` on the LATERAL join, and asserts the plan text contains `Index Scan` on the theodb_hnsw index for the inner branch.
2. **Why it is necessary now** — this is the structural oracle of "first-class index-served join" (D1). Without it, the whole M63 claim is unproven; blueprint [A1] shows the top-level join does NOT use the index, so we must prove the LATERAL inner *does*. Anchored on M52's `filtered_scan_preserves_recall_via_iterative` (`hnsw_page.rs:2283`) which proved the same shape for `WHERE … ORDER BY`.

#### Evidence
- `theodb_rs/src/am/mod.rs:78` — `amcanorderbyop = true` (the pushdown enabler).
- `.claude/knowledge-base/discoveries/blueprints/m63-vector-join-blueprint.md` § "Coverage Corner 1" (`vector_join_uses_index_scan` anchor).
- Blueprint [C1] — LATERAL evaluates the inner per outer row with that row's constant, reducing to the index-served single-vector case.

#### Files to edit
```
theodb_rs/src/am/hnsw_page.rs — add #[pg_test] fn vector_join_uses_index_scan() (RED first; lives beside filtered_scan_preserves_recall_via_iterative:2283)
```

#### Deep file dependency analysis
- `hnsw_page.rs` today holds the `theodb_hnsw` scan + the M52 `#[pg_test]` block (`:2283`, `:2313`). This task appends one sibling `#[pg_test]` in the same test module — no production code change, no downstream caller impact (test-only).

#### Deep Dives
- **Invariant:** the inner branch MUST be `Index Scan using <idx> on b`, and the plan MUST NOT contain a `Seq Scan on b` under the LATERAL. Assert both (positive + negative on the plan text).
- **Edge cases:** `k > ef_search` (index still used, just capped); table with a single row; the dedup shape (`WHERE b.id <> a.id`) — assert Index Scan survives the extra inner predicate (Q1).

#### Pseudo-code / Signatures
```pseudocode
#[pg_test] fn vector_join_uses_index_scan():
  create table a(id int, emb vector(8)); insert 5 rows
  create table b(id int, emb vector(8)); insert 50 rows
  create index on b using theodb_hnsw (emb vector_cosine_ops)
  plan := query_one_text(
    "EXPLAIN (VERBOSE) SELECT a.id, j.id FROM a CROSS JOIN LATERAL
       (SELECT b.id FROM b ORDER BY b.emb <=> a.emb LIMIT 3) j")
  assert plan contains "Index Scan" AND contains "theodb_hnsw"
  assert plan does NOT contain "Seq Scan on b"
```

#### Tasks
1. Write the RED `#[pg_test]` asserting Index Scan (fails if the plan Seq-Scans).
2. Confirm it passes with the existing AM (no production change — GREEN is "test passes as-is").
3. Add the dedup-shape assertion (`WHERE b.id <> a.id`) to cover Q1.

#### TDD
```
RED:     vector_join_uses_index_scan() — asserts inner branch is Index Scan on theodb_hnsw, NOT Seq Scan (fails if planner ignores index)
GREEN:   No production code needed — the AM already serves it (D1). GREEN = test green against current mod.rs:78.
REFACTOR: None expected (test-only).
VERIFY:  cargo pgrx test -p theodb_rs vector_join_uses_index_scan
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `EXPLAIN (VERBOSE)` output for the LATERAL join contains `Index Scan` on the `theodb_hnsw` index for the inner branch (assert on plan text via `Spi::get_one`).
- [ ] The plan does NOT contain `Seq Scan on b` under the LATERAL.
- [ ] The dedup-shape variant (`WHERE b.id <> a.id`) also shows Index Scan (Q1 resolved).
- [ ] Pass: lint — `cargo clippy -p theodb_rs` zero warnings on changed file.
- [ ] Pass: size — `hnsw_page.rs` change is test-only; no new production LoC.

#### DoD (Definition of Done)
- [ ] Test written RED-first, passes GREEN.
- [ ] `cargo pgrx test -p theodb_rs` green for the new test.
- [ ] Zero clippy warnings on changed file.

### T1.2 — join-recall matches exact O(n·m) ground truth within tolerance

#### Objective
Assert that the multiset of `(a.id, b.id)` top-k pairs from the LATERAL-index join equals the exact per-row top-k (seqscan brute force) within the AM's recall tolerance.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — adds a `#[pg_test]` that computes the exact top-k per outer row via a seqscan (`ORDER BY … LIMIT k` with index disabled or a brute-force distance), runs the LATERAL-index join, and asserts `recall_i` per row ≥ tolerance and the mean matches.
2. **Why it is necessary now** — the DoD says "recall preservado". An index-served join that silently drops neighbours is worse than no join. This is the correctness oracle paired with T1.1's structural oracle. R2 (misreporting recall) is mitigated here by asserting per-row (min), not just mean.

#### Evidence
- o blueprint `.claude/knowledge-base/discoveries/blueprints/m63-vector-join-blueprint.md` point 2 — the join-recall definition (`recall_i = |ANN ∩ EXACT| / k`, mean over rows).
- `theodb_rs/src/am/hnsw_page.rs:2280-2283` — M52 comment: "the index-scan top-k EQUALS the exact" under iterative scan; same guarantee reused per outer row.
- Blueprint R2 — report mean ± std + worst-case min.

#### Files to edit
```
theodb_rs/src/am/hnsw_page.rs — add #[pg_test] fn vector_join_recall_matches_exact_within_tol() (RED first)
```

#### Deep file dependency analysis
- Same test module as T1.1. Uses a seed-fixed small dataset so the exact GT is deterministic and O(n·m) is tractable inside `#[pg_test]`.

#### Deep Dives
- **Data structures:** exact GT = per outer row `a_i`, the set of `k` nearest `b.id` by exact distance. ANN = the LATERAL result set.
- **Invariant:** for each `a_i`, `|ANN_i ∩ EXACT_i| / k ≥ tol`; assert the MIN over rows (not just mean) is ≥ tol (R2). On a tiny tight-cluster dataset tol should be 1.0 (exact recovery); use a modest tol (e.g. ≥ 0.9) to avoid flake.
- **Edge cases:** k=1 (nearest-neighbour join); k ≥ |b| (all of b returned per row → recall 1.0); duplicate vectors in b (ties — assert count, not identity).

#### Pseudo-code / Signatures
```pseudocode
#[pg_test] fn vector_join_recall_matches_exact_within_tol():
  build a(5 rows), b(N rows) tight gaussian-mixture clusters, seed fixed
  create theodb_hnsw index on b
  for each a_i:
    exact_i := SPI top-k of b by exact distance (seqscan, index off)
    ann_i   := LATERAL top-k of b via theodb_hnsw
    recall_i := |ann_i ∩ exact_i| / k
  assert min(recall_i) >= TOL and mean(recall_i) >= TOL
```

#### Tasks
1. Write the RED test asserting per-row min-recall ≥ tol.
2. Confirm GREEN against current AM.
3. Add k=1 and k≥|b| edge variants.

#### TDD
```
RED:     vector_join_recall_matches_exact_within_tol() — asserts min per-row recall ≥ tol vs exact GT
RED:     vector_join_recall_k1_nearest_neighbour() — k=1 nearest-neighbour join recall
GREEN:   No production code — AM already serves it. GREEN = tests green.
REFACTOR: None expected.
VERIFY:  cargo pgrx test -p theodb_rs vector_join_recall
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] `min(recall_i)` over outer rows ≥ tolerance (asserted, not just mean — R2).
- [ ] `mean(recall_i)` ≥ tolerance.
- [ ] k=1 and k≥|b| edge cases pass.
- [ ] Pass: lint — `cargo clippy -p theodb_rs` zero warnings on changed file.

#### DoD (Definition of Done)
- [ ] Tests RED-first then GREEN.
- [ ] `cargo pgrx test -p theodb_rs` green.

### T1.3 — threshold/range LATERAL correct; negative τ → typed error

#### Objective
Assert `… LATERAL (SELECT … WHERE b.emb <=> a.emb < τ ORDER BY … LIMIT n) j` returns exactly the pairs below τ (edge τ=0 → self-match only; τ large → all), and a negative τ raises a typed error rather than crashing.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — adds `#[pg_test]`s for the threshold shape (correctness at τ=0, τ mid, τ large) and a negative-case test asserting τ < 0 returns a typed error (not a panic across the C boundary).
2. **Why it is necessary now** — the DoD mentions `a JOIN b ON a.emb <=> b.emb < τ`. The threshold shape is where R1 bites hardest (a `< τ` without `ORDER BY … LIMIT` may not push the index). This task pins the correct phrasing and covers the negative case (error-handling rule: fail-fast, typed).

#### Evidence
- ROADMAP.md § M63 DoD line — `a JOIN b ON a.emb <=> b.emb < τ`.
- Blueprint § "Coverage Corner 1" — "Negative-case: τ negativo → erro tipado, não crash".
- Blueprint R1 — `< τ` without `ORDER BY … LIMIT` may not trigger pushdown; phrase as `ORDER BY … LIMIT` wide + `< τ`.

#### Files to edit
```
theodb_rs/src/am/hnsw_page.rs — add #[pg_test] fn vector_join_threshold_correct() + fn vector_join_negative_threshold_errors() (RED first)
```

#### Deep file dependency analysis
- Same test module. The threshold form is phrased as `ORDER BY b.emb <=> a.emb LIMIT n` (index-served) then outer `WHERE dist < τ` (per blueprint R1 mitigation) so correctness does not depend on the un-pushable bare-`< τ` form.

#### Deep Dives
- **Invariant:** result at τ contains exactly `{(a_i,b_j) : dist(a_i,b_j) < τ}` (up to the ANN recall tolerance for the LIMIT-bounded phrasing).
- **Edge cases:** τ=0 (only exact-zero-distance self match); τ = +inf / very large (all pairs within LIMIT); τ negative → typed error.
- **Negative case (error-handling):** τ < 0 must surface a typed/`ERROR`-level message (e.g. a `CHECK`/validation in the helper OR a documented "empty result is meaningless" — assert the *specific* behaviour, not merely "it errors"). If phrased as raw SQL, negative τ yields an empty set (documented) — the typed error is enforced in the helper (D2) if shipped; if helper rejected, document the raw-SQL empty-set behaviour as the contract.

#### Pseudo-code / Signatures
```pseudocode
#[pg_test] fn vector_join_threshold_correct():
  build a, b with known distances
  for tau in [0.0, mid, large]:
    res := LATERAL(ORDER BY dist LIMIT n) then outer WHERE dist < tau
    assert res == { pairs with dist < tau }  (within ANN tol)

#[pg_test] fn vector_join_negative_threshold_errors():
  assert theodb.vector_join(..., tau=-1) raises ERROR  (IF helper shipped)
  # else: assert raw-SQL negative-tau returns empty set (documented contract)
```

#### Tasks
1. RED threshold-correctness test across τ ∈ {0, mid, large}.
2. RED negative-τ test (typed error via helper, or documented empty-set for raw SQL).
3. GREEN.

#### TDD
```
RED:     vector_join_threshold_correct() — exact pair set below τ at 3 τ values
RED:     vector_join_negative_threshold_errors() — τ<0 typed error (helper) OR documented empty-set (raw)
GREEN:   Correctness needs no production change; the negative-error path is enforced in the D2 helper IF shipped.
REFACTOR: None expected.
VERIFY:  cargo pgrx test -p theodb_rs vector_join_threshold vector_join_negative
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] Threshold result set is exactly the pairs below τ (within ANN tol) at τ ∈ {0, mid, large}.
- [ ] Negative τ raises a typed error (helper path) OR returns a documented empty set (raw-SQL path) — the specific behaviour is asserted, not merely "it doesn't crash".
- [ ] No panic across the C boundary on any τ input.
- [ ] Pass: lint — `cargo clippy -p theodb_rs` zero warnings.

#### DoD (Definition of Done)
- [ ] Tests RED-first then GREEN.
- [ ] `cargo pgrx test -p theodb_rs` green.

---

## Phase 2: Optional helper `theodb.vector_join` (gated by D2 acceptance test)

**Objective:** decide via an `EXPLAIN` acceptance test whether a codegen helper preserves the Index Scan; ship it only if it does, else record raw-LATERAL-only.

### T2.1 — D2 acceptance test: does the codegen helper preserve the Index Scan?

#### Objective
Prototype the `theodb.vector_join(...)` helper as an `extension_sql!` SQL function that emits the LATERAL, run `EXPLAIN` on its output, and DECIDE: ship (Index Scan preserved) or reject (pushdown lost → raw-LATERAL-only).

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — writes a minimal `theodb.vector_join(left_tbl regclass, left_col text, right_tbl regclass, right_col text, k int, metric text)` that builds the LATERAL via `format()`/dynamic SQL and returns `(left_id, right_id, distance)`; a `#[pg_test]` runs `EXPLAIN` on the generated query and asserts Index Scan.
2. **Why it is necessary now** — parsimony (D2): the helper is sugar only if it earns its existence. R5 is real — dynamic SQL can defeat the pushdown the static LATERAL keeps. This test is the go/no-go; it MUST run before any helper code is committed as "done".

#### Evidence
- `theodb_rs/src/api.rs:304` — `theodb.embed` `extension_sql!` template (+ REVOKE `:319`).
- Blueprint ADR-1 deliverable (4) — "opcional — uma função-helper SQL … que gera/encapsula o LATERAL idiomático (pura ergonomia; zero engine novo)".
- Risk R5 — dynamic SQL may lose the Index Scan.

#### Files to edit
```
theodb_rs/src/api.rs — add extension_sql! block defining theodb.vector_join (+ REVOKE FROM PUBLIC), IF D2 accepts
theodb_rs/src/api.rs — add #[pg_test] fn vector_join_helper_preserves_index_scan() (RED first — the go/no-go gate)
```

#### Deep file dependency analysis
- `api.rs` today holds the public `theodb.*` `extension_sql!` wrappers (`:304`, `:333`). The helper is appended as a new block, REVOKEd from PUBLIC for parity. It is a NEW public contract (SemVer) — the callers table lists its test + benchmark callers (wiring triad pillar a).

#### Deep Dives
- **Signature:** `theodb.vector_join(left_tbl regclass, left_col text, right_tbl regclass, right_col text, k int, metric text DEFAULT 'cosine') RETURNS TABLE(left_id bigint, right_id bigint, distance float8)`.
- **Algorithm:** validate `metric ∈ {cosine,l2,ip}` → map to operator (`<=>`,`<->`,`<#>`); validate `k > 0` (typed error else — covers T1.3 negative-case for the helper); `format()` the LATERAL with `%I`/`%s` (identifier-quoted to prevent injection — security); `RETURN QUERY EXECUTE`.
- **Invariant (go/no-go):** `EXPLAIN` on the generated query MUST show Index Scan; if not, the helper is REJECTED (D2 fallback) and this block is deleted, leaving M63 as validation+benchmark+docs only.
- **Edge cases:** k ≤ 0 → typed `ERROR`; unknown metric → typed `ERROR`; empty right table → empty result.

#### Pseudo-code / Signatures
```pseudocode
CREATE FUNCTION theodb.vector_join(left_tbl regclass, left_col text,
    right_tbl regclass, right_col text, k int, metric text DEFAULT 'cosine')
RETURNS TABLE(left_id bigint, right_id bigint, distance float8) AS $$
  -- validate k>0, metric in set  → else RAISE typed ERROR
  op := case metric when 'cosine' then '<=>' when 'l2' then '<->' when 'ip' then '<#>' end
  RETURN QUERY EXECUTE format(
    'SELECT a.id, j.id, j.d FROM %I a CROSS JOIN LATERAL
       (SELECT b.id, b.%I %s a.%I AS d FROM %I b ORDER BY b.%I %s a.%I LIMIT %s) j',
    left_tbl, right_col, op, left_col, right_tbl, right_col, op, left_col, k)
$$ LANGUAGE plpgsql;
```

#### Tasks
1. RED: `vector_join_helper_preserves_index_scan()` (fails if generated plan Seq-Scans).
2. Implement the minimal helper (GREEN) with `%I` identifier quoting + k/metric validation.
3. Run the go/no-go: if Index Scan preserved → keep + REVOKE + COMMENT; if lost → delete the block, record D2 fallback, this phase becomes a no-op with a documented decision.
4. REFACTOR: extract the operator-mapping if it clarifies (else none).

#### TDD
```
RED:     vector_join_helper_preserves_index_scan() — EXPLAIN on helper output shows Index Scan on theodb_hnsw
RED:     vector_join_helper_rejects_bad_k() — k<=0 raises typed ERROR
RED:     vector_join_helper_rejects_unknown_metric() — metric='xyz' raises typed ERROR
GREEN:   implement theodb.vector_join extension_sql! (only if RED#1 can pass; else reject helper per D2)
REFACTOR: extract op-map if clarifying; else None.
VERIFY:  cargo pgrx test -p theodb_rs vector_join_helper
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] GO/NO-GO recorded: `EXPLAIN` on the helper's generated query shows Index Scan → ship; else helper deleted and D2 fallback documented.
- [ ] IF shipped: identifiers are `%I`-quoted (no SQL injection via table/column names — security).
- [ ] IF shipped: k ≤ 0 and unknown metric raise typed `ERROR` (not empty/crash).
- [ ] IF shipped: `theodb.vector_join` is `REVOKE`d from `PUBLIC` (parity with `theodb.embed` `api.rs:319`) + has a `COMMENT`.
- [ ] Pass: lint — `cargo clippy -p theodb_rs` zero warnings; Pass: size — `api.rs` ≤ 500 lines after edit (currently ~540 — if the block pushes it over, split per `architecture.md`).

#### DoD (Definition of Done)
- [ ] Go/no-go decision recorded in the plan + ADR 0022.
- [ ] IF shipped: tests RED-first then GREEN; wiring triad pillar (a) = the benchmark + integration test call the helper.
- [ ] `cargo pgrx test -p theodb_rs` green.

---

## Phase 3: Benchmark 3 arms + end-to-end dedup

**Objective:** measure join-recall + latency across T1 (LATERAL-index) / T2 (naive cross-join+sort) / T3 (pgvector control), plus the dedup e2e case, into `docs/benchmarks/m63-vector-join.{md,json}` with an honest verdict.

### T3.1 — `run_m63_vector_join.py` harness: 3 arms, join-recall vs exact GT, latency

#### Objective
Create the benchmark harness mirroring `run_m52_filtered_ann.py`, emitting join-recall (min/mean±std) and p50/p95 latency for the 3 arms into the JSON, with a DoD gate (T1 join-recall ≥ T3 parity ±0.01).

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — writes `benchmarks/run_m63_vector_join.py` reusing `theodb_bench.metrics.latency_percentiles` + the M52 `_make_dataset`/`_ground_truth`/`_measure` shapes; computes exact O(n·m) per-row GT on a tractable subset; runs T1/T2/T3; dumps `docs/benchmarks/m63-vector-join.json`.
2. **Why it is necessary now** — Rule 5 (performance is a claim, not opinion): no verdict without a reproducible artifact. T2 is the O(n·m) anti-objective proving T1's gain; T3 is the pgvector honesty control (M45/M52 discipline).

#### Evidence
- `benchmarks/run_m52_filtered_ann.py:36` (arms), `:56` (`_make_dataset`), `:79` (`_ground_truth`), `:89` (`_measure`), `:148` (mean/pstdev), `:153` (DoD gate) — the template.
- `benchmarks/run_m51_sbq_inline.py:85` (`_make_dataset` gaussian-mixture, avoids ANN-degenerate data per ADR 0012), `:18` (`latency_percentiles`).
- o blueprint `.claude/knowledge-base/discoveries/blueprints/m63-vector-join-blueprint.md` — 3 arms, join-recall definition, honest verdict.

#### Files to edit
```
benchmarks/run_m63_vector_join.py (NEW) — 3-arm harness, exact GT on subset, join-recall min/mean±std + latency, json.dump
```

#### Deep file dependency analysis
- New standalone script; imports `theodb_bench.metrics.latency_percentiles` (Rule 9, no new infra). Reads a small SIFT/gaussian subset so the exact `a × b` GT is computable. Larger scale used only for the latency axis (where brute-force is intractable).

#### Deep Dives
- **Arms:** T1 = LATERAL over `theodb_hnsw`; T2 = `a CROSS JOIN b ORDER BY dist` top-level (O(n·m), recall 1.0 by construction — the ceiling/anti-objective); T3 = same LATERAL over a pgvector-hnsw container.
- **join-recall:** per outer row `recall_i`, report `min`, `mean`, `pstdev` (R2 — min surfaces recall-0 rows).
- **Statistical rigor:** ≥3 runs, mean ± std, cite hardware (analysis-golden-rule). Median latency, back-to-back (R4).
- **Edge cases:** the naive T2 will not finish at large `b` — run it only up to a tractable `b`; mark larger `b` T2 as `UNBENCHMARKED` (honest, not masked). If the planner does not push the index for some shape (R1), mark that arm `UNBENCHMARKED`/BLOCKED with the `EXPLAIN` evidence.

#### Pseudo-code / Signatures
```pseudocode
def run(n_a, n_b, dim, k, runs):
  a, b := gaussian_mixture(seeded)            # tractable n_b for exact GT
  exact_gt := { a_i: exact_topk(b, a_i, k) }  # O(n·m) brute force
  for arm in [T1_lateral_index, T2_naive_sort, T3_pgvector]:
    recs, lats := [], []
    for _ in range(runs):
      per_row := arm.join(a, b, k)
      recs.append([ jaccard_topk(per_row[i], exact_gt[a_i]) for a_i ])
      lats.append(latency_percentiles(arm.timings))
    agg[arm] = {recall_min, recall_mean, recall_std, p50, p95}
  gate := agg[T1].recall_mean >= agg[T3].recall_mean - 0.01
  json.dump({config, agg, gate, hardware}, out)
```

#### Tasks
1. Write the harness (arms, exact GT, join-recall, latency, json.dump) reusing M52 helpers.
2. Add the DoD gate (T1 recall ≥ T3 − 0.01).
3. Add `UNBENCHMARKED` markers for intractable T2 scale + any R1 non-pushed shape.

#### TDD
```
RED:     test_join_recall_definition() (in benchmarks/tests/) — jaccard_topk of identical sets == 1.0; disjoint == 0.0; asserts the metric math
RED:     test_arms_produce_json_schema() — run() on a tiny fixture emits {agg, gate, hardware} with the 3 arms
GREEN:   implement run_m63_vector_join.py to pass the metric + schema tests
REFACTOR: reuse M52 _make_dataset/_ground_truth rather than re-deriving (Rule 9)
VERIFY:  python -m pytest benchmarks/tests/test_run_m63_vector_join.py -v
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded; the harness runs arms sequentially, no shared mutable state)
```

#### Acceptance Criteria
- [ ] `benchmarks/run_m63_vector_join.py` emits `docs/benchmarks/m63-vector-join.json` with the 3 arms, join-recall (min/mean/std), p50/p95, hardware, and the DoD gate boolean.
- [ ] join-recall metric math is unit-tested (identical→1.0, disjoint→0.0) — R2.
- [ ] T2 intractable scale + any R1 non-pushed shape are marked `UNBENCHMARKED`, not silently dropped.
- [ ] Pass: lint — `ruff check benchmarks/run_m63_vector_join.py` zero warnings.
- [ ] Pass: size — file ≤ 500 lines.

#### DoD (Definition of Done)
- [ ] Harness tests RED-first then GREEN.
- [ ] `python -m pytest benchmarks/tests/` green for the new tests.
- [ ] JSON artifact reproducible (seed fixed, ≥3 runs).

### T3.2 — End-to-end dedup/entity-resolution self-join + honest verdict report

#### Objective
Run the kNN-self-join dedup (`… CROSS JOIN LATERAL (SELECT b.id FROM t b WHERE b.id <> a.id ORDER BY b.emb <=> a.emb LIMIT 1) j WHERE (a.emb <=> j.emb) < τ`) over a corpus with planted duplicates, measure duplicate-detection precision/recall, and write `docs/benchmarks/m63-vector-join.md` with a per-axis honest verdict.

#### Why this step (action + reasoning — ReAct discipline)
1. **What this step does** — adds the dedup case to the harness (planted duplicates → measure detection precision/recall) and writes the `.md` report that reads the `.json` and gives PARITY/SUPERIOR/GAP per axis (join-recall, p50, buffers) with no cherry-pick.
2. **Why it is necessary now** — the DoD requires the end-to-end dedup case in pure SQL; and Rule 5/public-copy require the honest verdict report alongside the JSON. This is the "vector-join as a real relational feature" proof, not just a micro-benchmark.

#### Evidence
- ROADMAP.md § M63 DoD line 3 — "Caso end-to-end: deduplicação/entity-resolution por similaridade em SQL puro".
- Blueprint § "Coverage Corner 4" (dedup query) + § "Design do benchmark" point 5.
- `docs/benchmarks/m52-filtered-ann.md` — the honest-verdict report template.
- `../../../rules/public-copy.md` § 4 — comparative claims need the benchmark artifact linked.

#### Files to edit
```
benchmarks/run_m63_vector_join.py — add dedup arm (planted duplicates → precision/recall)
docs/benchmarks/m63-vector-join.md (NEW) — honest per-axis verdict, reads the json, links the harness command
docs/benchmarks/m63-vector-join.json (NEW) — emitted by the harness (dedup metrics included)
docs/adr/0022-m63-vector-join-lateral-not-node.md (NEW) — records D1 + D2 decision (incl. helper go/no-go outcome)
CHANGELOG.md — [Unreleased] entry per shipped surface (benchmark, + helper if D2 accepted)
```

#### Deep file dependency analysis
- The `.md` reads only the `.json` (no re-run needed to read it). The ADR records the final D1/D2 outcome (helper shipped or not). CHANGELOG gains an `Added` entry for the benchmark and, if shipped, the helper.

#### Deep Dives
- **Dedup metric:** plant K known duplicate pairs; the self-join must recover them via `< τ`. precision = correct dup pairs / reported; recall = recovered / planted. Report both (a mean would hide missed dups — R2 discipline).
- **Invariant:** the self-join Index Scan survives the `WHERE b.id <> a.id` inner predicate (Q1, cross-checked with T1.1's dedup-shape `EXPLAIN`).
- **Honest verdict:** per axis PARITY/SUPERIOR/GAP; if T1 loses latency to T3 at scale, state it (Q2) — the DoD is "uses index, not O(n²)", which T1 meets and T2 fails, independent of the latency race.

#### Pseudo-code / Signatures
```pseudocode
# dedup arm
plant D duplicate pairs into corpus t
pairs := SELECT a.id, j.id FROM t a CROSS JOIN LATERAL
           (SELECT b.id, b.emb <=> a.emb AS d FROM t b WHERE b.id <> a.id
            ORDER BY b.emb <=> a.emb LIMIT 1) j WHERE j.d < tau
precision := |pairs ∩ planted| / |pairs|
recall    := |pairs ∩ planted| / D
# report writer
md := render(json.load("m63-vector-join.json"))  # per-axis PARITY/SUPERIOR/GAP, no cherry-pick
```

#### Tasks
1. Add the dedup arm (planted dups → precision/recall) to the harness.
2. Write `docs/benchmarks/m63-vector-join.md` reading the JSON, per-axis honest verdict + reproduction command + hardware.
3. Write ADR 0022 recording D1 + the D2 helper outcome.
4. Update CHANGELOG `[Unreleased] § Added`.

#### TDD
```
RED:     test_dedup_precision_recall() — on a fixture with K planted dups, the self-join recovers them; precision/recall computed correctly
GREEN:   implement the dedup arm; run the harness to emit the json; write the md from the json
REFACTOR: None expected (report is generated, not hand-typed numbers)
VERIFY:  python -m pytest benchmarks/tests/test_run_m63_vector_join.py -k dedup -v
```

#### Concurrency tests (only when applicable)
```
(none — single-threaded)
```

#### Acceptance Criteria
- [ ] Dedup arm reports duplicate-detection precision AND recall (both, not a blended score).
- [ ] `docs/benchmarks/m63-vector-join.md` gives a per-axis PARITY/SUPERIOR/GAP verdict with no cherry-pick, links the exact reproduction command + hardware (public-copy §4).
- [ ] `docs/benchmarks/m63-vector-join.json` exists and the `.md` numbers trace to it (no hand-typed perf claim).
- [ ] `docs/adr/0022-m63-vector-join-lateral-not-node.md` records D1 + the D2 helper go/no-go outcome.
- [ ] `CHANGELOG.md` `[Unreleased] § Added` has an entry per shipped surface.
- [ ] Pass: lint — `ruff check` zero warnings on changed files.

#### DoD (Definition of Done)
- [ ] Dedup test RED-first then GREEN.
- [ ] JSON + MD + ADR + CHANGELOG present and consistent.
- [ ] `python -m pytest benchmarks/tests/` green.

---

## Coverage Matrix

| # | Gap / Requirement (ROADMAP § M63 DoD + blueprint) | Task(s) | Resolution |
|---|---|---|---|
| 1 | Similarity join uses the index (not nested-loop O(n²)); planner picks the AM | T1.1 | `EXPLAIN` `#[pg_test]` asserts inner branch = Index Scan on theodb_hnsw, not Seq Scan |
| 2 | Recall preserved | T1.2 | join-recall vs exact O(n·m) GT, per-row min + mean±std ≥ tol |
| 3 | Threshold/range join `a JOIN b ON a.emb <=> b.emb < τ`; typed error on bad input | T1.3 | threshold-correct at τ∈{0,mid,large}; negative τ → typed error / documented contract |
| 4 | Optional helper `theodb.vector_join` (blueprint deliverable 4) — parsimony decision | T2.1, D2 | `EXPLAIN` go/no-go acceptance test; ship only if Index Scan preserved, else raw-LATERAL-only |
| 5 | Benchmark recall/latency of join vs seqscan → `docs/benchmarks/m63-vector-join.{md,json}` | T3.1, T3.2 | 3-arm harness (T1/T2/T3), join-recall + latency JSON + honest verdict MD |
| 6 | End-to-end dedup/entity-resolution in pure SQL | T3.2 | kNN-self-join with planted dups → detection precision/recall |
| 7 | Existing suite does not regress; runs on theodb:m63 | T4.1 | Integration Validation chain green on the existing image |
| 8 | ADR recording the LATERAL-vs-node + helper decisions | T3.2 | `docs/adr/0022-...` records D1 + D2 outcome |
| 9 | CHANGELOG updated (Rule 6) | T3.2 | `[Unreleased] § Added` per shipped surface |

**Coverage: 9/9 gaps covered (100%)**

## Global Definition of Done

- [ ] All phases completed
- [ ] All tests passing — `cargo pgrx test -p theodb_rs` + `python -m pytest benchmarks/tests/` green
- [ ] Zero type errors — `cargo check -p theodb_rs`
- [ ] Zero lint warnings — `cargo clippy -p theodb_rs` + `ruff check benchmarks/`
- [ ] File-size budget respected (per `rules/architecture.md`) — `api.rs` split if the helper pushes it over 500 LoC
- [ ] CHANGELOG.md updated under `[Unreleased]` (Unbreakable Rule 6)
- [ ] Backward compatibility preserved across public API — existing `theodb.*` untouched; `amcanorderbyop` unchanged
- [ ] Plan-specific criteria: `vector_join_uses_index_scan` GREEN (Index Scan proven); `vector_join_recall_matches_exact_within_tol` GREEN (recall preserved); `docs/benchmarks/m63-vector-join.json` reproducible; ADR 0022 records D1/D2
- [ ] **Runtime-metric proof** — N/A: M63 adds no runtime counter (validation + measurement milestone). The "observable in real workload" proof is the benchmark JSON (arms run against the real `theodb:m63` container) + the integration `EXPLAIN` against real Postgres, not a Prometheus counter. Declared honestly rather than fabricating a metric.
- [ ] **Plan archived** — after `/review` returns `READY_TO_MERGE` AND the PR is merged, move this plan to `knowledge-base/plans/completed/`.

## Failure scenarios (when I/O external)

M63's tests are `#[pg_test]` (in-process Postgres) and the benchmark queries a real Postgres/pgvector container (DB driver). The DB-boundary failure modes:

| Dependency | Failure mode | How the test reproduces it | Expected behavior |
|---|---|---|---|
| `theodb:m63` (Postgres, benchmark harness via psql/driver) | Planner does NOT push the index in a given LATERAL shape (R1) | `EXPLAIN` assertion in `run_m63_vector_join.py` per arm | that arm is marked `UNBENCHMARKED`/BLOCKED with the `EXPLAIN` text as evidence; NOT masked as a passing result |
| `theodb_hnsw` index scan (in-process) | negative/invalid τ or k passed to the helper | `vector_join_negative_threshold_errors` / `vector_join_helper_rejects_bad_k` | typed `ERROR`, no panic across the C boundary, no partial/garbage result |
| `pgvector` control container (T3) | container absent / connection refused | harness catches the connection error | T3 arm marked skipped with reason in JSON; T1/T2 still run (control is not a hard dependency of the DoD gate) |

## Final Phase: Integration Validation (MANDATORY)

> Runs after Phases 1–3. The plan is NOT done until this chain passes on the existing `theodb:m63` image.

#### T4.1 — Integration Validation (non-regression + reproducible artifact)

**Objective:** validate the LATERAL join + optional helper + benchmark work against real Postgres, and the existing suite does not regress.

### Execution

```
cargo pgrx test -p theodb_rs          # unit + #[pg_test] (incl. new vector_join_* + M52/M20-22/M45/M52 regression)
cargo clippy -p theodb_rs             # zero lint warnings
cargo check -p theodb_rs              # zero type errors
python -m pytest benchmarks/tests/    # harness metric + schema tests
ruff check benchmarks/                # zero lint warnings on the harness
python benchmarks/run_m63_vector_join.py --out docs/benchmarks/m63-vector-join.json   # emits the reproducible artifact against theodb:m63
```

### Acceptance Criteria

- [ ] All `#[pg_test]`s green, including the new `vector_join_*` and the pre-existing M20–M22 / M45 / M52 (no regression).
- [ ] `vector_join_uses_index_scan` proves Index Scan (structural gate); `vector_join_recall_matches_exact_within_tol` proves recall preserved (primary metric).
- [ ] `docs/benchmarks/m63-vector-join.json` produced against the real container; the `.md` verdict traces to it.
- [ ] Zero type errors; zero lint warnings.
- [ ] Runtime-metric proof — N/A (declared above); the equivalent "real workload" evidence is the benchmark JSON + integration `EXPLAIN`.
- [ ] Failure scenarios green — the R1 non-push shape is exercised and produces an `UNBENCHMARKED` marker, not a masked pass.

### If Validation Fails

1. Identify plan-caused vs pre-existing failures.
2. Fix all plan-caused failures (esp. an R1 shape that Seq-Scans — re-phrase or document the fallback per D1/blueprint alt-C).
3. Re-run the chain.
4. Pre-existing issues logged in the PR description, do NOT block completion.
