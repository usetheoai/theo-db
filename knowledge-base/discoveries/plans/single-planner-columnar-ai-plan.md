# Discovery Plan: Single-Planner In-Postgres Columnar + Vectorized Execution + AI (AlloyDB-class HTAP)

**Version:** v1.0
**Slug:** `single-planner-columnar-ai`
**Owner:** paulohenriquevn (CTO) / Eng
**Created:** 2026-07-14
**Time budget:** 26h (per-project breakdown in ADR D1)

## Context

The owner directed a long-horizon bet (time is NOT a constraint) to match AlloyDB's model — **one engine, one
planner, columnar in-core, auto-maintained** — so AI operators + vector search + columnar aggregation compose in a
**single query plan**. The pre-discovery deep-research dossier
(`knowledge-base/discoveries/single-planner-columnar-ai-research-dossier.md`) collected the 2026 SOTA sources and
surfaced the load-bearing finding: a **DataFusion-vectorized `CustomScan` single-planner** route EXISTS (ParadeDB
`pg_search` is the live AGPL reference impl), breaking the pg_duckdb two-engine ceiling (ADR-0023), and it re-opens
the M97 DEFER (ADR-0041 — which ALSO wrongly barred Hydra columnar as AGPL when it is Apache-2.0). Before locking any
architectural bet or roadmap, this discovery investigates HOW the reference implementations actually build the four
load-bearing pieces (single-planner CustomScan↔Arrow↔DataFusion; permissive columnar storage + MVCC; AI operators as
plan nodes; vector+columnar shared substrate) so the blueprint + roadmap are grounded in read code, not optimism.

## Objective

Produce a blueprint that answers, with line-exact citations from the reference clones, HOW to build a permissive
(Apache-2.0/MIT) single-planner columnar+vectorized+AI execution path in a pgrx PG17 extension — success = every
research question answered with ≥1 reference citation, all 4 coverage corners populated, and an honest feasibility
+ ceiling verdict (DuckDB/Photon-class, capability-match not superiority) that a multi-milestone roadmap can consume.

## In-Scope / Out-of-Scope

### In-Scope (per reference project)

- `.claude/knowledge-base/references/paradedb/pg_search/src/postgres/customscan/` + `.../src/scan/` — the single-planner CustomScan↔DataFusion seam (AGPL — STUDY DESIGN ONLY, never copy code).
- `.claude/knowledge-base/references/hydra/columnar/src/backend/columnar/` — the permissive (Apache-2.0) columnar TAM storage + MVCC.
- `.claude/knowledge-base/references/citus/src/backend/columnar/` — the canonical columnar-TAM design (AGPL — STUDY DESIGN ONLY; Hydra is its Apache-2.0 twin for citations).
- `.claude/knowledge-base/references/datafusion/datafusion/` — the vectorized physical `ExecutionPlan` + `Expr` model.
- `.claude/knowledge-base/references/lance/rust/lance-index/` + `.claude/knowledge-base/references/lance/docs/` — vector-in-columnar format.
- `.claude/knowledge-base/references/lotus/lotus/sem_ops/` + `.claude/knowledge-base/references/palimpzest/src/palimpzest/query/optimizer/` — semantic-operator algebra + AI-op cost model.
- `.claude/knowledge-base/references/postgresml/pgml-extension/src/` — batched in-Postgres model inference (the ADR-0007 per-row conflict).

### Out-of-Scope (explicit)

- `.claude/knowledge-base/references/pg_mooncake/` — its usable engine (moonlink) is BSL 1.1 (barred D1); out of scope except the license note already in the dossier.
- `.claude/knowledge-base/references/duckdb/` internals — the two-engine route is already shipped (M61) and rejected for single-planner; out of scope here.
- Build artifacts, `docs/`, vendored trees, `target/`, `node_modules/` in every reference — out of scope.
- ANY code copying from AGPL/BSL references (paradedb, citus, pg_mooncake) — design study only (D1/D3).
- Implementation / benchmarking — that is `/discover-execute` (blueprint) then the roadmap, not this plan.

## ADRs

### D1 — Time budget + stop conditions

**Decision:** 26h total — paradedb 8h (the closest analog, deepest dive), hydra 6h, datafusion 5h, lotus+palimpzest
4h, lance 2h, postgresml 1h. Per-question stop: after 3 empty Fase-A retries OR the project budget is exhausted,
mark the question BLOCKED with reason and advance.

**Rationale:** paradedb `pg_search` is the single live reference of the EXACT target architecture (evidence: the
dossier's 38-DataFusion-site finding), so it earns the deepest dive; hydra is the permissive storage twin; the rest
are focused technique reads. This mirrors the `discover-phd-rigor.md` frontier profile (techniques corner goes deep).

**Consequences:** a shallow read of a lower-budget project (lance/postgresml) may leave a follow-up question — accepted;
those are informational, not load-bearing for the go/no-go.

### D2 — Investigation depth (STUDY-design boundary for AGPL refs)

**Decision:** for AGPL/BSL references (paradedb, citus, pg_mooncake) — read for DESIGN + architecture only; capture the
PATTERN and the file:line as evidence of feasibility, NEVER transcribe code into the blueprint. For permissive refs
(hydra, datafusion, lance, lotus, palimpzest, postgresml) — read freely; code may be referenced/adapted downstream.

**Rationale:** D1 (`CLAUDE.md`) bars AGPL/BSL in the distribution; D3 + the `vectorchord-agpl-study-only` memory
establish "study design, reimplement clean". Honesty (Rule 3): the blueprint must be explicit about which findings
are study-only-design vs reusable-permissive.

**Consequences:** the blueprint's feasibility claims from paradedb are "the pattern is proven to work" not "here is
the code" — the own-code glue effort is scoped as essential complexity (Esforço ≠ Complexidade).

## Research Questions

Each question maps to exactly one Coverage Corner, declares its method (Fase A broad map + Fase B deep read), and a
target answer shape. All cited paths are pre-validated to exist.

| # | Question | Corner | Reference project(s) | Fase A (broad — ast-grep/grep map) | Fase B (deep — Read at each hotspot) | Expected answer shape |
|---|---|---|---|---|---|---|
| Q1 | How does pg_search wire a `CustomScan` node → Arrow `RecordBatch` → DataFusion `ExecutionPlan` inside ONE Postgres plan (the single-planner seam)? | techniques | `.claude/knowledge-base/references/paradedb/pg_search/src/postgres/customscan/`, `.claude/knowledge-base/references/paradedb/pg_search/src/scan/execution_plan.rs` | `grep -rn 'CustomScan\|create_upper_paths_hook\|set_rel_pathlist_hook\|RecordBatch' .claude/knowledge-base/references/paradedb/pg_search/src/postgres/customscan/` to map the hook + batch sites | Read `exec.rs`, `hook.rs`, `datafusion/` builders + `execution_plan.rs` — capture how a PG plan node materializes tuples into Arrow batches and drives a DataFusion plan (DESIGN only, AGPL) | Prose + diagram of the seam: planner hook → CustomPath → CustomScan exec → batch materialize → DataFusion ExecutionPlan, with file:line per stage |
| Q2 | How does the Hydra columnar TAM lay out storage (stripe/chunk/compression/skip-index + TID→stripe/row) AND enforce MVCC visibility + WAL + VACUUM for that columnar layout (the storage + the hardest MVCC problem, one coherent TAM design)? | techniques | `.claude/knowledge-base/references/hydra/columnar/src/backend/columnar/`, `.claude/knowledge-base/references/citus/src/backend/columnar/columnar_tableam.c` | `grep -rn 'stripe\|chunk_group\|compression\|Snapshot\|xmin\|xmax\|row_mask\|GenericXLog\|vacuum' .claude/knowledge-base/references/hydra/columnar/src/backend/columnar/columnar_storage.c .../columnar_writer.c .../write_state_row_mask.c .../columnar_tableam.c` | Read `columnar_storage.c`, `columnar_writer.c`, `columnar_reader.c`, `write_state_row_mask.c`, `columnar_tableam.c` — capture the stripe→chunk→per-column layout + compression + min/max skip + TID scheme, AND how visibility is resolved at chunk granularity + the update/delete envelope + WAL + the honest "append-mostly" scope (Hydra=Apache design; Citus=AGPL cross-check design only) | Two-part answer: (a) storage table [unit→size→compression→skip + TID→(stripe,row)]; (b) MVCC/WAL/vacuum prose + the append-mostly scope, file:line each |
| Q3 | How does DataFusion structure a vectorized physical `ExecutionPlan` + `Expr` so a PG qual/aggregate can be TRANSLATED to it programmatically? | techniques | `.claude/knowledge-base/references/datafusion/datafusion/` | `grep -rln 'trait ExecutionPlan\|impl ExecutionPlan\|pub enum Expr' .claude/knowledge-base/references/datafusion/datafusion/physical-plan/src/ .claude/knowledge-base/references/datafusion/datafusion/expr/` | Read the `ExecutionPlan` trait + a concrete physical operator (aggregate/filter) + the `Expr` enum — capture how to build a plan + translate a scalar predicate to a DataFusion `Expr` | The operator/`Expr` model + the programmatic-plan-build API surface a pgrx extension would call, file:line |
| Q4 | How does Lance store vector indexes (IVF/HNSW) NATIVELY in a columnar format alongside scalar columns (vector+columnar unified substrate)? | techniques | `.claude/knowledge-base/references/lance/rust/lance-index/src/`, `.claude/knowledge-base/references/lance/docs/` | `grep -rln 'ivf\|hnsw\|VectorIndex\|struct.*Index' .claude/knowledge-base/references/lance/rust/lance-index/src/` + Glob `.claude/knowledge-base/references/lance/docs/src/format/*` | Read the vector-index module + the format doc — capture how a vector index co-resides with columnar scalar data + random-access reads | Layout description: vector index ↔ columnar column co-location + the read path, file:line |
| Q5 | How do LOTUS (`sem_ops`) + Palimpzest (`optimizer`) model AI/semantic operators as OPTIMIZABLE physical operators (sem_filter/join/topk + a cost model that reorders a cheap filter before an expensive AI op)? | techniques | `.claude/knowledge-base/references/lotus/lotus/sem_ops/`, `.claude/knowledge-base/references/palimpzest/src/palimpzest/query/optimizer/` | `grep -rn 'def sem_filter\|def sem_join\|def sem_topk\|class.*Operator\|cost' .claude/knowledge-base/references/lotus/lotus/sem_ops/sem_filter.py .../sem_join.py` + `find .claude/knowledge-base/references/palimpzest/src/palimpzest/query/optimizer -name '*.py'` | Read `sem_filter.py`/`sem_join.py`/`sem_topk.py` + the palimpzest optimizer — capture the operator algebra + the cost/rewrite model (both permissive: Apache-2.0/MIT) | The semantic-operator algebra (inputs/outputs/optimizations) + the cost-model shape for placing an AI op in a plan, file:line |
| Q6 | What DataFusion + arrow-rs crate versions does pg_search pin, and are those compatible with pgrx `=0.16.1` / PG17 in a single extension build (FFI/version risk)? | deps | `.claude/knowledge-base/references/paradedb/pg_search/`, `.claude/knowledge-base/references/datafusion/` | Grep `Cargo.toml` for `datafusion`/`arrow`/`pgrx` version pins in `.claude/knowledge-base/references/paradedb/pg_search/Cargo.toml` + `.claude/knowledge-base/references/datafusion/Cargo.toml` (text-shape — Fase A skipped) | Read each `Cargo.toml` match in context — capture the version matrix + whether DataFusion+arrow can coexist with pgrx 0.16.1 in one crate | Version matrix (datafusion/arrow/pgrx) + the honest coexistence/FFI risk, citations |
| Q7 | Does pgrx `=0.16.1` expose the `TableAmRoutine` FFI (native columnar TAM feasibility), and what raw-FFI surface does a columnar TAM need? | deps | `.claude/knowledge-base/references/hydra/columnar/`, TheoDB `theodb_rs/src/am/` (existing IndexAmRoutine prior art) | Grep the pgrx pg_sys bindings for `TableAmRoutine` slots + cross-check against Hydra's C `columnar_tableam.c` callback set | Read the binding + the Hydra callback list — capture which TAM callbacks exist in pgrx 0.16.1 vs must be hand-FFI'd | The available `TableAmRoutine` FFI surface + the hand-FFI gaps + the append-only-first feasibility verdict, citations |
| Q8 | How does pg_search build the DataFusion+pgrx integration + run its CustomScan tests (the build/test toolchain we would mirror)? | tools | `.claude/knowledge-base/references/paradedb/pg_search/` | SKIP Fase A (text-shape) — Glob `.claude/knowledge-base/references/paradedb/pg_search/{Cargo.toml,Makefile,*.toml}` + find test dirs | Read the build files + a test harness file — capture the build command, the pgrx test flow, the CustomScan test shape | Build/test recipe (commands + test-harness shape) we would adapt, citations |
| Q9 | How do pg_search / Citus columnar test CORRECTNESS — MVCC visibility, crash-safety, and result-equivalence of the columnar/vectorized path vs the row-store (the correctness gate)? | tests | `.claude/knowledge-base/references/paradedb/pg_search/`, `.claude/knowledge-base/references/citus/src/test/` | `grep -rln 'mvcc\|snapshot\|isolation\|equivalen\|regress' .claude/knowledge-base/references/paradedb/pg_search/ .claude/knowledge-base/references/citus/src/test/` + find `*.sql`/`*.rs` test files | Read a correctness/isolation test — capture how they assert columnar results == row-store + how they test visibility/crash | The correctness-test patterns (result-equivalence + MVCC + crash) we must reproduce as our gate, citations |

## Coverage Matrix

Every Coverage Corner MUST have at least one Research Question mapped to it.

| Corner | Questions mapped | Status |
|---|---|---|
| Integration tests | Q9 | Covered |
| Dependencies | Q6, Q7 | Covered |
| Tools | Q8 | Covered |
| Techniques | Q1, Q2, Q3, Q4, Q5 | Covered |

**Coverage: 4/4 corners covered (100%)**

Techniques carries 5 questions (the `discover-phd-rigor.md` frontier profile: the SOTA/techniques axis goes deep — the
bet's load-bearing pieces are all "how do they build it"; Q2 folds the columnar-TAM storage + MVCC into one coherent
design question). 9 total, techniques at the max-5 ceiling — within the profile's window and the `MAX_PER_CORNER=5`
gate.

## Halt-loop Checkpoints

For `/discover-execute`:

| Checkpoint | Assertion | Action if fails |
|---|---|---|
| Before answering Qx | the `.claude/knowledge-base/references/{project}/{path}` declared in Fase A exists | Mark Qx BLOCKED "path not found", continue |
| Per-question Fase A budget | Fase A returned ≥1 hotspot OR 3 query-variant retries attempted | After 3 empty retries, mark Qx BLOCKED "Fase A exhausted", continue |
| After answering Qx | the blueprint section under Qx has ≥1 `.claude/knowledge-base/references/` citation | Re-iterate Qx (1 retry max) |
| AGPL-study guard (Q1, Q3-cross-check, Q10) | the blueprint captures DESIGN/pattern + file:line, NOT copied AGPL code | If code was transcribed, rewrite as design-prose (D2) |
| Per-project time budget | project budget (D1) not exhausted | When exhausted, mark that project's remaining Qx BLOCKED "budget exhausted", advance |
| Before promising complete | all 4 coverage corners have populated sections + ≥1 ADR synthesized | Refuse promise, continue iterating |

## Acceptance Criteria

- [ ] All 9 research questions answered OR explicitly BLOCKED with reason
- [ ] All four coverage corners have populated sections in the blueprint
- [ ] Every citation points to a real `.claude/knowledge-base/references/{...}` path
- [ ] AGPL/BSL findings (paradedb, citus, pg_mooncake) are DESIGN-only — no copied code (D2)
- [ ] At least one ADR in the blueprint synthesizes the go/no-go + the honest ceiling (capability-match, not superiority)
- [ ] The blueprint amends ADR-0041's Hydra-license error + records the DataFusion-CustomScan route it missed
- [ ] Time budget respected per project
- [ ] `/discover-confidence` verdict ≥ SHIPPABLE_WITH_CAVEATS
- [ ] Blueprint saved at `knowledge-base/discoveries/blueprints/single-planner-columnar-ai-blueprint.md`

## Global Definition of Done

- [ ] All phases completed (plan → edge-cases → plan-confidence → execute → confidence → improve if needed → re-score)
- [ ] Final `/discover-confidence` verdict recorded in the blueprint header
- [ ] No fabricated citations (all reference paths pre-validated)
- [ ] Coverage Matrix 100% covered
- [ ] ADRs reference a project rule/principle: D2 cites `CLAUDE.md` D1/D3 + the parsimony ladder (`.claude/rules/parsimony-ladder.md`); the honest-ceiling ADR cites `.claude/rules/public-copy.md` (performance is a claim, not opinion) + the M73/M97 discipline
